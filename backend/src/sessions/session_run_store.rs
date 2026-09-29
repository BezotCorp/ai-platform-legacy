use std::slice;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::{
    agents::ExecutionMode,
    conversation::{
        ConversationAuthor, ConversationEntry, MAX_ENTRIES, MAX_ENTRY_BYTES,
        MAX_HISTORY_BYTES, validate,
    },
    sessions::{SessionRun, SessionStore},
};

impl SessionStore {
    pub(crate) async fn queue_run(
        &self,
        session_id: String,
        expected_revision: i64,
        request_id: String,
        prompt: ConversationEntry,
    ) -> Result<(Vec<ConversationEntry>, ExecutionMode, i64)> {
        Self::validate_id(&session_id)?;
        if expected_revision < 1 {
            bail!("Révision de session invalide");
        }
        if request_id.is_empty() || request_id.len() > 128 {
            bail!("Identifiant d'exécution invalide");
        }
        validate(std::slice::from_ref(&prompt))?;
        if prompt.author != ConversationAuthor::Human {
            bail!("Une exécution exige un message utilisateur");
        }
        let project = self.project.clone();
        self.database
            .write(move |connection| {
                let stored = connection
                    .query_row(
                        "SELECT
                            revision,
                            messages,
                            configuration_mode
                         FROM sessions
                         WHERE project = ?1 AND id = ?2",
                        params![project, session_id],
                        |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, Option<String>>(2)?,
                            ))
                        },
                    )
                    .optional()?
                    .context("Session introuvable")?;
                if stored.0 != expected_revision {
                    bail!("Révision de session obsolète");
                }
                let encoded_mode = stored.2.context("Session sans configuration liée")?;
                let mode: ExecutionMode = serde_json::from_str(&encoded_mode)?;
                let mode = mode.validate()?;
                let mut messages: Vec<ConversationEntry> = serde_json::from_str(&stored.1)?;
                validate(&messages)?;
                let running: i64 = connection.query_row(
                    "SELECT COUNT(*)
                     FROM session_runs
                     WHERE project = ?1
                       AND session_id = ?2
                       AND status IN ('queued', 'running')",
                    params![project, session_id],
                    |row| row.get(0),
                )?;
                if running != 0 {
                    bail!("Session déjà en cours d'exécution");
                }
                // Réserver une place pour le prompt et une réponse de taille maximale.
                // Le nombre de messages et le budget en octets doivent tenir ensemble.
                let mut active_bytes = messages.iter().try_fold(0usize, |total, message| {
                    total
                        .checked_add(message.content.len())
                        .context("Conversation trop volumineuse")
                })?;
                let reserved_bytes = prompt
                    .content
                    .len()
                    .checked_add(MAX_ENTRY_BYTES)
                    .context("Conversation trop volumineuse")?;
                let max_previous_messages = MAX_ENTRIES - 2;
                let mut archived = Vec::new();
                while messages.len() > max_previous_messages
                    || active_bytes
                        .checked_add(reserved_bytes)
                        .is_none_or(|total| total > MAX_HISTORY_BYTES)
                {
                    let oldest = messages.remove(0);
                    active_bytes -= oldest.content.len();
                    archived.push(oldest);
                }
                if !archived.is_empty() {
                    let mut next_sequence: i64 = connection.query_row(
                        "SELECT COALESCE(MAX(sequence), 0)
                         FROM session_message_archive
                         WHERE project = ?1 AND session_id = ?2",
                        params![project, session_id],
                        |row| row.get(0),
                    )?;
                    for old in archived {
                        next_sequence = next_sequence
                            .checked_add(1)
                            .context("Archive de session saturée")?;
                        connection.execute(
                            "INSERT INTO session_message_archive
                                (project, session_id, sequence, role, content)
                             VALUES (?1, ?2, ?3, ?4, ?5)",
                            params![
                                project,
                                session_id,
                                next_sequence,
                                old.author.storage_name(),
                                old.content
                            ],
                        )?;
                    }
                }
                messages.push(prompt.clone());
                validate(&messages)?;
                let encoded = serde_json::to_string(&messages)?;
                let changed = connection.execute(
                    "UPDATE sessions
                     SET messages = ?3,
                         revision = revision + 1,
                         updated_at = unixepoch()
                     WHERE project = ?1
                       AND id = ?2
                       AND revision = ?4",
                    params![project, session_id, encoded, expected_revision,],
                )?;
                if changed != 1 {
                    bail!("Révision de session obsolète");
                }
                connection.execute(
                    "INSERT INTO session_runs (
                        project,
                        session_id,
                        request_id,
                        status,
                        prompt
                    )
                    VALUES (?1, ?2, ?3, 'queued', ?4)",
                    params![project, session_id, request_id, prompt.content,],
                )?;
                connection.execute(
                    "UPDATE session_runs
                     SET session_revision = ?4
                     WHERE project = ?1 AND session_id = ?2 AND request_id = ?3",
                    params![project, session_id, request_id, expected_revision + 1],
                )?;
                Ok((messages, mode, expected_revision + 1))
            })
            .await
    }

    pub(crate) async fn start_run(&self, session_id: String, request_id: String) -> Result<()> {
        Self::validate_id(&session_id)?;
        let project = self.project.clone();
        self.database
            .write(move |connection| {
                let changed = connection.execute(
                    "UPDATE session_runs SET status = 'running', updated_at = unixepoch()
                 WHERE project = ?1 AND session_id = ?2 AND request_id = ?3
                   AND status = 'queued'",
                    params![project, session_id, request_id],
                )?;
                if changed != 1 {
                    bail!("Exécution en attente introuvable ou déjà terminée");
                }
                Ok(())
            })
            .await
    }

    pub(crate) async fn complete_run(
        &self,
        session_id: String,
        request_id: String,
        answer: String,
    ) -> Result<i64> {
        Self::validate_id(&session_id)?;
        let response = ConversationEntry::ai(answer);
        validate(slice::from_ref(&response))?;
        let response_content = response.content.clone();
        let project = self.project.clone();
        self.database
            .write(move |connection| {
                let active: i64 = connection.query_row(
                    "SELECT COUNT(*)
                     FROM session_runs
                     WHERE project = ?1
                       AND session_id = ?2
                       AND request_id = ?3
                       AND status = 'running'",
                    params![project, session_id, request_id,],
                    |row| row.get(0),
                )?;
                if active != 1 {
                    bail!("Exécution de session introuvable");
                }
                let (revision, encoded): (i64, String) = connection.query_row(
                    "SELECT revision, messages
                         FROM sessions
                         WHERE project = ?1
                           AND id = ?2",
                    params![project, session_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                let mut messages: Vec<ConversationEntry> = serde_json::from_str(&encoded)?;
                messages.push(response);
                validate(&messages)?;
                let encoded = serde_json::to_string(&messages)?;
                connection.execute(
                    "UPDATE sessions
                     SET messages = ?3,
                         revision = revision + 1,
                         updated_at = unixepoch()
                     WHERE project = ?1
                       AND id = ?2",
                    params![project, session_id, encoded],
                )?;
                connection.execute(
                    "UPDATE session_runs
                     SET status = 'completed',
                         result = ?4,
                         session_revision = ?5,
                         updated_at = unixepoch()
                     WHERE project = ?1
                       AND session_id = ?2
                       AND request_id = ?3
                       AND status = 'running'",
                    params![
                        project,
                        session_id,
                        request_id,
                        response_content,
                        revision + 1
                    ],
                )?;
                Ok(revision + 1)
            })
            .await
    }

    pub(crate) async fn fail_run(
        &self,
        session_id: String,
        request_id: String,
        status: &'static str,
        error: String,
    ) -> Result<()> {
        Self::validate_id(&session_id)?;
        let project = self.project.clone();
        if !matches!(status, "failed" | "cancelled" | "interrupted") {
            bail!("État terminal d'exécution invalide");
        }
        let error = error.chars().take(2048).collect::<String>();
        self.database
            .write(move |connection| {
                let changed = connection.execute(
                    "UPDATE session_runs
                     SET status = ?4,
                         error = ?5,
                         updated_at = unixepoch()
                     WHERE project = ?1
                       AND session_id = ?2
                       AND request_id = ?3
                       AND status IN ('queued', 'running')",
                    params![project, session_id, request_id, status, error,],
                )?;
                if changed != 1 {
                    bail!("Exécution de session introuvable");
                }
                Ok(())
            })
            .await
    }

    pub(crate) async fn load_run(
        &self,
        session_id: String,
        request_id: String,
    ) -> Result<Option<SessionRun>> {
        Self::validate_id(&session_id)?;
        if request_id.is_empty() || request_id.len() > 128 {
            bail!("Identifiant d'exécution invalide");
        }
        let project = self.project.clone();
        self.database
            .read(move |connection| {
                Ok(connection
                    .query_row(
                        "SELECT request_id, status, prompt, error, result,
                        session_revision, created_at, updated_at
                 FROM session_runs
                 WHERE project = ?1 AND session_id = ?2 AND request_id = ?3",
                        params![project, session_id, request_id],
                        SessionRun::from_row,
                    )
                    .optional()?)
            })
            .await
    }

    pub(crate) async fn runs(
        &self,
        session_id: String,
        before: Option<(i64, String)>,
    ) -> Result<Vec<SessionRun>> {
        Self::validate_id(&session_id)?;
        if before.as_ref().is_some_and(|(created_at, request_id)| {
            *created_at < 0 || request_id.is_empty() || request_id.len() > 128
        }) {
            bail!("Curseur d'exécution invalide");
        }
        let (before_created_at, before_request_id) = match before {
            Some((created_at, request_id)) => (Some(created_at), Some(request_id)),
            None => (None, None),
        };
        let project = self.project.clone();
        self.database
            .read(move |connection| {
                let mut statement = connection.prepare(
                    "SELECT
                        request_id,
                        status,
                        prompt,
                        error,
                        result,
                        session_revision,
                        created_at,
                        updated_at
                     FROM session_runs
                     WHERE project = ?1
                       AND session_id = ?2
                       AND (
                           ?3 IS NULL OR created_at < ?3 OR
                           (created_at = ?3 AND request_id < ?4)
                       )
                     ORDER BY created_at DESC,
                              request_id DESC
                     LIMIT 50",
                )?;
                let rows = statement.query_map(
                    params![project, session_id, before_created_at, before_request_id],
                    SessionRun::from_row,
                )?;
                Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await
    }
}

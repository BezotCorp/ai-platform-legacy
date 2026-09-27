use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::{
    agents::ExecutionMode,
    configurations::SavedConfiguration,
    sessions::{
        ArchivedMessage, BoundSession, History, Message, Session, SessionRun, SessionStore,
    },
};

impl SessionStore {
    pub(crate) async fn bind(
        &self,
        id: String,
        expected_revision: i64,
        configuration: SavedConfiguration,
    ) -> Result<Session> {
        Self::validate_id(&id)?;
        if expected_revision < 0 {
            bail!("Révision de session invalide");
        }
        let mode = configuration.mode.validate()?;
        let encoded_mode = serde_json::to_string(&mode)?;
        let project = self.project.clone();
        self.database
            .write(move |connection| {
                let changed = if expected_revision == 0 {
                    connection.execute(
                        "INSERT INTO sessions (
                            project,
                            id,
                            messages,
                            configuration_id,
                            configuration_revision,
                            configuration_mode
                        )
                        VALUES (?1, ?2, '[]', ?3, ?4, ?5)
                        ON CONFLICT(project, id)
                        DO NOTHING",
                        params![
                            project,
                            id,
                            configuration.id,
                            configuration.revision,
                            encoded_mode,
                        ],
                    )?
                } else {
                    connection.execute(
                        "UPDATE sessions
                         SET configuration_id = ?3,
                             configuration_revision = ?4,
                             configuration_mode = ?5,
                             revision = revision + 1,
                             updated_at = unixepoch()
                         WHERE project = ?1
                           AND id = ?2
                           AND revision = ?6
                           AND NOT EXISTS (
                               SELECT 1
                               FROM session_runs
                               WHERE project = ?1
                                 AND session_id = ?2
                                 AND status = 'running'
                           )",
                        params![
                            project,
                            id,
                            configuration.id,
                            configuration.revision,
                            encoded_mode,
                            expected_revision,
                        ],
                    )?
                };
                if changed != 1 {
                    bail!("Session existante, révision obsolète ou exécution en cours");
                }
                let session = connection.query_row(
                    "SELECT id, revision, created_at, updated_at
                     FROM sessions
                     WHERE project = ?1 AND id = ?2",
                    params![project, id],
                    |row| {
                        Ok(Session {
                            id: row.get(0)?,
                            revision: row.get(1)?,
                            created_at: row.get(2)?,
                            updated_at: row.get(3)?,
                        })
                    },
                )?;
                Ok(session)
            })
            .await
    }

    pub(crate) async fn resume(&self, id: String) -> Result<Option<BoundSession>> {
        Self::validate_id(&id)?;
        let project = self.project.clone();
        self.database
            .read(move |connection| {
                let stored = connection
                    .query_row(
                        "SELECT
                            id,
                            revision,
                            created_at,
                            updated_at,
                            messages,
                            configuration_id,
                            configuration_revision,
                            configuration_mode
                         FROM sessions
                         WHERE project = ?1 AND id = ?2",
                        params![project, id],
                        |row| {
                            let session = Session {
                                id: row.get(0)?,
                                revision: row.get(1)?,
                                created_at: row.get(2)?,
                                updated_at: row.get(3)?,
                            };

                            Ok((
                                session,
                                row.get::<_, String>(4)?,
                                row.get::<_, Option<String>>(5)?,
                                row.get::<_, Option<i64>>(6)?,
                                row.get::<_, Option<String>>(7)?,
                            ))
                        },
                    )
                    .optional()?;
                stored
                    .map(
                        |(
                            session,
                            encoded_messages,
                            configuration_id,
                            configuration_revision,
                            encoded_mode,
                        )|
                         -> Result<BoundSession> {
                            let configuration_id =
                                configuration_id.context("Session sans configuration liée")?;
                            let configuration_revision = configuration_revision
                                .context("Révision de configuration absente")?;
                            let encoded_mode =
                                encoded_mode.context("Configuration de session absente")?;
                            let messages: Vec<Message> = serde_json::from_str(&encoded_messages)?;
                            History::validate(&messages)?;
                            let mode: ExecutionMode = serde_json::from_str(&encoded_mode)?;
                            Ok(BoundSession {
                                history: History { session, messages },
                                configuration_id,
                                configuration_revision,
                                mode: mode.validate()?,
                            })
                        },
                    )
                    .transpose()
            })
            .await
    }

    pub(crate) async fn begin_run(
        &self,
        session_id: String,
        expected_revision: i64,
        request_id: String,
        prompt: Message,
    ) -> Result<(Vec<Message>, ExecutionMode, i64)> {
        Self::validate_id(&session_id)?;
        if expected_revision < 1 {
            bail!("Révision de session invalide");
        }
        if request_id.is_empty() || request_id.len() > 128 {
            bail!("Identifiant d'exécution invalide");
        }
        History::validate(std::slice::from_ref(&prompt))?;
        if prompt.role != "user" {
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
                let mut messages: Vec<Message> = serde_json::from_str(&stored.1)?;
                History::validate(&messages)?;
                let running: i64 = connection.query_row(
                    "SELECT COUNT(*)
                     FROM session_runs
                     WHERE project = ?1
                       AND session_id = ?2
                       AND status = 'running'",
                    params![project, session_id],
                    |row| row.get(0),
                )?;
                if running != 0 {
                    bail!("Session déjà en cours d'exécution");
                }
                // Déplacer les messages anciens avant le nouveau tour, sans les perdre.
                let overflow = messages.len().saturating_sub(30);
                if overflow > 0 {
                    let mut next_sequence: i64 = connection.query_row(
                        "SELECT COALESCE(MAX(sequence), 0)
                         FROM session_message_archive
                         WHERE project = ?1 AND session_id = ?2",
                        params![project, session_id],
                        |row| row.get(0),
                    )?;
                    for old in messages.drain(..overflow) {
                        next_sequence = next_sequence
                            .checked_add(1)
                            .context("Archive de session saturée")?;
                        connection.execute(
                            "INSERT INTO session_message_archive
                                (project, session_id, sequence, role, content)
                             VALUES (?1, ?2, ?3, ?4, ?5)",
                            params![project, session_id, next_sequence, old.role, old.content],
                        )?;
                    }
                }
                messages.push(prompt.clone());
                History::validate(&messages)?;
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
                    VALUES (?1, ?2, ?3, 'running', ?4)",
                    params![project, session_id, request_id, prompt.content,],
                )?;
                Ok((messages, mode, expected_revision + 1))
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
        let response = Message {
            role: "assistant".to_owned(),
            content: answer,
        };
        History::validate(std::slice::from_ref(&response))?;
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
                let mut messages: Vec<Message> = serde_json::from_str(&encoded)?;
                messages.push(response);
                History::validate(&messages)?;
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
                         updated_at = unixepoch()
                     WHERE project = ?1
                       AND session_id = ?2
                       AND request_id = ?3
                       AND status = 'running'",
                    params![project, session_id, request_id,],
                )?;
                Ok(revision + 1)
            })
            .await
    }

    pub(crate) async fn fail_run(
        &self,
        session_id: String,
        request_id: String,
        cancelled: bool,
        error: String,
    ) -> Result<()> {
        Self::validate_id(&session_id)?;
        let project = self.project.clone();
        let status = if cancelled { "cancelled" } else { "failed" };
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
                       AND status = 'running'",
                    params![project, session_id, request_id, status, error,],
                )?;
                if changed != 1 {
                    bail!("Exécution de session introuvable");
                }
                Ok(())
            })
            .await
    }

    pub(crate) async fn runs(&self, session_id: String) -> Result<Vec<SessionRun>> {
        Self::validate_id(&session_id)?;
        let project = self.project.clone();
        self.database
            .read(move |connection| {
                let mut statement = connection.prepare(
                    "SELECT
                        request_id,
                        status,
                        prompt,
                        error,
                        created_at,
                        updated_at
                     FROM session_runs
                     WHERE project = ?1
                       AND session_id = ?2
                     ORDER BY created_at DESC,
                              request_id DESC
                     LIMIT 50",
                )?;
                let rows =
                    statement.query_map(params![project, session_id], SessionRun::from_row)?;
                Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await
    }
}

impl SessionStore {
    /// Pages d'archives ordonnées de la plus récente à la plus ancienne.
    pub(crate) async fn archive(
        &self,
        session_id: String,
        before_sequence: Option<i64>,
    ) -> Result<Vec<ArchivedMessage>> {
        Self::validate_id(&session_id)?;
        if before_sequence.is_some_and(|sequence| sequence < 1) {
            bail!("Curseur d'archive invalide");
        }
        let project = self.project.clone();
        self.database
            .read(move |connection| {
                let mut statement = connection.prepare(
                    "SELECT sequence, role, content, archived_at
                     FROM session_message_archive
                     WHERE project = ?1 AND session_id = ?2
                       AND (?3 IS NULL OR sequence < ?3)
                     ORDER BY sequence DESC LIMIT 50",
                )?;
                let messages =
                    statement.query_map(params![project, session_id, before_sequence], |row| {
                        Ok(ArchivedMessage {
                            sequence: row.get(0)?,
                            message: Message {
                                role: row.get(1)?,
                                content: row.get(2)?,
                            },
                            archived_at: row.get(3)?,
                        })
                    })?;
                Ok(messages.collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await
    }
}

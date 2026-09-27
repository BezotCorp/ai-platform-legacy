use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::sessions::{SessionAgentTurn, SessionRunEvent, SessionStore};

const MAX_EVENT_BYTES: usize = 64 * 1024;
const MAX_AGENT_ANSWER_BYTES: usize = 32 * 1024;
const MAX_EVENTS_PER_RUN: i64 = 4000;
const MAX_AGENT_TURNS_PER_RUN: i64 = 512;

impl SessionStore {
    pub(crate) async fn record_event(
        &self,
        session_id: String,
        request_id: String,
        kind: String,
        data: Value,
    ) -> Result<()> {
        Self::validate_id(&session_id)?;
        let encoded = serde_json::to_string(&data)?;
        let encoded = if encoded.len() > MAX_EVENT_BYTES {
            let digest = Sha256::digest(encoded.as_bytes());
            let checksum = digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            serde_json::to_string(&json!({
                "truncated": true,
                "original_bytes": encoded.len(),
                "sha256": checksum,
                "agent_id": data.get("agent_id"),
                "call_id": data.get("call_id"),
                "tool": data.get("tool"),
            }))?
        } else {
            encoded
        };
        let project = self.project.clone();
        self.database
            .write(move |connection| {
                let sequence: i64 = connection.query_row(
                    "SELECT COALESCE(MAX(sequence), 0) + 1
                     FROM session_run_events
                     WHERE project = ?1
                       AND session_id = ?2
                       AND request_id = ?3",
                    params![project, session_id, request_id],
                    |row| row.get(0),
                )?;
                if sequence > MAX_EVENTS_PER_RUN {
                    bail!("Journal d'exécution saturé");
                }
                let inserted = connection.execute(
                    "INSERT INTO session_run_events
                        (project, session_id, request_id, sequence, kind, data)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6
                     FROM session_runs
                     WHERE project = ?1
                       AND session_id = ?2
                       AND request_id = ?3",
                    params![project, session_id, request_id, sequence, kind, encoded,],
                )?;
                if inserted != 1 {
                    bail!("Exécution introuvable pour le journal");
                }

                Ok(())
            })
            .await
    }

    pub(crate) async fn record_agent_turn(
        &self,
        session_id: String,
        request_id: String,
        agent_id: String,
        layer: i64,
        answer: String,
    ) -> Result<()> {
        Self::validate_id(&session_id)?;
        if agent_id.is_empty() || agent_id.len() > 128 || layer < 0 {
            bail!("Identité ou couche d'agent invalide");
        }
        if answer.len() > MAX_AGENT_ANSWER_BYTES {
            bail!("Contribution de l'agent trop volumineuse");
        }
        let project = self.project.clone();
        self.database
            .write(move |connection| {
                let sequence: i64 = connection.query_row(
                    "SELECT COALESCE(MAX(sequence), 0) + 1
                     FROM session_agent_turns
                     WHERE project = ?1
                       AND session_id = ?2
                       AND request_id = ?3",
                    params![project, session_id, request_id],
                    |row| row.get(0),
                )?;
                if sequence > MAX_AGENT_TURNS_PER_RUN {
                    bail!("Journal des agents saturé");
                }
                let inserted = connection.execute(
                    "INSERT INTO session_agent_turns
                        (
                            project,
                            session_id,
                            request_id,
                            sequence,
                            agent_id,
                            layer,
                            answer
                        )
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7
                     FROM session_runs
                     WHERE project = ?1
                       AND session_id = ?2
                       AND request_id = ?3",
                    params![
                        project, session_id, request_id, sequence, agent_id, layer, answer,
                    ],
                )?;
                if inserted != 1 {
                    bail!("Exécution introuvable pour la contribution d'agent");
                }
                Ok(())
            })
            .await
    }

    pub(crate) async fn events(
        &self,
        session_id: String,
        run_id: String,
        after_sequence: i64,
    ) -> Result<Option<Vec<SessionRunEvent>>> {
        Self::validate_id(&session_id)?;
        if run_id.is_empty() || run_id.len() > 128 || after_sequence < 0 {
            bail!("Paramètres de journal invalides");
        }
        let project = self.project.clone();
        self.database
            .read(move |connection| {
                let exists = connection
                    .query_row(
                        "SELECT 1
                         FROM session_runs
                         WHERE project = ?1
                           AND session_id = ?2
                           AND request_id = ?3",
                        params![project, session_id, run_id],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?
                    .is_some();
                if !exists {
                    return Ok(None);
                }
                let mut statement = connection.prepare(
                    "SELECT sequence, kind, data, created_at
                     FROM session_run_events
                     WHERE project = ?1
                       AND session_id = ?2
                       AND request_id = ?3
                       AND sequence > ?4
                     ORDER BY sequence ASC
                     LIMIT 100",
                )?;
                let rows = statement.query_map(
                    params![project, session_id, run_id, after_sequence],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )?;
                let mut events = Vec::new();
                for row in rows {
                    let (sequence, kind, encoded, created_at) = row?;

                    events.push(SessionRunEvent {
                        sequence,
                        kind,
                        data: serde_json::from_str(&encoded)?,
                        created_at,
                    });
                }
                Ok(Some(events))
            })
            .await
    }

    pub(crate) async fn agent_turns(
        &self,
        session_id: String,
        run_id: String,
        after_sequence: i64,
    ) -> Result<Option<Vec<SessionAgentTurn>>> {
        Self::validate_id(&session_id)?;
        if run_id.is_empty() || run_id.len() > 128 || after_sequence < 0 {
            bail!("Paramètres de contributions invalides");
        }
        let project = self.project.clone();
        self.database
            .read(move |connection| {
                let exists = connection
                    .query_row(
                        "SELECT 1
                         FROM session_runs
                         WHERE project = ?1
                           AND session_id = ?2
                           AND request_id = ?3",
                        params![project, session_id, run_id],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?
                    .is_some();
                if !exists {
                    return Ok(None);
                }
                let mut statement = connection.prepare(
                    "SELECT
                        sequence,
                        agent_id,
                        layer,
                        answer,
                        created_at
                     FROM session_agent_turns
                     WHERE project = ?1
                       AND session_id = ?2
                       AND request_id = ?3
                       AND sequence > ?4
                     ORDER BY sequence ASC
                     LIMIT 50",
                )?;
                let rows = statement.query_map(
                    params![project, session_id, run_id, after_sequence],
                    |row| {
                        Ok(SessionAgentTurn {
                            sequence: row.get(0)?,
                            agent_id: row.get(1)?,
                            layer: row.get(2)?,
                            answer: row.get(3)?,
                            created_at: row.get(4)?,
                        })
                    },
                )?;
                Ok(Some(rows.collect::<rusqlite::Result<Vec<_>>>()?))
            })
            .await
    }
}

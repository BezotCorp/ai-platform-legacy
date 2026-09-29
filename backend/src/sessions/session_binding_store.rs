use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::{
    agents::ExecutionMode,
    configurations::SavedConfiguration,
    conversation::{ConversationEntry, validate},
    sessions::{BoundSession, History, Session, SessionStore},
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
                                 AND status IN ('queued', 'running')
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
                            let entries: Vec<ConversationEntry> = serde_json::from_str(&encoded_messages)?;
                            validate(&entries)?;
                            let mode: ExecutionMode = serde_json::from_str(&encoded_mode)?;
                            Ok(BoundSession {
                                history: History { session, entries },
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
}

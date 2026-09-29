use anyhow::{Result, bail};
use rusqlite::params;

use crate::{
    conversation::{ConversationAuthor, ConversationEntry},
    sessions::{ArchivedMessage, SessionStore},
};

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

                let entries =
                    statement.query_map(params![project, session_id, before_sequence], |row| {
                        let stored_author: String = row.get(1)?;
                        let author = ConversationAuthor::from_storage(&stored_author)
                            .ok_or(rusqlite::Error::InvalidQuery)?;

                        Ok(ArchivedMessage {
                            sequence: row.get(0)?,
                            entry: ConversationEntry {
                                author,
                                content: row.get(2)?,
                            },
                            archived_at: row.get(3)?,
                        })
                    })?;

                Ok(entries.collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await
    }
}

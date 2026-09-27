use anyhow::Result;
use rusqlite::Connection;

pub(crate) fn apply(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_entries (
            id INTEGER PRIMARY KEY,
            project TEXT NOT NULL,
            query TEXT NOT NULL,
            content TEXT NOT NULL,
            source TEXT NOT NULL,
            checksum TEXT NOT NULL,
            revision INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
            UNIQUE(project, checksum)
        );

        CREATE INDEX IF NOT EXISTS memory_entries_project_updated
            ON memory_entries(project, updated_at DESC);",
    )?;
    Ok(())
}

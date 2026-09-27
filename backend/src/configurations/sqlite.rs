use anyhow::Result;
use rusqlite::Connection;

pub(crate) fn apply(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS configurations (
            project TEXT NOT NULL,
            id TEXT NOT NULL,
            revision INTEGER NOT NULL DEFAULT 1,
            mode TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY(project, id)
        );

        CREATE INDEX IF NOT EXISTS configurations_project_updated
            ON configurations(project, updated_at DESC);",
    )?;
    Ok(())
}

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

fn has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn apply(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS configurations (
            project TEXT NOT NULL,
            id TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            revision INTEGER NOT NULL DEFAULT 1,
            mode TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY(project, id)
        );

        CREATE INDEX IF NOT EXISTS configurations_project_updated
            ON configurations(project, updated_at DESC);",
    )?;

    if !has_column(connection, "configurations", "title")? {
        connection.execute_batch(
            "ALTER TABLE configurations ADD COLUMN title TEXT NOT NULL DEFAULT '';",
        )?;
    }
    if !has_column(connection, "configurations", "description")? {
        connection.execute_batch(
            "ALTER TABLE configurations ADD COLUMN description TEXT NOT NULL DEFAULT '';",
        )?;
    }
    connection.execute(
        "UPDATE configurations
         SET title = id
         WHERE title = ''",
        [],
    )?;
    let invalid: Option<i64> = connection
        .query_row(
            "SELECT 1
             FROM configurations
             WHERE title = '' OR length(title) > 96 OR length(description) > 512
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if invalid.is_some() {
        anyhow::bail!("Métadonnées de configuration invalides");
    }
    Ok(())
}

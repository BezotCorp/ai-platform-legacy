use anyhow::{Result, bail};
use rusqlite::Connection;

pub(crate) fn apply(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_schema_version (
            version INTEGER PRIMARY KEY
        );",
    )?;

    let version: Option<i64> = connection.query_row(
        "SELECT MAX(version) FROM session_schema_version",
        [],
        |row| row.get(0),
    )?;

    match version {
        None => {
            connection.execute_batch(
                "CREATE TABLE sessions (
                    project TEXT NOT NULL,
                    id TEXT NOT NULL,
                    revision INTEGER NOT NULL DEFAULT 1,
                    messages TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                        DEFAULT (unixepoch()),
                    updated_at INTEGER NOT NULL
                        DEFAULT (unixepoch()),
                    PRIMARY KEY(project, id)
                );

                CREATE INDEX sessions_project_updated
                    ON sessions(project, updated_at DESC);

                INSERT INTO session_schema_version(version)
                    VALUES (1);",
            )?;

            migrate_to_v2(connection)?;
        }
        Some(1) => migrate_to_v2(connection)?,
        Some(2) => {}
        Some(version) => {
            bail!("Version du schéma sessions non prise en charge : {version}");
        }
    }

    Ok(())
}

fn migrate_to_v2(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "ALTER TABLE sessions
            ADD COLUMN configuration_id TEXT;

         ALTER TABLE sessions
            ADD COLUMN configuration_revision INTEGER;

         ALTER TABLE sessions
            ADD COLUMN configuration_mode TEXT;

         CREATE TABLE session_runs (
            project TEXT NOT NULL,
            session_id TEXT NOT NULL,
            request_id TEXT NOT NULL,
            status TEXT NOT NULL,
            prompt TEXT NOT NULL,
            error TEXT,
            created_at INTEGER NOT NULL
                DEFAULT (unixepoch()),
            updated_at INTEGER NOT NULL
                DEFAULT (unixepoch()),
            PRIMARY KEY(project, session_id, request_id)
         );

         CREATE INDEX session_runs_by_session
            ON session_runs(
                project,
                session_id,
                created_at DESC
            );

         UPDATE session_schema_version
            SET version = 2
            WHERE version = 1;",
    )?;
    Ok(())
}

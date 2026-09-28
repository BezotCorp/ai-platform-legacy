use anyhow::Result;
use rusqlite::Connection;

/// Schéma de lancement : aucune migration historique n'est nécessaire.
pub(crate) fn apply(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
            project TEXT NOT NULL,
            id TEXT NOT NULL,
            revision INTEGER NOT NULL DEFAULT 1,
            messages TEXT NOT NULL,
            configuration_id TEXT,
            configuration_revision INTEGER,
            configuration_mode TEXT,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY(project, id)
        );

        CREATE INDEX IF NOT EXISTS sessions_project_updated
            ON sessions(project, updated_at DESC);

        CREATE TABLE IF NOT EXISTS session_runs (
            project TEXT NOT NULL,
            session_id TEXT NOT NULL,
            request_id TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN (
                'queued', 'running', 'completed', 'failed',
                'cancelled', 'interrupted'
            )),
            prompt TEXT NOT NULL,
            error TEXT,
            result TEXT,
            session_revision INTEGER,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY(project, session_id, request_id),
            FOREIGN KEY(project, session_id)
                REFERENCES sessions(project, id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS session_runs_by_session
            ON session_runs(project, session_id, created_at DESC);

        CREATE UNIQUE INDEX IF NOT EXISTS session_runs_one_active
            ON session_runs(project, session_id)
            WHERE status IN ('queued', 'running');

        CREATE TABLE IF NOT EXISTS session_run_events (
            project TEXT NOT NULL,
            session_id TEXT NOT NULL,
            request_id TEXT NOT NULL,
            sequence INTEGER NOT NULL,
            kind TEXT NOT NULL,
            data TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY(project, session_id, request_id, sequence),
            FOREIGN KEY(project, session_id, request_id)
                REFERENCES session_runs(project, session_id, request_id)
                ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS session_agent_turns (
            project TEXT NOT NULL,
            session_id TEXT NOT NULL,
            request_id TEXT NOT NULL,
            sequence INTEGER NOT NULL,
            agent_id TEXT NOT NULL,
            layer INTEGER NOT NULL,
            answer TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY(project, session_id, request_id, sequence),
            FOREIGN KEY(project, session_id, request_id)
                REFERENCES session_runs(project, session_id, request_id)
                ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS session_message_archive (
            project TEXT NOT NULL,
            session_id TEXT NOT NULL,
            sequence INTEGER NOT NULL,
            role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
            content TEXT NOT NULL,
            archived_at INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY(project, session_id, sequence),
            FOREIGN KEY(project, session_id)
                REFERENCES sessions(project, id) ON DELETE CASCADE
        );",
    )?;
    Ok(())
}

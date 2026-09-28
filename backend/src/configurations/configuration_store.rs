use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::{
    agents::ExecutionMode,
    configurations::{ConfigurationSummary, SavedConfiguration},
    sqlite::Database,
};

#[derive(Clone)]
pub(crate) struct ConfigurationStore {
    database: Database,
    project: String,
}

impl ConfigurationStore {
    pub(crate) async fn open(database: Database, project: String) -> Result<Self> {
        Ok(Self { database, project })
    }

    fn validate_id(id: &str) -> Result<()> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            bail!("Identifiant de configuration invalide");
        }
        Ok(())
    }

    fn normalize_title(id: &str, title: Option<String>) -> Result<String> {
        let title = title.unwrap_or_else(|| id.to_owned());
        let title = title.trim().to_owned();
        if title.is_empty() || title.len() > 96 {
            bail!("Titre de configuration invalide");
        }
        Ok(title)
    }

    fn normalize_description(description: Option<String>) -> Result<String> {
        let description = description.unwrap_or_default().trim().to_owned();
        if description.len() > 512 {
            bail!("Description de configuration trop volumineuse");
        }
        Ok(description)
    }

    pub(crate) async fn save(
        &self,
        id: String,
        expected_revision: i64,
        title: Option<String>,
        description: Option<String>,
        mode: ExecutionMode,
    ) -> Result<ConfigurationSummary> {
        Self::validate_id(&id)?;
        if expected_revision < 0 {
            bail!("Révision de configuration invalide");
        }
        let title = Self::normalize_title(&id, title)?;
        let description = Self::normalize_description(description)?;
        let mode = mode.validate()?;
        let summary = mode.summary();
        let encoded = serde_json::to_string(&mode)?;
        if encoded.len() > 64 * 1024 {
            bail!("Configuration trop volumineuse");
        }
        let project = self.project.clone();
        self.database
            .write(move |connection| {
                let changed = if expected_revision == 0 {
                    connection.execute(
                        "INSERT INTO configurations (
                            project,
                            id,
                            title,
                            description,
                            mode
                        )
                        VALUES (?1, ?2, ?3, ?4, ?5)
                        ON CONFLICT(project, id)
                        DO NOTHING",
                        params![project, id, title, description, encoded],
                    )?
                } else {
                    connection.execute(
                        "UPDATE configurations
                         SET title = ?3,
                             description = ?4,
                             mode = ?5,
                             revision = revision + 1,
                             updated_at = unixepoch()
                         WHERE project = ?1
                           AND id = ?2
                           AND revision = ?6",
                        params![project, id, title, description, encoded, expected_revision],
                    )?
                };
                if changed != 1 {
                    bail!("Conflit de révision : configuration existante ou modifiée");
                }
                let configuration = connection.query_row(
                    "SELECT
                        id,
                        title,
                        description,
                        revision,
                        created_at,
                        updated_at
                     FROM configurations
                     WHERE project = ?1
                       AND id = ?2",
                    params![project, id],
                    |row| ConfigurationSummary::from_row(row, summary.clone()),
                )?;
                Ok(configuration)
            })
            .await
    }

    pub(crate) async fn load(&self, id: String) -> Result<Option<SavedConfiguration>> {
        Self::validate_id(&id)?;
        let project = self.project.clone();
        self.database
            .read(move |connection| {
                let stored = connection
                    .query_row(
                        "SELECT
                            id,
                            title,
                            description,
                            revision,
                            created_at,
                            updated_at,
                            mode
                         FROM configurations
                         WHERE project = ?1
                           AND id = ?2",
                        params![project, id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, i64>(3)?,
                                row.get::<_, i64>(4)?,
                                row.get::<_, i64>(5)?,
                                row.get::<_, String>(6)?,
                            ))
                        },
                    )
                    .optional()?;
                stored
                    .map(
                        |(id, title, description, revision, created_at, updated_at, encoded)| {
                            let mode: ExecutionMode = serde_json::from_str(&encoded)?;
                            let mode = mode.validate()?;
                            let summary = mode.summary();
                            Ok(SavedConfiguration {
                                id,
                                title,
                                description,
                                revision,
                                created_at,
                                updated_at,
                                summary,
                                mode,
                            })
                        },
                    )
                    .transpose()
            })
            .await
    }

    pub(crate) async fn list(&self) -> Result<Vec<ConfigurationSummary>> {
        let project = self.project.clone();
        self.database
            .read(move |connection| {
                let mut statement = connection.prepare(
                    "SELECT
                        id,
                        title,
                        description,
                        revision,
                        created_at,
                        updated_at,
                        mode
                     FROM configurations
                     WHERE project = ?1
                     ORDER BY updated_at DESC, id
                     LIMIT 50",
                )?;
                let rows = statement.query_map(params![project], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                })?;
                let mut configurations = Vec::new();
                for row in rows {
                    let (id, title, description, revision, created_at, updated_at, encoded) = row?;
                    let mode: ExecutionMode = serde_json::from_str(&encoded)
                        .with_context(|| format!("Configuration {id} illisible"))?;
                    let mode = mode.validate()?;
                    configurations.push(ConfigurationSummary {
                        id,
                        title,
                        description,
                        revision,
                        created_at,
                        updated_at,
                        mode: mode.summary(),
                    });
                }
                Ok(configurations)
            })
            .await
    }

    pub(crate) async fn delete(&self, id: String, expected_revision: i64) -> Result<bool> {
        Self::validate_id(&id)?;
        if expected_revision < 1 {
            bail!("Révision de configuration invalide");
        }
        let project = self.project.clone();
        self.database
            .write(move |connection| {
                let changed = connection.execute(
                    "DELETE FROM configurations
                     WHERE project = ?1
                       AND id = ?2
                       AND revision = ?3",
                    params![project, id, expected_revision],
                )?;
                Ok(changed == 1)
            })
            .await
    }
}

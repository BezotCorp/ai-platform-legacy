use anyhow::{Result, bail};
use serde::Deserialize;

use crate::{
    agents::ExecutionMode,
    conversation::{ConversationAuthor, ConversationEntry, MAX_ENTRIES, validate},
    io::UserInput,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunRequest {
    #[serde(rename = "type")]
    pub(crate) command: String,
    pub(crate) request_id: String,
    #[serde(default)]
    pub(crate) history: Vec<ConversationEntry>,
    pub(crate) input: UserInput,
    pub(crate) mode: Option<ExecutionMode>,
    pub(crate) configuration_id: Option<String>,
    pub(crate) configuration_revision: Option<i64>,
    pub(crate) session_id: Option<String>,
    pub(crate) expected_revision: Option<i64>,
}

impl RunRequest {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.command != "run.start" || self.request_id.is_empty() || self.request_id.len() > 128 {
            bail!("Demande d'exécution invalide");
        }

        let sources = usize::from(self.mode.is_some())
            + usize::from(self.configuration_id.is_some())
            + usize::from(self.session_id.is_some());

        if sources != 1 {
            bail!("Choisir exactement une source de configuration");
        }

        validate(&self.history)?;

        if self.history.len() >= MAX_ENTRIES {
            bail!("Historique trop long pour accueillir une nouvelle entrée");
        }

        if self.input.text.trim().is_empty() {
            bail!("Entrée utilisateur vide");
        }

        let current = ConversationEntry::human(self.input.text.clone());
        validate(std::slice::from_ref(&current))?;

        if self
            .history
            .last()
            .is_some_and(|entry| entry.author == ConversationAuthor::Human)
        {
            bail!("L'historique précédent ne peut pas se terminer par une entrée humaine");
        }

        if self.session_id.is_some() {
            if !self.history.is_empty()
                || self.expected_revision.is_none()
                || self.configuration_revision.is_some()
            {
                bail!("Demande de session invalide");
            }
        } else if self.expected_revision.is_some() {
            bail!("Révision de session sans identifiant");
        }

        if self.configuration_id.is_none() && self.configuration_revision.is_some() {
            bail!("Révision sans configuration");
        }

        Ok(())
    }

    pub(crate) fn current_entry(&self) -> ConversationEntry {
        ConversationEntry::human(self.input.text.clone())
    }
}

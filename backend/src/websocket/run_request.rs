use anyhow::{Result, bail};
use serde::Deserialize;

use crate::{
    agents::ExecutionMode,
    sessions::{History, Message},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunRequest {
    #[serde(rename = "type")]
    pub command: String,

    pub request_id: String,

    #[serde(default)]
    pub messages: Vec<Message>,

    pub mode: Option<ExecutionMode>,

    pub configuration_id: Option<String>,

    pub configuration_revision: Option<i64>,

    pub session_id: Option<String>,

    pub expected_revision: Option<i64>,
}

impl RunRequest {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.command != "run.start" || self.request_id.is_empty() || self.request_id.len() > 128
        {
            bail!("Demande d'exécution invalide");
        }
        let sources = usize::from(self.mode.is_some())
            + usize::from(self.configuration_id.is_some())
            + usize::from(self.session_id.is_some());
        if sources != 1 {
            bail!("Choisir exactement une source de configuration");
        }
        if self.messages.is_empty() || self.messages.len() > 32 {
            bail!("Nombre de messages invalide");
        }
        History::validate(&self.messages)?;
        if self
            .messages
            .last()
            .is_none_or(|message| message.role != "user")
        {
            bail!("Le dernier message doit venir de l'utilisateur");
        }
        if self.session_id.is_some() {
            if self.messages.len() != 1
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
}

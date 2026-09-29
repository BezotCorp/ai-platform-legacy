use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum ConversationAuthor {
    #[serde(rename = "human", alias = "user")]
    Human,
    #[serde(rename = "ai", alias = "assistant")]
    Ai,
}

impl ConversationAuthor {
    pub(crate) fn storage_name(self) -> &'static str {
        match self {
            Self::Human => "user",
            Self::Ai => "assistant",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Option<Self> {
        match value {
            "user" | "human" => Some(Self::Human),
            "assistant" | "ai" => Some(Self::Ai),
            _ => None,
        }
    }
}

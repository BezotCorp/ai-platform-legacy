use anyhow::Result;
use serde_json::Value;

use crate::{agents::context::limits, tools};

pub(crate) struct AgentResources {
    pub context_tokens: usize,
    pub output_tokens: usize,
    pub tool_tokens: usize,
    pub definitions: Vec<Value>,
}

impl AgentResources {
    pub(crate) fn new() -> Result<Self> {
        let (context_tokens, output_tokens) = limits()?;
        let definitions = tools::definitions();
        let tool_tokens = serde_json::to_vec(&definitions)?.len().saturating_add(256);

        Ok(Self {
            context_tokens,
            output_tokens,
            tool_tokens,
            definitions,
        })
    }
}

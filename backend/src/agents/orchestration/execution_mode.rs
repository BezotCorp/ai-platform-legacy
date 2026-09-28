use std::collections::HashSet;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::agents::{
    AgentConfig, LayeredMoa, MultiAgent, MultiAgentStrategy, PopulationDefinition, Supervised,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ExecutionModeSummary {
    pub strategy: &'static str,
    pub agent_count: usize,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ExecutionMode {
    Single { agent: AgentConfig },
    MultiAgent(MultiAgent),
}

impl ExecutionMode {
    pub(crate) fn summary(&self) -> ExecutionModeSummary {
        let mut models = HashSet::new();
        let (strategy, agent_count) = match self {
            Self::Single { agent } => {
                models.insert(format!("{}:{}", agent.model.provider, agent.model.name));
                ("single", 1)
            }
            Self::MultiAgent(multi) => match &multi.strategy {
                MultiAgentStrategy::LayeredMoa(config) => {
                    let mut count = 0usize;
                    for layer in &config.layers {
                        for agent in &layer.agents {
                            count += 1;
                            models.insert(format!("{}:{}", agent.model.provider, agent.model.name));
                        }
                    }
                    count += 1;
                    models.insert(format!(
                        "{}:{}",
                        config.aggregation.agent.model.provider,
                        config.aggregation.agent.model.name,
                    ));
                    ("layered_moa", count)
                }
                MultiAgentStrategy::Supervised(config) => {
                    models.insert(format!(
                        "{}:{}",
                        config.supervisor.model.provider, config.supervisor.model.name,
                    ));
                    for agent in &config.workers {
                        models.insert(format!("{}:{}", agent.model.provider, agent.model.name));
                    }
                    ("supervised", config.workers.len() + 1)
                }
                MultiAgentStrategy::Population(config) => {
                    models.insert(format!(
                        "{}:{}",
                        config.facilitator.model.provider, config.facilitator.model.name,
                    ));
                    for agent in &config.agents {
                        models.insert(format!("{}:{}", agent.model.provider, agent.model.name));
                    }
                    ("population", config.agents.len() + 1)
                }
            },
        };
        let mut models = models.into_iter().collect::<Vec<_>>();
        models.sort();
        ExecutionModeSummary {
            strategy,
            agent_count,
            models,
        }
    }
    pub(crate) fn validate(self) -> Result<Self> {
        match self {
            Self::Single { .. } => Ok(self),
            Self::MultiAgent(multi) => {
                let strategy = match multi.strategy {
                    MultiAgentStrategy::LayeredMoa(config) => {
                        if config.layers.is_empty()
                            || config.layers.len() > 6
                            || config.layers.iter().any(|layer| layer.agents.len() > 8)
                        {
                            bail!("Invalid layered MoA configuration");
                        }
                        let config = LayeredMoa::new(config.layers, config.aggregation)
                            .map_err(anyhow::Error::msg)?;
                        MultiAgentStrategy::LayeredMoa(config)
                    }
                    MultiAgentStrategy::Supervised(config) => {
                        let config = Supervised::new(
                            config.supervisor,
                            config.workers,
                            config.max_delegations,
                        )
                        .map_err(anyhow::Error::msg)?;
                        MultiAgentStrategy::Supervised(config)
                    }
                    MultiAgentStrategy::Population(config) => {
                        let config = PopulationDefinition::new(
                            config.agents,
                            config.facilitator,
                            config.rounds,
                            config.participation,
                        )
                        .map_err(anyhow::Error::msg)?;
                        MultiAgentStrategy::Population(config)
                    }
                };
                Ok(Self::MultiAgent(MultiAgent::new(strategy)))
            }
        }
    }
}

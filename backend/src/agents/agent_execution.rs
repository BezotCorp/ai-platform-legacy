use anyhow::{Result, bail};
use serde_json::json;

use crate::{
    agents::{
        AgentResources, AgentServices, AgentTurn, ExecutionMode, MultiAgentStrategy,
        PopulationExecution, Scheduler, SupervisedExecution,
    },
    io::Event,
    sessions::Message,
};

pub(crate) struct AgentExecution;

impl AgentExecution {
    pub(crate) async fn run(
        mode: &ExecutionMode,
        history: &[Message],
        services: &AgentServices<'_>,
    ) -> Result<()> {
        let request_id = services.request_id;
        let outbound = services.outbound;
        let cancel = services.cancel;
        let memory = services.memory;
        if let ExecutionMode::MultiAgent(multi) = mode
            && let MultiAgentStrategy::Supervised(config) = &multi.strategy
        {
            return SupervisedExecution::run(config, history, services).await;
        }
        if let ExecutionMode::MultiAgent(multi) = mode
            && let MultiAgentStrategy::Population(config) = &multi.strategy
        {
            return PopulationExecution::run(config, history, services).await;
        }
        let resources = AgentResources::new()?;
        let mut previous_layer: Vec<(String, String)> = Vec::new();
        for (layer_index, agents) in Scheduler::plan(mode)?.iter().enumerate() {
            let mut current_layer = Vec::new();
            for agent in agents {
                let answer = AgentTurn::run(
                    agent,
                    layer_index,
                    history,
                    &previous_layer,
                    services,
                    &resources,
                )
                .await?;
                current_layer.push((agent.identity.id.clone(), answer));
            }
            previous_layer = current_layer;
        }
        if cancel.is_cancelled() {
            bail!("Exécution annulée");
        }
        if let (Some(store), Some((_, answer)), Some(prompt)) =
            (memory, previous_layer.last(), history.last())
            && let Err(error) = store.remember(&prompt.content, answer).await
        {
            outbound
                .send(Event::new(
                    "memory.failed",
                    request_id,
                    json!({ "error": error.to_string() }),
                ))
                .await?;
        }
        outbound
            .send(Event::new(
                "run.completed",
                request_id,
                json!({
                    "result": previous_layer
                        .last()
                        .map(|(_, answer)| answer),
                }),
            ))
            .await?;
        Ok(())
    }
}

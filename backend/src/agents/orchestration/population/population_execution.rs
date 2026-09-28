use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::{
    agents::{
        AgentResources, AgentServices, AgentTurn, ParticipationPolicy, PopulationDefinition,
        PopulationSelector, WorkerReport,
    },
    event::Event,
    sessions::Message,
};

pub(crate) struct PopulationExecution;

impl PopulationExecution {
    pub(crate) async fn run(
        config: &PopulationDefinition,
        history: &[Message],
        services: &AgentServices<'_>,
    ) -> Result<()> {
        let request_id = services.request_id;
        let outbound = services.outbound;
        let cancel = services.cancel;
        let memory = services.memory;
        let resources = AgentResources::new()?;
        let original_request = &history.last().context("Conversation vide")?.content;
        let mut individual_histories = HashMap::<String, Vec<Message>>::new();
        let mut reports = Vec::<WorkerReport>::new();
        let mut previous_round = Vec::<(String, String)>::new();
        for round in 0..config.rounds {
            if cancel.is_cancelled() {
                bail!("Exécution annulée");
            }
            let selected = match &config.participation {
                ParticipationPolicy::Fixed => config
                    .agents
                    .iter()
                    .map(|agent| agent.identity.id.clone())
                    .collect::<Vec<_>>(),
                ParticipationPolicy::Adaptive {
                    min_agents,
                    max_agents,
                } => {
                    PopulationSelector::select(
                        config,
                        history,
                        &reports,
                        *min_agents..=*max_agents,
                        services,
                        &resources,
                    )
                    .await?
                    .agent_ids
                }
            };
            outbound
                .send(Event::new(
                    "population.round.started",
                    request_id,
                    json!({
                        "round": round,
                        "selected_agents": selected,
                    }),
                ))
                .await?;
            let mut current_round = Vec::new();
            for agent in &config.agents {
                if !selected.contains(&agent.identity.id) {
                    continue;
                }
                if cancel.is_cancelled() {
                    bail!("Exécution annulée");
                }
                let agent_history = individual_histories
                    .entry(agent.identity.id.clone())
                    .or_insert_with(|| history[..history.len() - 1].to_vec());
                agent_history.push(Message {
                    role: "user".to_owned(),
                    content: format!(
                        "Demande originale : {}\n\n\
                         Tour collaboratif {}. \
                         Apporte ta propre contribution. \
                         Examine les contributions reçues, \
                         mais vérifie toute information utile \
                         avant de la considérer comme établie.",
                        original_request,
                        round + 1,
                    ),
                });
                let answer = AgentTurn::run(
                    agent,
                    round,
                    agent_history,
                    &previous_round,
                    services,
                    &resources,
                )
                .await?;
                // Chaque participant conserve son historique propre.
                // Les échanges collectifs sont bornés séparément.
                agent_history.push(Message {
                    role: "assistant".to_owned(),
                    content: answer.chars().take(2000).collect(),
                });
                let shared = answer.chars().take(240).collect::<String>();
                current_round.push((agent.identity.id.clone(), shared));
                reports.push(WorkerReport {
                    agent_id: agent.identity.id.clone(),
                    task: format!("Tour collaboratif {}", round + 1),
                    answer: answer.chars().take(900).collect(),
                });
            }
            if current_round.is_empty() {
                bail!("No participant selected for collaborative round");
            }
            previous_round = current_round;
            outbound
                .send(Event::new(
                    "population.round.completed",
                    request_id,
                    json!({
                        "round": round,
                        "participants": selected,
                    }),
                ))
                .await?;
        }
        if cancel.is_cancelled() {
            bail!("Exécution annulée");
        }
        // La synthèse est confiée à un agent distinct.
        let answer = AgentTurn::run(
            &config.facilitator,
            config.rounds,
            history,
            &previous_round,
            services,
            &resources,
        )
        .await?;
        if let Some(store) = memory
            && let Err(error) = store.remember(original_request, &answer).await
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
                json!({ "result": answer }),
            ))
            .await?;

        Ok(())
    }
}

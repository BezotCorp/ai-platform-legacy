use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::{
    agents::{
        AgentResources, AgentServices, Supervised, SupervisorDecision, WorkerReport, agent_turn::AgentTurn,
        orchestration::supervisor_turn::SupervisorTurn,
    },
    event::Event,
    sessions::Message,
};

pub(crate) struct SupervisedExecution;

impl SupervisedExecution {
    pub(crate) async fn run(
        config: &Supervised,
        history: &[Message],
        services: &AgentServices<'_>,
    ) -> Result<()> {
        let client = services.client;
        let request_id = services.request_id;
        let outbound = services.outbound;
        let cancel = services.cancel;
        let memory = services.memory;
        let resources = AgentResources::new()?;
        let context_tokens = resources.context_tokens;
        let output_tokens = resources.output_tokens;
        let mut worker_histories: HashMap<String, Vec<Message>> = HashMap::new();
        let mut reports = Vec::<WorkerReport>::new();

        for step in 0..=config.max_delegations {
            if cancel.is_cancelled() {
                bail!("Exécution annulée");
            }
            outbound
                .send(Event::new(
                    "orchestration.deciding",
                    request_id,
                    json!({
                        "supervisor_id": config.supervisor.identity.id,
                        "step": step,
                    }),
                ))
                .await?;
            let decision = SupervisorTurn::decide(
                config,
                client,
                history,
                &reports,
                context_tokens,
                output_tokens,
                cancel,
            )
            .await?;
            match decision {
                SupervisorDecision::Complete { answer } => {
                    if let (Some(store), Some(prompt)) = (memory, history.last())
                        && let Err(error) = store.remember(&prompt.content, &answer).await
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
                    return Ok(());
                }
                SupervisorDecision::Delegate { agent_id, task } => {
                    if step == config.max_delegations {
                        bail!("Limite des délégations atteinte sans conclusion");
                    }
                    let worker = config
                        .workers
                        .iter()
                        .find(|worker| worker.identity.id == agent_id)
                        .context("Le superviseur a sélectionné un agent inconnu")?;
                    outbound
                        .send(Event::new(
                            "orchestration.delegated",
                            request_id,
                            json!({
                                "supervisor_id": config.supervisor.identity.id,
                                "agent_id": agent_id,
                                "task": task,
                                "step": step,
                            }),
                        ))
                        .await?;
                    let agent_history = worker_histories
                        .entry(agent_id.clone())
                        .or_insert_with(|| history[..history.len() - 1].to_vec());
                    agent_history.push(Message {
                        role: "user".to_owned(),
                        content: format!(
                            "Demande originale : {}\n\nTa tâche actuelle : {}",
                            history.last().context("Conversation vide")?.content,
                            task,
                        ),
                    });
                    let answer = AgentTurn::run(
                        worker,
                        step,
                        agent_history,
                        &[],
                        services,
                        &resources,
                    )
                    .await?;
                    agent_history.push(Message {
                        role: "assistant".to_owned(),
                        content: answer.clone(),
                    });
                    reports.push(WorkerReport {
                        agent_id: agent_id.clone(),
                        task,
                        answer,
                    });
                    outbound
                        .send(Event::new(
                            "orchestration.reported",
                            request_id,
                            json!({
                                "supervisor_id": config.supervisor.identity.id,
                                "agent_id": agent_id,
                                "step": step,
                            }),
                        ))
                        .await?;
                }
            }
        }
        bail!("Limite des délégations atteinte")
    }
}

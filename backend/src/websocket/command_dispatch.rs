use anyhow::anyhow;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    event::Event,
    websocket::{ActiveRun, Command, ConnectionContext, models::list},
};

pub(crate) async fn dispatch(
    value: Value,
    context: &ConnectionContext,
    active: &Option<ActiveRun>,
    tx: &mpsc::Sender<Event>,
) -> bool {
    match serde_json::from_value::<Command>(value) {
        Ok(Command::ModelsList { request_id }) => {
            let event = list(&context.client, &request_id)
                .await
                .unwrap_or_else(|error| {
                    Event::new("error", &request_id, json!({ "error": error.to_string() }))
                });
            send(tx, event).await
        }
        Ok(Command::ConfigurationSave {
            request_id,
            configuration_id,
            expected_revision,
            title,
            description,
            mode,
        }) => {
            let result = match context.configurations.as_ref() {
                Some(store) => {
                    store
                        .save(
                            configuration_id,
                            expected_revision,
                            title,
                            description,
                            mode,
                        )
                        .await
                }
                None => Err(anyhow!("Persistance des configurations désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(configuration) => Event::new(
                        "configuration.saved",
                        &request_id,
                        json!({ "configuration": configuration }),
                    ),
                    Err(error) => configuration_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::ConfigurationLoad {
            request_id,
            configuration_id,
        }) => {
            let result = match context.configurations.as_ref() {
                Some(store) => store.load(configuration_id).await,
                None => Err(anyhow!("Persistance des configurations désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(Some(configuration)) => Event::new(
                        "configuration.loaded",
                        &request_id,
                        json!({ "configuration": configuration }),
                    ),
                    Ok(None) => {
                        configuration_failed_message(&request_id, "Configuration introuvable")
                    }
                    Err(error) => configuration_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::ConfigurationList { request_id }) => {
            let result = match context.configurations.as_ref() {
                Some(store) => store.list().await,
                None => Err(anyhow!("Persistance des configurations désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(configurations) => Event::new(
                        "configuration.list",
                        &request_id,
                        json!({ "configurations": configurations }),
                    ),
                    Err(error) => configuration_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::ConfigurationDelete {
            request_id,
            configuration_id,
            expected_revision,
        }) => {
            let result = match context.configurations.as_ref() {
                Some(store) => store.delete(configuration_id, expected_revision).await,
                None => Err(anyhow!("Persistance des configurations désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(true) => Event::new("configuration.deleted", &request_id, json!({})),
                    Ok(false) => configuration_failed_message(
                        &request_id,
                        "Configuration absente ou révision modifiée",
                    ),
                    Err(error) => configuration_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::SessionBind {
            request_id,
            session_id,
            expected_revision,
            configuration_id,
            configuration_revision,
        }) => {
            let result: anyhow::Result<_> = async {
                let configurations = context
                    .configurations
                    .as_ref()
                    .ok_or_else(|| anyhow!("Persistance des configurations désactivée"))?;
                let sessions = context
                    .sessions
                    .as_ref()
                    .ok_or_else(|| anyhow!("Persistance des sessions désactivée"))?;
                let configuration = configurations
                    .load(configuration_id)
                    .await?
                    .ok_or_else(|| anyhow!("Configuration introuvable"))?;
                if configuration_revision.is_some_and(|revision| revision != configuration.revision)
                {
                    anyhow::bail!("Révision de configuration obsolète");
                }
                sessions
                    .bind(session_id, expected_revision, configuration)
                    .await
            }
            .await;
            send(
                tx,
                match result {
                    Ok(session) => {
                        Event::new("session.bound", &request_id, json!({ "session": session }))
                    }
                    Err(error) => session_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::SessionResume {
            request_id,
            session_id,
        }) => {
            let result = match context.sessions.as_ref() {
                Some(store) => store.resume(session_id).await,
                None => Err(anyhow!("Persistance des sessions désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(Some(session)) => Event::new(
                        "session.resumed",
                        &request_id,
                        json!({ "session": session }),
                    ),
                    Ok(None) => session_failed_message(&request_id, "Session introuvable"),
                    Err(error) => session_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::SessionArchive {
            request_id,
            session_id,
            before_sequence,
        }) => {
            let result = match context.sessions.as_ref() {
                Some(store) => store.archive(session_id, before_sequence).await,
                None => Err(anyhow!("Persistance des sessions désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(messages) => Event::new(
                        "session.archive",
                        &request_id,
                        json!({ "messages": messages }),
                    ),
                    Err(error) => session_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::SessionRunLoad {
            request_id,
            session_id,
            run_id,
        }) => {
            let result = match context.sessions.as_ref() {
                Some(store) => store.load_run(session_id, run_id).await,
                None => Err(anyhow!("Persistance des sessions désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(Some(run)) => {
                        Event::new("session.run.loaded", &request_id, json!({ "run": run }))
                    }
                    Ok(None) => session_failed_message(&request_id, "Exécution introuvable"),
                    Err(error) => session_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::SessionRunEvents {
            request_id,
            session_id,
            run_id,
            after_sequence,
        }) => {
            let result = match context.sessions.as_ref() {
                Some(store) => {
                    store
                        .events(session_id, run_id, after_sequence.unwrap_or(0))
                        .await
                }
                None => Err(anyhow!("Persistance des sessions désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(Some(events)) => Event::new(
                        "session.run.events",
                        &request_id,
                        json!({ "events": events }),
                    ),
                    Ok(None) => session_failed_message(&request_id, "Exécution introuvable"),
                    Err(error) => session_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::SessionRunAgents {
            request_id,
            session_id,
            run_id,
            after_sequence,
        }) => {
            let result = match context.sessions.as_ref() {
                Some(store) => {
                    store
                        .agent_turns(session_id, run_id, after_sequence.unwrap_or(0))
                        .await
                }
                None => Err(anyhow!("Persistance des sessions désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(Some(turns)) => {
                        Event::new("session.run.agents", &request_id, json!({ "turns": turns }))
                    }
                    Ok(None) => session_failed_message(&request_id, "Exécution introuvable"),
                    Err(error) => session_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::SessionRuns {
            request_id,
            session_id,
            before_created_at,
            before_request_id,
        }) => {
            let cursor = match (before_created_at, before_request_id) {
                (None, None) => Ok(None),
                (Some(created_at), Some(run_id)) => Ok(Some((created_at, run_id))),
                _ => Err(anyhow!("Curseur d'exécution incomplet")),
            };
            let result = match (context.sessions.as_ref(), cursor) {
                (Some(store), Ok(before)) => store.runs(session_id, before).await,
                (None, _) => Err(anyhow!("Persistance des sessions désactivée")),
                (_, Err(error)) => Err(error),
            };
            send(
                tx,
                match result {
                    Ok(runs) => Event::new("session.runs", &request_id, json!({ "runs": runs })),
                    Err(error) => session_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::SessionSave {
            request_id,
            session_id,
            expected_revision,
            messages,
        }) => {
            let result = match context.sessions.as_ref() {
                Some(store) => store.save(session_id, expected_revision, messages).await,
                None => Err(anyhow!("Persistance des sessions désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(session) => {
                        Event::new("session.saved", &request_id, json!({ "session": session }))
                    }
                    Err(error) => session_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::SessionLoad {
            request_id,
            session_id,
        }) => {
            let result = match context.sessions.as_ref() {
                Some(store) => store.load(session_id).await,
                None => Err(anyhow!("Persistance des sessions désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(Some(history)) => {
                        Event::new("session.loaded", &request_id, json!({ "history": history }))
                    }
                    Ok(None) => session_failed_message(&request_id, "Session introuvable"),
                    Err(error) => session_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::SessionList { request_id }) => {
            let result = match context.sessions.as_ref() {
                Some(store) => store.list().await,
                None => Err(anyhow!("Persistance des sessions désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(items) => {
                        Event::new("session.list", &request_id, json!({ "sessions": items }))
                    }
                    Err(error) => session_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::SessionDelete {
            request_id,
            session_id,
            expected_revision,
        }) => {
            let result = match context.sessions.as_ref() {
                Some(store) => store.delete(session_id, expected_revision).await,
                None => Err(anyhow!("Persistance des sessions désactivée")),
            };
            send(
                tx,
                match result {
                    Ok(true) => Event::new("session.deleted", &request_id, json!({})),
                    Ok(false) => {
                        session_failed_message(&request_id, "Session absente ou révision modifiée")
                    }
                    Err(error) => session_failed(&request_id, error),
                },
            )
            .await
        }
        Ok(Command::RunCancel { request_id }) => {
            match active {
                Some(run) if run.matches_running(&request_id) => run.cancel_by_user(),
                _ => {
                    let _ = tx
                        .send(Event::new(
                            "error",
                            &request_id,
                            json!({ "error": "Aucune exécution active correspondante" }),
                        ))
                        .await;
                }
            }
            true
        }
        Ok(Command::ApprovalResolve {
            request_id,
            call_id,
            approved,
            preview_sha256,
        }) => {
            let belongs_to_run = active
                .as_ref()
                .is_some_and(|run| run.matches_running(&request_id));
            let resolved = if belongs_to_run {
                context
                    .approvals
                    .resolve(&request_id, &call_id, approved, preview_sha256.as_deref())
                    .await
            } else {
                false
            };
            let _ = tx
                .send(Event::new(
                    "approval.resolved",
                    &request_id,
                    json!({
                        "call_id": call_id,
                        "accepted": resolved,
                        "approved": if resolved { Some(approved) } else { None },
                    }),
                ))
                .await;
            true
        }
        _ => {
            let _ = tx
                .send(Event::new(
                    "error",
                    "",
                    json!({ "error": "Commande inconnue" }),
                ))
                .await;
            true
        }
    }
}

async fn send(tx: &mpsc::Sender<Event>, event: Event) -> bool {
    tx.send(event).await.is_ok()
}

fn configuration_failed(request_id: &str, error: anyhow::Error) -> Event {
    configuration_failed_message(request_id, &error.to_string())
}

fn configuration_failed_message(request_id: &str, error: &str) -> Event {
    Event::new(
        "configuration.failed",
        request_id,
        json!({ "error": error }),
    )
}

fn session_failed(request_id: &str, error: anyhow::Error) -> Event {
    session_failed_message(request_id, &error.to_string())
}

fn session_failed_message(request_id: &str, error: &str) -> Event {
    Event::new("session.failed", request_id, json!({ "error": error }))
}

use std::sync::Arc;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::{
    agents::tool_call_context::ToolCallContext,
    io::Event,
    tools::{self, ApprovalRequest, ToolApprovalGate, WriteProposal},
};

pub(crate) struct ToolInvocation;

impl ToolInvocation {
    pub(crate) async fn execute(
        name: &str,
        arguments: &Value,
        context: &ToolCallContext<'_>,
    ) -> Result<Value> {
        let result: Result<Value> = async {
            if WriteProposal::is_write(name) {
                let proposal = tokio::select! {
                    () = context.cancel.cancelled() => bail!("Exécution annulée"),
                    prepared = WriteProposal::prepare(context.project_root, name, arguments) => prepared?,
                };
                let preview = proposal.preview();
                let preview_sha256 = ToolApprovalGate::preview_sha256(&preview)?;
                context
                    .outbound
                    .send(Event::new(
                        "tool.preview",
                        context.request_id,
                        json!({
                            "agent_id": context.agent_id,
                            "call_id": context.call_id,
                            "preview": preview,
                            "preview_sha256": preview_sha256,
                        }),
                    ))
                    .await?;
                context
                    .approvals
                    .request(ApprovalRequest {
                        request_id: context.request_id,
                        agent_id: context.agent_id,
                        call_id: context.call_id,
                        tool: name,
                        arguments,
                        preview_sha256: Some(&preview_sha256),
                        outbound: context.outbound,
                        cancel: context.cancel,
                    })
                    .await?;
                let guard = tokio::select! {
                    () = context.cancel.cancelled() => bail!("Exécution annulée"),
                    guard = Arc::clone(context.writes).lock_owned() => guard,
                };
                if context.cancel.is_cancelled() {
                    bail!("Exécution annulée");
                }
                proposal.commit(guard).await
            } else {
                if context.approve_reads {
                    context
                        .approvals
                        .request(ApprovalRequest {
                            request_id: context.request_id,
                            agent_id: context.agent_id,
                            call_id: context.call_id,
                            tool: name,
                            arguments,
                            preview_sha256: None,
                            outbound: context.outbound,
                            cancel: context.cancel,
                        })
                        .await?;
                }
                tokio::select! {
                    () = context.cancel.cancelled() => bail!("Exécution annulée"),
                    result = tools::execute(context.project_root, name, arguments) => result,
                }
            }
        }
        .await;
        let payload = match result {
            Ok(value) => {
                context
                    .outbound
                    .send(Event::new(
                        "tool.completed",
                        context.request_id,
                        json!({
                            "agent_id": context.agent_id,
                            "call_id": context.call_id,
                            "tool": name,
                            "result": value,
                        }),
                    ))
                    .await?;
                json!({ "ok": true, "result": value })
            }
            Err(error) => {
                context
                    .outbound
                    .send(Event::new(
                        "tool.failed",
                        context.request_id,
                        json!({
                            "agent_id": context.agent_id,
                            "call_id": context.call_id,
                            "tool": name,
                            "error": error.to_string(),
                        }),
                    ))
                    .await?;
                json!({ "ok": false, "error": error.to_string() })
            }
        };
        Ok(payload)
    }
}

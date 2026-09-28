use serde::Deserialize;

use crate::{agents::ExecutionMode, sessions::Message};

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum Command {
    #[serde(rename = "authenticate")]
    Authenticate { token: String },
    #[serde(rename = "models.list")]
    ModelsList { request_id: String },
    #[serde(rename = "configuration.save")]
    ConfigurationSave {
        request_id: String,
        configuration_id: String,
        expected_revision: i64,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        description: Option<String>,
        mode: ExecutionMode,
    },
    #[serde(rename = "configuration.load")]
    ConfigurationLoad {
        request_id: String,
        configuration_id: String,
    },
    #[serde(rename = "configuration.list")]
    ConfigurationList { request_id: String },
    #[serde(rename = "configuration.delete")]
    ConfigurationDelete {
        request_id: String,
        configuration_id: String,
        expected_revision: i64,
    },
    #[serde(rename = "session.bind")]
    SessionBind {
        request_id: String,
        session_id: String,
        expected_revision: i64,
        configuration_id: String,
        configuration_revision: Option<i64>,
    },
    #[serde(rename = "session.resume")]
    SessionResume {
        request_id: String,
        session_id: String,
    },
    #[serde(rename = "session.archive")]
    SessionArchive {
        request_id: String,
        session_id: String,
        before_sequence: Option<i64>,
    },
    #[serde(rename = "session.run.load")]
    SessionRunLoad {
        request_id: String,
        session_id: String,
        run_id: String,
    },
    #[serde(rename = "session.run.events")]
    SessionRunEvents {
        request_id: String,
        session_id: String,
        run_id: String,
        after_sequence: Option<i64>,
    },
    #[serde(rename = "session.run.agents")]
    SessionRunAgents {
        request_id: String,
        session_id: String,
        run_id: String,
        after_sequence: Option<i64>,
    },
    #[serde(rename = "session.runs")]
    SessionRuns {
        request_id: String,
        session_id: String,
        before_created_at: Option<i64>,
        before_request_id: Option<String>,
    },
    #[serde(rename = "session.save")]
    SessionSave {
        request_id: String,
        session_id: String,
        expected_revision: i64,
        messages: Vec<Message>,
    },
    #[serde(rename = "session.load")]
    SessionLoad {
        request_id: String,
        session_id: String,
    },
    #[serde(rename = "session.list")]
    SessionList { request_id: String },
    #[serde(rename = "session.delete")]
    SessionDelete {
        request_id: String,
        session_id: String,
        expected_revision: i64,
    },
    #[serde(rename = "run.cancel")]
    RunCancel { request_id: String },
    #[serde(rename = "approval.resolve")]
    ApprovalResolve {
        request_id: String,
        call_id: String,
        approved: bool,
        #[serde(default)]
        preview_sha256: Option<String>,
    },
}

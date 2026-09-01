use super::super::{BridgeProgress, BridgeRequest, BridgeResponse};
use serde::{Deserialize, Serialize};

/// Private wire protocol between a CLI/MCP process and the running desktop
/// application. The public MCP protocol remains on stdio; this protocol is
/// intentionally scoped to one authenticated local bridge session.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum BridgeFrame {
    Hello {
        protocol_version: u32,
        token: String,
        client_id: String,
    },
    HelloAck {
        protocol_version: u32,
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Request {
        request_id: String,
        request: BridgeRequest,
    },
    Progress {
        request_id: String,
        progress: BridgeProgress,
    },
    Response {
        request_id: String,
        response: BridgeResponse,
    },
    Cancel {
        request_id: String,
    },
    Ping {
        nonce: String,
    },
    Pong {
        nonce: String,
    },
    Close,
}

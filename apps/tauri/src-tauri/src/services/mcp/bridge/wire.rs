use super::super::{BridgeProgress, BridgeRequest, BridgeResponse};
use serde::{Deserialize, Serialize};

/// Private wire protocol between a CLI/MCP process and the running desktop
/// application. The public MCP protocol remains on stdio; this protocol is
/// intentionally scoped to one authenticated local bridge session.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum BridgeFrame {
    Hello {
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        token: String,
        #[serde(rename = "clientId")]
        client_id: String,
    },
    HelloAck {
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Request {
        #[serde(rename = "requestId")]
        request_id: String,
        request: BridgeRequest,
    },
    Progress {
        #[serde(rename = "requestId")]
        request_id: String,
        progress: BridgeProgress,
    },
    Response {
        #[serde(rename = "requestId")]
        request_id: String,
        response: BridgeResponse,
    },
    Cancel {
        #[serde(rename = "requestId")]
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

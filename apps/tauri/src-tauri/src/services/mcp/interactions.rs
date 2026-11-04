// Keep UI interaction facts in the final response: MCP hosts may hide progress.
fn record_interaction(history: &mut Vec<Value>, progress: &BridgeProgress) {
    if !progress.is_interaction() {
        return;
    }
    let event = json!({
        "event": progress.event,
        "status": progress.status,
        "code": progress.code,
    });
    history.push(event);
}

fn attach_interaction_history(mut value: Value, progress: Vec<BridgeProgress>) -> Value {
    let mut history = Vec::new();
    for event in progress {
        record_interaction(&mut history, &event);
    }
    if !history.is_empty() {
        value["interactionHistory"] = json!(history);
    }
    value
}

fn attach_tool_interaction_history(mut result: Value, history: Vec<BridgeProgress>) -> Value {
    if history.is_empty() {
        return result;
    }
    let is_error = result["isError"].as_bool().unwrap_or(false);
    let value = result["structuredContent"].take();
    tool_result(attach_interaction_history(value, history), is_error)
}

fn finalize_bridge_interactions(
    mut response: BridgeResponse,
    mut history: Vec<BridgeProgress>,
    receiver: &mut mpsc::Receiver<BridgeProgress>,
) -> BridgeResponse {
    // Dispatch may finish in the same poll as its final event. Keep the queued
    // outcome even if live notifications were dropped by the writer or host.
    while let Ok(progress) = receiver.try_recv() {
        if progress.is_interaction() {
            history.push(progress);
        }
    }
    response.interaction_history = history;
    response
}

fn with_interaction_output_schema(mut schema: Value) -> Value {
    schema["properties"]["interactionHistory"] = json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "event": { "type": "string" },
                "status": { "type": "string" },
                "code": { "type": "string" }
            },
            "required": ["event", "status", "code"],
            "additionalProperties": false
        }
    });
    schema
}

#[cfg(test)]
#[path = "tests/interaction_tests.rs"]
mod interaction_tests;
#[cfg(test)]
#[path = "tests/interaction_delivery_tests.rs"]
mod interaction_delivery_tests;

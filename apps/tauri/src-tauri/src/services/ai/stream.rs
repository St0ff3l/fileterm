// SSE decoding, provider payload parsing, and stream consumption.
fn text_from_content(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    let parts = value.as_array()?;
    let text = parts
        .iter()
        .filter_map(|part| {
            part.get("text")
                .and_then(Value::as_str)
                .or_else(|| part.get("content").and_then(Value::as_str))
        })
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn apply_openai_usage(payload: &Value, stream: &mut ChatStreamResult) {
    let Some(usage) = payload.get("usage") else {
        return;
    };
    stream.input_tokens = usage
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .or(stream.input_tokens);
    stream.output_tokens = usage
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .or(stream.output_tokens);
}

fn apply_responses_usage(payload: &Value, stream: &mut ChatStreamResult) {
    let usage = payload
        .get("response")
        .and_then(|response| response.get("usage"))
        .or_else(|| payload.get("usage"));
    let Some(usage) = usage else {
        return;
    };
    stream.input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .or(stream.input_tokens);
    stream.output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .or(stream.output_tokens);
}

fn apply_anthropic_usage(payload: &Value, stream: &mut ChatStreamResult) {
    let usage = payload
        .get("message")
        .and_then(|message| message.get("usage"))
        .or_else(|| payload.get("usage"));
    let Some(usage) = usage else {
        return;
    };
    stream.input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .or(stream.input_tokens);
    stream.output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .or(stream.output_tokens);
}

fn append_stream_text(
    stream: &mut ChatStreamResult,
    text: String,
    channel: &Channel<AiStreamEvent>,
) -> Result<(), AppError> {
    if text.is_empty() {
        return Ok(());
    }
    if stream
        .content
        .chars()
        .count()
        .saturating_add(text.chars().count())
        > MAX_ASSISTANT_MESSAGE_LENGTH
    {
        return Err(ai_error(
            "AI_CONVERSATION_LIMIT",
            "AI 回答超过本地对话长度限制",
        ));
    }
    stream.content.push_str(&text);
    emit_stream_event(channel, AiStreamEvent::TextDelta { text })
}

fn append_tool_call_fragment(
    stream: &mut ChatStreamResult,
    key: impl Into<String>,
    id: Option<&str>,
    name: Option<&str>,
    arguments: Option<&str>,
) {
    let entry = stream.tool_call_accumulators.entry(key.into()).or_default();
    if entry.id.is_empty() {
        if let Some(id) = id.filter(|value| !value.is_empty()) {
            entry.id = id.to_string();
        }
    }
    if let Some(name) = name.filter(|value| !value.is_empty()) {
        entry.name.push_str(name);
    }
    if let Some(arguments) = arguments {
        entry.arguments.push_str(arguments);
    }
}

fn append_complete_tool_call(
    stream: &mut ChatStreamResult,
    key: impl Into<String>,
    id: Option<&str>,
    name: Option<&str>,
    arguments: Option<&str>,
) {
    let key = key.into();
    append_tool_call_fragment(stream, key.clone(), id, None, None);
    if let Some(entry) = stream.tool_call_accumulators.get_mut(&key) {
        if let Some(name) = name.filter(|value| !value.is_empty()) {
            entry.name = name.to_string();
        }
        if let Some(arguments) = arguments {
            entry.arguments = arguments.to_string();
        }
    }
}

fn set_tool_call_item_id(stream: &mut ChatStreamResult, key: &str, item_id: Option<&str>) {
    if let Some(item_id) = item_id.filter(|value| !value.is_empty()) {
        stream
            .tool_call_accumulators
            .entry(key.to_string())
            .or_default()
            .item_id = item_id.to_string();
    }
}

fn process_openai_payload(
    payload: Value,
    stream: &mut ChatStreamResult,
    channel: &Channel<AiStreamEvent>,
) -> Result<(), AppError> {
    if payload.get("error").is_some_and(|error| !error.is_null()) {
        return Err(ai_error(
            "AI_PROVIDER_HTTP_ERROR",
            "AI Provider 返回了对话错误",
        ));
    }
    apply_openai_usage(&payload, stream);
    let Some(choice) = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
    else {
        return Ok(());
    };
    if let Some(finish_reason) = choice.get("finish_reason").and_then(Value::as_str) {
        stream.finish_reason = Some(finish_reason.to_string());
    }
    if let Some(tool_calls) = choice
        .get("delta")
        .and_then(|delta| delta.get("tool_calls"))
        .and_then(Value::as_array)
    {
        for call in tool_calls {
            let index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
            let function = call.get("function");
            append_tool_call_fragment(
                stream,
                format!("openai:{index}"),
                call.get("id").and_then(Value::as_str),
                function
                    .and_then(|value| value.get("name"))
                    .and_then(Value::as_str),
                function
                    .and_then(|value| value.get("arguments"))
                    .and_then(Value::as_str),
            );
        }
    }
    if let Some(tool_calls) = choice
        .get("message")
        .and_then(|message| message.get("tool_calls"))
        .and_then(Value::as_array)
    {
        for (index, call) in tool_calls.iter().enumerate() {
            let function = call.get("function");
            append_complete_tool_call(
                stream,
                format!("openai:{index}"),
                call.get("id").and_then(Value::as_str),
                function
                    .and_then(|value| value.get("name"))
                    .and_then(Value::as_str),
                function
                    .and_then(|value| value.get("arguments"))
                    .and_then(Value::as_str),
            );
        }
    }
    let text = choice
        .get("delta")
        .and_then(|delta| delta.get("content"))
        .and_then(text_from_content)
        .or_else(|| {
            choice
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(text_from_content)
        });
    if let Some(text) = text {
        append_stream_text(stream, text, channel)?;
    }
    Ok(())
}

fn response_output_text(response: &Value) -> Option<String> {
    let text = response
        .get("output")
        .and_then(Value::as_array)?
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"))
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("output_text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn anthropic_content_text(message: &Value) -> Option<String> {
    let text = message
        .get("content")
        .and_then(Value::as_array)?
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn process_openai_responses_payload(
    payload: Value,
    stream: &mut ChatStreamResult,
    channel: &Channel<AiStreamEvent>,
) -> Result<(), AppError> {
    if payload.get("type").and_then(Value::as_str) == Some("error")
        || payload.get("error").is_some_and(|error| !error.is_null())
    {
        return Err(ai_error(
            "AI_PROVIDER_HTTP_ERROR",
            "OpenAI Responses Provider 返回了对话错误",
        ));
    }
    apply_responses_usage(&payload, stream);
    let event_type = payload.get("type").and_then(Value::as_str);
    match event_type {
        Some("response.output_text.delta") => {
            if let Some(text) = payload.get("delta").and_then(Value::as_str) {
                append_stream_text(stream, text.to_string(), channel)?;
            }
        }
        Some("response.output_item.added") | Some("response.output_item.done") => {
            let item = payload.get("item").unwrap_or(&payload);
            if item.get("type").and_then(Value::as_str) == Some("function_call") {
                let call_id = item
                    .get("call_id")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("id").and_then(Value::as_str));
                let key_id = item.get("id").and_then(Value::as_str).or(call_id);
                let key = format!("responses:{}", key_id.unwrap_or("unknown"));
                append_complete_tool_call(
                    stream,
                    key.clone(),
                    call_id,
                    item.get("name").and_then(Value::as_str),
                    item.get("arguments").and_then(Value::as_str),
                );
                set_tool_call_item_id(stream, &key, item.get("id").and_then(Value::as_str));
            }
        }
        Some("response.function_call_arguments.delta") => {
            let id = payload
                .get("item_id")
                .and_then(Value::as_str)
                .or_else(|| payload.get("call_id").and_then(Value::as_str));
            let key = format!("responses:{}", id.unwrap_or("unknown"));
            append_tool_call_fragment(
                stream,
                key.clone(),
                payload
                    .get("call_id")
                    .and_then(Value::as_str)
                    .or_else(|| payload.get("item_id").and_then(Value::as_str)),
                None,
                payload.get("delta").and_then(Value::as_str),
            );
            set_tool_call_item_id(stream, &key, payload.get("item_id").and_then(Value::as_str));
        }
        Some("response.function_call_arguments.done") => {
            let id = payload
                .get("item_id")
                .and_then(Value::as_str)
                .or_else(|| payload.get("call_id").and_then(Value::as_str));
            let key = format!("responses:{}", id.unwrap_or("unknown"));
            append_complete_tool_call(
                stream,
                key.clone(),
                payload
                    .get("call_id")
                    .and_then(Value::as_str)
                    .or_else(|| payload.get("item_id").and_then(Value::as_str)),
                payload.get("name").and_then(Value::as_str),
                payload.get("arguments").and_then(Value::as_str),
            );
            set_tool_call_item_id(stream, &key, payload.get("item_id").and_then(Value::as_str));
        }
        Some("response.completed") => {
            let response = payload.get("response").unwrap_or(&payload);
            apply_responses_usage(response, stream);
            stream.finish_reason = response
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| stream.finish_reason.clone());
            if stream.content.is_empty() {
                if let Some(text) = response_output_text(response) {
                    append_stream_text(stream, text, channel)?;
                }
            }
            if let Some(output) = response.get("output").and_then(Value::as_array) {
                for item in output {
                    if item.get("type").and_then(Value::as_str) != Some("function_call") {
                        continue;
                    }
                    let call_id = item
                        .get("call_id")
                        .and_then(Value::as_str)
                        .or_else(|| item.get("id").and_then(Value::as_str));
                    let key_id = item.get("id").and_then(Value::as_str).or(call_id);
                    let key = format!("responses:{}", key_id.unwrap_or("unknown"));
                    append_complete_tool_call(
                        stream,
                        key.clone(),
                        call_id,
                        item.get("name").and_then(Value::as_str),
                        item.get("arguments").and_then(Value::as_str),
                    );
                    set_tool_call_item_id(stream, &key, item.get("id").and_then(Value::as_str));
                }
            }
        }
        Some("response.failed") | Some("response.incomplete") => {
            return Err(ai_error(
                "AI_PROVIDER_RESPONSE_INVALID",
                "OpenAI Responses Provider 未完成回答",
            ));
        }
        _ => {
            // Responses adds typed events over time. This L0 chat only needs
            // user-visible output text and must ignore unrelated events.
            if event_type.is_none() && stream.content.is_empty() {
                if let Some(text) = response_output_text(&payload) {
                    append_stream_text(stream, text, channel)?;
                }
                if let Some(status) = payload.get("status").and_then(Value::as_str) {
                    stream.finish_reason = Some(status.to_string());
                }
            }
        }
    }
    Ok(())
}

fn process_anthropic_payload(
    payload: Value,
    stream: &mut ChatStreamResult,
    channel: &Channel<AiStreamEvent>,
) -> Result<(), AppError> {
    if payload.get("type").and_then(Value::as_str) == Some("error")
        || payload.get("error").is_some_and(|error| !error.is_null())
    {
        return Err(ai_error(
            "AI_PROVIDER_HTTP_ERROR",
            "Anthropic Messages Provider 返回了对话错误",
        ));
    }
    apply_anthropic_usage(&payload, stream);
    match payload.get("type").and_then(Value::as_str) {
        Some("content_block_start") => {
            let index = payload.get("index").and_then(Value::as_u64).unwrap_or(0);
            let block = payload.get("content_block").unwrap_or(&payload);
            if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                let input = block
                    .get("input")
                    .filter(|value| {
                        !value.is_object()
                            || !value.as_object().is_some_and(|object| object.is_empty())
                    })
                    .map(Value::to_string);
                append_complete_tool_call(
                    stream,
                    format!("anthropic:{index}"),
                    block.get("id").and_then(Value::as_str),
                    block.get("name").and_then(Value::as_str),
                    input.as_deref(),
                );
            }
        }
        Some("content_block_delta") => {
            let delta = payload.get("delta");
            if delta
                .and_then(|value| value.get("type"))
                .and_then(Value::as_str)
                == Some("text_delta")
            {
                if let Some(text) = delta
                    .and_then(|value| value.get("text"))
                    .and_then(Value::as_str)
                {
                    append_stream_text(stream, text.to_string(), channel)?;
                }
            }
            if delta
                .and_then(|value| value.get("type"))
                .and_then(Value::as_str)
                == Some("input_json_delta")
            {
                let index = payload.get("index").and_then(Value::as_u64).unwrap_or(0);
                append_tool_call_fragment(
                    stream,
                    format!("anthropic:{index}"),
                    None,
                    None,
                    delta
                        .and_then(|value| value.get("partial_json"))
                        .and_then(Value::as_str),
                );
            }
        }
        Some("message_delta") => {
            if let Some(stop_reason) = payload
                .get("delta")
                .and_then(|delta| delta.get("stop_reason"))
                .and_then(Value::as_str)
            {
                stream.finish_reason = Some(stop_reason.to_string());
            }
        }
        Some("message_stop") => {
            if stream.finish_reason.is_none() {
                stream.finish_reason = Some("message_stop".to_string());
            }
        }
        _ => {
            // A few compatible gateways return the final non-streaming
            // Messages object even when streaming was requested.
            if stream.content.is_empty() {
                if let Some(text) = anthropic_content_text(&payload) {
                    append_stream_text(stream, text, channel)?;
                }
                if let Some(stop_reason) = payload.get("stop_reason").and_then(Value::as_str) {
                    stream.finish_reason = Some(stop_reason.to_string());
                }
            }
            if let Some(content) = payload.get("content").and_then(Value::as_array) {
                for (index, block) in content.iter().enumerate() {
                    if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                        continue;
                    }
                    let input = block
                        .get("input")
                        .filter(|value| {
                            !value.is_object()
                                || !value.as_object().is_some_and(|object| object.is_empty())
                        })
                        .map(Value::to_string);
                    append_complete_tool_call(
                        stream,
                        format!("anthropic:{index}"),
                        block.get("id").and_then(Value::as_str),
                        block.get("name").and_then(Value::as_str),
                        input.as_deref(),
                    );
                }
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct SseDecoder {
    line_buffer: Vec<u8>,
    data_lines: Vec<String>,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, AppError> {
        self.line_buffer.extend_from_slice(chunk);
        if self.line_buffer.len() > MAX_SSE_LINE_BYTES {
            return Err(ai_error(
                "AI_PROVIDER_RESPONSE_INVALID",
                "AI Provider 返回了过长的流式事件",
            ));
        }
        let mut events = Vec::new();
        while let Some(position) = self.line_buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.line_buffer.drain(..=position).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if let Some(event) = self.consume_line(line)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<String>, AppError> {
        let mut events = Vec::new();
        if !self.line_buffer.is_empty() {
            let mut line = std::mem::take(&mut self.line_buffer);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if let Some(event) = self.consume_line(line)? {
                events.push(event);
            }
        }
        if !self.data_lines.is_empty() {
            events.push(std::mem::take(&mut self.data_lines).join("\n"));
        }
        Ok(events)
    }

    fn consume_line(&mut self, line: Vec<u8>) -> Result<Option<String>, AppError> {
        if line.is_empty() {
            return Ok((!self.data_lines.is_empty())
                .then(|| std::mem::take(&mut self.data_lines).join("\n")));
        }
        if line.first() == Some(&b':') {
            return Ok(None);
        }
        let Some(separator) = line.iter().position(|byte| *byte == b':') else {
            return Ok(None);
        };
        if &line[..separator] != b"data" {
            return Ok(None);
        }
        let mut raw = &line[separator + 1..];
        if raw.first() == Some(&b' ') {
            raw = &raw[1..];
        }
        let data = String::from_utf8(raw.to_vec()).map_err(|_| {
            ai_error(
                "AI_PROVIDER_RESPONSE_INVALID",
                "AI Provider 返回了无效的流式 UTF-8 数据",
            )
        })?;
        self.data_lines.push(data);
        Ok(None)
    }
}

type StreamPayloadProcessor =
    fn(Value, &mut ChatStreamResult, &Channel<AiStreamEvent>) -> Result<(), AppError>;

fn chat_request_error(error: reqwest::Error, stage: &str) -> AppError {
    if error.is_timeout() {
        ai_error("AI_PROVIDER_TIMEOUT", format!("AI Provider {stage}超时"))
    } else {
        ai_error(
            "AI_PROVIDER_CONNECTION_FAILED",
            format!("AI Provider {stage}失败，请检查网络和 API 地址"),
        )
    }
}

/// A cancellation can race with reqwest returning an aborted socket/body read.
/// Once the user has cancelled, that transport error is an expected side
/// effect rather than a Provider failure and must retain the cancellation
/// code all the way to the renderer.
fn cancellation_or_request_error(cancellation: &CancellationToken, fallback: AppError) -> AppError {
    if cancellation.is_cancelled() {
        request_cancelled_error()
    } else {
        fallback
    }
}

async fn send_streaming_request(
    request: reqwest::RequestBuilder,
    cancellation: &CancellationToken,
) -> Result<reqwest::Response, AppError> {
    let response = tokio::select! {
        _ = cancellation.cancelled() => return Err(request_cancelled_error()),
        response = request.send() => match response {
            Ok(response) => response,
            Err(error) => return Err(cancellation_or_request_error(
                cancellation,
                chat_request_error(error, "对话请求"),
            )),
        },
    };
    if cancellation.is_cancelled() {
        return Err(request_cancelled_error());
    }
    if !response.status().is_success() {
        return Err(ai_error(
            "AI_PROVIDER_HTTP_ERROR",
            format!("AI Provider 返回 HTTP {}", response.status()),
        ));
    }
    Ok(response)
}

async fn send_json_request(request: reqwest::RequestBuilder) -> Result<Value, AppError> {
    let response = request
        .send()
        .await
        .map_err(|error| chat_request_error(error, "标题请求"))?;
    if !response.status().is_success() {
        return Err(ai_error(
            "AI_PROVIDER_HTTP_ERROR",
            format!("AI Provider 返回 HTTP {}", response.status()),
        ));
    }
    let payload = response.json::<Value>().await.map_err(|_| {
        ai_error(
            "AI_PROVIDER_RESPONSE_INVALID",
            "AI Provider 未返回有效 JSON 对象",
        )
    })?;
    if !payload.is_object() {
        return Err(ai_error(
            "AI_PROVIDER_RESPONSE_INVALID",
            "AI Provider 未返回有效 JSON 对象",
        ));
    }
    Ok(payload)
}

fn parse_stream_payload(event: &str) -> Result<Value, AppError> {
    serde_json::from_str::<Value>(event).map_err(|_| {
        ai_error(
            "AI_PROVIDER_RESPONSE_INVALID",
            "AI Provider 返回了无效的流式 JSON 数据",
        )
    })
}

async fn consume_streaming_response(
    mut response: reqwest::Response,
    cancellation: &CancellationToken,
    channel: &Channel<AiStreamEvent>,
    process_payload: StreamPayloadProcessor,
) -> Result<ChatStreamResult, AppError> {
    let is_json_response = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|header| header.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("application/json"));
    let mut stream = ChatStreamResult::default();
    if is_json_response {
        let payload = tokio::select! {
            _ = cancellation.cancelled() => return Err(request_cancelled_error()),
            payload = response.json::<Value>() => match payload {
                Ok(payload) => payload,
                Err(_) => return Err(cancellation_or_request_error(
                    cancellation,
                    ai_error("AI_PROVIDER_RESPONSE_INVALID", "AI Provider 未返回有效 JSON 对象"),
                )),
            },
        };
        if cancellation.is_cancelled() {
            return Err(request_cancelled_error());
        }
        process_payload(payload, &mut stream, channel)?;
        stream.finalize_tool_calls();
        return Ok(stream);
    }

    let mut decoder = SseDecoder::default();
    loop {
        let chunk = tokio::select! {
            _ = cancellation.cancelled() => return Err(request_cancelled_error()),
            chunk = response.chunk() => match chunk {
                Ok(chunk) => chunk,
                Err(error) => return Err(cancellation_or_request_error(
                    cancellation,
                    chat_request_error(error, "流式响应"),
                )),
            },
        };
        if cancellation.is_cancelled() {
            return Err(request_cancelled_error());
        }
        let Some(chunk) = chunk else {
            break;
        };
        for event in decoder.push(&chunk)? {
            if event.trim() == "[DONE]" {
                stream.finalize_tool_calls();
                return Ok(stream);
            }
            process_payload(parse_stream_payload(&event)?, &mut stream, channel)?;
        }
    }
    if cancellation.is_cancelled() {
        return Err(request_cancelled_error());
    }
    for event in decoder.finish()? {
        if event.trim() == "[DONE]" {
            break;
        }
        process_payload(parse_stream_payload(&event)?, &mut stream, channel)?;
    }
    stream.finalize_tool_calls();
    Ok(stream)
}


use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder, Response};
use serde_json::{json, Value};
use std::time::Duration;

use crate::domain::provider::GenerationOptions;

use super::{
    capability,
    error::AiRuntimeError,
    prompt::Prompt,
    ProviderAdapter,
    ProviderConfig,
};

trait ProtocolAdapter: Sync {
    #[cfg(test)]
    fn kind(&self) -> ProviderAdapter;
    fn build_stream_request(
        &self,
        client: &Client,
        provider: &ProviderConfig,
        prompt: &Prompt,
        options: &GenerationOptions,
    ) -> RequestBuilder;
    fn build_models_request(&self, client: &Client, provider: &ProviderConfig) -> RequestBuilder;
    fn parse_stream_value(&self, value: &Value) -> Option<String>;
    fn parse_complete_value(&self, value: &Value) -> Option<String>;
    fn parse_models_value(&self, value: &Value) -> Vec<String>;
}

struct OpenAiChatAdapter;
struct OpenAiResponsesAdapter;
struct AnthropicAdapter;
struct GeminiAdapter;

static OPENAI_CHAT: OpenAiChatAdapter = OpenAiChatAdapter;
static OPENAI_RESPONSES: OpenAiResponsesAdapter = OpenAiResponsesAdapter;
static ANTHROPIC: AnthropicAdapter = AnthropicAdapter;
static GEMINI: GeminiAdapter = GeminiAdapter;

const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

struct ProviderRegistry;

impl ProviderRegistry {
    fn get(adapter: ProviderAdapter) -> &'static dyn ProtocolAdapter {
        match adapter {
            ProviderAdapter::OpenAiCompatible => &OPENAI_CHAT,
            ProviderAdapter::OpenAiResponses => &OPENAI_RESPONSES,
            ProviderAdapter::Anthropic => &ANTHROPIC,
            ProviderAdapter::Gemini => &GEMINI,
        }
    }
}

impl ProtocolAdapter for OpenAiChatAdapter {
    #[cfg(test)]
    fn kind(&self) -> ProviderAdapter {
        ProviderAdapter::OpenAiCompatible
    }

    fn build_stream_request(
        &self,
        client: &Client,
        provider: &ProviderConfig,
        prompt: &Prompt,
        options: &GenerationOptions,
    ) -> RequestBuilder {
        openai_request(client, provider, prompt, options)
    }

    fn build_models_request(&self, client: &Client, provider: &ProviderConfig) -> RequestBuilder {
        with_custom_headers(
            with_openai_auth(client.get(openai_models_url(&provider.api_base)), provider),
            provider,
        )
    }

    fn parse_stream_value(&self, value: &Value) -> Option<String> {
        extract_openai_stream(value)
    }

    fn parse_complete_value(&self, value: &Value) -> Option<String> {
        extract_openai_complete(value)
    }

    fn parse_models_value(&self, value: &Value) -> Vec<String> {
        extract_openai_models(value)
    }
}

impl ProtocolAdapter for OpenAiResponsesAdapter {
    #[cfg(test)]
    fn kind(&self) -> ProviderAdapter {
        ProviderAdapter::OpenAiResponses
    }

    fn build_stream_request(
        &self,
        client: &Client,
        provider: &ProviderConfig,
        prompt: &Prompt,
        options: &GenerationOptions,
    ) -> RequestBuilder {
        openai_responses_request(client, provider, prompt, options)
    }

    fn build_models_request(&self, client: &Client, provider: &ProviderConfig) -> RequestBuilder {
        with_custom_headers(
            with_openai_auth(client.get(openai_models_url(&provider.api_base)), provider),
            provider,
        )
    }

    fn parse_stream_value(&self, value: &Value) -> Option<String> {
        extract_openai_responses_stream(value)
    }

    fn parse_complete_value(&self, value: &Value) -> Option<String> {
        extract_openai_responses_complete(value)
    }

    fn parse_models_value(&self, value: &Value) -> Vec<String> {
        extract_openai_models(value)
    }
}

impl ProtocolAdapter for AnthropicAdapter {
    #[cfg(test)]
    fn kind(&self) -> ProviderAdapter {
        ProviderAdapter::Anthropic
    }

    fn build_stream_request(
        &self,
        client: &Client,
        provider: &ProviderConfig,
        prompt: &Prompt,
        options: &GenerationOptions,
    ) -> RequestBuilder {
        anthropic_request(client, provider, prompt, options)
    }

    fn build_models_request(&self, client: &Client, provider: &ProviderConfig) -> RequestBuilder {
        with_custom_headers(
            with_anthropic_auth(client.get(anthropic_models_url(&provider.api_base)), provider),
            provider,
        )
    }

    fn parse_stream_value(&self, value: &Value) -> Option<String> {
        extract_anthropic_stream(value)
    }

    fn parse_complete_value(&self, value: &Value) -> Option<String> {
        extract_anthropic_complete(value)
    }

    fn parse_models_value(&self, value: &Value) -> Vec<String> {
        value
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .map(str::to_string)
            .collect()
    }
}

impl ProtocolAdapter for GeminiAdapter {
    #[cfg(test)]
    fn kind(&self) -> ProviderAdapter {
        ProviderAdapter::Gemini
    }

    fn build_stream_request(
        &self,
        client: &Client,
        provider: &ProviderConfig,
        prompt: &Prompt,
        options: &GenerationOptions,
    ) -> RequestBuilder {
        gemini_request(client, provider, prompt, options)
    }

    fn build_models_request(&self, client: &Client, provider: &ProviderConfig) -> RequestBuilder {
        with_custom_headers(
            with_gemini_auth(client.get(gemini_models_url(&provider.api_base)), provider),
            provider,
        )
    }

    fn parse_stream_value(&self, value: &Value) -> Option<String> {
        extract_gemini_text(value)
    }

    fn parse_complete_value(&self, value: &Value) -> Option<String> {
        extract_gemini_text(value)
    }

    fn parse_models_value(&self, value: &Value) -> Vec<String> {
        value
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|item| {
                item.get("supportedGenerationMethods")
                    .and_then(Value::as_array)
                    .map(|methods| methods.iter().any(|method| method.as_str() == Some("generateContent")))
                    .unwrap_or(true)
            })
            .filter_map(|item| item.get("name").and_then(Value::as_str))
            .map(|name| name.strip_prefix("models/").unwrap_or(name).to_string())
            .collect()
    }
}

pub async fn stream_completion<F>(
    client: &Client,
    provider: &ProviderConfig,
    prompt: &Prompt,
    options: &GenerationOptions,
    mut on_chunk: F,
) -> Result<(), AiRuntimeError>
where
    F: FnMut(String) + Send,
{
    const MAX_ATTEMPTS: usize = 3;
    let mut last_error = AiRuntimeError::new(
        super::error::AiErrorKind::Provider,
        "provider request failed",
    );

    let adapter = ProviderRegistry::get(provider.adapter);
    let effective_options = capability::constrain_options(
        options,
        capability::infer(provider.adapter, &provider.model),
    );

    for attempt in 0..MAX_ATTEMPTS {
        let request = adapter.build_stream_request(client, provider, prompt, &effective_options);

        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                last_error = AiRuntimeError::from_reqwest(error);
                if attempt + 1 < MAX_ATTEMPTS {
                    retry_delay(attempt).await;
                    continue;
                }
                return Err(last_error);
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .map_err(AiRuntimeError::from_reqwest)?;
            last_error = AiRuntimeError::from_http(status.as_u16(), &body);
            if is_retryable_status(status.as_u16()) && attempt + 1 < MAX_ATTEMPTS {
                retry_delay(attempt).await;
                continue;
            }
            return Err(last_error);
        }

        let mut emitted = false;
        let result = consume_response(response, adapter, &mut |chunk| {
            emitted = true;
            on_chunk(chunk);
        })
        .await;

        match result {
            Ok(()) => return Ok(()),
            Err(error) if emitted => return Err(error.into_partial()),
            Err(error) => {
                last_error = error;
                if attempt + 1 < MAX_ATTEMPTS {
                    retry_delay(attempt).await;
                    continue;
                }
            }
        }
    }

    Err(last_error)
}

async fn retry_delay(attempt: usize) {
    let delay = if attempt == 0 { 300 } else { 800 };
    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
}

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

pub async fn list_models(client: &Client, provider: &ProviderConfig) -> Result<Vec<String>, String> {
    let adapter = ProviderRegistry::get(provider.adapter);
    let response = adapter
        .build_models_request(client, provider)
        .send()
        .await
        .map_err(|error| AiRuntimeError::from_reqwest(error).to_string())?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| AiRuntimeError::from_reqwest(error).to_string())?;
    if !status.is_success() {
        return Err(AiRuntimeError::from_http(status.as_u16(), &body).to_string());
    }

    let value: Value = serde_json::from_str(&body).map_err(|error| format!("invalid model list response: {error}"))?;
    let mut models = adapter.parse_models_value(&value);

    models.sort_by_key(|name| name.to_ascii_lowercase());
    models.dedup();
    Ok(models)
}

fn openai_request(
    client: &Client,
    provider: &ProviderConfig,
    prompt: &Prompt,
    options: &GenerationOptions,
) -> RequestBuilder {
    let mut body = json!({
        "model": provider.model,
        "stream": true,
        "messages": [
            { "role": "system", "content": prompt.system },
            { "role": "user", "content": prompt.user }
        ]
    });
    if let Some(value) = options.temperature {
        body["temperature"] = json!(value);
    }
    if let Some(value) = options.top_p {
        body["top_p"] = json!(value);
    }
    if let Some(value) = options.max_output_tokens {
        body["max_tokens"] = json!(value);
    }
    if let Some(mode) = normalized_reasoning_mode(options) {
        body["reasoning_effort"] = json!(mode);
    }
    with_custom_headers(
        with_openai_auth(
            client.post(openai_chat_url(&provider.api_base)).json(&body),
            provider,
        ),
        provider,
    )
}

fn openai_responses_request(
    client: &Client,
    provider: &ProviderConfig,
    prompt: &Prompt,
    options: &GenerationOptions,
) -> RequestBuilder {
    let mut body = json!({
        "model": provider.model,
        "stream": true,
        "instructions": prompt.system,
        "input": prompt.user
    });
    if let Some(value) = options.temperature {
        body["temperature"] = json!(value);
    }
    if let Some(value) = options.top_p {
        body["top_p"] = json!(value);
    }
    if let Some(value) = options.max_output_tokens {
        body["max_output_tokens"] = json!(value);
    }
    if let Some(mode) = normalized_reasoning_mode(options) {
        body["reasoning"] = json!({ "effort": mode });
    }
    with_custom_headers(
        with_openai_auth(
            client.post(openai_responses_url(&provider.api_base)).json(&body),
            provider,
        ),
        provider,
    )
}

fn anthropic_request(
    client: &Client,
    provider: &ProviderConfig,
    prompt: &Prompt,
    options: &GenerationOptions,
) -> RequestBuilder {
    let mut body = json!({
        "model": provider.model,
        "max_tokens": options.max_output_tokens.unwrap_or(4096),
        "stream": true,
        "system": prompt.system,
        "messages": [
            { "role": "user", "content": prompt.user }
        ]
    });
    if let Some(value) = options.temperature {
        body["temperature"] = json!(value);
    }
    if let Some(value) = options.top_p {
        body["top_p"] = json!(value);
    }
    if let Some(mode) = normalized_reasoning_mode(options) {
        if mode == "none" {
            body["thinking"] = json!({ "type": "disabled" });
        } else {
            body["thinking"] = json!({ "type": "adaptive" });
            body["output_config"] = json!({ "effort": anthropic_effort(mode) });
        }
    }
    with_custom_headers(
        with_anthropic_auth(
            client.post(anthropic_messages_url(&provider.api_base)).json(&body),
            provider,
        ),
        provider,
    )
}

fn gemini_request(
    client: &Client,
    provider: &ProviderConfig,
    prompt: &Prompt,
    options: &GenerationOptions,
) -> RequestBuilder {
    let mut body = json!({
        "systemInstruction": { "parts": [{ "text": prompt.system }] },
        "contents": [{
            "role": "user",
            "parts": [{ "text": prompt.user }]
        }]
    });
    let mut generation = serde_json::Map::new();
    if let Some(value) = options.temperature {
        generation.insert("temperature".into(), json!(value));
    }
    if let Some(value) = options.top_p {
        generation.insert("topP".into(), json!(value));
    }
    if let Some(value) = options.max_output_tokens {
        generation.insert("maxOutputTokens".into(), json!(value));
    }
    if let Some(mode) = normalized_reasoning_mode(options) {
        generation.insert(
            "thinkingConfig".into(),
            gemini_thinking_config(&provider.model, mode),
        );
    }
    if !generation.is_empty() {
        body["generationConfig"] = Value::Object(generation);
    }
    with_custom_headers(
        with_gemini_auth(
            client
                .post(gemini_stream_url(&provider.api_base, &provider.model))
                .json(&body),
            provider,
        ),
        provider,
    )
}

fn normalized_reasoning_mode(options: &GenerationOptions) -> Option<&str> {
    let mode = options.reasoning_mode.as_deref()?.trim();
    if mode.is_empty() || mode.eq_ignore_ascii_case("auto") {
        None
    } else {
        Some(mode)
    }
}

fn anthropic_effort(mode: &str) -> &str {
    match mode.to_ascii_lowercase().as_str() {
        "minimal" => "low",
        "xhigh" | "max" => "max",
        "low" => "low",
        "medium" => "medium",
        _ => "high",
    }
}

fn gemini_thinking_config(model: &str, mode: &str) -> Value {
    let normalized = mode.to_ascii_lowercase();
    if model.to_ascii_lowercase().contains("2.5") {
        let budget = match normalized.as_str() {
            "none" => 0,
            "minimal" | "low" => 1024,
            "medium" => 8192,
            _ => 24_576,
        };
        json!({ "thinkingBudget": budget, "includeThoughts": false })
    } else {
        let level = match normalized.as_str() {
            "none" | "minimal" => "MINIMAL",
            "low" => "LOW",
            "medium" => "MEDIUM",
            _ => "HIGH",
        };
        json!({ "thinkingLevel": level, "includeThoughts": false })
    }
}

fn with_openai_auth(request: RequestBuilder, provider: &ProviderConfig) -> RequestBuilder {
    if provider.api_key.trim().is_empty() {
        request
    } else {
        request.bearer_auth(provider.api_key.trim())
    }
}

fn with_anthropic_auth(mut request: RequestBuilder, provider: &ProviderConfig) -> RequestBuilder {
    request = request.header("anthropic-version", "2023-06-01");
    if provider.api_key.trim().is_empty() {
        request
    } else {
        request.header("x-api-key", provider.api_key.trim())
    }
}

fn with_gemini_auth(request: RequestBuilder, provider: &ProviderConfig) -> RequestBuilder {
    if provider.api_key.trim().is_empty() {
        request
    } else {
        request.header("x-goog-api-key", provider.api_key.trim())
    }
}

fn with_custom_headers(mut request: RequestBuilder, provider: &ProviderConfig) -> RequestBuilder {
    for (name, value) in &provider.headers {
        request = request.header(name.trim(), value);
    }
    request
}

async fn consume_response<F>(
    response: Response,
    adapter: &dyn ProtocolAdapter,
    on_chunk: &mut F,
) -> Result<(), AiRuntimeError>
where
    F: FnMut(String),
{
    consume_response_with_idle(response, adapter, on_chunk, STREAM_IDLE_TIMEOUT).await
}

async fn consume_response_with_idle<F>(
    response: Response,
    adapter: &dyn ProtocolAdapter,
    on_chunk: &mut F,
    idle_timeout: Duration,
) -> Result<(), AiRuntimeError>
where
    F: FnMut(String),
{
    let mut stream = response.bytes_stream();
    let mut pending = Vec::<u8>::new();
    let mut raw = Vec::<u8>::new();
    let mut sse_data = Vec::<String>::new();
    let mut emitted = false;

    loop {
        let next = tokio::time::timeout(idle_timeout, stream.next())
            .await
            .map_err(|_| AiRuntimeError::idle_timeout())?;
        let Some(next) = next else { break };
        let bytes = next.map_err(AiRuntimeError::from_reqwest)?;
        raw.extend_from_slice(&bytes);
        pending.extend_from_slice(&bytes);

        while let Some(line_end) = pending.iter().position(|byte| *byte == b'\n') {
            let mut line = pending.drain(..=line_end).collect::<Vec<_>>();
            if line.last() == Some(&b'\n') { line.pop(); }
            if line.last() == Some(&b'\r') { line.pop(); }
            for chunk in process_stream_wire_line(adapter, &line, &mut sse_data)? {
                if !chunk.is_empty() {
                    emitted = true;
                    on_chunk(chunk);
                }
            }
        }
    }

    if !pending.is_empty() {
        for chunk in process_stream_wire_line(adapter, &pending, &mut sse_data)? {
            if !chunk.is_empty() {
                emitted = true;
                on_chunk(chunk);
            }
        }
    }
    if let Some(chunk) = flush_sse_event(adapter, &mut sse_data)? {
        if !chunk.is_empty() {
            emitted = true;
            on_chunk(chunk);
        }
    }

    if !emitted {
        let body = String::from_utf8_lossy(&raw);
        if let Some(text) = parse_complete_body(adapter, body.trim()) {
            if !text.is_empty() {
                on_chunk(text);
                return Ok(());
            }
        }
        return Err(AiRuntimeError::empty());
    }

    Ok(())
}

fn process_stream_wire_line(
    adapter: &dyn ProtocolAdapter,
    line: &[u8],
    sse_data: &mut Vec<String>,
) -> Result<Vec<String>, AiRuntimeError> {
    let line = String::from_utf8_lossy(line);
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(flush_sse_event(adapter, sse_data)?.into_iter().collect());
    }
    if trimmed.starts_with(':') || trimmed.starts_with("event:") || trimmed.starts_with("id:") || trimmed.starts_with("retry:") {
        return Ok(Vec::new());
    }
    if let Some(data) = trimmed.strip_prefix("data:") {
        let mut chunks = Vec::new();
        if !sse_data.is_empty() && stream_data_is_complete(&sse_data.join("\n")) {
            if let Some(chunk) = flush_sse_event(adapter, sse_data)? {
                chunks.push(chunk);
            }
        }
        sse_data.push(data.trim_start().to_string());
        return Ok(chunks);
    }
    if trimmed.starts_with('{') {
        let mut chunks = Vec::new();
        if let Some(chunk) = flush_sse_event(adapter, sse_data)? {
            chunks.push(chunk);
        }
        if let Some(chunk) = parse_stream_data(adapter, trimmed)? {
            chunks.push(chunk);
        }
        return Ok(chunks);
    }
    Ok(Vec::new())
}

fn flush_sse_event(
    adapter: &dyn ProtocolAdapter,
    sse_data: &mut Vec<String>,
) -> Result<Option<String>, AiRuntimeError> {
    if sse_data.is_empty() {
        return Ok(None);
    }
    let data = sse_data.join("\n");
    sse_data.clear();
    parse_stream_data(adapter, data.trim())
}

fn stream_data_is_complete(data: &str) -> bool {
    let data = data.trim();
    data == "[DONE]" || serde_json::from_str::<Value>(data).is_ok()
}

fn parse_stream_data(
    adapter: &dyn ProtocolAdapter,
    data: &str,
) -> Result<Option<String>, AiRuntimeError> {
    if data.is_empty() || data == "[DONE]" {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(data).map_err(|error| {
        AiRuntimeError::malformed(format!("stream contained malformed JSON event: {error}"))
    })?;
    Ok(adapter.parse_stream_value(&value))
}

#[cfg(test)]
fn parse_stream_line(
    adapter: &dyn ProtocolAdapter,
    line: &[u8],
) -> Result<Option<String>, AiRuntimeError> {
    let mut sse_data = Vec::new();
    let chunks = process_stream_wire_line(adapter, line, &mut sse_data)?;
    if let Some(chunk) = chunks.into_iter().next() {
        return Ok(Some(chunk));
    }
    flush_sse_event(adapter, &mut sse_data)
}

#[cfg(test)]
fn parse_sse_line(adapter: ProviderAdapter, line: &[u8]) -> Option<String> {
    parse_stream_line(ProviderRegistry::get(adapter), line).ok().flatten()
}

fn parse_complete_body(adapter: &dyn ProtocolAdapter, body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    adapter.parse_complete_value(&value)
}

fn extract_openai_models(value: &Value) -> Vec<String> {
    value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.get("id")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)
        })
        .map(|name| name.strip_prefix("models/").unwrap_or(name).to_string())
        .collect()
}

fn extract_openai_stream(value: &Value) -> Option<String> {
    let content = value.pointer("/choices/0/delta/content")?;
    extract_text_value(content)
}

fn extract_openai_responses_stream(value: &Value) -> Option<String> {
    (value.get("type").and_then(Value::as_str) == Some("response.output_text.delta"))
        .then(|| value.get("delta").and_then(Value::as_str).map(str::to_string))
        .flatten()
}

fn extract_openai_responses_complete(value: &Value) -> Option<String> {
    if let Some(text) = value.get("output_text").and_then(Value::as_str) {
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    let output = value.get("output")?.as_array()?;
    let text = output
        .iter()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("output_text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn extract_openai_complete(value: &Value) -> Option<String> {
    value
        .pointer("/choices/0/message/content")
        .and_then(extract_text_value)
        .or_else(|| value.pointer("/choices/0/text").and_then(Value::as_str).map(str::to_string))
        .or_else(|| value.get("output_text").and_then(Value::as_str).map(str::to_string))
}

fn extract_anthropic_stream(value: &Value) -> Option<String> {
    value.pointer("/delta/text").and_then(Value::as_str).map(str::to_string)
}

fn extract_anthropic_complete(value: &Value) -> Option<String> {
    let parts = value.get("content")?.as_array()?;
    let text = parts
        .iter()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn extract_gemini_text(value: &Value) -> Option<String> {
    let parts = value.pointer("/candidates/0/content/parts")?.as_array()?;
    let text = parts
        .iter()
        .filter(|part| part.get("thought").and_then(Value::as_bool) != Some(true))
        .filter(|part| !is_reasoning_part(part))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn extract_text_value(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    let items = value.as_array()?;
    let text = items
        .iter()
        .filter(|item| !is_reasoning_part(item))
        .filter_map(|item| {
            item.get("text")
                .and_then(Value::as_str)
                .or_else(|| item.get("content").and_then(Value::as_str))
        })
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn is_reasoning_part(value: &Value) -> bool {
    ["type", "kind", "role"]
        .into_iter()
        .filter_map(|key| value.get(key).and_then(Value::as_str))
        .any(|kind| {
            let kind = kind.to_ascii_lowercase();
            kind.contains("reasoning") || kind.contains("thinking") || kind == "analysis"
        })
}

fn openai_chat_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

fn openai_responses_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/responses") {
        base.to_string()
    } else {
        format!("{base}/responses")
    }
}

fn openai_models_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    let base = base.strip_suffix("/chat/completions").unwrap_or(base);
    let base = base.strip_suffix("/responses").unwrap_or(base);
    format!("{base}/models")
}

fn anthropic_messages_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/v1/messages") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

fn anthropic_models_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    let base = base.strip_suffix("/v1/messages").unwrap_or(base);
    if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    }
}

fn gemini_stream_url(base: &str, model: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    let model = model.trim().strip_prefix("models/").unwrap_or(model.trim());
    format!("{base}/models/{model}:streamGenerateContent?alt=sse")
}

fn gemini_models_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    format!("{base}/models?pageSize=1000")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };

    #[test]
    fn registry_resolves_all_builtin_protocols() {
        assert_eq!(ProviderRegistry::get(ProviderAdapter::OpenAiCompatible).kind(), ProviderAdapter::OpenAiCompatible);
        assert_eq!(ProviderRegistry::get(ProviderAdapter::OpenAiResponses).kind(), ProviderAdapter::OpenAiResponses);
        assert_eq!(ProviderRegistry::get(ProviderAdapter::Anthropic).kind(), ProviderAdapter::Anthropic);
        assert_eq!(ProviderRegistry::get(ProviderAdapter::Gemini).kind(), ProviderAdapter::Gemini);
    }

    #[test]
    fn endpoint_builders_accept_v1_and_full_endpoint() {
        assert_eq!(
            openai_chat_url("https://example.com/v1"),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(
            openai_chat_url("https://example.com/v1/chat/completions"),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(openai_responses_url("https://example.com/v1"), "https://example.com/v1/responses");
        assert_eq!(openai_responses_url("https://example.com/v1/responses"), "https://example.com/v1/responses");
        assert_eq!(
            anthropic_messages_url("https://api.example.com/v1"),
            "https://api.example.com/v1/messages"
        );
    }

    #[test]
    fn parses_openai_string_and_array_stream_content() {
        let line = br#"data: {"choices":[{"delta":{"content":"hello"}}]}"#;
        assert_eq!(
            parse_sse_line(ProviderAdapter::OpenAiCompatible, line).as_deref(),
            Some("hello")
        );

        let line = br#"data: {"choices":[{"delta":{"content":[{"type":"text","text":"world"}]}}]}"#;
        assert_eq!(
            parse_sse_line(ProviderAdapter::OpenAiCompatible, line).as_deref(),
            Some("world")
        );
    }

    #[test]
    fn parses_multiline_sse_and_openai_models_variants() {
        let adapter = ProviderRegistry::get(ProviderAdapter::OpenAiCompatible);
        let mut data = Vec::new();
        assert!(process_stream_wire_line(adapter, br#"data: {"choices":[{"delta":"#, &mut data).unwrap().is_empty());
        assert!(process_stream_wire_line(adapter, br#"data: {"content":"joined"}}]}"#, &mut data).unwrap().is_empty());
        let chunk = flush_sse_event(adapter, &mut data).unwrap();
        assert_eq!(chunk.as_deref(), Some("joined"));

        assert_eq!(
            extract_openai_models(&json!({"models": [{"id": "alpha"}, {"name": "models/beta"}]})),
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn parses_anthropic_and_gemini_streams() {
        let anthropic = br#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"A"}}"#;
        assert_eq!(
            parse_sse_line(ProviderAdapter::Anthropic, anthropic).as_deref(),
            Some("A")
        );

        let gemini = br#"data: {"candidates":[{"content":{"parts":[{"thought":true,"text":"hidden"},{"text":"B"}]}}]}"#;
        assert_eq!(
            parse_sse_line(ProviderAdapter::Gemini, gemini).as_deref(),
            Some("B")
        );

        let raw_json_line = br#"{"choices":[{"delta":{"content":"raw"}}]}"#;
        assert_eq!(
            parse_sse_line(ProviderAdapter::OpenAiCompatible, raw_json_line).as_deref(),
            Some("raw")
        );

        let responses = br#"data: {"type":"response.output_text.delta","delta":"R"}"#;
        assert_eq!(
            parse_sse_line(ProviderAdapter::OpenAiResponses, responses).as_deref(),
            Some("R")
        );
    }

    #[test]
    fn retry_statuses_are_limited_to_transient_failures() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(503));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
    }

    #[test]
    fn retries_transient_http_failure_before_any_output() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for attempt in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 8192];
                let _ = socket.read(&mut buffer).unwrap();
                if attempt == 0 {
                    write!(
                        socket,
                        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                } else {
                    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"recovered\"}}]}\n\ndata: [DONE]\n\n";
                    write!(
                        socket,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                }
                socket.flush().unwrap();
            }
        });

        let provider = ProviderConfig {
            adapter: ProviderAdapter::OpenAiCompatible,
            api_base: format!("http://{address}/v1"),
            api_key: String::new(),
            model: "mock-model".into(),
            headers: Default::default(),
        };
        let prompt = Prompt { system: "system".into(), user: "hello".into() };
        let chunks = Arc::new(Mutex::new(Vec::<String>::new()));
        let collected = chunks.clone();

        tauri::async_runtime::block_on(async {
            stream_completion(
                &Client::new(),
                &provider,
                &prompt,
                &GenerationOptions::default(),
                move |chunk| {
                    collected.lock().unwrap().push(chunk);
                },
            )
            .await
            .unwrap();
        });

        server.join().unwrap();
        assert_eq!(chunks.lock().unwrap().concat(), "recovered");
    }

    #[test]
    fn interrupted_stream_keeps_partial_output_and_reports_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 8192];
            let _ = socket.read(&mut buffer).unwrap();
            let body = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n";
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len() + 32,
                body
            )
            .unwrap();
            socket.flush().unwrap();
        });

        let provider = ProviderConfig {
            adapter: ProviderAdapter::OpenAiCompatible,
            api_base: format!("http://{address}/v1"),
            api_key: String::new(),
            model: "mock-model".into(),
            headers: Default::default(),
        };
        let prompt = Prompt { system: "system".into(), user: "hello".into() };
        let chunks = Arc::new(Mutex::new(Vec::<String>::new()));
        let collected = chunks.clone();

        let error = tauri::async_runtime::block_on(async {
            stream_completion(
                &Client::new(),
                &provider,
                &prompt,
                &GenerationOptions::default(),
                move |chunk| {
                    collected.lock().unwrap().push(chunk);
                },
            )
            .await
            .unwrap_err()
        });

        server.join().unwrap();
        assert_eq!(chunks.lock().unwrap().concat(), "partial");
        assert!(error.message.contains("partial result was kept"));
        assert_eq!(error.kind, super::super::error::AiErrorKind::Interrupted);
        assert!(error.partial);
    }

    #[test]
    fn clean_eof_with_incomplete_sse_json_reports_partial_result() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 8192];
            let _ = socket.read(&mut buffer).unwrap();
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"kept\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"cut"
            );
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
                body
            )
            .unwrap();
            socket.flush().unwrap();
        });

        let provider = ProviderConfig {
            adapter: ProviderAdapter::OpenAiCompatible,
            api_base: format!("http://{address}/v1"),
            api_key: String::new(),
            model: "mock-model".into(),
            headers: Default::default(),
        };
        let prompt = Prompt { system: "system".into(), user: "hello".into() };
        let chunks = Arc::new(Mutex::new(Vec::<String>::new()));
        let collected = chunks.clone();

        let error = tauri::async_runtime::block_on(async {
            stream_completion(
                &Client::new(),
                &provider,
                &prompt,
                &GenerationOptions::default(),
                move |chunk| {
                    collected.lock().unwrap().push(chunk);
                },
            )
            .await
            .unwrap_err()
        });

        server.join().unwrap();
        assert_eq!(chunks.lock().unwrap().concat(), "kept");
        assert!(error.message.contains("partial result was kept"));
        assert!(error.message.contains("malformed JSON"));
        assert_eq!(error.kind, super::super::error::AiErrorKind::Interrupted);
    }

    #[test]
    fn openai_stream_completion_works_against_local_sse_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 8192];
            let count = socket.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..count]);
            assert!(request.starts_with("POST /v1/chat/completions "));
            assert!(request.contains("\"stream\":true"));

            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"hello \"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"world\"}}]}\n\n",
                "data: [DONE]\n\n"
            );
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            socket.flush().unwrap();
        });

        let provider = ProviderConfig {
            adapter: ProviderAdapter::OpenAiCompatible,
            api_base: format!("http://{address}/v1"),
            api_key: String::new(),
            model: "mock-model".into(),
            headers: Default::default(),
        };
        let prompt = Prompt {
            system: "system".into(),
            user: "hello".into(),
        };
        let chunks = Arc::new(Mutex::new(Vec::<String>::new()));
        let collected = chunks.clone();

        tauri::async_runtime::block_on(async {
            stream_completion(
                &Client::new(),
                &provider,
                &prompt,
                &GenerationOptions::default(),
                move |chunk| {
                    collected.lock().unwrap().push(chunk);
                },
            )
            .await
            .unwrap();
        });

        server.join().unwrap();
        assert_eq!(chunks.lock().unwrap().concat(), "hello world");
    }

    fn request_json(request: RequestBuilder) -> Value {
        let request = request.build().unwrap();
        let body = request.body().and_then(|body| body.as_bytes()).unwrap();
        serde_json::from_slice(body).unwrap()
    }

    fn assert_json_float(value: &Value, expected: f64) {
        let actual = value.as_f64().unwrap();
        assert!((actual - expected).abs() < 1e-6, "expected {expected}, got {actual}");
    }

    #[test]
    fn generation_options_are_mapped_to_all_protocols() {
        let prompt = Prompt {
            system: "system".into(),
            user: "hello".into(),
        };
        let options = GenerationOptions {
            temperature: Some(0.7),
            max_output_tokens: Some(2048),
            top_p: Some(0.8),
            reasoning_mode: Some("high".into()),
        };

        let openai = ProviderConfig {
            adapter: ProviderAdapter::OpenAiCompatible,
            api_base: "https://example.com/v1".into(),
            api_key: String::new(),
            model: "gpt-test".into(),
            headers: Default::default(),
        };
        let openai_body = request_json(openai_request(&Client::new(), &openai, &prompt, &options));
        assert_json_float(&openai_body["temperature"], 0.7);
        assert_json_float(&openai_body["top_p"], 0.8);
        assert_eq!(openai_body["max_tokens"], json!(2048));
        assert_eq!(openai_body["reasoning_effort"], json!("high"));

        let anthropic = ProviderConfig {
            adapter: ProviderAdapter::Anthropic,
            api_base: "https://example.com".into(),
            api_key: String::new(),
            model: "claude-test".into(),
            headers: Default::default(),
        };
        let anthropic_body =
            request_json(anthropic_request(&Client::new(), &anthropic, &prompt, &options));
        assert_json_float(&anthropic_body["temperature"], 0.7);
        assert_json_float(&anthropic_body["top_p"], 0.8);
        assert_eq!(anthropic_body["max_tokens"], json!(2048));
        assert_eq!(anthropic_body["thinking"]["type"], json!("adaptive"));
        assert_eq!(anthropic_body["output_config"]["effort"], json!("high"));

        let gemini = ProviderConfig {
            adapter: ProviderAdapter::Gemini,
            api_base: "https://example.com/v1beta".into(),
            api_key: String::new(),
            model: "gemini-3-pro".into(),
            headers: Default::default(),
        };
        let gemini_body = request_json(gemini_request(&Client::new(), &gemini, &prompt, &options));
        assert_json_float(&gemini_body["generationConfig"]["temperature"], 0.7);
        assert_json_float(&gemini_body["generationConfig"]["topP"], 0.8);
        assert_eq!(gemini_body["generationConfig"]["maxOutputTokens"], json!(2048));
        assert_eq!(
            gemini_body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            json!("HIGH")
        );

        let responses = ProviderConfig {
            adapter: ProviderAdapter::OpenAiResponses,
            api_base: "https://example.com/v1".into(),
            api_key: String::new(),
            model: "gpt-4.1-mini".into(),
            headers: std::collections::BTreeMap::from([
                ("HTTP-Referer".into(), "https://example.org".into()),
                ("X-Title".into(), "ZhiForge".into()),
            ]),
        };
        let request = openai_responses_request(&Client::new(), &responses, &prompt, &options)
            .build()
            .unwrap();
        assert_eq!(request.url().as_str(), "https://example.com/v1/responses");
        assert_eq!(request.headers().get("http-referer").unwrap(), "https://example.org");
        assert_eq!(request.headers().get("x-title").unwrap(), "ZhiForge");
        let body: Value = serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(body["instructions"], json!("system"));
        assert_eq!(body["input"], json!("hello"));
        assert_eq!(body["max_output_tokens"], json!(2048));
        assert_eq!(body["reasoning"]["effort"], json!("high"));
        assert_eq!(
            extract_openai_responses_complete(&json!({
                "output": [{"content": [{"type": "output_text", "text": "done"}]}]
            })),
            Some("done".into())
        );
    }

    #[test]
    fn gemini_25_reasoning_mode_uses_budget_mapping() {
        assert_eq!(gemini_thinking_config("gemini-2.5-flash", "none")["thinkingBudget"], json!(0));
        assert_eq!(gemini_thinking_config("gemini-2.5-flash", "medium")["thinkingBudget"], json!(8192));
        assert_eq!(gemini_thinking_config("gemini-2.5-pro", "high")["thinkingBudget"], json!(24_576));
    }
}

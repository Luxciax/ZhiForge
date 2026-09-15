mod capability;
mod credential;
mod error;
mod execution;
mod prompt;
mod provider;

use futures_util::future::{AbortHandle, Abortable};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{diagnostics::DiagnosticLog, settings::SettingsState};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ProviderAdapter {
    #[serde(rename = "openai-compatible")]
    OpenAiCompatible,
    #[serde(rename = "openai-responses")]
    OpenAiResponses,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "gemini")]
    Gemini,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub adapter: ProviderAdapter,
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AiActionKind {
    Translate,
    Explain,
    Summarize,
    Polish,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiActionRequest {
    pub request_id: String,
    pub action: AiActionKind,
    pub text: String,
    pub target_language: String,
    pub alternate_language: String,
    pub provider: ProviderConfig,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutedAiActionRequest {
    pub request_id: String,
    pub action: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiStartedEvent {
    request_id: String,
    provider_id: String,
    provider_name: String,
    adapter: ProviderAdapter,
    model: String,
    attempt: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiChunkEvent {
    request_id: String,
    chunk: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiDoneEvent {
    request_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiErrorEvent {
    request_id: String,
    kind: error::AiErrorKind,
    message: String,
    status: Option<u16>,
    retryable: bool,
    partial: bool,
}

impl AiErrorEvent {
    fn from_runtime(request_id: String, error: error::AiRuntimeError) -> Self {
        Self {
            request_id,
            kind: error.kind,
            message: error.message,
            status: error.status,
            retryable: error.retryable,
            partial: error.partial,
        }
    }
}

struct ActiveRequest {
    request_id: String,
    abort: AbortHandle,
}

pub struct AiRuntime {
    client: Client,
    active: Arc<Mutex<Option<ActiveRequest>>>,
}

impl AiRuntime {
    pub fn new() -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to initialize HTTP client");

        Self {
            client,
            active: Arc::new(Mutex::new(None)),
        }
    }
}

pub(crate) async fn collect_routed_text(
    app: &AppHandle,
    action_id: &str,
    system: String,
    user: String,
) -> Result<String, String> {
    let runtime = app.state::<AiRuntime>();
    let settings = app.state::<SettingsState>();
    let plan = execution::resolve_plan(&settings.snapshot()?, action_id)?;
    let prompt = prompt::Prompt { system, user };
    let output = Arc::new(Mutex::new(String::new()));
    let output_sink = output.clone();
    let diagnostic_start_app = app.clone();
    let diagnostic_action = action_id.to_string();
    let diagnostic_start_action = diagnostic_action.clone();

    let execution_result = execution::execute_plan(
        &runtime.client,
        plan,
        &prompt,
        execution::hydrate_provider,
        move |target, attempt| {
            diagnostic_start_app.state::<DiagnosticLog>().record(
                "ai.background.start",
                &format!(
                    "action={} provider={} adapter={:?} model={} attempt={}",
                    diagnostic_start_action,
                    target.provider_name,
                    target.adapter,
                    target.model,
                    attempt
                ),
            );
        },
        move |chunk| {
            if let Ok(mut buffer) = output_sink.lock() {
                buffer.push_str(&chunk);
            }
        },
    )
    .await;

    if let Err(error) = execution_result {
        app.state::<DiagnosticLog>().record(
            "ai.background.error",
            &format!("action={} error={}", diagnostic_action, error),
        );
        return Err(error.to_string());
    }

    let text = output
        .lock()
        .map_err(|_| "AI output buffer lock poisoned".to_string())?
        .clone();
    if text.trim().is_empty() {
        let error = error::AiRuntimeError::empty().to_string();
        app.state::<DiagnosticLog>().record(
            "ai.background.error",
            &format!("action={} error={}", diagnostic_action, error),
        );
        return Err(error);
    }
    app.state::<DiagnosticLog>().record(
        "ai.background.done",
        &format!("action={} chars={}", diagnostic_action, text.chars().count()),
    );
    Ok(text)
}

#[tauri::command]
pub async fn start_routed_ai_action(
    app: AppHandle,
    state: State<'_, AiRuntime>,
    settings: State<'_, SettingsState>,
    request: RoutedAiActionRequest,
) -> Result<(), String> {
    validate_routed_request(&request)?;
    let plan = execution::resolve_plan(&settings.snapshot()?, &request.action)?;
    let prompt = prompt::build_action_prompt(
        &plan.action,
        &request.text,
        &plan.target_language,
        &plan.alternate_language,
    )?;

    let (abort, registration) = AbortHandle::new_pair();
    {
        let mut active = state
            .active
            .lock()
            .map_err(|_| "AI state lock poisoned".to_string())?;
        if let Some(previous) = active.replace(ActiveRequest {
            request_id: request.request_id.clone(),
            abort,
        }) {
            previous.abort.abort();
        }
    }

    let client = state.client.clone();
    let active = state.active.clone();
    let request_id = request.request_id.clone();
    let event_app = app.clone();

    tauri::async_runtime::spawn(async move {
        let routed = execution::execute_plan(
            &client,
            plan,
            &prompt,
            execution::hydrate_provider,
            |target, attempt| {
                event_app.state::<DiagnosticLog>().record(
                    "ai.start",
                    &format!(
                        "provider={} adapter={:?} model={} attempt={}",
                        target.provider_name, target.adapter, target.model, attempt
                    ),
                );
                emit_to_result(
                    &event_app,
                    "ai://started",
                    AiStartedEvent {
                        request_id: request_id.clone(),
                        provider_id: target.provider_id.clone(),
                        provider_name: target.provider_name.clone(),
                        adapter: target.adapter,
                        model: target.model.clone(),
                        attempt,
                    },
                );
            },
            |chunk| {
                emit_to_result(
                    &event_app,
                    "ai://chunk",
                    AiChunkEvent {
                        request_id: request_id.clone(),
                        chunk,
                    },
                );
            },
        );

        match Abortable::new(routed, registration).await {
            Ok(Ok(())) => {
                event_app.state::<DiagnosticLog>().record("ai.done", "routed request completed");
                emit_to_result(
                    &event_app,
                    "ai://done",
                    AiDoneEvent {
                        request_id: request_id.clone(),
                    },
                )
            }
            Ok(Err(error)) => {
                event_app.state::<DiagnosticLog>().record(
                    "ai.error",
                    &format!(
                        "kind={:?} status={:?} partial={} message={}",
                        error.kind, error.status, error.partial, error.message
                    ),
                );
                emit_to_result(
                    &event_app,
                    "ai://error",
                    AiErrorEvent::from_runtime(request_id.clone(), error),
                )
            }
            Err(_) => {
                event_app.state::<DiagnosticLog>().record("ai.cancel", "routed request cancelled");
            }
        }

        clear_active_if_current(&active, &request_id);
    });

    Ok(())
}

#[tauri::command]
pub async fn start_ai_action(
    app: AppHandle,
    state: State<'_, AiRuntime>,
    request: AiActionRequest,
) -> Result<(), String> {
    validate_request(&request)?;
    let prompt = prompt::build_prompt(
        request.action,
        &request.text,
        &request.target_language,
        &request.alternate_language,
    );

    let (abort, registration) = AbortHandle::new_pair();
    {
        let mut active = state.active.lock().map_err(|_| "AI state lock poisoned".to_string())?;
        if let Some(previous) = active.replace(ActiveRequest {
            request_id: request.request_id.clone(),
            abort,
        }) {
            previous.abort.abort();
        }
    }

    let client = state.client.clone();
    let active = state.active.clone();
    let provider = request.provider.clone();
    let request_id = request.request_id.clone();
    let event_app = app.clone();
    app.state::<DiagnosticLog>().record(
        "ai.start",
        &format!("provider=legacy adapter={:?} model={}", provider.adapter, provider.model),
    );

    tauri::async_runtime::spawn(async move {
        let options = crate::domain::provider::GenerationOptions::default();
        let stream_request = provider::stream_completion(&client, &provider, &prompt, &options, |chunk| {
            emit_to_result(
                &event_app,
                "ai://chunk",
                AiChunkEvent {
                    request_id: request_id.clone(),
                    chunk,
                },
            );
        });

        match Abortable::new(stream_request, registration).await {
            Ok(Ok(())) => {
                event_app.state::<DiagnosticLog>().record("ai.done", "legacy request completed");
                emit_to_result(
                    &event_app,
                    "ai://done",
                    AiDoneEvent {
                        request_id: request_id.clone(),
                    },
                )
            }
            Ok(Err(error)) => {
                event_app.state::<DiagnosticLog>().record(
                    "ai.error",
                    &format!(
                        "kind={:?} status={:?} partial={} message={}",
                        error.kind, error.status, error.partial, error.message
                    ),
                );
                emit_to_result(
                    &event_app,
                    "ai://error",
                    AiErrorEvent::from_runtime(request_id.clone(), error),
                )
            }
            Err(_) => {
                event_app.state::<DiagnosticLog>().record("ai.cancel", "legacy request cancelled");
            }
        }

        if let Ok(mut guard) = active.lock() {
            let should_clear = guard
                .as_ref()
                .map(|active| active.request_id == request_id)
                .unwrap_or(false);
            if should_clear {
                *guard = None;
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn cancel_ai(state: State<'_, AiRuntime>, request_id: Option<String>) -> Result<(), String> {
    let mut active = state
        .active
        .lock()
        .map_err(|_| "AI state lock poisoned".to_string())?;
    let matches = active
        .as_ref()
        .map(|current| request_id.as_ref().map(|id| id == &current.request_id).unwrap_or(true))
        .unwrap_or(false);
    if matches {
        if let Some(current) = active.take() {
            current.abort.abort();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn load_api_key() -> Result<String, String> {
    credential::load()
}

#[tauri::command]
pub fn save_api_key(api_key: String) -> Result<(), String> {
    credential::save(&api_key)
}

#[tauri::command]
pub fn load_provider_api_key(credential_id: String) -> Result<String, String> {
    credential::load_provider(&credential_id)
}

#[tauri::command]
pub fn save_provider_api_key(credential_id: String, api_key: String) -> Result<(), String> {
    credential::save_provider(&credential_id, &api_key)
}

#[tauri::command]
pub fn migrate_legacy_api_key(credential_id: String) -> Result<bool, String> {
    credential::migrate_legacy_to_provider(&credential_id)
}

#[tauri::command]
pub fn model_capabilities(adapter: ProviderAdapter, model: String) -> capability::ModelCapabilities {
    capability::infer(adapter, &model)
}

#[tauri::command]
pub async fn list_models(
    state: State<'_, AiRuntime>,
    provider: ProviderConfig,
) -> Result<Vec<String>, String> {
    validate_provider(&provider, false)?;
    provider::list_models(&state.client, &provider).await
}

fn validate_routed_request(request: &RoutedAiActionRequest) -> Result<(), String> {
    validate_request_core(&request.request_id, &request.text)
}

fn validate_request_core(request_id: &str, text: &str) -> Result<(), String> {
    if request_id.trim().is_empty() {
        return Err("request id is empty".to_string());
    }
    if text.trim().is_empty() {
        return Err("selected text is empty".to_string());
    }
    if text.len() > 1_048_576 {
        return Err("selected text is too large".to_string());
    }
    Ok(())
}

fn validate_request(request: &AiActionRequest) -> Result<(), String> {
    validate_request_core(&request.request_id, &request.text)?;
    validate_provider(&request.provider, true)
}

fn validate_provider(provider: &ProviderConfig, require_model: bool) -> Result<(), String> {
    let url = reqwest::Url::parse(provider.api_base.trim())
        .map_err(|_| "API base URL is invalid".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("API base URL must use http or https".to_string());
    }
    if require_model && provider.model.trim().is_empty() {
        return Err("model is empty".to_string());
    }
    Ok(())
}

fn clear_active_if_current(active: &Arc<Mutex<Option<ActiveRequest>>>, request_id: &str) {
    if let Ok(mut guard) = active.lock() {
        let should_clear = guard
            .as_ref()
            .map(|current| current.request_id == request_id)
            .unwrap_or(false);
        if should_clear {
            *guard = None;
        }
    }
}

fn emit_to_result<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Some(window) = app.get_webview_window("result") {
        let _ = window.emit(event, payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_adapter_deserializes_openai_compatible() {
        let adapter: ProviderAdapter = serde_json::from_str("\"openai-compatible\"").unwrap();
        assert_eq!(adapter, ProviderAdapter::OpenAiCompatible);
    }

    #[test]
    fn request_validation_allows_keyless_local_endpoint() {
        let request = AiActionRequest {
            request_id: "1".into(),
            action: AiActionKind::Translate,
            text: "hello".into(),
            target_language: "简体中文".into(),
            alternate_language: "English".into(),
            provider: ProviderConfig {
                adapter: ProviderAdapter::OpenAiCompatible,
                api_base: "http://127.0.0.1:11434/v1".into(),
                api_key: String::new(),
                model: "local-model".into(),
                headers: BTreeMap::new(),
            },
        };
        assert!(validate_request(&request).is_ok());
    }
}

use std::collections::{BTreeMap, HashSet};

use reqwest::Client;

use crate::{
    domain::{
        action::ActionDefinition,
        provider::{GenerationOptions, ProviderProfile},
        route::ModelRef,
    },
    settings::schema::AppConfig,
};

use super::{
    credential,
    error::AiRuntimeError,
    prompt::Prompt,
    provider,
    ProviderAdapter,
    ProviderConfig,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionTarget {
    pub provider_id: String,
    pub provider_name: String,
    pub adapter: ProviderAdapter,
    pub api_base: String,
    pub headers: BTreeMap<String, String>,
    pub credential_id: String,
    pub model: String,
    pub options: GenerationOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionPlan {
    pub action: ActionDefinition,
    pub target_language: String,
    pub alternate_language: String,
    pub targets: Vec<ExecutionTarget>,
}

pub fn resolve_plan(config: &AppConfig, action_id: &str) -> Result<ExecutionPlan, String> {
    let action_id = action_id.trim();
    let definition = config
        .actions
        .iter()
        .find(|item| item.id == action_id)
        .ok_or_else(|| format!("unknown action '{action_id}'"))?;
    if !definition.enabled {
        return Err(format!("action '{action_id}' is disabled"));
    }

    let route = definition
        .route_id
        .as_deref()
        .and_then(|route_id| config.routes.iter().find(|route| route.id == route_id))
        .or_else(|| config.routes.iter().find(|route| route.action_id == action_id));

    let mut targets = Vec::new();
    let mut seen = HashSet::<(String, String)>::new();

    if let Some(route) = route {
        push_model_ref(config, &route.primary, &route.options, &mut targets, &mut seen)?;
        for fallback in &route.fallbacks {
            push_model_ref(config, fallback, &route.options, &mut targets, &mut seen)?;
        }
    } else if let Some(profile) = active_profile(config) {
        let model = profile
            .default_model
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string();
        if !model.is_empty() {
            push_target(
                profile,
                &model,
                &GenerationOptions::default(),
                &mut targets,
                &mut seen,
            )?;
        }
    }

    if targets.is_empty() {
        return Err(format!("no enabled provider/model is configured for action '{action_id}'"));
    }

    let target_language = definition
        .target_language
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&config.ui.target_language)
        .to_string();
    let alternate_language = definition
        .alternate_language
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&config.ui.alternate_language)
        .to_string();

    Ok(ExecutionPlan {
        action: definition.clone(),
        target_language,
        alternate_language,
        targets,
    })
}

pub fn hydrate_provider(target: &ExecutionTarget) -> Result<ProviderConfig, String> {
    Ok(ProviderConfig {
        adapter: target.adapter,
        api_base: target.api_base.clone(),
        api_key: credential::load_provider(&target.credential_id)?,
        model: target.model.clone(),
        headers: target.headers.clone(),
    })
}

pub async fn execute_plan<H, S, C>(
    client: &Client,
    plan: ExecutionPlan,
    prompt: &Prompt,
    hydrate: H,
    mut on_start: S,
    mut on_chunk: C,
) -> Result<(), AiRuntimeError>
where
    H: Fn(&ExecutionTarget) -> Result<ProviderConfig, String>,
    S: FnMut(&ExecutionTarget, usize),
    C: FnMut(String) + Send,
{
    let target_count = plan.targets.len();
    let mut failures = Vec::<AiRuntimeError>::new();

    for (index, target) in plan.targets.into_iter().enumerate() {
        let runtime_provider = match hydrate(&target) {
            Ok(provider) => provider,
            Err(error) => {
                let error = AiRuntimeError::config(format!(
                    "{} / {}: {error}",
                    target.provider_name, target.model
                ));
                failures.push(error);
                continue;
            }
        };

        on_start(&target, index + 1);
        let mut emitted = false;
        let result = provider::stream_completion(
            client,
            &runtime_provider,
            prompt,
            &target.options,
            |chunk| {
            emitted = true;
                on_chunk(chunk);
            },
        )
        .await;

        match result {
            Ok(()) => return Ok(()),
            Err(error) if emitted => return Err(error),
            Err(error) => {
                let mut routed_error = error;
                routed_error.message = format!("{} / {}: {}", target.provider_name, target.model, routed_error.message);
                failures.push(routed_error);
                if index + 1 == target_count {
                    break;
                }
            }
        }
    }

    if failures.len() == 1 {
        return Err(failures.remove(0));
    }
    Err(AiRuntimeError::all_providers_failed(
        failures.into_iter().map(|error| error.message).collect(),
    ))
}

fn push_model_ref(
    config: &AppConfig,
    model_ref: &ModelRef,
    route_options: &GenerationOptions,
    targets: &mut Vec<ExecutionTarget>,
    seen: &mut HashSet<(String, String)>,
) -> Result<(), String> {
    let Some(profile) = config
        .providers
        .iter()
        .find(|provider| provider.id == model_ref.provider_id && provider.enabled)
    else {
        return Ok(());
    };
    push_target(profile, model_ref.model.trim(), route_options, targets, seen)
}

fn push_target(
    profile: &ProviderProfile,
    model: &str,
    route_options: &GenerationOptions,
    targets: &mut Vec<ExecutionTarget>,
    seen: &mut HashSet<(String, String)>,
) -> Result<(), String> {
    if model.is_empty() {
        return Ok(());
    }
    let key = (profile.id.clone(), model.to_string());
    if !seen.insert(key) {
        return Ok(());
    }
    targets.push(ExecutionTarget {
        provider_id: profile.id.clone(),
        provider_name: profile.name.clone(),
        adapter: adapter_from_id(&profile.adapter_id)?,
        api_base: profile.api_base.clone(),
        headers: profile.headers.clone(),
        credential_id: profile
            .credential_id
            .clone()
            .unwrap_or_else(|| profile.id.clone()),
        model: model.to_string(),
        options: profile.defaults.overlay(route_options),
    });
    Ok(())
}

fn active_profile(config: &AppConfig) -> Option<&ProviderProfile> {
    config
        .ui
        .active_provider_id
        .as_deref()
        .and_then(|id| config.providers.iter().find(|provider| provider.id == id && provider.enabled))
        .or_else(|| config.providers.iter().find(|provider| provider.enabled))
}

fn adapter_from_id(adapter_id: &str) -> Result<ProviderAdapter, String> {
    match adapter_id.trim() {
        "openai-chat" | "openai-compatible" => Ok(ProviderAdapter::OpenAiCompatible),
        "openai-responses" => Ok(ProviderAdapter::OpenAiResponses),
        "anthropic" => Ok(ProviderAdapter::Anthropic),
        "gemini" => Ok(ProviderAdapter::Gemini),
        other => Err(format!("unsupported provider adapter '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{provider::ProviderProfile, route::ActionRoute};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };

    fn provider(id: &str, model: &str, enabled: bool) -> ProviderProfile {
        ProviderProfile {
            id: id.into(),
            name: id.into(),
            adapter_id: "openai-chat".into(),
            api_base: "http://127.0.0.1:11434/v1".into(),
            headers: BTreeMap::new(),
            credential_id: Some(format!("cred-{id}")),
            default_model: Some(model.into()),
            enabled,
            defaults: Default::default(),
            extra: Default::default(),
        }
    }

    #[test]
    fn route_plan_keeps_primary_then_fallback_order() {
        let mut config = AppConfig {
            providers: vec![provider("primary", "p-default", true), provider("backup", "b-default", true)],
            ..Default::default()
        };
        config.routes.push(ActionRoute {
            id: "translate-route".into(),
            action_id: "translate".into(),
            primary: ModelRef { provider_id: "primary".into(), model: "p-model".into() },
            fallbacks: vec![ModelRef { provider_id: "backup".into(), model: "b-model".into() }],
            options: Default::default(),
        });
        config.actions[0].route_id = Some("translate-route".into());

        let plan = resolve_plan(&config, "translate").unwrap();
        assert_eq!(plan.targets.len(), 2);
        assert_eq!(plan.targets[0].provider_id, "primary");
        assert_eq!(plan.targets[0].model, "p-model");
        assert_eq!(plan.targets[1].provider_id, "backup");
        assert_eq!(plan.targets[1].model, "b-model");
    }

    #[test]
    fn route_options_override_provider_defaults_field_by_field() {
        let mut primary = provider("primary", "p-default", true);
        primary.defaults = GenerationOptions {
            temperature: Some(0.2),
            max_output_tokens: Some(1024),
            top_p: Some(0.9),
            reasoning_mode: Some("low".into()),
        };
        let mut config = AppConfig {
            providers: vec![primary],
            ..Default::default()
        };
        config.routes.push(ActionRoute {
            id: "translate-route".into(),
            action_id: "translate".into(),
            primary: ModelRef { provider_id: "primary".into(), model: "p-model".into() },
            fallbacks: Vec::new(),
            options: GenerationOptions {
                temperature: Some(0.7),
                reasoning_mode: Some("high".into()),
                ..Default::default()
            },
        });
        config.actions[0].route_id = Some("translate-route".into());

        let plan = resolve_plan(&config, "translate").unwrap();
        let options = &plan.targets[0].options;
        assert_eq!(options.temperature, Some(0.7));
        assert_eq!(options.max_output_tokens, Some(1024));
        assert_eq!(options.top_p, Some(0.9));
        assert_eq!(options.reasoning_mode.as_deref(), Some("high"));
    }

    #[test]
    fn disabled_primary_is_skipped_in_favor_of_fallback() {
        let mut config = AppConfig {
            providers: vec![provider("primary", "p", false), provider("backup", "b", true)],
            ..Default::default()
        };
        config.routes.push(ActionRoute {
            id: "translate-route".into(),
            action_id: "translate".into(),
            primary: ModelRef { provider_id: "primary".into(), model: "p-model".into() },
            fallbacks: vec![ModelRef { provider_id: "backup".into(), model: "b-model".into() }],
            options: Default::default(),
        });

        let plan = resolve_plan(&config, "translate").unwrap();
        assert_eq!(plan.targets.len(), 1);
        assert_eq!(plan.targets[0].provider_id, "backup");
    }

    #[test]
    fn no_route_uses_active_provider_default_model() {
        let mut config = AppConfig {
            providers: vec![provider("first", "first-model", true), provider("active", "active-model", true)],
            ..Default::default()
        };
        config.ui.active_provider_id = Some("active".into());

        let plan = resolve_plan(&config, "explain").unwrap();
        assert_eq!(plan.targets.len(), 1);
        assert_eq!(plan.targets[0].provider_id, "active");
        assert_eq!(plan.targets[0].model, "active-model");
    }

    #[test]
    fn action_language_overrides_global_languages() {
        let mut config = AppConfig {
            providers: vec![provider("active", "model", true)],
            ..Default::default()
        };
        config.ui.target_language = "简体中文".into();
        config.ui.alternate_language = "English".into();
        config.actions[1].target_language = Some("日本語".into());
        config.actions[1].alternate_language = Some("Deutsch".into());

        let plan = resolve_plan(&config, "explain").unwrap();
        assert_eq!(plan.target_language, "日本語");
        assert_eq!(plan.alternate_language, "Deutsch");
    }

    #[test]
    fn disabled_action_is_rejected() {
        let mut config = AppConfig {
            providers: vec![provider("active", "model", true)],
            ..Default::default()
        };
        config.actions[0].enabled = false;
        assert!(resolve_plan(&config, "translate")
            .unwrap_err()
            .contains("disabled"));
    }

    fn target(id: &str, api_base: String) -> ExecutionTarget {
        ExecutionTarget {
            provider_id: id.into(),
            provider_name: id.into(),
            adapter: ProviderAdapter::OpenAiCompatible,
            api_base,
            headers: BTreeMap::new(),
            credential_id: id.into(),
            model: format!("{id}-model"),
            options: GenerationOptions::default(),
        }
    }

    fn test_hydrate(target: &ExecutionTarget) -> Result<ProviderConfig, String> {
        Ok(ProviderConfig {
            adapter: target.adapter,
            api_base: target.api_base.clone(),
            api_key: String::new(),
            model: target.model.clone(),
            headers: target.headers.clone(),
        })
    }

    #[test]
    fn pre_output_failure_falls_back_to_next_target() {
        let primary = TcpListener::bind("127.0.0.1:0").unwrap();
        let primary_address = primary.local_addr().unwrap();
        let primary_server = thread::spawn(move || {
            let (mut socket, _) = primary.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let _ = socket.read(&mut buffer).unwrap();
            let body = r#"{"error":{"message":"primary rejected"}}"#;
            write!(
                socket,
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            socket.flush().unwrap();
        });

        let backup = TcpListener::bind("127.0.0.1:0").unwrap();
        let backup_address = backup.local_addr().unwrap();
        let backup_server = thread::spawn(move || {
            let (mut socket, _) = backup.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let _ = socket.read(&mut buffer).unwrap();
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"fallback\"}}]}\n\n",
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

        let plan = ExecutionPlan {
            action: crate::domain::action::builtin_actions()[0].clone(),
            target_language: "简体中文".into(),
            alternate_language: "English".into(),
            targets: vec![
                target("primary", format!("http://{primary_address}/v1")),
                target("backup", format!("http://{backup_address}/v1")),
            ],
        };
        let prompt = Prompt { system: "system".into(), user: "hello".into() };
        let starts = Arc::new(Mutex::new(Vec::<String>::new()));
        let chunks = Arc::new(Mutex::new(Vec::<String>::new()));
        let starts_out = starts.clone();
        let chunks_out = chunks.clone();

        tauri::async_runtime::block_on(async {
            execute_plan(
                &Client::new(),
                plan,
                &prompt,
                test_hydrate,
                move |target, _| starts_out.lock().unwrap().push(target.provider_id.clone()),
                move |chunk| chunks_out.lock().unwrap().push(chunk),
            )
            .await
            .unwrap();
        });

        primary_server.join().unwrap();
        backup_server.join().unwrap();
        assert_eq!(starts.lock().unwrap().as_slice(), ["primary", "backup"]);
        assert_eq!(chunks.lock().unwrap().concat(), "fallback");
    }

    #[test]
    fn single_target_auth_failure_preserves_status_and_kind() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let server_thread = thread::spawn(move || {
            let (mut socket, _) = server.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let _ = socket.read(&mut buffer).unwrap();
            let body = r#"{"error":{"message":"invalid api key"}}"#;
            write!(
                socket,
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            socket.flush().unwrap();
        });

        let plan = ExecutionPlan {
            action: crate::domain::action::builtin_actions()[0].clone(),
            target_language: "简体中文".into(),
            alternate_language: "English".into(),
            targets: vec![target("primary", format!("http://{address}/v1"))],
        };
        let prompt = Prompt { system: "system".into(), user: "hello".into() };

        let error = tauri::async_runtime::block_on(async {
            execute_plan(
                &Client::new(),
                plan,
                &prompt,
                test_hydrate,
                |_, _| {},
                |_| {},
            )
            .await
            .unwrap_err()
        });

        server_thread.join().unwrap();
        assert_eq!(error.kind, crate::ai::error::AiErrorKind::Auth);
        assert_eq!(error.status, Some(401));
        assert!(error.message.contains("invalid api key"));
    }

    #[test]
    fn partial_output_never_falls_back_to_next_target() {
        let primary = TcpListener::bind("127.0.0.1:0").unwrap();
        let primary_address = primary.local_addr().unwrap();
        let primary_server = thread::spawn(move || {
            let (mut socket, _) = primary.accept().unwrap();
            let mut buffer = [0u8; 4096];
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

        let plan = ExecutionPlan {
            action: crate::domain::action::builtin_actions()[0].clone(),
            target_language: "简体中文".into(),
            alternate_language: "English".into(),
            targets: vec![
                target("primary", format!("http://{primary_address}/v1")),
                target("backup", "http://127.0.0.1:9/v1".into()),
            ],
        };
        let prompt = Prompt { system: "system".into(), user: "hello".into() };
        let starts = Arc::new(Mutex::new(Vec::<String>::new()));
        let chunks = Arc::new(Mutex::new(Vec::<String>::new()));
        let starts_out = starts.clone();
        let chunks_out = chunks.clone();

        let error = tauri::async_runtime::block_on(async {
            execute_plan(
                &Client::new(),
                plan,
                &prompt,
                test_hydrate,
                move |target, _| starts_out.lock().unwrap().push(target.provider_id.clone()),
                move |chunk| chunks_out.lock().unwrap().push(chunk),
            )
            .await
            .unwrap_err()
        });

        primary_server.join().unwrap();
        assert_eq!(starts.lock().unwrap().as_slice(), ["primary"]);
        assert_eq!(chunks.lock().unwrap().concat(), "partial");
        assert!(error.message.contains("partial result was kept"));
    }
}

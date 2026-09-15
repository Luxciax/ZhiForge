use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GenerationOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_mode: Option<String>,
}

impl GenerationOptions {
    pub fn overlay(&self, override_options: &Self) -> Self {
        Self {
            temperature: override_options.temperature.or(self.temperature),
            max_output_tokens: override_options.max_output_tokens.or(self.max_output_tokens),
            top_p: override_options.top_p.or(self.top_p),
            reasoning_mode: override_options
                .reasoning_mode
                .clone()
                .or_else(|| self.reasoning_mode.clone()),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(temperature) = self.temperature {
            if !(0.0..=2.0).contains(&temperature) {
                return Err("temperature must be between 0 and 2".into());
            }
        }
        if let Some(top_p) = self.top_p {
            if !(0.0..=1.0).contains(&top_p) {
                return Err("topP must be between 0 and 1".into());
            }
        }
        if matches!(self.max_output_tokens, Some(0)) {
            return Err("maxOutputTokens must be greater than 0".into());
        }
        if let Some(mode) = self.reasoning_mode.as_deref() {
            let mode = mode.trim().to_ascii_lowercase();
            if !matches!(
                mode.as_str(),
                "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
            ) {
                return Err(format!("unsupported reasoningMode '{mode}'"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub adapter_id: String,
    pub api_base: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub defaults: GenerationOptions,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn default_true() -> bool {
    true
}

fn validate_headers(provider_id: &str, headers: &BTreeMap<String, String>) -> Result<(), String> {
    const RESERVED: [&str; 7] = [
        "authorization",
        "x-api-key",
        "x-goog-api-key",
        "anthropic-version",
        "content-type",
        "content-length",
        "host",
    ];
    for (name, value) in headers {
        let normalized = name.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Err(format!("provider '{provider_id}' contains an empty header name"));
        }
        if RESERVED.contains(&normalized.as_str()) {
            return Err(format!("provider '{provider_id}' cannot override reserved header '{name}'"));
        }
        reqwest::header::HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| format!("provider '{provider_id}' contains invalid header name '{name}'"))?;
        reqwest::header::HeaderValue::from_str(value)
            .map_err(|_| format!("provider '{provider_id}' contains invalid value for header '{name}'"))?;
    }
    Ok(())
}

impl ProviderProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("provider id is empty".into());
        }
        if self.name.trim().is_empty() {
            return Err(format!("provider '{}' has an empty name", self.id));
        }
        if self.adapter_id.trim().is_empty() {
            return Err(format!("provider '{}' has an empty adapter id", self.id));
        }
        let url = reqwest::Url::parse(self.api_base.trim())
            .map_err(|_| format!("provider '{}' has an invalid API base", self.id))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(format!("provider '{}' API base must use http or https", self.id));
        }
        self.defaults
            .validate()
            .map_err(|error| format!("provider '{}' has invalid defaults: {error}", self.id))?;
        validate_headers(&self.id, &self.headers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_metadata_headers_are_allowed() {
        let headers = BTreeMap::from([
            ("HTTP-Referer".into(), "https://example.org".into()),
            ("X-Title".into(), "ZhiForge".into()),
        ]);
        assert!(validate_headers("openrouter", &headers).is_ok());
    }

    #[test]
    fn reserved_auth_headers_cannot_be_overridden() {
        let headers = BTreeMap::from([("Authorization".into(), "Bearer nope".into())]);
        let error = validate_headers("unsafe", &headers).unwrap_err();
        assert!(error.contains("cannot override reserved header"));
    }
}

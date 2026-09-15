use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::knowledge::SourceCaptureInput;

const BRIDGE_ADDRESS: &str = "127.0.0.1:17832";
const MAX_HEADER_BYTES: usize = 32 * 1024;
const MAX_BODY_BYTES: usize = 256 * 1024;
const CONTEXT_TTL_MS: i64 = 45_000;
const CLIENT_NAME: &str = "browser-helper";

#[derive(Clone)]
pub struct BrowserBridgeState {
    inner: Arc<BrowserBridgeInner>,
}

struct BrowserBridgeInner {
    token: String,
    pending: Mutex<Option<PendingZhihuContext>>,
}

#[derive(Clone, Debug)]
struct PendingZhihuContext {
    capture_id: String,
    selection_key: String,
    url: String,
    title: Option<String>,
    page_title: Option<String>,
    author: Option<String>,
    context_before: Option<String>,
    context_after: Option<String>,
    received_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserCaptureRequest {
    platform: String,
    url: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    page_title: Option<String>,
    #[serde(default)]
    author: Option<String>,
    selected_text: String,
    #[serde(default)]
    context_before: Option<String>,
    #[serde(default)]
    context_after: Option<String>,
}

struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl BrowserBridgeState {
    fn new() -> Self {
        Self {
            inner: Arc::new(BrowserBridgeInner {
                token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
                pending: Mutex::new(None),
            }),
        }
    }

    fn token(&self) -> &str {
        &self.inner.token
    }

    fn store(&self, context: PendingZhihuContext) -> Result<(), String> {
        let mut pending = self
            .inner
            .pending
            .lock()
            .map_err(|_| "browser bridge context lock poisoned".to_string())?;
        *pending = Some(context);
        Ok(())
    }

    fn take_matching(&self, selected_text: &str, now: i64) -> Option<PendingZhihuContext> {
        let Ok(mut pending) = self.inner.pending.lock() else {
            return None;
        };
        let Some(context) = pending.as_ref() else {
            return None;
        };
        if now.saturating_sub(context.received_at) > CONTEXT_TTL_MS {
            pending.take();
            return None;
        }
        if context.selection_key != selection_key(selected_text) {
            return None;
        }
        pending.take()
    }
}

pub fn start(app: &AppHandle) {
    let state = BrowserBridgeState::new();
    let server_state = state.clone();
    let server_app = app.clone();
    app.manage(state);

    let spawn_result = std::thread::Builder::new()
        .name("zhiforge-browser-bridge".into())
        .spawn(move || {
            let listener = match TcpListener::bind(BRIDGE_ADDRESS) {
                Ok(listener) => listener,
                Err(error) => {
                    server_app.state::<crate::diagnostics::DiagnosticLog>().record(
                        "browser_bridge.bind_failed",
                        &format!("address={BRIDGE_ADDRESS} error={error}"),
                    );
                    return;
                }
            };
            server_app
                .state::<crate::diagnostics::DiagnosticLog>()
                .record("browser_bridge.listening", BRIDGE_ADDRESS);

            for incoming in listener.incoming() {
                let Ok(mut stream) = incoming else {
                    continue;
                };
                if let Err(error) = handle_connection(&mut stream, &server_state) {
                    server_app
                        .state::<crate::diagnostics::DiagnosticLog>()
                        .record("browser_bridge.request_failed", &error);
                }
            }
        });

    if let Err(error) = spawn_result {
        app.state::<crate::diagnostics::DiagnosticLog>().record(
            "browser_bridge.thread_failed",
            &error.to_string(),
        );
    }
}

pub(crate) fn enrich_capture(app: &AppHandle, mut input: SourceCaptureInput) -> SourceCaptureInput {
    if !matches!(input.platform.trim().to_ascii_lowercase().as_str(), "desktop" | "unknown") {
        return input;
    }
    let Some(context) = app
        .state::<BrowserBridgeState>()
        .take_matching(&input.selected_text, crate::knowledge::now_ms())
    else {
        return input;
    };

    input.platform = "zhihu".into();
    input.url = Some(context.url);
    input.title = context.title;
    input.author = context.author;
    input.context_before = context.context_before;
    input.context_after = context.context_after;
    if context.page_title.is_some() {
        input.window_title = context.page_title;
    }
    input
}

#[tauri::command]
pub fn open_source_url(url: String) -> Result<(), String> {
    let url = validate_external_url(&url)?;
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(url.as_str())
            .spawn()
            .map_err(|error| format!("failed to open source URL: {error}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err("opening source URLs is currently supported on Windows only".into())
    }
}

fn handle_connection(stream: &mut TcpStream, state: &BrowserBridgeState) -> Result<(), String> {
    let request = read_http_request(stream)?;
    let origin = request.headers.get("origin").map(String::as_str);
    let allowed_origin = origin.filter(|value| is_extension_origin(value));

    if request.method == "OPTIONS" {
        if allowed_origin.is_none() {
            return respond_json(stream, 403, json!({"success": false, "error": "origin denied"}), None);
        }
        return respond_empty(stream, 204, allowed_origin);
    }

    if allowed_origin.is_none() || request.headers.get("x-zhiforge-client").map(String::as_str) != Some(CLIENT_NAME) {
        return respond_json(stream, 403, json!({"success": false, "error": "client denied"}), None);
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/pair") => respond_json(
            stream,
            200,
            json!({
                "success": true,
                "token": state.token(),
                "version": 1,
                "expiresOnRestart": true
            }),
            allowed_origin,
        ),
        ("POST", "/capture") => {
            if !authorized(&request.headers, state.token()) {
                return respond_json(
                    stream,
                    401,
                    json!({"success": false, "error": "invalid bridge token"}),
                    allowed_origin,
                );
            }
            let request: BrowserCaptureRequest = match serde_json::from_slice(&request.body) {
                Ok(request) => request,
                Err(_) => {
                    return respond_json(
                        stream,
                        400,
                        json!({"success": false, "error": "invalid capture JSON"}),
                        allowed_origin,
                    );
                }
            };
            let context = match validate_capture_request(request) {
                Ok(context) => context,
                Err(error) => {
                    return respond_json(
                        stream,
                        400,
                        json!({"success": false, "error": error}),
                        allowed_origin,
                    );
                }
            };
            let capture_id = context.capture_id.clone();
            state.store(context)?;
            respond_json(
                stream,
                200,
                json!({"success": true, "captureId": capture_id}),
                allowed_origin,
            )
        }
        _ => respond_json(
            stream,
            404,
            json!({"success": false, "error": "not found"}),
            allowed_origin,
        ),
    }
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("failed to configure bridge socket: {error}"))?;

    let mut buffer = Vec::with_capacity(4096);
    let header_end = loop {
        if buffer.len() > MAX_HEADER_BYTES {
            return Err("browser bridge headers too large".into());
        }
        if let Some(index) = find_bytes(&buffer, b"\r\n\r\n") {
            break index + 4;
        }
        let mut chunk = [0_u8; 4096];
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("failed to read browser bridge request: {error}"))?;
        if read == 0 {
            return Err("browser bridge request ended before headers".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let header_text = std::str::from_utf8(&buffer[..header_end])
        .map_err(|_| "browser bridge headers are not UTF-8".to_string())?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or_else(|| "missing request line".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_ascii_uppercase();
    let raw_path = request_parts.next().unwrap_or_default();
    if method.is_empty() || raw_path.is_empty() {
        return Err("invalid request line".into());
    }
    let path = raw_path.split('?').next().unwrap_or(raw_path).to_string();

    let mut headers = HashMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err("browser bridge body too large".into());
    }

    while buffer.len().saturating_sub(header_end) < content_length {
        let mut chunk = [0_u8; 4096];
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("failed to read browser bridge body: {error}"))?;
        if read == 0 {
            return Err("browser bridge request body ended early".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len().saturating_sub(header_end) > MAX_BODY_BYTES {
            return Err("browser bridge body too large".into());
        }
    }

    Ok(HttpRequest {
        method,
        path,
        headers,
        body: buffer[header_end..header_end + content_length].to_vec(),
    })
}

fn validate_capture_request(request: BrowserCaptureRequest) -> Result<PendingZhihuContext, String> {
    if !request.platform.trim().eq_ignore_ascii_case("zhihu") {
        return Err("browser capture platform must be zhihu".into());
    }
    let selected_text = request.selected_text.trim();
    if selected_text.is_empty() || selected_text.chars().count() > 200_000 {
        return Err("browser capture selection is empty or too large".into());
    }
    let url = validate_zhihu_url(&request.url)?;
    let title = bounded_optional(request.title, 500);
    let page_title = bounded_optional(request.page_title, 500);
    let title = title.or_else(|| page_title.clone());

    Ok(PendingZhihuContext {
        capture_id: Uuid::new_v4().to_string(),
        selection_key: selection_key(selected_text),
        url,
        title,
        page_title,
        author: bounded_optional(request.author, 300),
        context_before: bounded_optional(request.context_before, 4_000),
        context_after: bounded_optional(request.context_after, 4_000),
        received_at: crate::knowledge::now_ms(),
    })
}

fn bounded_optional(value: Option<String>, max_chars: usize) -> Option<String> {
    value
        .map(|value| value.trim().chars().take(max_chars).collect::<String>())
        .filter(|value| !value.is_empty())
}

fn validate_zhihu_url(value: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(value.trim()).map_err(|_| "invalid Zhihu URL".to_string())?;
    if url.scheme() != "https" {
        return Err("Zhihu URL must use HTTPS".into());
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host != "zhihu.com" && !host.ends_with(".zhihu.com") {
        return Err("capture URL is not a Zhihu host".into());
    }
    Ok(url.to_string())
}

fn validate_external_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value.trim()).map_err(|_| "invalid source URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only HTTP(S) source URLs can be opened".into());
    }
    Ok(url)
}

fn selection_key(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_extension_origin(origin: &str) -> bool {
    origin.starts_with("chrome-extension://") || origin.starts_with("moz-extension://")
}

fn authorized(headers: &HashMap<String, String>, token: &str) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        == Some(token)
}

fn respond_empty(stream: &mut TcpStream, status: u16, origin: Option<&str>) -> Result<(), String> {
    write_response(stream, status, "", "text/plain; charset=utf-8", origin)
}

fn respond_json(
    stream: &mut TcpStream,
    status: u16,
    value: serde_json::Value,
    origin: Option<&str>,
) -> Result<(), String> {
    let body = serde_json::to_string(&value).map_err(|error| error.to_string())?;
    write_response(stream, status, &body, "application/json; charset=utf-8", origin)
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    body: &str,
    content_type: &str,
    origin: Option<&str>,
) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.as_bytes().len()
    );
    if let Some(origin) = origin {
        response.push_str(&format!(
            "Access-Control-Allow-Origin: {origin}\r\nVary: Origin\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, Content-Type, X-ZhiForge-Client\r\nAccess-Control-Allow-Private-Network: true\r\nCache-Control: no-store\r\n"
        ));
    }
    response.push_str("\r\n");
    response.push_str(body);
    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("failed to write browser bridge response: {error}"))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Shutdown;

    fn request(text: &str) -> BrowserCaptureRequest {
        BrowserCaptureRequest {
            platform: "zhihu".into(),
            url: "https://www.zhihu.com/question/1/answer/2".into(),
            title: Some("为什么会这样？".into()),
            page_title: Some("为什么会这样？ - 知乎".into()),
            author: Some("作者".into()),
            selected_text: text.into(),
            context_before: Some("前文".into()),
            context_after: Some("后文".into()),
        }
    }

    #[test]
    fn zhihu_capture_rejects_non_zhihu_hosts() {
        let mut invalid = request("内容");
        invalid.url = "https://example.com/question/1".into();
        assert!(validate_capture_request(invalid).is_err());
    }

    #[test]
    fn selection_key_tolerates_browser_and_windows_whitespace_differences() {
        assert_eq!(selection_key("第一行\n 第二行\t第三行"), selection_key("第一行 第二行 第三行"));
    }

    #[test]
    fn pending_context_is_consumed_only_by_matching_selection() {
        let state = BrowserBridgeState::new();
        let context = validate_capture_request(request("要掌握的知识")).unwrap();
        let now = context.received_at;
        state.store(context).unwrap();

        assert!(state.take_matching("别的文本", now).is_none());
        let matched = state.take_matching("要掌握的知识", now).unwrap();
        assert_eq!(matched.title.as_deref(), Some("为什么会这样？"));
        assert!(state.take_matching("要掌握的知识", now).is_none());
    }

    #[test]
    fn stale_context_is_discarded() {
        let state = BrowserBridgeState::new();
        let context = validate_capture_request(request("内容")).unwrap();
        let stale_at = context.received_at.saturating_add(CONTEXT_TTL_MS + 1);
        state.store(context).unwrap();
        assert!(state.take_matching("内容", stale_at).is_none());
    }

    #[test]
    fn only_browser_extension_origins_can_pair() {
        assert!(is_extension_origin("chrome-extension://abcdef"));
        assert!(is_extension_origin("moz-extension://abcdef"));
        assert!(!is_extension_origin("https://www.zhihu.com"));
        assert!(!is_extension_origin("null"));
    }

    fn tcp_round_trip(state: BrowserBridgeState, request: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            handle_connection(&mut stream, &state).unwrap();
        });

        let mut client = TcpStream::connect(address).unwrap();
        client.write_all(request.as_bytes()).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        server.join().unwrap();
        response
    }

    fn response_json(response: &str) -> serde_json::Value {
        let body = response.split("\r\n\r\n").nth(1).unwrap_or_default();
        serde_json::from_str(body).unwrap()
    }

    #[test]
    fn extension_can_pair_then_post_authenticated_capture_over_http() {
        let state = BrowserBridgeState::new();
        let pair_response = tcp_round_trip(
            state.clone(),
            concat!(
                "GET /pair HTTP/1.1\r\n",
                "Host: 127.0.0.1:17832\r\n",
                "Origin: chrome-extension://test-extension\r\n",
                "X-ZhiForge-Client: browser-helper\r\n",
                "Connection: close\r\n\r\n"
            )
            .to_string(),
        );
        assert!(pair_response.starts_with("HTTP/1.1 200 OK"));
        assert!(pair_response.contains("Access-Control-Allow-Origin: chrome-extension://test-extension"));
        let token = response_json(&pair_response)["token"].as_str().unwrap().to_string();

        let body = serde_json::json!({
            "platform": "zhihu",
            "url": "https://www.zhihu.com/question/1/answer/2",
            "title": "问题",
            "pageTitle": "问题 - 知乎",
            "author": "作者",
            "selectedText": "真实选区",
            "contextBefore": "前文",
            "contextAfter": "后文"
        })
        .to_string();
        let capture_request = format!(
            "POST /capture HTTP/1.1\r\nHost: 127.0.0.1:17832\r\nOrigin: chrome-extension://test-extension\r\nX-ZhiForge-Client: browser-helper\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.as_bytes().len()
        );
        let capture_response = tcp_round_trip(state.clone(), capture_request);
        assert!(capture_response.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(response_json(&capture_response)["success"], true);
        assert!(state.take_matching("真实选区", crate::knowledge::now_ms()).is_some());
    }

    #[test]
    fn normal_web_origin_cannot_pair() {
        let state = BrowserBridgeState::new();
        let response = tcp_round_trip(
            state,
            concat!(
                "GET /pair HTTP/1.1\r\n",
                "Host: 127.0.0.1:17832\r\n",
                "Origin: https://www.zhihu.com\r\n",
                "X-ZhiForge-Client: browser-helper\r\n",
                "Connection: close\r\n\r\n"
            )
            .to_string(),
        );
        assert!(response.starts_with("HTTP/1.1 403 Forbidden"));
        assert!(!response.contains("Access-Control-Allow-Origin"));
    }
}

use std::{
    collections::HashSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use keyring::{Entry, Error as KeyringError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const SERVICE: &str = "ZhiForge";
const CREDENTIAL_ACCOUNT: &str = "zhihu:access-secret";
const SEARCH_URL: &str = "https://developer.zhihu.com/api/v1/content/zhihu_search";
const GLOBAL_SEARCH_URL: &str = "https://developer.zhihu.com/api/v1/content/global_search";

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    #[serde(rename = "Code")]
    code: i64,
    #[serde(rename = "Message", default)]
    message: String,
    #[serde(rename = "Data")]
    data: Option<T>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZhihuSearchItem {
    #[serde(rename(deserialize = "Title"), alias = "title", default)]
    pub title: String,
    #[serde(rename(deserialize = "ContentType"), alias = "contentType", default)]
    pub content_type: String,
    #[serde(rename(deserialize = "ContentID"), alias = "contentId", default)]
    pub content_id: String,
    #[serde(rename(deserialize = "ContentText"), alias = "contentText", default)]
    pub content_text: String,
    #[serde(rename(deserialize = "Url"), alias = "url", default)]
    pub url: String,
    #[serde(rename(deserialize = "CommentCount"), alias = "commentCount", default)]
    pub comment_count: i64,
    #[serde(rename(deserialize = "VoteUpCount"), alias = "voteUpCount", default)]
    pub vote_up_count: i64,
    #[serde(rename(deserialize = "AuthorName"), alias = "authorName", default)]
    pub author_name: String,
    #[serde(rename(deserialize = "AuthorAvatar"), alias = "authorAvatar", default)]
    pub author_avatar: String,
    #[serde(rename(deserialize = "AuthorBadgeText"), alias = "authorBadgeText", default)]
    pub author_badge_text: String,
    #[serde(rename(deserialize = "EditTime"), alias = "editTime", default)]
    pub edit_time: i64,
    #[serde(rename(deserialize = "AuthorityLevel"), alias = "authorityLevel", default)]
    pub authority_level: String,
    #[serde(rename(deserialize = "RankingScore"), alias = "rankingScore", default)]
    pub ranking_score: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct ZhihuSearchData {
    #[serde(rename = "HasMore", default)]
    has_more: bool,
    #[serde(rename = "SearchHashId", default)]
    search_hash_id: String,
    #[serde(rename = "Items", default)]
    items: Vec<ZhihuSearchItem>,
    #[serde(rename = "EmptyReason", default)]
    empty_reason: String,
}

#[derive(Clone, Debug, Deserialize)]
struct GlobalSearchData {
    #[serde(rename = "HasMore", default)]
    has_more: bool,
    #[serde(rename = "Items", default)]
    items: Vec<ZhihuSearchItem>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZhihuSearchResult {
    pub has_more: bool,
    pub search_hash_id: String,
    pub items: Vec<ZhihuSearchItem>,
    pub empty_reason: String,
}

fn credential_entry() -> Result<Entry, String> {
    Entry::new(SERVICE, CREDENTIAL_ACCOUNT)
        .map_err(|error| format!("知乎 Access Secret 凭证存储不可用: {error}"))
}

fn load_access_secret() -> Result<String, String> {
    match credential_entry()?.get_password() {
        Ok(secret) => Ok(secret),
        Err(KeyringError::NoEntry) => Ok(String::new()),
        Err(error) => Err(format!("读取知乎 Access Secret 失败: {error}")),
    }
}

fn save_access_secret(secret: &str) -> Result<(), String> {
    let entry = credential_entry()?;
    if secret.is_empty() {
        return match entry.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(format!("清除知乎 Access Secret 失败: {error}")),
        };
    }
    entry
        .set_password(secret)
        .map_err(|error| format!("保存知乎 Access Secret 失败: {error}"))?;
    Ok(())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn normalized_query(query: &str) -> Result<String, String> {
    let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = query.chars().count();
    if count < 2 {
        return Err("搜索内容至少需要 2 个字符".into());
    }
    if count > 100 {
        return Err("搜索内容不能超过 100 个字符，请先精简问题".into());
    }
    Ok(query)
}

fn deduplicate_search_items(items: Vec<ZhihuSearchItem>) -> Vec<ZhihuSearchItem> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| {
            let key = if !item.url.trim().is_empty() {
                Some(format!("url:{}", item.url.trim()))
            } else if !item.content_id.trim().is_empty() {
                Some(format!(
                    "{}:{}",
                    item.content_type.trim(),
                    item.content_id.trim()
                ))
            } else {
                None
            };
            key.map(|value| seen.insert(value)).unwrap_or(true)
        })
        .collect()
}

#[tauri::command]
pub fn zhihu_access_secret_configured() -> Result<bool, String> {
    Ok(!load_access_secret()?.trim().is_empty())
}

#[tauri::command]
pub fn zhihu_save_access_secret(access_secret: String) -> Result<(), String> {
    save_access_secret(access_secret.trim())
}

#[tauri::command]
pub fn zhihu_open_search(app: AppHandle, query: String) -> Result<(), String> {
    let query = query.trim().to_string();
    if query.chars().count() > 20_000 {
        return Err("选中文本过长，无法作为搜索入口".into());
    }
    let window = crate::platform::window::main_window(&app)?;
    window
        .emit("zhihu://search-request", query)
        .map_err(|error| format!("无法打开知乎搜索: {error}"))?;
    crate::platform::window::show_window(&window)
}

#[tauri::command]
pub async fn zhihu_search(query: String, count: Option<u32>) -> Result<ZhihuSearchResult, String> {
    let query = normalized_query(&query)?;
    let count = count.unwrap_or(10).clamp(1, 10);
    let secret = load_access_secret()?;
    if secret.trim().is_empty() {
        return Err("尚未配置知乎开放平台 Access Secret".into());
    }

    let count_string = count.to_string();
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("初始化知乎搜索客户端失败: {error}"))?;
    let response = client
        .get(SEARCH_URL)
        .query(&[("Query", query.as_str()), ("Count", count_string.as_str())])
        .bearer_auth(secret)
        .header("X-Request-Timestamp", unix_seconds().to_string())
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("知乎搜索请求失败: {error}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取知乎搜索响应失败: {error}"))?;
    if !status.is_success() {
        let preview = body.chars().take(300).collect::<String>();
        return Err(format!(
            "知乎搜索返回 HTTP {}: {}",
            status.as_u16(),
            preview
        ));
    }

    let payload = serde_json::from_str::<ApiResponse<ZhihuSearchData>>(&body)
        .map_err(|error| format!("知乎搜索响应格式异常: {error}"))?;
    if payload.code != 0 {
        return Err(format!(
            "知乎搜索失败（Code={}）：{}",
            payload.code, payload.message
        ));
    }
    let data = payload
        .data
        .ok_or_else(|| "知乎搜索成功但没有返回 Data".to_string())?;
    let items = deduplicate_search_items(data.items);
    Ok(ZhihuSearchResult {
        has_more: data.has_more,
        search_hash_id: data.search_hash_id,
        items,
        empty_reason: data.empty_reason,
    })
}

#[tauri::command]
pub async fn zhihu_global_search(query: String, count: Option<u32>) -> Result<ZhihuSearchResult, String> {
    let query = normalized_query(&query)?;
    let count = count.unwrap_or(20).clamp(1, 20);
    let secret = load_access_secret()?;
    if secret.trim().is_empty() {
        return Err("尚未配置知乎开放平台 Access Secret".into());
    }

    let count_string = count.to_string();
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("初始化知乎扩展搜索客户端失败: {error}"))?;
    let response = client
        .get(GLOBAL_SEARCH_URL)
        .query(&[
            ("Query", query.as_str()),
            ("Count", count_string.as_str()),
            ("Filter", "host==\"zhihu.com\""),
            ("SearchDB", "all"),
        ])
        .bearer_auth(secret)
        .header("X-Request-Timestamp", unix_seconds().to_string())
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("知乎扩展搜索请求失败: {error}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取知乎扩展搜索响应失败: {error}"))?;
    if !status.is_success() {
        let preview = body.chars().take(300).collect::<String>();
        return Err(format!("知乎扩展搜索返回 HTTP {}: {}", status.as_u16(), preview));
    }
    let payload = serde_json::from_str::<ApiResponse<GlobalSearchData>>(&body)
        .map_err(|error| format!("知乎扩展搜索响应格式异常: {error}"))?;
    if payload.code != 0 {
        return Err(format!("知乎扩展搜索失败（Code={}）：{}", payload.code, payload.message));
    }
    let data = payload
        .data
        .ok_or_else(|| "知乎扩展搜索成功但没有返回 Data".to_string())?;
    Ok(ZhihuSearchResult {
        has_more: data.has_more,
        search_hash_id: String::new(),
        items: deduplicate_search_items(data.items),
        empty_reason: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_validation_is_bounded() {
        assert_eq!(normalized_query("  RAG   评测  ").unwrap(), "RAG 评测");
        assert!(normalized_query("a").is_err());
        assert!(normalized_query(&"x".repeat(101)).is_err());
    }

    #[test]
    fn duplicate_results_can_be_identified_by_url_or_content_id() {
        let items = vec![
            ZhihuSearchItem {
                title: "A".into(),
                content_type: "Answer".into(),
                content_id: "1".into(),
                content_text: "x".into(),
                url: "https://www.zhihu.com/a".into(),
                comment_count: 0,
                vote_up_count: 0,
                author_name: String::new(),
                author_avatar: String::new(),
                author_badge_text: String::new(),
                edit_time: 0,
                authority_level: String::new(),
                ranking_score: 0.0,
            },
            ZhihuSearchItem {
                title: "A2".into(),
                content_type: "Answer".into(),
                content_id: "2".into(),
                content_text: "y".into(),
                url: "https://www.zhihu.com/a".into(),
                comment_count: 0,
                vote_up_count: 0,
                author_name: String::new(),
                author_avatar: String::new(),
                author_badge_text: String::new(),
                edit_time: 0,
                authority_level: String::new(),
                ranking_score: 0.0,
            },
        ];
        let kept = deduplicate_search_items(items);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].title, "A");
    }

    #[test]
    fn global_search_shape_deserializes() {
        let payload = serde_json::from_str::<ApiResponse<GlobalSearchData>>(
            r#"{"Code":0,"Message":"ok","Data":{"HasMore":true,"Items":[{"Title":"T","ContentType":"Answer","ContentID":"1","ContentText":"Body","Url":"https://www.zhihu.com/question/1/answer/1"}]}}"#,
        )
        .unwrap();
        let data = payload.data.unwrap();
        assert!(data.has_more);
        assert_eq!(data.items.len(), 1);
        assert_eq!(data.items[0].content_text, "Body");
    }

    #[test]
    fn official_search_shape_deserializes() {
        let payload = serde_json::from_str::<ApiResponse<ZhihuSearchData>>(
            r#"{"Code":0,"Message":"ok","Data":{"HasMore":false,"SearchHashId":"h","Items":[{"Title":"T","ContentType":"Answer","ContentID":"1","ContentText":"Body","Url":"https://www.zhihu.com/question/1/answer/1","VoteUpCount":12,"CommentCount":3,"AuthorName":"A"}]}}"#,
        )
        .unwrap();
        let data = payload.data.unwrap();
        assert_eq!(data.items[0].title, "T");
        assert_eq!(data.items[0].vote_up_count, 12);
        let ipc = serde_json::to_value(&data.items[0]).unwrap();
        assert_eq!(ipc["contentText"], "Body", "IPC must match the React camelCase contract");
        assert_eq!(ipc["contentId"], "1");
        assert_eq!(ipc["voteUpCount"], 12);
        assert!(ipc.get("ContentText").is_none());
    }
}

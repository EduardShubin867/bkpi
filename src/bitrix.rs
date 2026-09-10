use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{Mutex, Semaphore};

use anyhow::{Context, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

type ResponseCache = Arc<Mutex<HashMap<String, (tokio::time::Instant, Vec<u8>)>>>;

#[derive(Clone)]
pub struct BitrixClient {
    http: Client,
    webhook_base: String,
    cache: ResponseCache,
    permits: Arc<Semaphore>,
    next_request: Arc<Mutex<tokio::time::Instant>>,
}

impl BitrixClient {
    pub fn new(webhook_base: &str) -> Result<Self> {
        let webhook_base = webhook_base.trim().trim_end_matches('/').to_string();
        anyhow::ensure!(!webhook_base.is_empty(), "Webhook URL пуст");
        Ok(Self {
            http: Client::builder()
                .user_agent(concat!("bkpi/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .context("Не удалось создать HTTP client")?,
            webhook_base,
            cache: Arc::new(Mutex::new(HashMap::new())),
            permits: Arc::new(Semaphore::new(4)),
            next_request: Arc::new(Mutex::new(tokio::time::Instant::now())),
        })
    }

    async fn call<T: DeserializeOwned>(&self, method: &str, body: Value) -> Result<T> {
        let url = format!("{}/{}", self.webhook_base, method);
        let cache_key = format!("{method}:{}", serde_json::to_string(&body)?);
        {
            let cache = self.cache.lock().await;
            if let Some((at, bytes)) = cache.get(&cache_key)
                && at.elapsed() < Duration::from_secs(15)
            {
                let envelope: ApiEnvelope<T> = serde_json::from_slice(bytes)
                    .map_err(|_| anyhow::anyhow!("Invalid cached response"))?;
                return envelope.result.context("Cached response missing result");
            }
        }
        let _permit = self.permits.acquire().await?;
        for attempt in 0..3 {
            {
                let mut next = self.next_request.lock().await;
                tokio::time::sleep_until(*next).await;
                *next = tokio::time::Instant::now() + Duration::from_millis(500);
            }
            let response = self.http.post(&url).json(&body).send().await;
            let response = match response {
                Ok(r) => r,
                Err(_) if attempt < 2 => {
                    tokio::time::sleep(Duration::from_millis(500 << attempt)).await;
                    continue;
                }
                Err(_) => anyhow::bail!("Bitrix transport failed (URL and response suppressed)"),
            };
            let status = response.status();
            if (status.as_u16() == 429 || status.is_server_error()) && attempt < 2 {
                tokio::time::sleep(Duration::from_millis(500 << attempt)).await;
                continue;
            }
            anyhow::ensure!(
                status.is_success(),
                "Bitrix HTTP {} (body suppressed)",
                status.as_u16()
            );
            let bytes = response
                .bytes()
                .await
                .map_err(|_| anyhow::anyhow!("Cannot read Bitrix response"))?;
            let payload: ApiEnvelope<T> = serde_json::from_slice(&bytes)
                .map_err(|_| anyhow::anyhow!("Invalid Bitrix response (body suppressed)"))?;
            if let Some(error) = payload.error {
                if matches!(
                    error.as_str(),
                    "QUERY_LIMIT_EXCEEDED" | "OPERATION_TIME_LIMIT"
                ) && attempt < 2
                {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                anyhow::bail!("Bitrix API rejected read operation (details suppressed)");
            }
            let value = payload.result.context("Bitrix response missing result")?;
            let mut cache = self.cache.lock().await;
            if cache.len() >= 1000 {
                cache.clear();
            }
            cache.insert(
                cache_key.clone(),
                (tokio::time::Instant::now(), bytes.to_vec()),
            );
            return Ok(value);
        }
        anyhow::bail!("Bitrix retries exhausted")
    }
    pub fn sanitize(&self, value: serde_json::Value) -> Result<serde_json::Value> {
        let secret = crate::credentials::Secret::new(self.webhook_base.clone());
        let text = secret.redact(&serde_json::to_string(&value)?);
        let mut value = serde_json::from_str(&text)?;
        crate::credentials::redact_value(&mut value);
        Ok(value)
    }
    pub async fn users(&self, search: &str) -> Result<Vec<User>> {
        let mut all = Vec::new();
        for page in 0..200 {
            let result: ApiEnvelope<Vec<User>> =
                self.call_envelope_users(search, page * 50).await?;
            all.extend(result.result.context("Missing user results")?);
            if result.next.is_none() {
                return serde_json::from_value(self.sanitize(serde_json::to_value(all)?)?)
                    .map_err(Into::into);
            }
        }
        anyhow::bail!("User pagination limit reached; narrow search")
    }
    async fn call_envelope_users(
        &self,
        search: &str,
        start: usize,
    ) -> Result<ApiEnvelope<Vec<User>>> {
        let users: Vec<User> = self
            .call(
                "user.get",
                json!({"FILTER": {"ACTIVE":"Y", "FIND":search}, "start":start}),
            )
            .await?;
        let next = (users.len() == 50).then_some(start + 50);
        Ok(ApiEnvelope {
            result: Some(users),
            error: None,
            next,
        })
    }
    pub async fn task(&self, id: &str) -> Result<Task> {
        #[derive(Deserialize)]
        struct Detail {
            task: Task,
        }
        let detail: Detail = self.call("tasks.task.get", json!({"taskId":id})).await?;
        Ok(detail.task)
    }

    pub async fn current_user(&self) -> Result<User> {
        let user: User = self.call("user.current", json!({})).await?;
        serde_json::from_value(self.sanitize(serde_json::to_value(user)?)?).map_err(Into::into)
    }

    pub async fn tasks_for_user(&self, user_id: &str) -> Result<Vec<Task>> {
        let mut all = Vec::new();
        let mut start = 0usize;

        loop {
            let result: TaskListResult = self
                .call(
                    "tasks.task.list",
                    json!({
                        "order": { "DEADLINE": "asc", "ID": "desc" },
                        "filter": { "RESPONSIBLE_ID": user_id },
                        "select": [
                            "ID", "TITLE", "STATUS", "REAL_STATUS", "PRIORITY",
                            "DEADLINE", "CREATED_DATE", "CHANGED_DATE", "CLOSED_DATE",
                            "RESPONSIBLE_ID", "DESCRIPTION"
                        ],
                        "start": start
                    }),
                )
                .await?;

            let page_len = result.tasks.len();
            all.extend(result.tasks);
            if page_len < 50 {
                break;
            }
            start += 50;
            anyhow::ensure!(start <= 10_000, "Bitrix pagination limit reached");
        }

        Ok(all)
    }

    pub async fn checklist_for_task(&self, task_id: &str) -> Result<Vec<ChecklistItem>> {
        let mut all = Vec::new();
        let mut start = 0usize;

        loop {
            let page: Vec<ChecklistItem> = self
                .call(
                    "task.checklistitem.getlist",
                    json!({
                        "TASKID": task_id,
                        "ORDER": { "SORT_INDEX": "asc" },
                        "start": start
                    }),
                )
                .await?;

            let page_len = page.len();
            all.extend(page);
            if page_len < 50 {
                break;
            }
            start += 50;
            anyhow::ensure!(start <= 10_000, "Bitrix pagination limit reached");
        }

        Ok(all)
    }

    pub async fn results_for_task(&self, task_id: &str) -> Result<Vec<TaskResult>> {
        let mut all = Vec::new();
        let mut start = 0usize;

        loop {
            let page: Vec<TaskResult> = self
                .call(
                    "tasks.task.result.list",
                    json!({
                        "taskId": task_id,
                        "start": start
                    }),
                )
                .await?;

            let page_len = page.len();
            all.extend(page);
            if page_len < 50 {
                break;
            }
            start += 50;
            anyhow::ensure!(start <= 10_000, "Bitrix pagination limit reached");
        }

        Ok(all)
    }
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    result: Option<T>,
    error: Option<String>,
    #[serde(default)]
    next: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "NAME", default)]
    pub name: String,
    #[serde(rename = "LAST_NAME", default)]
    pub last_name: String,
    #[serde(rename = "EMAIL", default)]
    pub email: String,
}

impl User {
    pub fn display_name(&self) -> String {
        let name = format!("{} {}", self.name, self.last_name)
            .trim()
            .to_string();
        if !name.is_empty() {
            name
        } else if !self.email.is_empty() {
            self.email.clone()
        } else {
            format!("user {}", self.id)
        }
    }
}

#[derive(Debug, Deserialize)]
struct TaskListResult {
    tasks: Vec<Task>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default)]
    pub created_date: Option<String>,
    #[serde(default)]
    pub changed_date: Option<String>,
    #[serde(default)]
    pub closed_date: Option<String>,
    #[serde(default)]
    pub responsible_id: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

impl Task {
    pub fn is_closed(&self) -> bool {
        self.status == "5" || self.closed_date.is_some()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub formatted_text: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub status: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChecklistItem {
    #[serde(rename = "ID", default)]
    pub id: String,
    #[serde(rename = "TITLE", default)]
    pub title: String,
    #[serde(rename = "PARENT_ID", default)]
    pub parent_id: Value,
    #[serde(rename = "IS_COMPLETE", default)]
    pub is_complete: Value,
}

impl ChecklistItem {
    pub fn is_root(&self) -> bool {
        match &self.parent_id {
            Value::String(value) => value == "0" || value.is_empty(),
            Value::Number(value) => value.as_i64() == Some(0),
            Value::Null => true,
            _ => false,
        }
    }

    pub fn completed(&self) -> bool {
        match &self.is_complete {
            Value::String(value) => {
                value.eq_ignore_ascii_case("Y")
                    || value == "1"
                    || value.eq_ignore_ascii_case("true")
            }
            Value::Bool(value) => *value,
            Value::Number(value) => value.as_i64().is_some_and(|v| v != 0),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChecklistProgress {
    pub completed: usize,
    pub total: usize,
}

impl ChecklistProgress {
    pub fn from_items(items: &[ChecklistItem]) -> Self {
        let mut progress = Self::default();
        for item in items.iter().filter(|item| !item.is_root()) {
            progress.total += 1;
            if item.completed() {
                progress.completed += 1;
            }
        }
        progress
    }

    pub fn percent(&self) -> Option<f64> {
        (self.total > 0).then(|| self.completed as f64 / self.total as f64 * 100.0)
    }
}

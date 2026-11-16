//! Relay API client over HTTP/HTTPS with reqwest.

use crate::model::*;
use reqwest::Client;
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP error {code}: {message}")]
    Http { code: u16, message: String },
    #[error("Wrong token for {name}")]
    WrongToken { name: String },
    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),
    #[error("{0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, ApiError>;

/// Relay API client.
pub struct RelayClient {
    machine: Machine,
    client: Client,
}

impl RelayClient {
    /// Create a new client for a machine.
    pub fn new(machine: Machine) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(RelayClient { machine, client })
    }

    /// Get the machine this client is connected to.
    pub fn machine(&self) -> &Machine {
        &self.machine
    }

    /// Build a path URL with authorization.
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.machine.base(), path)
    }

    /// Make a GET request.
    async fn get(&self, path: &str) -> Result<String> {
        let url = self.url(path);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.machine.token)
            .send()
            .await?;

        let status = resp.status().as_u16();
        match status {
            401 => Err(ApiError::WrongToken {
                name: self.machine.name.clone(),
            }),
            200 => Ok(resp.text().await?),
            _ => {
                let _ = resp.text().await;
                Err(ApiError::Http {
                    code: status,
                    message: format!("Relay error {}", status),
                })
            }
        }
    }

    /// Make a POST request with JSON body.
    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        let url = self.url(path);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.machine.token)
            .json(&body)
            .send()
            .await?;

        let status = resp.status().as_u16();
        match status {
            401 => Err(ApiError::WrongToken {
                name: self.machine.name.clone(),
            }),
            200 => {
                let text = resp.text().await?;
                Ok(serde_json::from_str(&text).unwrap_or_else(|_| json!({})))
            }
            _ => {
                let _ = resp.text().await;
                Err(ApiError::Http {
                    code: status,
                    message: format!("Relay error {}", status),
                })
            }
        }
    }

    // ── Sessions and messages ──

    /// List all sessions.
    pub async fn sessions(&self) -> Result<Vec<Sess>> {
        let text = self.get("/sessions").await?;
        let arr: Vec<Sess> = serde_json::from_str(&text)?;
        Ok(arr)
    }

    /// Get messages from a session (with tail paging, default 80).
    pub async fn messages(&self, id: &str, tail: Option<i32>) -> Result<Vec<Msg>> {
        let tail = tail.unwrap_or(80);
        let path = format!("/sessions/{}/messages?tail={}", id, tail);
        let text = self.get(&path).await?;
        let arr: Vec<Msg> = serde_json::from_str(&text)?;
        Ok(arr)
    }

    // ── Projects ──

    /// Get projects.
    pub async fn projects(&self) -> Result<(Option<String>, Vec<Project>)> {
        let text = self.get("/projects").await?;
        let o: Value = serde_json::from_str(&text)?;
        let root = o.get("root").and_then(|r| r.as_str()).map(|s| s.to_string());
        let projects: Vec<Project> = o
            .get("projects")
            .and_then(|p| p.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| serde_json::from_value(p.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok((root, projects))
    }

    /// Create a new project.
    pub async fn create_project(&self, name: &str) -> Result<String> {
        let o = self.post("/projects", json!({ "name": name })).await?;
        o.get("path")
            .and_then(|p| p.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| ApiError::Custom("No path in response".to_string()))
    }

    // ── Usage ──

    /// Get current usage.
    pub async fn usage(&self) -> Result<Option<Usage>> {
        let text = self.get("/usage").await?;
        let o: Value = serde_json::from_str(&text)?;
        if !o.get("at").is_some() {
            return Ok(None);
        }
        let now = o
            .get("at")
            .and_then(|a| a.as_i64())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0)
            });
        Ok(Some(Usage::from_api(&o, now)))
    }

    /// Refresh usage (make a small request to update limits).
    pub async fn refresh_usage(&self) -> Result<()> {
        self.post("/usage/refresh", json!({})).await?;
        Ok(())
    }

    /// Get cost report.
    pub async fn cost(&self, days: i32) -> Result<Option<ApiCost>> {
        let path = format!("/usage-report?days=30");
        let text = self.get(&path).await?;
        let o: Value = serde_json::from_str(&text)?;
        let days_arr = o
            .get("days")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|d| d.get("cost").and_then(|c| c.as_f64()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if days_arr.is_empty() {
            return Ok(None);
        }

        let total = o
            .get("totals")
            .and_then(|t| t.get("cost"))
            .and_then(|c| c.as_f64())
            .unwrap_or_else(|| days_arr.iter().sum());

        let tail = days.min(days_arr.len() as i32) as usize;
        let recent = if days_arr.len() > tail {
            days_arr[days_arr.len() - tail..].to_vec()
        } else {
            days_arr
        };

        Ok(Some(ApiCost {
            total,
            days: recent,
            out_tokens: 0.0,
        }))
    }

    // ── Week plan ──

    /// Get week plan (tz in minutes east of UTC).
    pub async fn week_plan(&self, tz: i32) -> Result<WeekPlan> {
        let path = format!("/weekplan?tz={}", tz);
        let text = self.get(&path).await?;
        let o: Value = serde_json::from_str(&text)?;
        Ok(Self::parse_plan(&o))
    }

    /// Set a day in the week plan.
    pub async fn set_week_day(
        &self,
        day: i32,
        pct: Option<f64>,
        tz: i32,
    ) -> Result<WeekPlan> {
        let mut body = json!({
            "day": day,
            "tz": tz,
        });
        if let Some(p) = pct {
            body["pct"] = json!(p);
        } else {
            body["pct"] = Value::Null;
        }
        let o = self.post("/weekplan", body).await?;
        Ok(Self::parse_plan(&o))
    }

    fn parse_plan(o: &Value) -> WeekPlan {
        let days = o
            .get("days")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|d| serde_json::from_value(d.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        let segs = o
            .get("segs")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|d| serde_json::from_value(d.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        let targets = o
            .get("targets")
            .and_then(|t| t.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| k.parse::<i32>().ok().and_then(|_ki| v.as_f64().map(|vf| (k.clone(), vf))))
                    .collect()
            })
            .unwrap_or_default();

        WeekPlan {
            unavailable: o
                .get("unavailable")
                .and_then(|u| u.as_str())
                .map(|s| s.to_string()),
            left: o.get("left").and_then(|l| l.as_f64()).unwrap_or(0.0),
            over: o.get("over").and_then(|o| o.as_bool()).unwrap_or(false),
            leftover: o.get("leftover").and_then(|l| l.as_f64()).unwrap_or(0.0),
            unknown: o.get("unknown").and_then(|u| u.as_f64()).unwrap_or(0.0),
            auto_days: o.get("autoDays").and_then(|a| a.as_i64()).unwrap_or(0) as i32,
            auto_each: o.get("autoEach").and_then(|a| a.as_f64()).unwrap_or(0.0),
            days,
            segs,
            targets,
            r: o.get("R").and_then(|r| r.as_f64()).unwrap_or(0.0),
        }
    }

    // ── Utilities ──

    // ── Pairing ──

    /// Ask to connect (send a request to the computer for approval).
    pub async fn ask_to_connect(&self, device_name: &str) -> Result<String> {
        let o = self.post("/pair/request", json!({ "name": device_name })).await?;
        o.get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| ApiError::Custom("No request id in response".to_string()))
    }

    /// Check status of a pairing request.
    pub async fn ask_status(&self, request_id: &str) -> Result<Value> {
        let path = format!("/pair/request/{}", request_id);
        let text = self.get(&path).await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Trade a 6-digit code for a pairing token.
    pub async fn pair(&self, code: &str) -> Result<Machine> {
        let o = self.post("/pair", json!({ "code": code })).await?;
        let token = o
            .get("token")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| ApiError::Custom("No token in response".to_string()))?;

        Ok(Machine {
            name: o.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.machine.host.clone()),
            host: self.machine.host.clone(),
            port: self.machine.port,
            token,
            secure: self.machine.secure,
            guest: false,
            label: String::new(),
            share: String::new(),
        })
    }

    // ── Calls and pending ──

    /// Get pending requests (questions, permission requests, calls).
    pub async fn pending(&self) -> Result<Vec<Value>> {
        let text = self.get("/pending").await?;
        Ok(serde_json::from_str(&text).unwrap_or_default())
    }

    /// Answer a pending request.
    pub async fn answer(&self, req_id: &str, answer: &str) -> Result<()> {
        self.post("/answer", json!({ "req": req_id, "answer": answer })).await?;
        Ok(())
    }

    // ── Safety and rules ──

    /// Get safety settings.
    pub async fn safety(&self) -> Result<Value> {
        let text = self.get("/admin/safety").await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Update safety settings.
    pub async fn set_safety(&self, patch: Value) -> Result<Value> {
        self.post("/admin/safety", patch).await
    }

    /// Get rule options.
    pub async fn rules(&self) -> Result<Value> {
        let text = self.get("/admin/options").await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Update rule options.
    pub async fn set_rules(&self, patch: Value) -> Result<Value> {
        self.post("/admin/options", patch).await
    }

    // ── Memory ──

    /// Get memory info.
    pub async fn memory(&self) -> Result<Value> {
        let text = self.get("/admin/memory").await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Set memory limit.
    pub async fn set_memory(&self, pct: i32, restart: bool) -> Result<()> {
        self.post("/admin/memory", json!({ "pct": pct, "restart": restart })).await?;
        Ok(())
    }

    // ── Health and admin ──

    /// Get doctor (health check).
    pub async fn doctor(&self) -> Result<Value> {
        let text = self.get("/admin/doctor").await?;
        Ok(serde_json::from_str(&text)?)
    }

    // ── Chat tags (pins and names) ──

    /// Get all chat tags (pins and custom names).
    pub async fn chat_tags(&self) -> Result<Value> {
        let text = self.get("/chat-tags").await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Set a chat tag (pin and/or name).
    pub async fn set_chat_tag(&self, id: &str, pin: Option<bool>, name: Option<&str>) -> Result<()> {
        let mut body = json!({ "id": id });
        if let Some(p) = pin {
            body["pin"] = json!(p);
        }
        if let Some(n) = name {
            body["name"] = json!(n);
        }
        self.post("/chat-tags", body).await?;
        Ok(())
    }

    // ── File operations ──

    /// Browse a directory.
    pub async fn browse(&self, path: &str, hidden: bool) -> Result<crate::model::Listing> {
        let encoded = percent_encoding::percent_encode(path.as_bytes(), percent_encoding::NON_ALPHANUMERIC).to_string();
        let hidden_param = if hidden { "&hidden=1" } else { "" };
        let p = format!("/fs?path={}{}", encoded, hidden_param);
        let text = self.get(&p).await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Create a directory.
    pub async fn make_dir(&self, parent: &str, name: &str) -> Result<String> {
        let o = self.post("/fs/mkdir", json!({ "path": parent, "name": name })).await?;
        o.get("path")
            .and_then(|p| p.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| ApiError::Custom("No path in response".to_string()))
    }

    // ── Prefs and routing ──

    /// Get call preference for a session ("off", "done", "attention").
    pub async fn prefs(&self, id: &str) -> Result<String> {
        let text = self.get(&format!("/sessions/{}/prefs", id)).await?;
        let o: Value = serde_json::from_str(&text)?;
        Ok(o.get("call")
            .and_then(|c| c.as_str())
            .unwrap_or("off")
            .to_string())
    }

    /// Set call preference for a session.
    pub async fn set_prefs(&self, id: &str, call: &str) -> Result<()> {
        self.post(&format!("/sessions/{}/prefs", id), json!({ "call": call })).await?;
        Ok(())
    }

    /// Get run preference for a session ("auto", "laptop", "vps").
    pub async fn run_pref(&self, id: &str) -> Result<String> {
        let text = self.get(&format!("/sessions/{}/prefs", id)).await?;
        let o: Value = serde_json::from_str(&text)?;
        Ok(o.get("run")
            .and_then(|r| r.as_str())
            .unwrap_or("auto")
            .to_string())
    }

    /// Set run preference for a session.
    pub async fn set_run(&self, id: &str, run: &str) -> Result<()> {
        self.post(&format!("/sessions/{}/prefs", id), json!({ "run": run })).await?;
        Ok(())
    }

    /// Set auto mode for a session.
    pub async fn set_auto(&self, id: &str, on: bool) -> Result<()> {
        self.post(&format!("/sessions/{}/auto", id), json!({ "on": on })).await?;
        Ok(())
    }

    // ── Workflow ──

    /// Get workflow state for a session.
    pub async fn workflow(&self, id: &str) -> Result<Value> {
        let text = self.get(&format!("/sessions/{}/workflow", id)).await?;
        Ok(serde_json::from_str(&text)?)
    }

    // ── Templates ──

    /// Get templates.
    pub async fn templates(&self) -> Result<Vec<(String, String)>> {
        let text = self.get("/templates").await?;
        let o: Value = serde_json::from_str(&text)?;
        let templates = o
            .get("templates")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        let name = t.get("name")?.as_str()?.to_string();
                        let text = t.get("text")?.as_str()?.to_string();
                        Some((name, text))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(templates)
    }

    /// Add a template.
    pub async fn add_template(&self, name: &str, text: &str) -> Result<()> {
        self.post(
            "/templates",
            json!({ "add": { "name": name, "text": text } }),
        ).await?;
        Ok(())
    }

    // ── MCP ──

    /// Get MCP catalog.
    pub async fn mcp_catalog(&self) -> Result<Vec<(String, String, String)>> {
        let text = self.get("/mcp/catalog").await?;
        let o: Value = serde_json::from_str(&text)?;
        let tools = o
            .get("tools")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        let id = t.get("id")?.as_str()?.to_string();
                        let name = t.get("name")?.as_str()?.to_string();
                        let about = t.get("about")?.as_str().unwrap_or("").to_string();
                        let needs = t.get("needs")?.as_str()?.to_string();
                        Some((id, format!("{}|{}", name, about), needs))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(tools)
    }

    /// Install an MCP tool.
    pub async fn mcp_install(&self, id: &str, secret: &str) -> Result<String> {
        let o = self.post("/mcp/install", json!({ "id": id, "secret": secret })).await?;
        Ok(o.get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("Added.")
            .to_string())
    }

    // ── API Keys ──

    /// Get API keys info.
    pub async fn api_keys(&self) -> Result<Value> {
        let text = self.get("/admin/apikeys").await?;
        Ok(serde_json::from_str(&text)?)
    }

    // ── Skills ──

    /// Get available skills/slash commands.
    pub async fn skills(&self) -> Result<Vec<crate::model::Skill>> {
        let text = self.get("/commands").await?;
        let arr: Vec<Value> = serde_json::from_str(&text)?;
        Ok(arr
            .iter()
            .filter_map(|s| {
                // Filter out built-in skills
                if s.get("src")
                    .and_then(|s| s.as_str())
                    .map(|src| src == "built in")
                    .unwrap_or(false)
                {
                    return None;
                }
                serde_json::from_value(s.clone()).ok()
            })
            .collect())
    }

    // ── Computers ──

    /// Get list of computers.
    pub async fn computers(&self) -> Result<Vec<Computer>> {
        let text = self.get("/computers").await?;
        let arr: Vec<Computer> = serde_json::from_str(&text)?;
        Ok(arr)
    }

    // ── Undo ──

    /// Undo the last action in a session.
    pub async fn undo(&self, id: &str) -> Result<()> {
        self.post(&format!("/sessions/{}/undo", id), json!({})).await?;
        Ok(())
    }

    // ── Decisions ──

    /// Get recent decisions (approvals/denials).
    pub async fn decisions(&self, tail: i32) -> Result<Vec<(i64, String, String)>> {
        let text = self.get(&format!("/admin/decisions?tail={}", tail)).await?;
        let o: Value = serde_json::from_str(&text)?;
        let decisions = o
            .get("decisions")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|d| {
                        let at = d.get("at")?.as_i64()?;
                        let result = d.get("result")?.as_str()?.to_string();
                        let text = d.get("text")?.as_str()?.to_string();
                        Some((at, result, text))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(decisions)
    }

    /// Get a recap (summary).
    pub async fn recap(&self, days: i32) -> Result<Value> {
        let text = self.get(&format!("/recap?days={}", days)).await?;
        Ok(serde_json::from_str(&text)?)
    }

    // ── Search ──

    /// Search for text in sessions.
    pub async fn search(&self, q: &str) -> Result<Vec<(Sess, String)>> {
        let encoded = percent_encoding::percent_encode(q.as_bytes(), percent_encoding::NON_ALPHANUMERIC).to_string();
        let path = format!("/search?q={}", encoded);
        let text = self.get(&path).await?;
        let o: Value = serde_json::from_str(&text)?;
        let results = o
            .get("results")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let sess: Sess = serde_json::from_value(item.clone()).ok()?;
                        let snippet = item.get("snippet")?.as_str().unwrap_or("").to_string();
                        Some((sess, snippet))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(results)
    }

    /// Get logs.
    pub async fn logs(&self, tail: i32) -> Result<String> {
        let path = format!("/admin/logs?tail={}", tail);
        let text = self.get(&path).await?;
        let o: Value = serde_json::from_str(&text)?;
        let version = o.get("version").and_then(|v| v.as_str()).unwrap_or("unknown");
        let lines = o
            .get("lines")
            .and_then(|l| l.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| l.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        let content = if lines.is_empty() { "(nothing logged)" } else { &lines };
        Ok(format!("Server {}\n{}", version, content))
    }

    /// Stop a running session.
    pub async fn stop(&self, id: &str) -> Result<()> {
        self.post("/stop", json!({ "session": id })).await?;
        Ok(())
    }

    /// Send a message to a session (`id` empty = new chat) and stream the run's records.
    /// `extra` is merged into the request body (cwd, model, kind, effort, ...; nulls are dropped).
    /// A dropped connection is joined again by itself; see [`crate::stream::run`].
    pub fn send(&self, id: &str, text: &str, mode: &str, brief: bool, extra: Value) -> crate::stream::RunEvents {
        let mut body = json!({ "text": text, "mode": mode, "brief": brief });
        if let Some(x) = extra.as_object() {
            for (k, v) in x {
                if !v.is_null() {
                    body[k] = v.clone();
                }
            }
        }
        crate::stream::run(self.machine.clone(), id.to_string(), body)
    }

    /// The relay's event feed (SSE) with automatic reconnect.
    pub fn events(&self) -> crate::stream::EventFeed {
        crate::stream::events(self.machine.clone())
    }

    /// Get sync status.
    pub async fn sync_status(&self) -> Result<String> {
        let text = self.get("/sync/status").await?;
        let o: Value = serde_json::from_str(&text)?;
        let mut parts = Vec::new();

        if let Some(chats) = o.get("chats").and_then(|c| c.as_i64()) {
            let word = if chats == 1 { "chat" } else { "chats" };
            parts.push(format!("{} {}", chats, word));
        }

        if let Some(files) = o.get("files").and_then(|f| f.get("count")).and_then(|c| c.as_i64()) {
            let word = if files == 1 { "file" } else { "files" };
            parts.push(format!("{} project {}", files, word));
        }

        if let Some(computers) = o.get("computers").and_then(|c| c.as_array()) {
            for comp in computers {
                if let Some(name) = comp.get("name").and_then(|n| n.as_str()) {
                    let status = if comp.get("online").and_then(|o| o.as_bool()).unwrap_or(false) {
                        "online"
                    } else {
                        "offline"
                    };
                    parts.push(format!("{} {}", name, status));
                }
            }
        }

        if parts.is_empty() {
            Ok("Nothing is synced here yet".to_string())
        } else {
            Ok(parts.join(" · "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_client_creation() {
        let m = Machine {
            name: "test".to_string(),
            host: "localhost".to_string(),
            port: 47601,
            token: "token".to_string(),
            secure: false,
            guest: false,
            label: String::new(),
            share: String::new(),
        };
        let _client = RelayClient::new(m).unwrap();
    }

    #[test]
    fn test_url_building() {
        let m = Machine {
            name: "test".to_string(),
            host: "example.com".to_string(),
            port: 443,
            token: "token".to_string(),
            secure: true,
            guest: false,
            label: String::new(),
            share: String::new(),
        };
        let client = RelayClient::new(m).unwrap();
        assert_eq!(
            client.url("/sessions"),
            "https://example.com/sessions"
        );
    }
}

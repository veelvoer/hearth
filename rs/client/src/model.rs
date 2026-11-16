//! Data models for Hearth: sessions, messages, usage, machines, etc.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A computer or relay server to connect to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Machine {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub token: String,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub guest: bool,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub share: String,
}

impl Machine {
    /// Base URL for API calls to this machine.
    pub fn base(&self) -> String {
        let scheme = if self.secure { "https" } else { "http" };
        let port_str = if (self.secure && self.port == 443) || (!self.secure && self.port == 80) {
            String::new()
        } else {
            format!(":{}", self.port)
        };
        format!("{}://{}{}", scheme, self.host, port_str)
    }
}

/// One chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sess {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub mtime: i64,
    pub live: bool,
    #[serde(default)]
    pub busy: bool,
    #[serde(default)]
    pub running: bool,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub elsewhere: String,
    #[serde(default)]
    pub tokens: i64,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub root: bool,
    #[serde(default = "default_neg_one")]
    pub shared: i32,
    #[serde(default)]
    pub interrupted: bool,
}

fn default_kind() -> String {
    "code".to_string()
}

fn default_neg_one() -> i32 {
    -1
}

/// One message in a chat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Msg {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub detail: String,
    pub result: Option<String>,
    #[serde(default)]
    pub err: bool,
    #[serde(default)]
    pub own: bool,
    #[serde(default)]
    pub ts: i64,
}

/// A project directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub path: String,
}

/// A remote computer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Computer {
    pub id: String,
    pub name: String,
    pub online: bool,
    pub kind: String,
}

/// One usage limit window (session, week, opus, sonnet, etc).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Limit {
    pub pct: f64,
    pub resets_at: Option<i64>,
}

impl Limit {
    /// A window that already rolled over has nothing used yet.
    pub fn rolled(&self, t: i64) -> bool {
        self.resets_at.map(|r| r < t).unwrap_or(false)
    }

    /// Current utilization, accounting for rollover.
    pub fn at(&self, t: i64) -> f64 {
        if self.rolled(t) { 0.0 } else { self.pct }
    }
}

/// Extra (overage) usage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Extra {
    pub used: f64,
    pub limit: f64,
    pub pct: f64,
}

/// Usage report with limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub session: Option<Limit>,
    pub week: Option<Limit>,
    pub opus: Option<Limit>,
    pub sonnet: Option<Limit>,
    pub extra: Option<Extra>,
    pub at: i64,
}

impl Usage {
    /// Parse from relay API response format.
    pub fn from_api(o: &serde_json::Value, now: i64) -> Self {
        let parse_limit = |key: &str| -> Option<Limit> {
            o.get(key).and_then(|lim| {
                let pct = lim.get("utilization")?.as_f64()?;
                let resets_at = lim
                    .get("resets_at")
                    .and_then(|r| r.as_str())
                    .and_then(|s| {
                        if s.is_empty() || s == "null" {
                            None
                        } else {
                            chrono::DateTime::parse_from_rfc3339(s)
                                .ok()
                                .map(|dt| dt.timestamp_millis())
                        }
                    });
                Some(Limit { pct, resets_at })
            })
        };

        let extra = o
            .get("extra_usage")
            .and_then(|ex| {
                if !ex.get("is_enabled")?.as_bool().unwrap_or(false) {
                    return None;
                }
                let limit = ex.get("monthly_limit")?.as_f64()?;
                Some(Extra {
                    used: ex.get("used_credits")?.as_f64().unwrap_or(0.0) / 100.0,
                    limit,
                    pct: ex.get("utilization")?.as_f64().unwrap_or(0.0),
                })
            });

        Usage {
            session: parse_limit("five_hour"),
            week: parse_limit("seven_day"),
            opus: parse_limit("seven_day_opus"),
            sonnet: parse_limit("seven_day_sonnet"),
            extra,
            at: now,
        }
    }
}

/// One day of the week plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanDay {
    pub wd: i32,
    pub name: String,
    pub state: String,
    pub used: f64,
    pub planned: f64,
    pub fixed: bool,
    pub target: Option<f64>,
    #[serde(default)]
    pub start: bool,
}

/// One segment of the week plan (may span days).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSeg {
    pub wd: i32,
    pub f: f64,
    pub state: String,
    pub used: f64,
    pub w: f64,
}

/// The weekly plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeekPlan {
    pub unavailable: Option<String>,
    pub left: f64,
    pub over: bool,
    pub leftover: f64,
    pub unknown: f64,
    #[serde(rename = "autoDays")]
    pub auto_days: i32,
    #[serde(rename = "autoEach")]
    pub auto_each: f64,
    pub days: Vec<PlanDay>,
    #[serde(default)]
    pub segs: Vec<PlanSeg>,
    #[serde(default)]
    pub targets: HashMap<String, f64>,
    #[serde(default = "default_r")]
    pub r: f64,
}

fn default_r() -> f64 {
    0.0
}

/// Cost report with daily breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCost {
    pub total: f64,
    pub days: Vec<f64>,
    #[serde(default)]
    pub out_tokens: f64,
}

/// Profile info (account).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub email: String,
    pub plan: String,
}

/// A pending call event (question, permission request, call).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEvent {
    #[serde(default)]
    pub id: i64,
    pub kind: String,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub at: i64,
    #[serde(default)]
    pub req: String,
    #[serde(default)]
    pub machine: String,
    #[serde(default)]
    pub call: bool,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub multi: bool,
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub suggest: String,
}

/// Directory in file browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dir {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub git: bool,
}

/// Directory listing response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing {
    pub path: String,
    pub parent: Option<String>,
    pub dirs: Vec<Dir>,
}

/// Skill/slash command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub src: String,
    #[serde(default)]
    pub own: bool,
}

/// Pace info: elapsed and projected usage of a limit.
#[derive(Debug, Clone)]
pub struct Pace {
    pub elapsed: f64,
    pub projected: f64,
    pub hits_at: Option<i64>,
}

/// Calculate pace for a limit and window.
pub fn pace(l: Option<&Limit>, window: i64, t: i64) -> Option<Pace> {
    let l = l?;
    let r = l.resets_at?;
    if r < t {
        return None;
    }
    let start = r - window;
    let el = (t - start).max(1).min(window);
    let frac = el as f64 / window as f64;
    if frac < 0.04 || l.pct <= 0.0 {
        return Some(Pace {
            elapsed: frac,
            projected: l.pct,
            hits_at: None,
        });
    }
    let proj = l.pct / frac;
    let hits_at = if proj > 100.0 {
        Some(start + ((el as f64 * 100.0 / l.pct) as i64))
    } else {
        None
    };
    Some(Pace {
        elapsed: frac,
        projected: proj,
        hits_at,
    })
}

/// Get a word describing pace status.
pub fn pace_word(l: Option<&Limit>, p: Option<&Pace>) -> Option<&'static str> {
    let l = l?;
    let p = p?;
    let delta = l.pct - p.elapsed * 100.0;
    if delta > 5.0 {
        Some("Ahead of pace")
    } else if delta < -5.0 {
        Some("Under pace")
    } else {
        Some("On pace")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_machine_base() {
        let m = Machine {
            name: "laptop".to_string(),
            host: "192.168.1.1".to_string(),
            port: 47601,
            token: "abc".to_string(),
            secure: false,
            guest: false,
            label: String::new(),
            share: String::new(),
        };
        assert_eq!(m.base(), "http://192.168.1.1:47601");

        let m2 = Machine {
            name: "server".to_string(),
            host: "example.com".to_string(),
            port: 443,
            token: "abc".to_string(),
            secure: true,
            guest: false,
            label: String::new(),
            share: String::new(),
        };
        assert_eq!(m2.base(), "https://example.com");
    }

    #[test]
    fn test_limit_rolled() {
        let now = 1000000i64;
        let l = Limit {
            pct: 0.5,
            resets_at: Some(500000),
        };
        assert!(l.rolled(now));
        assert_eq!(l.at(now), 0.0);

        let l2 = Limit {
            pct: 0.5,
            resets_at: Some(2000000),
        };
        assert!(!l2.rolled(now));
        assert_eq!(l2.at(now), 0.5);
    }

    #[test]
    fn test_pace() {
        let l = Limit {
            pct: 50.0,
            resets_at: Some(10000),
        };
        let window = 10000i64;
        let now = 7500i64;
        let p = pace(Some(&l), window, now);
        assert!(p.is_some());
        let p = p.unwrap();
        assert!((p.elapsed - 0.75).abs() < 0.01);
    }
}

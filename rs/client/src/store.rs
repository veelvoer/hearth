//! Persistent storage for machines, settings, and pins using atomic JSON writes.

use crate::model::{Machine, Usage};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Persistent store for machines, settings, pins, names, etc.
pub struct Store {
    path: PathBuf,
    data: Value,
}

impl Store {
    /// Open or create a store at the given path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let data = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
        };
        Ok(Store { path, data })
    }

    /// Get all machines.
    pub fn machines(&self) -> Vec<Machine> {
        self.data
            .get("machines")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| serde_json::from_value(m.clone()).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Save machines.
    pub fn set_machines(&mut self, machines: Vec<Machine>) -> Result<()> {
        self.data["machines"] = serde_json::to_value(machines)?;
        self.save()
    }

    /// Get cached usage.
    pub fn usage(&self) -> Option<Usage> {
        self.data
            .get("usage")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Save usage cache.
    pub fn set_usage(&mut self, usage: Usage) -> Result<()> {
        self.data["usage"] = serde_json::to_value(usage)?;
        self.save()
    }

    /// Get usage thresholds (notification percentages).
    pub fn thresholds(&self) -> Vec<i32> {
        self.data
            .get("thresholds")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_i64().map(|i| i as i32))
                    .collect()
            })
            .unwrap_or_else(|| vec![75, 90, 100])
    }

    /// Set usage thresholds.
    pub fn set_thresholds(&mut self, thresholds: Vec<i32>) -> Result<()> {
        self.data["thresholds"] = serde_json::to_value(thresholds)?;
        self.save()
    }

    /// Get pinned status of a session.
    pub fn pinned(&self, sid: &str) -> bool {
        self.data
            .get("pins")
            .and_then(|v| v.get(sid))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Set pinned status.
    pub fn set_pinned(&mut self, sid: &str, pinned: bool) -> Result<()> {
        if self.data.get("pins").is_none() {
            self.data["pins"] = json!({});
        }
        if pinned {
            self.data["pins"][sid] = Value::Bool(true);
        } else {
            if let Some(pins) = self.data["pins"].as_object_mut() {
                pins.remove(sid);
            }
        }
        self.save()
    }

    /// Get custom name for a session.
    pub fn chat_name(&self, sid: &str) -> String {
        self.data
            .get("names")
            .and_then(|v| v.get(sid))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    /// Set custom name for a session.
    pub fn set_chat_name(&mut self, sid: &str, name: &str) -> Result<()> {
        if self.data.get("names").is_none() {
            self.data["names"] = json!({});
        }
        if name.is_empty() {
            if let Some(names) = self.data["names"].as_object_mut() {
                names.remove(sid);
            }
        } else {
            self.data["names"][sid] = Value::String(name.to_string());
        }
        self.save()
    }

    /// Get a generic setting.
    pub fn get_setting(&self, key: &str) -> Option<Value> {
        self.data.get("settings").and_then(|v| v.get(key).cloned())
    }

    /// Set a generic setting.
    pub fn set_setting(&mut self, key: &str, value: Value) -> Result<()> {
        if self.data.get("settings").is_none() {
            self.data["settings"] = json!({});
        }
        self.data["settings"][key] = value;
        self.save()
    }

    /// Save the store atomically.
    fn save(&self) -> Result<()> {
        let json_str = serde_json::to_string_pretty(&self.data)?;
        // Atomic write: write to temp file then move
        let temp_path = self.path.with_extension("tmp");
        fs::write(&temp_path, json_str.as_bytes())?;
        fs::rename(&temp_path, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_store_create() -> Result<()> {
        let dir = TempDir::new()?;
        let path = dir.path().join("store.json");
        let store = Store::open(&path)?;
        assert_eq!(store.machines().len(), 0);
        Ok(())
    }

    #[test]
    fn test_store_machines() -> Result<()> {
        let dir = TempDir::new()?;
        let path = dir.path().join("store.json");
        let mut store = Store::open(&path)?;

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

        store.set_machines(vec![m.clone()])?;
        assert_eq!(store.machines().len(), 1);
        assert_eq!(store.machines()[0].host, "192.168.1.1");

        // Reopen and verify persistence
        let store2 = Store::open(&path)?;
        assert_eq!(store2.machines().len(), 1);
        Ok(())
    }

    #[test]
    fn test_store_pins() -> Result<()> {
        let dir = TempDir::new()?;
        let path = dir.path().join("store.json");
        let mut store = Store::open(&path)?;

        assert!(!store.pinned("chat1"));
        store.set_pinned("chat1", true)?;
        assert!(store.pinned("chat1"));

        store.set_pinned("chat1", false)?;
        assert!(!store.pinned("chat1"));
        Ok(())
    }

    #[test]
    fn test_store_chat_names() -> Result<()> {
        let dir = TempDir::new()?;
        let path = dir.path().join("store.json");
        let mut store = Store::open(&path)?;

        assert_eq!(store.chat_name("chat1"), "");
        store.set_chat_name("chat1", "My chat")?;
        assert_eq!(store.chat_name("chat1"), "My chat");

        store.set_chat_name("chat1", "")?;
        assert_eq!(store.chat_name("chat1"), "");
        Ok(())
    }
}

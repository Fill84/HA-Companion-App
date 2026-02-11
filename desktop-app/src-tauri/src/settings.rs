use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const STORE_PATH: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub server_url: String,
    pub access_token: String,
    pub webhook_id: Option<String>,
    pub device_id: String,
    pub update_interval: u64,
    pub language: String,
    pub enabled_sensors: HashMap<String, bool>,
    pub autostart: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            access_token: String::new(),
            webhook_id: None,
            device_id: uuid::Uuid::new_v4().to_string(),
            update_interval: 60,
            language: "en".to_string(),
            enabled_sensors: HashMap::new(),
            autostart: false,
        }
    }
}

impl AppSettings {
    /// Load settings from the Tauri store
    pub fn load(app: &AppHandle) -> Self {
        let store = match app.store(STORE_PATH) {
            Ok(s) => s,
            Err(_) => return Self::default(),
        };

        let server_url = store
            .get("server_url")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        let access_token = store
            .get("access_token")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        let webhook_id = store
            .get("webhook_id")
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        let device_id = store
            .get("device_id")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| {
                let id = uuid::Uuid::new_v4().to_string();
                let _ = store.set("device_id", serde_json::json!(id));
                id
            });

        let update_interval = store
            .get("update_interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        let language = store
            .get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "en".to_string());

        let enabled_sensors: HashMap<String, bool> = store
            .get("enabled_sensors")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let autostart = store
            .get("autostart")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Self {
            server_url,
            access_token,
            webhook_id,
            device_id,
            update_interval,
            language,
            enabled_sensors,
            autostart,
        }
    }

    /// Save settings to the Tauri store
    pub fn save(&self, app: &AppHandle) -> Result<(), String> {
        let store = app.store(STORE_PATH).map_err(|e| e.to_string())?;

        store.set("server_url", serde_json::json!(self.server_url));
        store.set("access_token", serde_json::json!(self.access_token));
        store.set("webhook_id", serde_json::json!(self.webhook_id));
        store.set("device_id", serde_json::json!(self.device_id));
        store.set("update_interval", serde_json::json!(self.update_interval));
        store.set("language", serde_json::json!(self.language));
        store.set(
            "enabled_sensors",
            serde_json::to_value(&self.enabled_sensors).unwrap_or_default(),
        );
        store.set("autostart", serde_json::json!(self.autostart));

        Ok(())
    }
}

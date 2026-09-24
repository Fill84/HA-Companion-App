use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const STORE_PATH: &str = "settings.json";
const TOKEN_SERVICE: &str = "com.ha-companion.desktop.access-token";

#[derive(Serialize, Deserialize)]
struct StoredToken {
    server_url: String,
    access_token: String,
}

fn token_entry(device_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(TOKEN_SERVICE, device_id)
        .map_err(|error| format!("Could not open system credential store: {error}"))
}

fn encoded_token(server_url: &str, access_token: &str) -> Result<String, String> {
    serde_json::to_string(&StoredToken {
        server_url: server_url.to_string(),
        access_token: access_token.to_string(),
    })
    .map_err(|error| error.to_string())
}

fn token_for_server(raw: &str, server_url: &str) -> Option<String> {
    let stored: StoredToken = serde_json::from_str(raw).ok()?;
    (stored.server_url == server_url && !stored.access_token.is_empty())
        .then_some(stored.access_token)
}

fn persist_entries(
    app: &AppHandle,
    entries: Vec<(String, serde_json::Value)>,
) -> Result<(), String> {
    let path = tauri_plugin_store::resolve_store_path(app, STORE_PATH)
        .map_err(|error| format!("Could not locate settings store: {error}"))?;
    write_settings_document(&path, entries)
}

fn write_settings_document(
    path: &Path,
    entries: Vec<(String, serde_json::Value)>,
) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid settings path")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create settings folder: {error}"))?;
    let content =
        serde_json::to_vec_pretty(&entries.into_iter().collect::<serde_json::Map<_, _>>())
            .map_err(|error| format!("Could not encode settings: {error}"))?;
    let temporary = parent.join(format!(".settings-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<(), String> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|error| format!("Could not create temporary settings file: {error}"))?;
        file.write_all(&content)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Could not write settings: {error}"))?;
        drop(file);
        fs::rename(&temporary, path)
            .map_err(|error| format!("Could not replace settings file: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn verify_settings_document(path: &Path) -> Result<bool, String> {
    if !path
        .try_exists()
        .map_err(|error| format!("Could not check settings store: {error}"))?
    {
        return Ok(false);
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read settings store: {error}"))?;
    serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&bytes)
        .map_err(|error| format!("Invalid settings store: {error}"))?;
    Ok(true)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub server_url: String,
    pub access_token: String,
    pub webhook_id: Option<String>,
    pub device_id: String,
    pub update_interval: u64,
    pub language: String,
    pub enabled_sensors: HashMap<String, bool>,
    #[serde(default)]
    pub sensor_identity_map: HashMap<String, String>,
    #[serde(default)]
    pub legacy_gpu_aliases: HashMap<String, Vec<String>>,
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
            sensor_identity_map: HashMap::new(),
            legacy_gpu_aliases: HashMap::new(),
            autostart: false,
        }
    }
}

impl AppSettings {
    pub fn save_identity_map(
        &mut self,
        app: &AppHandle,
        identity_map: HashMap<String, String>,
    ) -> Result<(), String> {
        let store = app
            .store_builder(STORE_PATH)
            .disable_auto_save()
            .build()
            .map_err(|error| error.to_string())?;
        let previous = store.get("sensor_identity_map");
        store.set(
            "sensor_identity_map",
            serde_json::to_value(&identity_map).map_err(|error| error.to_string())?,
        );
        if let Err(error) = persist_entries(app, store.entries()) {
            match previous {
                Some(value) => store.set("sensor_identity_map", value),
                None => {
                    store.delete("sensor_identity_map");
                }
            }
            return Err(error);
        }
        self.sensor_identity_map = identity_map;
        Ok(())
    }

    /// Load settings from the Tauri store
    pub fn load(app: &AppHandle) -> Result<Self, String> {
        let path = tauri_plugin_store::resolve_store_path(app, STORE_PATH)
            .map_err(|error| format!("Could not locate settings store: {error}"))?;
        let file_exists = verify_settings_document(&path)?;
        let store = app
            .store_builder(STORE_PATH)
            .disable_auto_save()
            .build()
            .map_err(|error| format!("Could not load settings store: {error}"))?;
        if file_exists {
            store
                .reload_ignore_defaults()
                .map_err(|error| format!("Could not read settings store: {error}"))?;
        }

        let server_url = store
            .get("server_url")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        let legacy_token = store
            .get("access_token")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        if !legacy_token.is_empty() && server_url.is_empty() {
            return Err("Saved token has no server URL; settings require repair".into());
        }

        let webhook_id = store
            .get("webhook_id")
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        let existing_device_id = store
            .get("device_id")
            .and_then(|v| v.as_str().filter(|id| !id.is_empty()).map(str::to_owned));
        let device_id = match existing_device_id {
            Some(id) => id,
            None if !store.keys().is_empty() => {
                return Err("Existing settings have no valid device identity".into());
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                store.set("device_id", serde_json::json!(id));
                persist_entries(app, store.entries())
                    .map_err(|error| format!("Could not save new device identity: {error}"))?;
                id
            }
        };

        // Identity metadata must be valid before any credential migration can
        // write to disk. Replacing a damaged map would risk reusing old IDs.
        let enabled_sensors = parse_saved_map(store.get("enabled_sensors"), "enabled_sensors")?;
        let sensor_identity_map =
            parse_saved_map(store.get("sensor_identity_map"), "sensor_identity_map")?;
        let legacy_gpu_aliases =
            parse_saved_map(store.get("legacy_gpu_aliases"), "legacy_gpu_aliases")?;

        let access_token = if server_url.is_empty() && legacy_token.is_empty() {
            String::new()
        } else {
            token_entry(&device_id).and_then(|entry| match entry.get_password() {
                Ok(raw) => match token_for_server(&raw, &server_url) {
                    Some(token) => Ok(token),
                    None if !legacy_token.is_empty() => {
                        let payload = encoded_token(&server_url, &legacy_token)?;
                        entry.set_password(&payload).map_err(|error| {
                            format!("Could not migrate token to system credential store: {error}")
                        })?;
                        Ok(legacy_token.clone())
                    }
                    None => Ok(String::new()),
                },
                Err(keyring::Error::NoEntry)
                    if !legacy_token.is_empty() && !server_url.is_empty() =>
                {
                    let payload = encoded_token(&server_url, &legacy_token)?;
                    entry.set_password(&payload).map_err(|error| {
                        format!("Could not migrate token to system credential store: {error}")
                    })?;
                    Ok(legacy_token.clone())
                }
                Err(keyring::Error::NoEntry) => Ok(String::new()),
                Err(error) => Err(format!("Could not read system credential store: {error}")),
            })?
        };
        if !legacy_token.is_empty() {
            store.delete("access_token");
            if let Err(error) = persist_entries(app, store.entries()) {
                store.set("access_token", serde_json::json!(legacy_token));
                return Err(format!("Could not remove legacy plaintext token: {error}"));
            }
        }

        let update_interval = store
            .get("update_interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        let language = store
            .get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "en".to_string());

        let autostart = store
            .get("autostart")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(Self {
            server_url,
            access_token,
            webhook_id,
            device_id,
            update_interval,
            language,
            enabled_sensors,
            sensor_identity_map,
            legacy_gpu_aliases,
            autostart,
        })
    }

    /// Save settings to the Tauri store
    pub fn save(&self, app: &AppHandle) -> Result<(), String> {
        let store = app
            .store_builder(STORE_PATH)
            .disable_auto_save()
            .build()
            .map_err(|error| error.to_string())?;
        let previous_settings = store.entries();
        let entry = token_entry(&self.device_id)?;
        let previous = match entry.get_password() {
            Ok(raw) => Some(raw),
            Err(keyring::Error::NoEntry) => None,
            Err(error) => return Err(format!("Could not read system credential store: {error}")),
        };
        let desired = encoded_token(&self.server_url, &self.access_token)?;
        let credential_changed = previous.as_deref() != Some(desired.as_str());
        if credential_changed {
            entry.set_password(&desired).map_err(|error| {
                format!("Could not save token in system credential store: {error}")
            })?;
        }
        store.set("server_url", serde_json::json!(self.server_url));
        store.delete("access_token");
        store.set("webhook_id", serde_json::json!(self.webhook_id));
        store.set("device_id", serde_json::json!(self.device_id));
        store.set("update_interval", serde_json::json!(self.update_interval));
        store.set("language", serde_json::json!(self.language));
        store.set(
            "enabled_sensors",
            serde_json::to_value(&self.enabled_sensors).unwrap_or_default(),
        );
        store.set(
            "sensor_identity_map",
            serde_json::to_value(&self.sensor_identity_map).unwrap_or_default(),
        );
        store.set(
            "legacy_gpu_aliases",
            serde_json::to_value(&self.legacy_gpu_aliases).unwrap_or_default(),
        );
        store.set("autostart", serde_json::json!(self.autostart));
        store.delete("cpu_temperature_provider");
        if let Err(error) = persist_entries(app, store.entries()) {
            store.clear();
            for (key, value) in previous_settings {
                store.set(key, value);
            }
            if credential_changed {
                let rollback = match previous {
                    Some(raw) => entry.set_password(&raw),
                    None => entry.delete_credential(),
                };
                if let Err(rollback_error) = rollback {
                    log::error!("Could not restore credential after settings save failure: {rollback_error}");
                }
            }
            return Err(error);
        }
        Ok(())
    }

    /// Drop the saved webhook_id and persist. Used when HA reports the
    /// webhook is gone (404/410) so the next startup correctly shows the
    /// setup screen instead of looping on a dead webhook.
    pub fn clear_webhook(&mut self, app: &AppHandle) -> Result<(), String> {
        if self.webhook_id.is_none() {
            return Ok(());
        }
        let previous = self.webhook_id.take();
        if let Err(error) = self.save(app) {
            self.webhook_id = previous;
            return Err(error);
        }
        Ok(())
    }
}

fn parse_saved_map<T: serde::de::DeserializeOwned>(
    value: Option<serde_json::Value>,
    name: &str,
) -> Result<HashMap<String, T>, String> {
    value
        .map(|value| {
            serde_json::from_value(value).map_err(|error| format!("Invalid {name}: {error}"))
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_provider_choice_does_not_break_settings_load() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value["cpu_temperature_provider"] = serde_json::json!(false);
        value["webhook_id"] = serde_json::json!("existing-webhook");
        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.webhook_id.as_deref(), Some("existing-webhook"));
    }

    #[test]
    fn stored_token_is_bound_to_its_server() {
        let encoded = encoded_token("https://ha.example", "secret").unwrap();
        assert_eq!(
            token_for_server(&encoded, "https://ha.example"),
            Some("secret".into())
        );
        assert_eq!(token_for_server(&encoded, "https://other.example"), None);
        assert_eq!(token_for_server("broken", "https://ha.example"), None);
    }

    #[test]
    fn corrupt_sensor_identity_map_cannot_be_silently_replaced() {
        let corrupted = serde_json::json!({"gpu:id": ["wrong type"]});
        let result = parse_saved_map::<String>(Some(corrupted), "sensor_identity_map");
        assert!(result.is_err());
        let absent = parse_saved_map::<String>(None, "sensor_identity_map").unwrap();
        assert!(absent.is_empty());
    }

    #[test]
    fn settings_file_replace_preserves_old_data_when_replace_fails() {
        let folder =
            std::env::temp_dir().join(format!("ha-settings-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&folder).unwrap();
        let path = folder.join("settings.json");
        write_settings_document(&path, vec![("device_id".into(), serde_json::json!("old"))])
            .unwrap();
        write_settings_document(&path, vec![("device_id".into(), serde_json::json!("new"))])
            .unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["device_id"], "new");
        assert!(verify_settings_document(&path).unwrap());

        fs::write(&path, b"{corrupted").unwrap();
        assert!(verify_settings_document(&path).is_err());

        let directory_target = folder.join("cannot-replace-directory");
        fs::create_dir(&directory_target).unwrap();
        assert!(write_settings_document(&directory_target, vec![]).is_err());
        assert!(directory_target.is_dir());
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 2);
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&directory_target).unwrap();
        fs::remove_dir(&folder).unwrap();
    }

    #[test]
    #[ignore = "writes and removes a disposable credential in the OS vault"]
    fn os_credential_store_roundtrip() {
        let account = format!("test-{}", uuid::Uuid::new_v4());
        let entry = token_entry(&account).unwrap();
        entry.set_password("disposable-test-secret").unwrap();
        let read = entry.get_password();
        let cleanup = entry.delete_credential();
        assert_eq!(read.unwrap(), "disposable-test-secret");
        cleanup.unwrap();
    }
}

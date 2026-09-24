use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;

use crate::ha_client::normalize_server_url;
use crate::sensors::collector::SensorListItem;
use crate::AppState;
use reqwest::Client;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsResponse {
    pub server_url: String,
    pub has_access_token: bool,
    pub webhook_id_preview: Option<String>,
    pub device_id: String,
    pub update_interval: u64,
    pub language: String,
    pub enabled_sensors: HashMap<String, bool>,
    pub autostart: bool,
    pub cpu_temperature_provider: bool,
    pub cpu_temperature_provider_supported: bool,
    pub is_registered: bool,
}

/// Get current settings
#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<SettingsResponse, String> {
    let settings = state.settings.lock().await;
    let is_registered = *state.is_registered.lock().await;

    Ok(SettingsResponse {
        server_url: settings.server_url.clone(),
        has_access_token: !settings.access_token.is_empty(),
        webhook_id_preview: settings.webhook_id.as_ref().map(|id| {
            let prefix: String = id.chars().take(8).collect();
            format!("{prefix}…")
        }),
        device_id: settings.device_id.clone(),
        update_interval: settings.update_interval,
        language: settings.language.clone(),
        enabled_sensors: settings.enabled_sensors.clone(),
        autostart: settings.autostart,
        cpu_temperature_provider: settings.cpu_temperature_provider,
        cpu_temperature_provider_supported: cfg!(windows),
        is_registered,
    })
}

/// Save settings and reinitialize connection
#[tauri::command]
#[allow(
    clippy::too_many_arguments,
    reason = "Preserve named IPC fields for existing setup callers"
)]
pub async fn save_settings(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    server_url: String,
    access_token: String,
    update_interval: u64,
    language: String,
    autostart: bool,
    cpu_temperature_provider: Option<bool>,
    enabled_sensors: Option<HashMap<String, bool>>,
) -> Result<(), String> {
    let server_url = normalize_server_url(&server_url);
    let submitted_token = access_token.trim();
    let parsed_url =
        url::Url::parse(&server_url).map_err(|_| "Enter a valid Home Assistant URL".to_string())?;
    if !matches!(parsed_url.scheme(), "http" | "https")
        || parsed_url.host_str().is_none()
        || !parsed_url.username().is_empty()
        || parsed_url.password().is_some()
        || parsed_url.query().is_some()
        || parsed_url.fragment().is_some()
    {
        return Err("Home Assistant URL must use HTTP(S) without embedded credentials".into());
    }
    if !(5..=3600).contains(&update_interval) {
        return Err("Update interval must be between 5 and 3600 seconds".into());
    }

    let mut settings = state.settings.lock().await;
    let access_token = resolve_access_token(&settings, &server_url, submitted_token)?;
    let url_changed = settings.server_url != server_url;
    let token_changed = settings.access_token != access_token;
    let preferences_changed = enabled_sensors
        .as_ref()
        .is_some_and(|preferences| preferences != &settings.enabled_sensors);
    let mut next = settings.clone();
    next.server_url = server_url.clone();
    next.access_token = access_token.clone();
    next.update_interval = update_interval;
    next.language = language;
    next.autostart = autostart;
    if let Some(enabled) = cpu_temperature_provider {
        next.cpu_temperature_provider = enabled && cfg!(windows);
    }
    if let Some(preferences) = enabled_sensors {
        next.enabled_sensors = preferences;
    }
    let provider_changed = next.cpu_temperature_provider != settings.cpu_temperature_provider;
    if url_changed || token_changed {
        next.webhook_id = None;
    }
    let autostart_changed = next.autostart != settings.autostart;
    if autostart_changed {
        let result = if next.autostart {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        };
        result.map_err(|e| format!("Could not update system autostart: {e}"))?;
    }
    if let Err(e) = next.save(&app) {
        if autostart_changed {
            let _ = if settings.autostart {
                app.autolaunch().enable()
            } else {
                app.autolaunch().disable()
            };
        }
        log::error!("[HA] Save settings failed: {}", e);
        return Err(e);
    }
    *settings = next;
    {
        let mut collector = state.collector.lock().await;
        collector.set_temperature_provider(settings.cpu_temperature_provider);
        collector.set_enabled_sensors(settings.enabled_sensors.clone());
    }

    // If server URL or token changed, re-register
    {
        let mut ha_client = state.ha_client.lock().await;
        ha_client.set_update_interval(settings.update_interval);
        if url_changed || token_changed {
            ha_client.update_config(server_url, access_token);
            ha_client.clear_webhook_id();
            *state.is_registered.lock().await = false;
        }
    }

    drop(settings);
    if (preferences_changed || provider_changed) && !url_changed && !token_changed {
        sync_all_sensors(state.inner().clone(), &app).await?;
    }

    Ok(())
}

async fn sync_all_sensors(state: Arc<AppState>, app: &tauri::AppHandle) -> Result<(), String> {
    if !*state.is_registered.lock().await {
        return Ok(());
    }
    let sensors = crate::collect_snapshot(state.clone(), true).await?;
    let client = state.ha_client.lock().await;
    let result = async {
        client.register_sensors(&sensors).await?;
        client.update_sensors(&sensors, "all").await
    }
    .await;
    if let Err(error) = result {
        let reason = error.to_string();
        let failed_webhook = client.webhook_id().unwrap_or_default().to_owned();
        drop(client);
        if reason.contains("404") || reason.contains("410") {
            mark_unregistered(&state, app, &failed_webhook, &reason).await;
        }
        return Err(format!(
            "Settings saved, but Home Assistant sensor sync failed: {reason}"
        ));
    }
    Ok(())
}

fn resolve_access_token(
    current: &crate::settings::AppSettings,
    server_url: &str,
    submitted_token: &str,
) -> Result<String, String> {
    if !submitted_token.is_empty() {
        return Ok(submitted_token.to_string());
    }
    if current.server_url == server_url && !current.access_token.is_empty() {
        return Ok(current.access_token.clone());
    }
    Err("Enter a token for this Home Assistant server".into())
}

#[cfg(test)]
mod settings_contract_tests {
    use super::resolve_access_token;
    use crate::settings::AppSettings;

    #[test]
    fn blank_token_reuses_only_for_same_server() {
        let current = AppSettings {
            server_url: "https://ha.example".into(),
            access_token: "saved-secret".into(),
            ..AppSettings::default()
        };
        assert_eq!(
            resolve_access_token(&current, "https://ha.example", "").unwrap(),
            "saved-secret"
        );
        assert!(resolve_access_token(&current, "https://other.example", "").is_err());
        assert_eq!(
            resolve_access_token(&current, "https://other.example", "new-secret").unwrap(),
            "new-secret"
        );
    }
}

/// Register device with HA
#[tauri::command]
pub async fn register_device(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let _guard = state.registration_lock.lock().await;
    register_device_inner(&state, &app).await
}

pub(crate) async fn register_device_inner(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    let all_sensors = crate::collect_snapshot(state.clone(), true).await?;
    let mut settings = state.settings.lock().await;
    let mut ha_client = state.ha_client.lock().await;

    match crate::registration::register_device(&mut settings, &mut ha_client, &all_sensors, app)
        .await
    {
        Ok(_) => (),
        Err(e) => {
            log::error!("[HA] Registration failed: {}", e);
            return Err(e);
        }
    };

    *state.is_registered.lock().await = true;

    Ok(())
}

/// Get list of all sensors
#[tauri::command]
pub async fn get_sensor_list(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SensorListItem>, String> {
    let collector = state.collector.lock().await;
    Ok(collector.get_sensor_list())
}

/// Force immediate sensor update
#[tauri::command]
pub async fn update_sensors_now(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let is_registered = *state.is_registered.lock().await;
    if !is_registered {
        log::error!("[HA] update_sensors_now: device not registered");
        return Err("Device not registered".to_string());
    }

    let sensor_data = crate::collect_snapshot(state.inner().clone(), false).await?;

    let ha_client = state.ha_client.lock().await;
    if let Err(e) = ha_client.update_sensors(&sensor_data, "dynamic").await {
        let err_str = e.to_string();
        log::error!("[HA] Update sensors failed: {}", err_str);
        let failed_webhook = ha_client.webhook_id().unwrap_or_default().to_owned();
        drop(ha_client);
        if err_str.contains("404") || err_str.contains("410") {
            mark_unregistered(&state, &app, &failed_webhook, &err_str).await;
        }
        return Err(format!("Update failed: {}", err_str));
    }

    Ok(())
}

/// Drop the webhook locally and trigger a fresh registration with HA.
/// Used by the "Reconnect" button in settings when the saved webhook is dead
/// or the user wants to force a clean re-registration.
#[tauri::command]
pub async fn reregister_device(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let _guard = state.registration_lock.lock().await;
    // Drop in-memory + persisted webhook so register_device starts fresh.
    let (server_url, access_token) = {
        let mut settings = state.settings.lock().await;
        settings.clear_webhook(&app)?;
        (settings.server_url.clone(), settings.access_token.clone())
    };
    {
        let mut ha_client = state.ha_client.lock().await;
        ha_client.update_config(server_url, access_token);
    }
    *state.is_registered.lock().await = false;

    register_device_inner(&state, &app).await
}

/// Connection status reported back to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ConnectionStatus {
    /// Integration reachable and webhook (if any) is alive.
    Ok,
    /// No webhook_id stored locally — user has never registered.
    NotRegistered,
    /// Integration reachable but the stored webhook returns 404/410 — HA
    /// forgot about us. Caller should drop the webhook and offer re-register.
    WebhookDead,
    /// Cannot reach the HA Desktop App integration at all (404 on ping,
    /// network error, wrong URL, reverse proxy misconfigured). Caller should
    /// NOT clear the webhook — this may be transient (HA restarting).
    Unreachable { reason: String },
    /// 401 on a request — the access token is invalid or expired.
    TokenInvalid,
}

/// Verify the integration is reachable and that any stored webhook is alive.
/// Called by the frontend on startup so we can show a meaningful UI instead
/// of "everything looks fine" followed by a silent loop of update errors.
#[tauri::command]
pub async fn check_connection(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<ConnectionStatus, String> {
    let (has_token, has_url, has_webhook) = {
        let settings = state.settings.lock().await;
        (
            !settings.access_token.is_empty(),
            !settings.server_url.is_empty(),
            settings.webhook_id.is_some(),
        )
    };

    if !has_url || !has_token {
        return Ok(ConnectionStatus::NotRegistered);
    }

    let ha_client = state.ha_client.lock().await;

    if let Err(e) = ha_client.check_integration_reachable().await {
        return Ok(ConnectionStatus::Unreachable {
            reason: e.to_string(),
        });
    }

    if !has_webhook {
        return Ok(ConnectionStatus::NotRegistered);
    }

    match ha_client.check_webhook().await {
        Ok(()) => Ok(ConnectionStatus::Ok),
        Err(error) => {
            let reason = error.to_string();
            if reason.contains("HTTP 404") || reason.contains("HTTP 410") {
                let failed_webhook = ha_client.webhook_id().unwrap_or_default().to_owned();
                drop(ha_client);
                mark_unregistered(
                    &state,
                    &app,
                    &failed_webhook,
                    "startup health check: webhook dead",
                )
                .await;
                Ok(ConnectionStatus::WebhookDead)
            } else {
                Ok(ConnectionStatus::Unreachable { reason })
            }
        }
    }
}

/// Shared with sensor_update_loop in lib.rs.
pub async fn mark_unregistered(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    failed_webhook: &str,
    reason: &str,
) {
    let _guard = state.registration_lock.lock().await;
    let mut settings = state.settings.lock().await;
    if settings.webhook_id.as_deref() != Some(failed_webhook) {
        log::debug!("Ignoring failure from a previous webhook registration");
        return;
    }
    log::warn!("Marking device unregistered: {}", reason);
    if let Err(e) = settings.clear_webhook(app) {
        log::error!("Failed to persist unregistered state: {}", e);
    }
    drop(settings);
    *state.is_registered.lock().await = false;
    let _ = app.emit("registration-lost", reason.to_string());
}

/// Toggle a sensor on/off
#[tauri::command]
pub async fn toggle_sensor(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    sensor_id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = state.settings.lock().await;
    let mut next = settings.clone();
    next.enabled_sensors.insert(sensor_id, enabled);
    if let Err(e) = next.save(&app) {
        log::error!("[HA] Save settings failed: {}", e);
        return Err(e);
    }
    *settings = next;

    // Update collector
    let mut collector = state.collector.lock().await;
    collector.set_enabled_sensors(settings.enabled_sensors.clone());
    drop(collector);
    drop(settings);
    sync_all_sensors(state.inner().clone(), &app).await
}

/// Get current language
#[tauri::command]
pub async fn get_current_language(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let settings = state.settings.lock().await;
    Ok(settings.language.clone())
}

/// Open the HA dashboard as a child webview inside the main window.
/// Only injects hassTokens in localStorage (no externalApp, which would
/// hijack the auth flow and break it for long-lived tokens).
pub fn open_dashboard_view<R: tauri::Runtime, M: Manager<R>>(
    manager: &M,
    server_url: &str,
    token: &str,
) -> Result<(), String> {
    let base_url = server_url.trim_end_matches('/');
    log::info!("[Dashboard] Opening configured Home Assistant view");

    // Close existing HA child webview if any
    if let Some(existing) = manager.get_webview("ha-view") {
        log::info!("[Dashboard] Closing existing ha-view");
        let _ = existing.close();
    }

    let url: url::Url = base_url
        .parse()
        .map_err(|e: url::ParseError| format!("Invalid dashboard URL: {e}"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Dashboard URL must be an HTTP(S) URL without credentials".into());
    }
    let origin = url.origin().ascii_serialization();
    let allowed_origin = origin.clone();
    let escaped_origin = serde_json::to_string(&origin).map_err(|e| e.to_string())?;
    let escaped_token = serde_json::to_string(token).map_err(|e| e.to_string())?;
    let escaped_url = serde_json::to_string(base_url).map_err(|e| e.to_string())?;

    // Initialization script: set hassTokens in localStorage BEFORE HA frontend loads.
    // Do NOT set window.externalApp — it hijacks auth and breaks long-lived tokens.
    let init_script = format!(
        r#"
        (function() {{
            try {{
                if (window.top !== window.self || location.origin !== {escaped_origin}) return;
                localStorage.setItem("hassTokens", JSON.stringify({{
                    hassUrl: {escaped_url},
                    access_token: {escaped_token},
                    token_type: "Bearer",
                    expires_in: 315360000,
                    refresh_token: "",
                    expires: Date.now() + 315360000000
                }}));
            }} catch(e) {{
                console.warn("[HA Companion] Failed to inject hassTokens:", e);
            }}
        }})();
        "#
    );

    let window = manager.get_window("main").ok_or("Main window not found")?;
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let phys = window.inner_size().map_err(|e| e.to_string())?;
    let logical = phys.to_logical::<f64>(scale);

    log::info!(
        "[Dashboard] Adding child webview {}x{}",
        logical.width,
        logical.height
    );

    window
        .add_child(
            tauri::webview::WebviewBuilder::new("ha-view", tauri::WebviewUrl::External(url))
                .initialization_script(&init_script)
                .on_navigation(move |next| next.origin().ascii_serialization() == allowed_origin)
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                .auto_resize(),
            tauri::LogicalPosition::new(0.0, 0.0),
            logical,
        )
        .map_err(|e| format!("Failed to create HA webview: {}", e))?;

    log::info!("[Dashboard] Dashboard view created");
    Ok(())
}

/// Remove the HA dashboard child webview (to reveal the main HTML underneath).
pub fn close_dashboard_view<R: tauri::Runtime, M: Manager<R>>(manager: &M) {
    if let Some(wv) = manager.get_webview("ha-view") {
        let _ = wv.close();
        log::info!("[Dashboard] Closed ha-view");
    }
}

/// Tauri command: open (or re-open) the HA dashboard view
#[tauri::command]
pub async fn load_dashboard(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let settings = state.settings.lock().await;
    let server_url = settings.server_url.clone();
    let token = settings.access_token.clone();
    drop(settings);

    open_dashboard_view(&app, &server_url, &token)
}

/// Tauri command: close the HA dashboard view (used when opening settings)
#[tauri::command]
pub async fn hide_dashboard(app: tauri::AppHandle) -> Result<(), String> {
    close_dashboard_view(&app);
    Ok(())
}

/// Get this machine's public (outbound) IP. Use this in your reverse proxy allowlist.
#[tauri::command]
pub async fn get_my_public_ip() -> Result<String, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    let body = client
        .get("https://api.ipify.org")
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    Ok(body.trim().to_string())
}

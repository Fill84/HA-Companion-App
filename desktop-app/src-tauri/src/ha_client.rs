use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::sensors::collector::SensorValue;

/// Normalize server URL: trim whitespace and strip trailing /api so we never build double /api/api/ paths.
pub fn normalize_server_url(url: &str) -> String {
    let s = url.trim().trim_end_matches('/');
    s.strip_suffix("/api")
        .map(|u| u.trim_end_matches('/'))
        .unwrap_or(s)
        .to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationRequest {
    pub device_id: String,
    pub device_name: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub app_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationResponse {
    pub success: bool,
    pub webhook_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct WebhookPayload {
    #[serde(rename = "type")]
    command_type: String,
    data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
struct SensorRegistration {
    sensor_unique_id: String,
    sensor_name: String,
    sensor_type: String,
    sensor_state: serde_json::Value,
    sensor_device_class: Option<String>,
    sensor_unit_of_measurement: Option<String>,
    sensor_state_class: Option<String>,
    sensor_icon: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SensorStateUpdate {
    sensor_unique_id: String,
    sensor_state: serde_json::Value,
    sensor_attributes: serde_json::Value,
    sensor_icon: Option<String>,
}

pub struct HaClient {
    client: Client,
    server_url: String,
    access_token: String,
    webhook_id: Option<String>,
}

impl HaClient {
    pub fn new(server_url: String, access_token: String, webhook_id: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true) // Allow self-signed certs for local HA
            .build()
            .unwrap_or_default();

        Self {
            client,
            server_url: normalize_server_url(&server_url),
            access_token: access_token.trim().to_string(),
            webhook_id,
        }
    }

    pub fn update_config(&mut self, server_url: String, access_token: String) {
        self.server_url = normalize_server_url(&server_url);
        self.access_token = access_token.trim().to_string();
    }

    /// Base URL for API calls (no trailing slash, no trailing /api)
    fn base_url(&self) -> &str {
        self.server_url.trim_end_matches('/')
    }

    /// Check if the Desktop App integration is reachable (GET /api/desktop_app/ping, no auth).
    /// Returns Ok(()) if reachable, Err with message if 404 or connection failed.
    pub async fn check_integration_reachable(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/desktop_app/ping", self.base_url());
        log::info!("[HA] GET {}", url);
        let response = self.client.get(&url).send().await.map_err(|e| {
            log::error!("[HA] Ping request failed (connection/network): {}", e);
            e
        })?;
        let status = response.status();
        log::info!("[HA] ping response: {}", status);
        if status.as_u16() == 404 {
            let msg = "404: Desktop App integration not loaded or URL not reachable. \
                Install the integration in HA, restart HA, and ensure the server URL is correct (base URL without /api). \
                If using a reverse proxy, ensure /api/ is forwarded to Home Assistant.";
            log::error!("[HA] Ping failed: {} - URL was: {}", msg, url);
            return Err(msg.into());
        }
        if !response.status().is_success() {
            let err = format!("Server returned {} for {}", response.status(), url);
            log::error!("[HA] {}", err);
            return Err(err.into());
        }
        Ok(())
    }

    pub fn set_webhook_id(&mut self, webhook_id: String) {
        self.webhook_id = Some(webhook_id);
    }

    pub fn webhook_id(&self) -> Option<&str> {
        self.webhook_id.as_deref()
    }

    /// Register device with HA
    pub async fn register_device(
        &self,
        registration: &RegistrationRequest,
    ) -> Result<RegistrationResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/desktop_app/registrations", self.base_url());
        log::info!("[HA] POST {}", url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.access_token.trim()))
            .header("Content-Type", "application/json")
            .json(registration)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        log::info!("[HA] registration response: {} body_len={}", status, body.len());
        if !body.is_empty() && body.len() <= 500 {
            log::info!("[HA] registration body: {}", body);
        }
        if !status.is_success() {
            if status.as_u16() == 404 {
                let msg = "404 Not Found: Desktop App integration not loaded or URL not reachable.";
                log::error!("[HA] Registration {} - URL: {} body: {}", msg, url, body);
                return Err(
                    "404 Not Found: Desktop App integration not loaded or URL not reachable. \
                    Check: (1) Integration installed in HA and HA restarted, (2) Server URL is the HA base URL without /api, (3) Reverse proxy forwards /api/ to HA."
                        .into(),
                );
            }
            if status.as_u16() == 401 {
                log::error!("[HA] Registration 401 Unauthorized - URL: {}", url);
                return Err("401 Unauthorized: Invalid or expired access token.".into());
            }
            log::error!("[HA] Registration failed {} - URL: {} body: {}", status, url, body);
            return Err(format!("Registration failed ({}): {}", status, body).into());
        }

        let result: RegistrationResponse = serde_json::from_str(&body).map_err(|e| {
            let err = format!("Invalid JSON response: {} body: {}", e, body);
            log::error!("[HA] {}", err);
            err
        })?;
        Ok(result)
    }

    /// Register a single sensor with HA
    pub async fn register_sensor(
        &self,
        sensor: &SensorValue,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let webhook_id = self
            .webhook_id
            .as_ref()
            .ok_or("No webhook_id configured")?;

        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);

        let payload = WebhookPayload {
            command_type: "register_sensor".to_string(),
            data: serde_json::to_value(SensorRegistration {
                sensor_unique_id: sensor.unique_id.clone(),
                sensor_name: sensor.name.clone(),
                sensor_type: sensor.sensor_type.clone(),
                sensor_state: sensor.state.clone(),
                sensor_device_class: sensor.device_class.clone(),
                sensor_unit_of_measurement: sensor.unit_of_measurement.clone(),
                sensor_state_class: sensor.state_class.clone(),
                sensor_icon: sensor.icon.clone(),
            })?,
        };

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        let status = response.status();
        if status.as_u16() == 410 {
            log::error!("[HA] Sensor registration 410 Gone - webhook expired");
            return Err("410 Gone - webhook expired".into());
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            log::error!("[HA] Sensor registration failed {} - URL: {} body: {}", status, url, body);
            return Err(format!("Sensor registration failed ({}): {}", status, body).into());
        }

        Ok(())
    }

    /// Register multiple sensors with HA
    pub async fn register_sensors(
        &self,
        sensors: &[SensorValue],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for sensor in sensors {
            self.register_sensor(sensor).await?;
        }
        Ok(())
    }

    /// Batch update sensor states
    pub async fn update_sensors(
        &self,
        sensors: &[SensorValue],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if sensors.is_empty() {
            return Ok(());
        }

        let webhook_id = self
            .webhook_id
            .as_ref()
            .ok_or("No webhook_id configured")?;

        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);

        let sensor_updates: Vec<SensorStateUpdate> = sensors
            .iter()
            .map(|s| SensorStateUpdate {
                sensor_unique_id: s.unique_id.clone(),
                sensor_state: s.state.clone(),
                sensor_attributes: serde_json::to_value(&s.attributes).unwrap_or_default(),
                sensor_icon: s.icon.clone(),
            })
            .collect();

        let payload = WebhookPayload {
            command_type: "update_sensor_states".to_string(),
            data: serde_json::json!({
                "sensors": sensor_updates
            }),
        };

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        let status = response.status();
        if status.as_u16() == 410 {
            log::error!("[HA] Sensor update 410 Gone - webhook expired, URL: {}", url);
            return Err("410 Gone - webhook expired".into());
        }
        if status.as_u16() == 404 {
            log::error!("[HA] Sensor update 404 - webhook not found, URL: {}", url);
            return Err(
                "404 Not Found: Webhook not found. Device may not be registered yet, or the Desktop App integration was removed/restarted. Try re-registering in the app.".into(),
            );
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            log::error!("[HA] Sensor update failed {} - URL: {} body: {}", status, url, body);
            return Err(format!("Sensor update failed ({}): {}", status, body).into());
        }

        Ok(())
    }

    /// Check if the webhook is still valid
    pub async fn check_webhook(&self) -> bool {
        let webhook_id = match &self.webhook_id {
            Some(id) => id,
            None => return false,
        };

        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);

        // Send a minimal payload to check if webhook exists
        let payload = WebhookPayload {
            command_type: "update_sensor_states".to_string(),
            data: serde_json::json!({"sensors": []}),
        };

        match self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
}

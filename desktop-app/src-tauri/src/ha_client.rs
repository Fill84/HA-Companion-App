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
    protocol_version: u8,
    #[serde(rename = "type")]
    command_type: String,
    data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
struct SensorRegistration {
    #[serde(flatten)]
    metadata: SensorMetadata,
    sensor_state: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SensorMetadata {
    sensor_unique_id: String,
    sensor_name: String,
    sensor_type: String,
    sensor_device_class: Option<String>,
    sensor_unit_of_measurement: Option<String>,
    sensor_state_class: Option<String>,
    sensor_icon: Option<String>,
    update_at_interval: bool,
}

impl From<&SensorValue> for SensorMetadata {
    fn from(sensor: &SensorValue) -> Self {
        Self {
            sensor_unique_id: sensor.unique_id.clone(),
            sensor_name: sensor.name.clone(),
            sensor_type: sensor.sensor_type.clone(),
            sensor_device_class: sensor.device_class.clone(),
            sensor_unit_of_measurement: sensor.unit_of_measurement.clone(),
            sensor_state_class: sensor.state_class.clone(),
            sensor_icon: sensor.icon.clone(),
            update_at_interval: sensor.update_at_interval,
        }
    }
}

fn sensor_descriptors(sensors: &[SensorValue]) -> Vec<SensorMetadata> {
    let mut descriptors: Vec<_> = sensors.iter().map(SensorMetadata::from).collect();
    descriptors.sort_unstable_by(|left, right| left.sensor_unique_id.cmp(&right.sensor_unique_id));
    descriptors
}

fn changed_sensors<'a>(
    sensors: &'a [SensorValue],
    registered: Option<&[SensorMetadata]>,
) -> Vec<&'a SensorValue> {
    sensors
        .iter()
        .filter(|sensor| {
            let descriptor = SensorMetadata::from(*sensor);
            !registered.is_some_and(|known| {
                known
                    .binary_search_by(|item| {
                        item.sensor_unique_id.cmp(&descriptor.sensor_unique_id)
                    })
                    .ok()
                    .is_some_and(|index| known[index] == descriptor)
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
struct SensorStateUpdate {
    sensor_unique_id: String,
    sensor_state: serde_json::Value,
    sensor_attributes: serde_json::Value,
    sensor_icon: Option<String>,
}

fn registration_payload(sensor: &SensorValue) -> serde_json::Result<WebhookPayload> {
    Ok(WebhookPayload {
        protocol_version: 1,
        command_type: "register_sensor".to_string(),
        data: serde_json::to_value(SensorRegistration {
            metadata: SensorMetadata::from(sensor),
            sensor_state: sensor.state.clone(),
        })?,
    })
}

fn sensor_update_payload(
    sensors: &[SensorValue],
    update_interval: u64,
    snapshot_scope: &str,
) -> WebhookPayload {
    let sensor_updates: Vec<SensorStateUpdate> = sensors
        .iter()
        .map(|sensor| SensorStateUpdate {
            sensor_unique_id: sensor.unique_id.clone(),
            sensor_state: sensor.state.clone(),
            sensor_attributes: serde_json::to_value(&sensor.attributes).unwrap_or_default(),
            sensor_icon: sensor.icon.clone(),
        })
        .collect();
    WebhookPayload {
        protocol_version: 1,
        command_type: "update_sensor_states".to_string(),
        data: serde_json::json!({
            "sensors": sensor_updates,
            "update_interval": update_interval,
            "snapshot_scope": snapshot_scope,
        }),
    }
}

pub struct HaClient {
    client: Client,
    server_url: String,
    access_token: String,
    webhook_id: Option<String>,
    update_interval: u64,
    registered_descriptors: Option<Vec<SensorMetadata>>,
}

impl HaClient {
    fn webhook_transport_error(error: reqwest::Error) -> String {
        if error.is_timeout() {
            "Webhook request timed out".into()
        } else if error.is_connect() {
            "Webhook connection failed".into()
        } else {
            "Webhook transport failed".into()
        }
    }

    async fn require_webhook_ack(
        response: reqwest::Response,
        command: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let status = response.status();
        if !status.is_success() {
            return Err(format!("{command} returned HTTP {status}").into());
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|_| format!("{command} did not return a JSON acknowledgement"))?;
        if body.get("success") != Some(&serde_json::Value::Bool(true)) {
            return Err(format!("{command} was not acknowledged by Home Assistant").into());
        }
        if body.get("protocol_version").is_some()
            && body.get("protocol_version") != Some(&serde_json::json!(1))
        {
            return Err(format!("{command} returned an unsupported protocol version").into());
        }
        Ok(())
    }

    pub fn new(server_url: String, access_token: String, webhook_id: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            client,
            server_url: normalize_server_url(&server_url),
            access_token: access_token.trim().to_string(),
            webhook_id,
            update_interval: 60,
            registered_descriptors: None,
        }
    }

    pub fn update_config(&mut self, server_url: String, access_token: String) {
        let server_url = normalize_server_url(&server_url);
        let access_token = access_token.trim().to_string();
        if self.server_url != server_url || self.access_token != access_token {
            self.registered_descriptors = None;
        }
        self.server_url = server_url;
        self.access_token = access_token;
    }

    /// Base URL for API calls (no trailing slash, no trailing /api)
    fn base_url(&self) -> &str {
        self.server_url.trim_end_matches('/')
    }

    /// Check if the Desktop App integration is reachable (GET /api/desktop_app/ping, no auth).
    /// Returns Ok(()) if reachable, Err with message if 404 or connection failed.
    pub async fn check_integration_reachable(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.registration_api_available().await? {
            Ok(())
        } else {
            Err("Desktop App compatibility API is not loaded yet".into())
        }
    }

    /// The compatibility API is absent when the first desktop activates the
    /// integration after HA's HTTP router has frozen.
    pub async fn registration_api_available(
        &self,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/desktop_app/ping", self.base_url());
        log::info!("[HA] Checking integration ping endpoint");
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;
        let status = response.status();
        log::info!("[HA] ping response: {}", status);
        if status.as_u16() == 404 {
            return Ok(false);
        }
        if !response.status().is_success() {
            let err = format!("Integration ping returned {}", response.status());
            log::error!("[HA] {}", err);
            return Err(err.into());
        }
        Ok(true)
    }

    /// Create the first device through Home Assistant's administrator-only
    /// config-flow API. This does not require our custom HTTP routes to be
    /// registered before the first device exists.
    pub async fn bootstrap_device(
        &self,
        registration: &RegistrationRequest,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let flow_url = format!("{}/api/config/config_entries/flow", self.base_url());
        let response = self
            .client
            .post(&flow_url)
            .bearer_auth(&self.access_token)
            .json(&serde_json::json!({"handler": "desktop_app"}))
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;
        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(
                "The first desktop registration requires a Home Assistant administrator token"
                    .into(),
            );
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err("Invalid Home Assistant access token".into());
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err("Desktop App integration is not installed or Home Assistant has not been restarted after installation".into());
        }
        if !status.is_success() {
            return Err(
                format!("Could not start Home Assistant registration flow: HTTP {status}").into(),
            );
        }
        let flow: serde_json::Value = response.json().await?;
        if flow.get("type").and_then(serde_json::Value::as_str) != Some("form") {
            return Err("Home Assistant did not open the Desktop App registration flow".into());
        }
        let flow_id = flow
            .get("flow_id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 128)
            .ok_or("Home Assistant registration flow has no valid ID")?;
        let webhook_id = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let mut data = serde_json::to_value(registration)?;
        data["webhook_id"] = serde_json::Value::String(webhook_id.clone());
        let response = self
            .client
            .post(format!("{flow_url}/{flow_id}"))
            .bearer_auth(&self.access_token)
            .json(&data)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(
                format!("Home Assistant rejected desktop registration: HTTP {status}").into(),
            );
        }
        let result: serde_json::Value = response.json().await?;
        if result.get("type").and_then(serde_json::Value::as_str) == Some("abort")
            && result.get("reason").and_then(serde_json::Value::as_str)
                == Some("already_registered")
        {
            let existing = result
                .pointer("/description_placeholders/webhook_id")
                .and_then(serde_json::Value::as_str)
                .filter(|id| id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .ok_or("Home Assistant did not return a valid existing webhook")?;
            return Ok(existing.to_owned());
        }
        if result.get("type").and_then(serde_json::Value::as_str) != Some("create_entry") {
            let reason = result
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unexpected flow result");
            return Err(format!("Home Assistant did not register the desktop: {reason}").into());
        }
        if result
            .pointer("/result/domain")
            .and_then(serde_json::Value::as_str)
            != Some("desktop_app")
        {
            return Err("Home Assistant returned an unexpected integration entry".into());
        }
        Ok(webhook_id)
    }

    /// Check the saved token without changing device registration or its webhook.
    pub async fn check_access_token(&self) -> Result<reqwest::StatusCode, String> {
        let url = format!("{}/api/", self.base_url());
        self.client
            .get(url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map(|response| response.status())
            .map_err(Self::webhook_transport_error)
    }

    pub fn set_webhook_id(&mut self, webhook_id: String) {
        if self.webhook_id.as_deref() != Some(webhook_id.as_str()) {
            self.registered_descriptors = None;
        }
        self.webhook_id = Some(webhook_id);
    }

    pub fn set_update_interval(&mut self, seconds: u64) {
        self.update_interval = seconds.clamp(5, 3600);
    }

    pub fn clear_webhook_id(&mut self) {
        self.webhook_id = None;
        self.registered_descriptors = None;
    }

    pub fn webhook_id(&self) -> Option<&str> {
        self.webhook_id.as_deref()
    }

    pub async fn update_registration(
        &self,
        registration: &RegistrationRequest,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let webhook_id = self.webhook_id.as_ref().ok_or("No webhook_id configured")?;
        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);
        let payload = WebhookPayload {
            protocol_version: 1,
            command_type: "update_registration".into(),
            data: serde_json::json!({
                "device_name": registration.device_name,
                "os_version": registration.os_version,
                "app_version": registration.app_version,
            }),
        };
        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;
        Self::require_webhook_ack(response, "update_registration").await
    }

    /// Register device with HA
    pub async fn register_device(
        &self,
        registration: &RegistrationRequest,
    ) -> Result<RegistrationResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/desktop_app/registrations", self.base_url());
        log::info!("[HA] Registering device");

        let response = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.access_token.trim()),
            )
            .header("Content-Type", "application/json")
            .json(registration)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        log::info!(
            "[HA] registration response: {} body_len={}",
            status,
            body.len()
        );
        if !status.is_success() {
            if status.as_u16() == 404 {
                let msg = "404 Not Found: Desktop App integration not loaded or URL not reachable.";
                log::error!("[HA] Registration {}", msg);
                return Err(
                    "404 Not Found: Desktop App integration not loaded or URL not reachable. \
                    Check: (1) Integration installed in HA and HA restarted, (2) Server URL is the HA base URL without /api, (3) Reverse proxy forwards /api/ to HA."
                        .into(),
                );
            }
            if status.as_u16() == 401 {
                log::error!("[HA] Registration 401 Unauthorized");
                return Err("401 Unauthorized: Invalid or expired access token.".into());
            }
            log::error!("[HA] Registration failed {}", status);
            return Err(format!("Registration failed ({status})").into());
        }

        let result: RegistrationResponse = serde_json::from_str(&body).map_err(|e| {
            let err = format!("Invalid JSON registration response: {e}");
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
        let webhook_id = self.webhook_id.as_ref().ok_or("No webhook_id configured")?;

        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);

        let payload = registration_payload(sensor)?;

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;

        Self::require_webhook_ack(response, "register_sensor").await
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

    pub async fn register_sensors_if_changed(
        &mut self,
        sensors: &[SensorValue],
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let descriptors = sensor_descriptors(sensors);
        if self.registered_descriptors.as_ref() == Some(&descriptors) {
            return Ok(false);
        }
        for sensor in changed_sensors(sensors, self.registered_descriptors.as_deref()) {
            self.register_sensor(sensor).await?;
        }
        self.registered_descriptors = Some(descriptors);
        Ok(true)
    }

    pub fn remember_registered_sensors(&mut self, sensors: &[SensorValue]) {
        self.registered_descriptors = Some(sensor_descriptors(sensors));
    }

    pub fn forget_registered_sensors(&mut self) {
        self.registered_descriptors = None;
    }

    /// Batch update sensor states
    pub async fn update_sensors(
        &self,
        sensors: &[SensorValue],
        snapshot_scope: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let webhook_id = self.webhook_id.as_ref().ok_or("No webhook_id configured")?;

        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);

        let payload = sensor_update_payload(sensors, self.update_interval, snapshot_scope);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;

        Self::require_webhook_ack(response, "update_sensor_states").await
    }

    /// Tell HA that this device is shutting down. Best-effort; the caller
    /// must time-bound the await (e.g. with tokio::time::timeout) because
    /// the OS may have already started reclaiming sockets when this runs.
    pub async fn send_device_offline(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let webhook_id = self.webhook_id.as_ref().ok_or("No webhook_id configured")?;
        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);
        let payload = WebhookPayload {
            protocol_version: 1,
            command_type: "device_offline".to_string(),
            data: serde_json::json!({}),
        };
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;
        Self::require_webhook_ack(response, "device_offline").await
    }

    /// Check if the webhook is still valid
    pub async fn check_webhook(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let webhook_id = match &self.webhook_id {
            Some(id) => id,
            None => return Err("No webhook_id configured".into()),
        };

        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);

        // Send a minimal payload to check if webhook exists
        let payload = WebhookPayload {
            protocol_version: 1,
            command_type: "update_sensor_states".to_string(),
            data: serde_json::json!({"sensors": [], "update_interval": self.update_interval}),
        };

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;
        Self::require_webhook_ack(response, "webhook health check").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut request = Vec::new();
        loop {
            let mut chunk = [0_u8; 4096];
            let length = socket.read(&mut chunk).await.unwrap();
            assert!(length > 0);
            request.extend_from_slice(&chunk[..length]);
            if let Some(headers_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..headers_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if request.len() >= headers_end + 4 + content_length {
                    return String::from_utf8(request).unwrap();
                }
            }
        }
    }

    #[tokio::test]
    async fn first_device_uses_ha_flow_and_can_recover_existing_webhook() {
        for (flow_result, expected_webhook) in [
            (
                serde_json::json!({"type":"create_entry","result":{"domain":"desktop_app"}}),
                None,
            ),
            (
                serde_json::json!({"type":"abort","reason":"already_registered","description_placeholders":{"webhook_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}),
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let request = read_request(&mut socket).await;
                assert!(request.starts_with("POST /api/config/config_entries/flow HTTP/1.1"));
                assert!(request.contains("authorization: Bearer test-token"));
                assert!(request.contains("\"handler\":\"desktop_app\""));
                let response = r#"{"type":"form","flow_id":"flow-1"}"#;
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).await.unwrap();

                let (mut socket, _) = listener.accept().await.unwrap();
                let request = read_request(&mut socket).await;
                assert!(request.starts_with("POST /api/config/config_entries/flow/flow-1 HTTP/1.1"));
                let payload: serde_json::Value =
                    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
                assert_eq!(payload["device_id"], "device-1");
                assert_eq!(payload["webhook_id"].as_str().unwrap().len(), 64);
                let response = flow_result.to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).await.unwrap();
            });
            let client = HaClient::new(format!("http://{address}"), "test-token".into(), None);
            let registration = RegistrationRequest {
                device_id: "device-1".into(),
                device_name: "Test PC".into(),
                manufacturer: None,
                model: None,
                os_name: None,
                os_version: None,
                app_version: None,
            };
            let webhook = client.bootstrap_device(&registration).await.unwrap();
            assert_eq!(webhook.len(), 64);
            if let Some(expected) = expected_webhook {
                assert_eq!(webhook, expected);
            }
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn token_check_uses_core_api_for_integration_upgrade_compatibility() {
        for (reply_status, expected_status) in [
            ("200 OK", reqwest::StatusCode::OK),
            ("401 Unauthorized", reqwest::StatusCode::UNAUTHORIZED),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 2048];
                let length = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..length]);
                assert!(request.starts_with("GET /api/ HTTP/1.1"));
                assert!(request.contains("authorization: Bearer test-token"));
                let response = format!(
                    "HTTP/1.1 {reply_status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            });
            let client = HaClient::new(format!("http://{address}"), "test-token".into(), None);
            assert_eq!(client.check_access_token().await.unwrap(), expected_status);
            server.await.unwrap();
        }
    }

    #[test]
    fn desktop_wire_payloads_match_shared_protocol_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../contracts/protocol-v1.json")).unwrap();
        let data = &fixture["register_sensor"]["data"];
        let mut sensor = SensorValue {
            unique_id: data["sensor_unique_id"].as_str().unwrap().into(),
            name: data["sensor_name"].as_str().unwrap().into(),
            state: data["sensor_state"].clone(),
            sensor_type: data["sensor_type"].as_str().unwrap().into(),
            device_class: None,
            unit_of_measurement: Some("%".into()),
            state_class: Some("measurement".into()),
            icon: Some("mdi:cpu-64-bit".into()),
            attributes: HashMap::new(),
            update_at_interval: true,
        };
        assert_eq!(
            serde_json::to_value(registration_payload(&sensor).unwrap()).unwrap(),
            fixture["register_sensor"]
        );

        sensor.state = serde_json::json!(21.5);
        sensor
            .attributes
            .insert("measurement_source".into(), serde_json::json!("system"));
        assert_eq!(
            serde_json::to_value(sensor_update_payload(&[sensor], 60, "all")).unwrap(),
            fixture["update_sensor_states"]
        );
    }

    #[test]
    fn device_offline_payload_serializes_with_expected_shape() {
        let payload = WebhookPayload {
            protocol_version: 1,
            command_type: "device_offline".to_string(),
            data: serde_json::json!({}),
        };
        let s = serde_json::to_string(&payload).expect("serialize");
        assert!(s.contains(r#""type":"device_offline""#), "payload was: {s}");
        assert!(s.contains(r#""data":{}"#), "payload was: {s}");
        assert!(s.contains(r#""protocol_version":1"#), "payload was: {s}");
    }

    #[test]
    fn registration_cache_ignores_measurements_but_detects_metadata_and_webhook_changes() {
        let mut sensor = SensorValue {
            unique_id: "cpu_usage".into(),
            name: "CPU Usage".into(),
            state: serde_json::json!(15),
            sensor_type: "sensor".into(),
            device_class: None,
            unit_of_measurement: Some("%".into()),
            state_class: Some("measurement".into()),
            icon: Some("mdi:cpu-64-bit".into()),
            attributes: HashMap::new(),
            update_at_interval: true,
        };
        let mut client = HaClient::new(
            "https://ha.example".into(),
            "token".into(),
            Some("old".into()),
        );
        client.remember_registered_sensors(&[sensor.clone()]);
        sensor.state = serde_json::json!(42);
        sensor
            .attributes
            .insert("source".into(), serde_json::json!("new"));
        assert_eq!(
            client.registered_descriptors,
            Some(sensor_descriptors(&[sensor.clone()]))
        );
        assert!(
            changed_sensors(&[sensor.clone()], client.registered_descriptors.as_deref()).is_empty()
        );

        sensor.name = "Processor Usage".into();
        assert_eq!(
            changed_sensors(&[sensor.clone()], client.registered_descriptors.as_deref()).len(),
            1
        );
        assert_ne!(
            client.registered_descriptors,
            Some(sensor_descriptors(&[sensor]))
        );
        client.set_webhook_id("new".into());
        assert!(client.registered_descriptors.is_none());
    }

    #[tokio::test]
    async fn webhook_health_requires_a_real_acknowledgement() {
        for (status, body, accepted) in [
            ("200 OK", "", false),
            ("200 OK", r#"{"success":false}"#, false),
            ("200 OK", r#"{"success":true}"#, true),
            ("200 OK", r#"{"success":true,"protocol_version":1}"#, true),
            ("200 OK", r#"{"success":true,"protocol_version":2}"#, false),
            ("410 Gone", r#"{"success":false}"#, false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 2048];
                let length = socket.read(&mut request).await.unwrap();
                assert!(
                    String::from_utf8_lossy(&request[..length]).contains("/api/webhook/test-id")
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
            });
            let client = HaClient::new(
                format!("http://{address}"),
                "test-token".into(),
                Some("test-id".into()),
            );
            let result = client.check_webhook().await;
            assert_eq!(result.is_ok(), accepted, "status={status}, body={body}");
            server.await.unwrap();
        }
    }
}

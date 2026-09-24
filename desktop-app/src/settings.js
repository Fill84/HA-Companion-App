/**
 * Settings modal logic for Home Assistant Companion
 */

let currentSettings = null;
let pendingSensorPreferences = {};
let settingsFocusReturn = null;
let versionRequestId = 0;

async function refreshSettingsVersions(settings) {
    const requestId = ++versionRequestId;
    document.getElementById("settings-app-version").textContent = settings.app_version
        ? `v${settings.app_version}` : t("version_unknown");
    const integrationVersion = document.getElementById("settings-integration-version");
    if (!settings.is_registered) {
        integrationVersion.textContent = t("version_not_connected");
        return;
    }

    integrationVersion.textContent = t("version_checking");
    try {
        const version = await window.__TAURI__.core.invoke("get_integration_version");
        if (requestId !== versionRequestId) return;
        integrationVersion.textContent = version ? `v${version}` : t("version_unknown");
    } catch {
        if (requestId !== versionRequestId) return;
        integrationVersion.textContent = t("version_unavailable");
    }
}

/**
 * Open settings modal and populate with current values
 */
async function openSettings() {
    settingsFocusReturn = document.activeElement;
    document.getElementById("settings-save").disabled = false;
    try {
        currentSettings = await window.__TAURI__.core.invoke("get_settings");
        pendingSensorPreferences = { ...(currentSettings.enabled_sensors || {}) };

        // Populate fields
        document.getElementById("settings-server-url").value = currentSettings.server_url || "";
        document.getElementById("settings-token").value = "";
        document.getElementById("settings-token").placeholder = currentSettings.has_access_token
            ? t("keep_existing_token") : t("enter_new_token");
        document.getElementById("settings-interval").value = currentSettings.update_interval || 60;
        document.getElementById("settings-language").value = currentSettings.language || "en";
        document.getElementById("settings-autostart").checked = currentSettings.autostart || false;

        // Device info
        document.getElementById("info-device-id").textContent = currentSettings.device_id || "-";
        document.getElementById("info-webhook-id").textContent = currentSettings.webhook_id_preview || "-";
        document.getElementById("info-status").textContent = currentSettings.is_registered
            ? t("registered")
            : t("not_registered");
        document.getElementById("info-status").className =
            "info-value " + (currentSettings.is_registered ? "status-ok" : "status-error");

        // Reset "My IP" until user clicks Show
        document.getElementById("info-my-ip").textContent = "-";

        // Hide any leftover reconnect-status from a previous open
        const reconnectStatus = document.getElementById("settings-reconnect-status");
        reconnectStatus.classList.add("hidden");
        reconnectStatus.classList.remove("status-ok", "status-error");
        reconnectStatus.textContent = "";

        // Populate sensor list
        await populateSensorList();

        // Show modal
        document.getElementById("settings-overlay").classList.remove("hidden");
        document.getElementById("settings-close").focus?.();
        void refreshSettingsVersions(currentSettings);
    } catch (err) {
        console.error("Failed to load settings:", err);
        document.getElementById("settings-save").disabled = true;
        const statusEl = document.getElementById("settings-reconnect-status");
        statusEl.classList.remove("hidden", "status-ok");
        statusEl.classList.add("status-error");
        statusEl.textContent = t("settings_load_failed");
        document.getElementById("settings-overlay").classList.remove("hidden");
        document.getElementById("settings-close").focus?.();
    }
}

/**
 * Close settings modal and restore the HA dashboard overlay
 */
async function closeSettings() {
    if (document.getElementById("settings-overlay").classList.contains("hidden")) return;
    // Settings can also be opened from setup, where no dashboard exists yet.
    if (document.getElementById("setup-screen").classList.contains("hidden")) {
        try {
            await window.__TAURI__.core.invoke("load_dashboard");
        } catch (err) {
            console.error("Failed to restore dashboard:", err);
            const statusEl = document.getElementById("settings-reconnect-status");
            statusEl.classList.remove("hidden", "status-ok");
            statusEl.classList.add("status-error");
            statusEl.textContent = t("dashboard_load_failed");
            return;
        }
    }
    versionRequestId++;
    document.getElementById("settings-overlay").classList.add("hidden");
    settingsFocusReturn?.focus?.();
    settingsFocusReturn = null;
}

/**
 * Save settings
 */
async function saveSettings() {
    const serverUrl = document.getElementById("settings-server-url").value.trim();
    const token = document.getElementById("settings-token").value.trim();
    const interval = Number(document.getElementById("settings-interval").value);
    const language = document.getElementById("settings-language").value;
    const autostart = document.getElementById("settings-autostart").checked;

    let phase = "save";
    try {
        if (!Number.isInteger(interval) || interval < 5 || interval > 3600) {
            throw new Error(t("error_update_interval"));
        }
        const saved = await window.__TAURI__.core.invoke("save_settings", {
            serverUrl: serverUrl,
            accessToken: token,
            updateInterval: interval,
            language: language,
            autostart: autostart,
            enabledSensors: pendingSensorPreferences,
        });

        if (serverUrl !== currentSettings.server_url || token) {
            phase = "register";
            await window.__TAURI__.core.invoke("register_device");
        }

        // Update language
        setLanguage(language);

        // Close settings modal (this also re-opens the HA dashboard view)
        phase = "dashboard";
        await closeSettings();
        if (saved?.sensor_sync_pending) {
            alert(t("settings_saved_sync_pending"));
        }
    } catch (err) {
        console.error("Failed to save settings:", err);
        alert(err instanceof Error && err.message === t("error_update_interval")
            ? err.message : t(phase === "register" ? "error_connection"
                : phase === "dashboard" ? "dashboard_load_failed" : "settings_save_failed"));
    }
}

/**
 * Populate sensor list with checkboxes
 */
async function populateSensorList() {
    try {
        const sensors = await window.__TAURI__.core.invoke("get_sensor_list");
        const container = document.getElementById("sensor-list");
        container.removeAttribute("role");
        container.innerHTML = "";

        for (const sensor of sensors) {
            const row = document.createElement("div");
            row.className = "sensor-row";

            const checkbox = document.createElement("input");
            checkbox.type = "checkbox";
            checkbox.id = `sensor-${sensor.id}`;
            checkbox.checked = sensor.enabled;
            checkbox.addEventListener("change", () => {
                pendingSensorPreferences[sensor.id] = checkbox.checked;
            });

            const label = document.createElement("label");
            label.htmlFor = `sensor-${sensor.id}`;
            label.setAttribute("data-sensor-key", sensor.id);
            label.setAttribute("data-sensor-en", sensor.name);
            label.setAttribute("data-sensor-nl", sensor.name_nl || sensor.name);
            label.textContent = typeof currentLanguage !== "undefined" && currentLanguage === "nl"
                ? (sensor.name_nl || sensor.name) : sensor.name;

            const badge = document.createElement("span");
            badge.className = "sensor-badge " + (sensor.updates_at_interval ? "badge-dynamic" : "badge-static");
            badge.textContent = sensor.updates_at_interval ? t("updates_at_interval") : t("static_sensor");

            row.appendChild(checkbox);
            row.appendChild(label);
            row.appendChild(badge);
            container.appendChild(row);
        }
    } catch (err) {
        console.error("Failed to load sensor list:", err);
        const container = document.getElementById("sensor-list");
        container.textContent = t("sensor_list_failed");
        container.setAttribute("role", "alert");
    }
}

/**
 * Toggle password visibility
 */
function togglePassword(inputId) {
    const input = document.getElementById(inputId);
    input.type = input.type === "password" ? "text" : "password";
    const button = document.querySelector(`[data-password-target="${inputId}"]`);
    if (button) {
        const key = input.type === "password" ? "show_password" : "hide_password";
        button.setAttribute("data-i18n-aria-label", key);
        button.setAttribute("aria-label", t(key));
    }
}

/**
 * Force a fresh registration with HA. Used when the saved webhook is dead
 * or the user simply wants to start over without changing URL/token.
 */
async function reconnectNow() {
    const btn = document.getElementById("settings-reconnect");
    const statusEl = document.getElementById("settings-reconnect-status");
    btn.disabled = true;
    statusEl.classList.remove("hidden", "status-ok", "status-error");
    statusEl.textContent = t("reconnecting");

    try {
        await window.__TAURI__.core.invoke("reregister_device");
        // Refresh device info panel (new webhook_id, status flipped)
        try {
            const fresh = await window.__TAURI__.core.invoke("get_settings");
            document.getElementById("info-webhook-id").textContent = fresh.webhook_id_preview || "-";
            const statusInfo = document.getElementById("info-status");
            statusInfo.textContent = fresh.is_registered ? t("registered") : t("not_registered");
            statusInfo.className = "info-value " + (fresh.is_registered ? "status-ok" : "status-error");
            void refreshSettingsVersions(fresh);
        } catch (e) { /* best effort */ }

        statusEl.classList.add("status-ok");
        statusEl.textContent = t("reconnect_success");
    } catch (err) {
        console.error("Failed to reconnect:", err);
        statusEl.classList.add("status-error");
        statusEl.textContent = t("reconnect_failed");
    } finally {
        btn.disabled = false;
    }
}

/**
 * Show this machine's public IP (for proxy allowlist)
 */
async function showMyIp() {
    const el = document.getElementById("info-my-ip");
    const btn = document.getElementById("settings-show-ip");
    el.textContent = "...";
    btn.disabled = true;
    try {
        const ip = await window.__TAURI__.core.invoke("get_my_public_ip");
        el.textContent = ip || "-";
    } catch (err) {
        console.error("Failed to get IP:", err);
        el.textContent = t("error_generic");
    }
    btn.disabled = false;
}

// Event listeners
document.addEventListener("DOMContentLoaded", () => {
    for (const button of document.querySelectorAll("[data-password-target]")) {
        button.addEventListener("click", () => togglePassword(button.dataset.passwordTarget));
    }
    document.getElementById("settings-close").addEventListener("click", closeSettings);
    document.getElementById("settings-cancel").addEventListener("click", closeSettings);
    document.getElementById("settings-save").addEventListener("click", saveSettings);
    document.getElementById("settings-show-ip").addEventListener("click", showMyIp);
    document.getElementById("settings-reconnect").addEventListener("click", reconnectNow);

    // Close on overlay click
    document.getElementById("settings-overlay").addEventListener("click", (e) => {
        if (e.target === document.getElementById("settings-overlay")) {
            closeSettings();
        }
    });

    // Close on Escape
    document.addEventListener("keydown", (e) => {
        const overlay = document.getElementById("settings-overlay");
        if (overlay.classList.contains("hidden")) return;
        if (e.key === "Escape") {
            closeSettings();
        } else if (e.key === "Tab") {
            const focusable = [...overlay.querySelectorAll('button:not([disabled]), input:not([disabled]), select:not([disabled]), a[href]')]
                .filter(el => el.getClientRects().length > 0);
            if (!focusable.length) return;
            const first = focusable[0];
            const last = focusable[focusable.length - 1];
            if (!overlay.contains(document.activeElement)) {
                e.preventDefault();
                first.focus();
            } else if (e.shiftKey && document.activeElement === first) {
                e.preventDefault();
                last.focus();
            } else if (!e.shiftKey && document.activeElement === last) {
                e.preventDefault();
                first.focus();
            }
        }
    });
});

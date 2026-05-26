/**
 * Main frontend logic for Home Assistant Companion
 * Handles app initialization, setup form, tray events
 */

/**
 * Show the setup screen. Optional reason: "webhook_dead" | "unreachable" | null.
 * The banner is hidden by default and only shown when there is something
 * useful to tell the user (i.e. they had a registration that just failed).
 */
function showSetupScreen(reason) {
    const banner = document.getElementById("setup-banner");
    const bannerText = document.getElementById("setup-banner-text");
    if (banner && bannerText) {
        if (reason === "webhook_dead") {
            bannerText.textContent = t("setup_banner_webhook_dead");
            banner.classList.remove("hidden");
        } else if (reason === "unreachable") {
            bannerText.textContent = t("setup_banner_unreachable");
            banner.classList.remove("hidden");
        } else if (reason === "default") {
            bannerText.textContent = t("setup_banner_default");
            banner.classList.remove("hidden");
        } else {
            banner.classList.add("hidden");
        }
    }
    document.getElementById("setup-screen").classList.remove("hidden");
}

/**
 * Hide the setup screen
 */
function hideSetupScreen() {
    document.getElementById("setup-screen").classList.add("hidden");
    const banner = document.getElementById("setup-banner");
    if (banner) banner.classList.add("hidden");
}

/**
 * Handle initial setup form
 */
async function handleSetup(e) {
    e.preventDefault();

    const serverUrl = document.getElementById("setup-server-url").value.trim();
    const token = document.getElementById("setup-token").value.trim();
    const errorEl = document.getElementById("setup-error");
    const loadingEl = document.getElementById("setup-loading");

    errorEl.classList.add("hidden");
    loadingEl.classList.remove("hidden");

    try {
        // Save settings first
        await window.__TAURI__.core.invoke("save_settings", {
            serverUrl: serverUrl,
            accessToken: token,
            updateInterval: 60,
            language: currentLanguage,
            autostart: false,
        });

        // Register device
        await window.__TAURI__.core.invoke("register_device");

        // Success — open HA dashboard as child webview overlay
        hideSetupScreen();
        await window.__TAURI__.core.invoke("load_dashboard");
    } catch (err) {
        errorEl.textContent = err.toString();
        errorEl.classList.remove("hidden");
    } finally {
        loadingEl.classList.add("hidden");
    }
}

/**
 * Show the setup screen pre-filled with the previously entered credentials
 * (used after a registration loss — the user's URL and token are usually fine,
 * only the HA-side webhook was lost).
 */
function prefillSetupFromSettings(settings) {
    if (settings && settings.server_url) {
        document.getElementById("setup-server-url").value = settings.server_url;
    }
    if (settings && settings.access_token) {
        document.getElementById("setup-token").value = settings.access_token;
    }
}

/**
 * Initialize the app. The startup flow always runs a connection check before
 * jumping to the dashboard, so we never silently load a dashboard against a
 * dead webhook (which used to manifest as "everything looks fine for 60s,
 * then errors pile up in logs and the user has to manually intervene").
 */
async function initApp() {
    try {
        // Get current settings
        const settings = await window.__TAURI__.core.invoke("get_settings");

        // Set language
        setLanguage(settings.language || "en");

        if (!settings.server_url || !settings.access_token) {
            // No config — show setup wizard, no banner.
            showSetupScreen(null);
            return;
        }

        // Health check before showing the dashboard. Returns one of:
        // "ok" | "not_registered" | "webhook_dead" | { unreachable: { reason } } | "token_invalid"
        let status;
        try {
            status = await window.__TAURI__.core.invoke("check_connection");
        } catch (err) {
            console.error("check_connection failed:", err);
            // Treat as unreachable so we don't wipe the webhook on a transient error.
            status = { unreachable: { reason: err.toString() } };
        }

        // Status can be a string ("ok", "not_registered", "webhook_dead",
        // "token_invalid") OR an object like { unreachable: { reason } }
        // depending on serde's enum representation.
        const statusName = typeof status === "string" ? status : Object.keys(status)[0];

        if (statusName === "ok") {
            hideSetupScreen();
            await window.__TAURI__.core.invoke("load_dashboard");
            return;
        }

        // Anything else => show setup screen. Pick the banner message that
        // matches the actual failure mode so the user knows what to do.
        prefillSetupFromSettings(settings);
        if (statusName === "webhook_dead") {
            showSetupScreen("webhook_dead");
        } else if (statusName === "unreachable") {
            showSetupScreen("unreachable");
        } else if (statusName === "not_registered") {
            // Has URL + token but no webhook yet — first run after losing webhook
            // through a local app reset, or after save_settings changed creds.
            showSetupScreen("default");
        } else {
            showSetupScreen("default");
        }
    } catch (err) {
        console.error("Failed to initialize app:", err);
        showSetupScreen(null);
    }
}

// Event listeners
document.addEventListener("DOMContentLoaded", () => {
    // Setup form
    document.getElementById("setup-form").addEventListener("submit", handleSetup);

    // Listen for tray events
    if (window.__TAURI__) {
        window.__TAURI__.event.listen("tray-show-settings", () => {
            openSettings();
        });

        // Fired by Rust when the sensor loop detects HA has forgotten our
        // webhook (404/410). Bring the user back to the setup screen with a
        // clear message rather than letting them stare at a stale dashboard.
        window.__TAURI__.event.listen("registration-lost", async (event) => {
            console.warn("registration-lost:", event.payload);
            try {
                await window.__TAURI__.core.invoke("hide_dashboard");
            } catch (e) { /* dashboard may already be closed */ }
            try {
                const settings = await window.__TAURI__.core.invoke("get_settings");
                prefillSetupFromSettings(settings);
            } catch (e) { /* best effort */ }
            showSetupScreen("webhook_dead");
        });
    }

    // Initialize
    initApp();
});

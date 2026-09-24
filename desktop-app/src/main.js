/**
 * Main frontend logic for Home Assistant Companion
 * Handles app initialization, setup form, tray events
 */
let setupBusy = false;
let initialSettings = null;
let savedTokenInvalid = false;
let connectionRetryTimer = null;
let connectionRetryBusy = false;

function stopConnectionRetry() {
    if (connectionRetryTimer !== null) {
        window.clearInterval(connectionRetryTimer);
        connectionRetryTimer = null;
    }
}

async function retryUnreachableConnection() {
    if (connectionRetryBusy || setupBusy || savedTokenInvalid ||
        document.getElementById("setup-screen").classList.contains("hidden")) return;
    if (document.getElementById("setup-server-url").value.trim() !== initialSettings?.server_url ||
        document.getElementById("setup-token").value.trim()) return;

    connectionRetryBusy = true;
    try {
        const status = await window.__TAURI__.core.invoke("check_connection");
        const name = typeof status === "string" ? status : Object.keys(status)[0];
        if (name === "ok") {
            await window.__TAURI__.core.invoke("load_dashboard");
            hideSetupScreen();
        } else if (name === "token_invalid") {
            savedTokenInvalid = true;
            updateSetupTokenRequirement();
            showSetupScreen("token_invalid");
        } else if (name === "webhook_dead") {
            showSetupScreen("webhook_dead");
        }
    } catch (error) {
        console.warn("Connection retry failed:", error);
    } finally {
        connectionRetryBusy = false;
    }
}

function startConnectionRetry() {
    if (connectionRetryTimer === null) {
        connectionRetryTimer = window.setInterval(retryUnreachableConnection, 30_000);
    }
}


/**
 * Show the setup screen. Optional reason: "webhook_dead" | "unreachable" | null.
 * The banner is hidden by default and only shown when there is something
 * useful to tell the user (i.e. they had a registration that just failed).
 */
function showSetupScreen(reason) {
    if (reason === "unreachable") startConnectionRetry();
    else stopConnectionRetry();
    const banner = document.getElementById("setup-banner");
    const bannerText = document.getElementById("setup-banner-text");
    if (banner && bannerText) {
        if (reason === "webhook_dead") {
            bannerText.setAttribute("data-i18n", "setup_banner_webhook_dead");
            bannerText.textContent = t("setup_banner_webhook_dead");
            banner.classList.remove("hidden");
        } else if (reason === "unreachable") {
            bannerText.setAttribute("data-i18n", "setup_banner_unreachable");
            bannerText.textContent = t("setup_banner_unreachable");
            banner.classList.remove("hidden");
        } else if (reason === "token_invalid") {
            bannerText.setAttribute("data-i18n", "setup_banner_token_invalid");
            bannerText.textContent = t("setup_banner_token_invalid");
            banner.classList.remove("hidden");
        } else if (reason === "default") {
            bannerText.setAttribute("data-i18n", "setup_banner_default");
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
    stopConnectionRetry();
    document.getElementById("setup-screen").classList.add("hidden");
    const banner = document.getElementById("setup-banner");
    if (banner) banner.classList.add("hidden");
}

/**
 * Handle initial setup form
 */
async function handleSetup(e) {
    e.preventDefault();
    if (setupBusy) return;
    setupBusy = true;

    const serverUrl = document.getElementById("setup-server-url").value.trim();
    const token = document.getElementById("setup-token").value.trim();
    const errorEl = document.getElementById("setup-error");
    const loadingEl = document.getElementById("setup-loading");

    errorEl.classList.add("hidden");
    loadingEl.classList.remove("hidden");

    let phase = "save";
    try {
        if (!serverUrl) {
            throw new Error(t("error_server_url"));
        }
        if (!token && !canReuseSavedToken(serverUrl)) {
            throw new Error(t("error_token"));
        }
        // Save settings first
        await window.__TAURI__.core.invoke("save_settings", {
            serverUrl: serverUrl,
            accessToken: token,
            updateInterval: initialSettings?.update_interval || 60,
            language: currentLanguage,
            autostart: initialSettings?.autostart || false,
        });

        // Register device
        phase = "register";
        await window.__TAURI__.core.invoke("register_device");

        // Success — open HA dashboard as child webview overlay
        phase = "dashboard";
        await window.__TAURI__.core.invoke("load_dashboard");
        hideSetupScreen();
    } catch (err) {
        console.error("Connection setup failed:", err);
        errorEl.textContent = err instanceof Error && [t("error_server_url"), t("error_token")].includes(err.message)
            ? err.message : t(phase === "save" ? "settings_save_failed"
                : phase === "dashboard" ? "dashboard_load_failed" : "error_connection");
        errorEl.classList.remove("hidden");
    } finally {
        setupBusy = false;
        loadingEl.classList.add("hidden");
    }
}

/**
 * Restore the server URL after registration loss. The token stays inside Rust;
 * an empty input means reuse only when the server URL is unchanged.
 */
function prefillSetupFromSettings(settings) {
    if (settings && settings.server_url) {
        document.getElementById("setup-server-url").value = settings.server_url;
    }
    document.getElementById("setup-token").value = "";
    updateSetupTokenRequirement();
}

function canReuseSavedToken(url) {
    return !savedTokenInvalid && initialSettings?.has_access_token && url === initialSettings.server_url;
}

function updateSetupTokenRequirement() {
    const url = document.getElementById("setup-server-url").value.trim();
    const tokenInput = document.getElementById("setup-token");
    const canReuse = canReuseSavedToken(url);
    tokenInput.required = !canReuse;
    tokenInput.placeholder = canReuse ? t("keep_existing_token") : t("enter_new_token");
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
        let settings;
        try {
            settings = await window.__TAURI__.core.invoke("get_settings");
        } catch (err) {
            console.error("Failed to load saved settings:", err);
            showSetupScreen(null);
            const errorEl = document.getElementById("setup-error");
            errorEl.textContent = t("settings_startup_failed");
            errorEl.classList.remove("hidden");
            document.querySelector('#setup-form button[type="submit"]').disabled = true;
            return;
        }
        initialSettings = settings;

        // Set language
        setLanguage(settings.language || "en");
        const lang = settings.language || "en";
        const setupLang = document.getElementById("setup-language");
        if (setupLang) setupLang.value = lang;

        if (!settings.server_url || !settings.has_access_token) {
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
        } else if (statusName === "token_invalid") {
            savedTokenInvalid = true;
            updateSetupTokenRequirement();
            showSetupScreen("token_invalid");
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
    document.getElementById("setup-server-url").addEventListener("input", updateSetupTokenRequirement);

    // Language picker in the setup footer — live-changes the UI strings.
    // Persistence happens on next save_settings call (e.g. when the user
    // submits the form) so the desktop process picks up the choice too.
    const setupLang = document.getElementById("setup-language");
    if (setupLang) {
        setupLang.addEventListener("change", (e) => {
            setLanguage(e.target.value);
        });
    }

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
        window.__TAURI__.event.listen("registration-restored", async () => {
            if (!setupBusy && !document.getElementById("setup-screen").classList.contains("hidden")) {
                await initApp();
            }
        });
    }

    // Initialize
    initApp();
});

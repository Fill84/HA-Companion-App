/**
 * Internationalization module for Home Assistant Companion
 * Supports English (en) and Dutch (nl)
 */

const translations = {
    en: {
        // Setup
        setup_title: "Home Assistant Companion",
        setup_subtitle: "Connect your desktop to Home Assistant",
        server_url: "Server URL",
        access_token: "Long-Lived Access Token",
        token_help: "Create a token in HA: Profile → Long-Lived Access Tokens",
        connect: "Connect",
        help: "Help",
        registering: "Registering device...",
        setup_banner_title: "Reconnection needed",
        setup_banner_default: "Your previous registration is no longer valid. Click Connect to re-register this device with Home Assistant.",
        setup_banner_webhook_dead: "Home Assistant no longer recognises this device (the integration was reinstalled or its storage was cleared). Click Connect to register again — your credentials below are still valid.",
        setup_banner_unreachable: "Cannot reach the Desktop App integration on Home Assistant. Check your server URL, reverse proxy, and that the integration is installed and HA has been restarted.",

        // Settings
        settings: "Settings",
        app_version: "App",
        integration_version: "HA integration",
        version_unknown: "Unknown",
        version_unavailable: "Unavailable",
        version_checking: "Checking...",
        version_not_connected: "Not connected",
        connection: "Connection",
        general: "General",
        update_interval: "Update Interval (seconds)",
        language: "Language",
        autostart: "Start at login",
        sensors: "Sensors",
        device_info: "Device Info",
        device_id: "Device ID",
        webhook_id: "Webhook ID",
        status: "Status",
        my_ip_for_proxy: "My IP (for proxy allowlist)",
        show_ip: "Show",
        save: "Save",
        cancel: "Cancel",
        registered: "Registered",
        not_registered: "Not registered",
        updates_at_interval: "Updates at interval",
        static_sensor: "Static (startup only)",
        reconnect_label: "Connection",
        reconnect: "Reconnect",
        reconnecting: "Reconnecting...",
        reconnect_success: "Reconnected successfully.",
        reconnect_failed: "Reconnect failed: ",
        keep_existing_token: "Leave blank to keep the saved token",
        enter_new_token: "Enter a Home Assistant access token",
        setup_banner_token_invalid: "The saved token is no longer accepted. Enter a new Home Assistant access token.",
        close_settings: "Close settings",
        show_password: "Show password",
        hide_password: "Hide password",

        // Sensor names
        cpu_usage: "CPU Usage",
        cpu_frequency: "CPU Frequency",
        cpu_temperature: "CPU Temperature",
        cpu_model: "CPU Model",
        memory_usage: "Memory Usage",
        memory_used: "Memory Used",
        memory_total: "Memory Total",
        disk_usage: "Disk Usage",
        gpu: "GPU Sensors",
        network: "Network Sensors",
        battery: "Battery Sensors",
        os_version: "OS Version",
        hostname: "Hostname",
        motherboard: "Motherboard",
        bios_version: "BIOS Version",
        bios_vendor: "BIOS Vendor",
        bios_date: "BIOS Date",
        swap_usage: "Swap Usage",
        system_uptime: "System Uptime",
        process_count: "Process Count",
        last_boot: "Last Boot",
        logged_in_user: "Logged-in User",
        display: "Display",

        // Messages
        error_server_url: "Please enter a valid server URL",
        error_token: "Please enter an access token",
        error_connection: "Could not connect to Home Assistant",
        dashboard_load_failed: "Connected, but the Home Assistant dashboard could not be opened",
        error_update_interval: "Update interval must be between 5 and 3600 seconds",
        settings_save_failed: "Failed to save settings",
        settings_load_failed: "Settings could not be loaded. Close and reopen Settings to retry.",
        settings_startup_failed: "Saved settings could not be read. Connection is paused to protect this device's identity. Check the app log and local settings file, then restart the app.",
        sensor_list_failed: "The sensor list could not be loaded. Reopen Settings to retry.",
        error_generic: "Error",
        success_saved: "Settings saved successfully",
        success_registered: "Device registered successfully",
    },

    nl: {
        // Setup
        setup_title: "Home Assistant Companion",
        setup_subtitle: "Verbind je desktop met Home Assistant",
        server_url: "Server URL",
        access_token: "Langlevend Toegangstoken",
        token_help: "Maak een token aan in HA: Profiel → Langlevende Toegangstokens",
        connect: "Verbinden",
        help: "Help",
        registering: "Apparaat registreren...",
        setup_banner_title: "Opnieuw verbinden nodig",
        setup_banner_default: "Je vorige registratie is niet meer geldig. Klik op Verbinden om dit apparaat opnieuw te registreren bij Home Assistant.",
        setup_banner_webhook_dead: "Home Assistant kent dit apparaat niet meer (de integratie is opnieuw geïnstalleerd of de opslag is gewist). Klik op Verbinden om opnieuw te registreren — je gegevens hieronder zijn nog geldig.",
        setup_banner_unreachable: "Kan de Desktop App-integratie op Home Assistant niet bereiken. Controleer je server-URL, reverse proxy, en of de integratie geïnstalleerd is en HA opnieuw is gestart.",

        // Settings
        settings: "Instellingen",
        app_version: "App",
        integration_version: "HA-integratie",
        version_unknown: "Onbekend",
        version_unavailable: "Niet beschikbaar",
        version_checking: "Controleren...",
        version_not_connected: "Niet verbonden",
        connection: "Verbinding",
        general: "Algemeen",
        update_interval: "Update Interval (seconden)",
        language: "Taal",
        autostart: "Starten bij inloggen",
        sensors: "Sensoren",
        device_info: "Apparaat Info",
        device_id: "Apparaat ID",
        webhook_id: "Webhook ID",
        status: "Status",
        my_ip_for_proxy: "Mijn IP (voor proxy allowlist)",
        show_ip: "Tonen",
        save: "Opslaan",
        cancel: "Annuleren",
        registered: "Geregistreerd",
        not_registered: "Niet geregistreerd",
        updates_at_interval: "Update bij interval",
        static_sensor: "Statisch (alleen bij start)",
        reconnect_label: "Verbinding",
        reconnect: "Opnieuw verbinden",
        reconnecting: "Opnieuw verbinden...",
        reconnect_success: "Opnieuw verbonden.",
        reconnect_failed: "Opnieuw verbinden mislukt: ",
        keep_existing_token: "Leeg laten om het opgeslagen token te behouden",
        enter_new_token: "Voer een Home Assistant-toegangstoken in",
        setup_banner_token_invalid: "Het opgeslagen token wordt niet meer geaccepteerd. Voer een nieuw Home Assistant-toegangstoken in.",
        close_settings: "Instellingen sluiten",
        show_password: "Wachtwoord tonen",
        hide_password: "Wachtwoord verbergen",

        // Sensor names
        cpu_usage: "CPU Gebruik",
        cpu_frequency: "CPU Snelheid",
        cpu_temperature: "CPU Temperatuur",
        cpu_model: "CPU Model",
        memory_usage: "Geheugen Gebruik",
        memory_used: "Geheugen Gebruikt",
        memory_total: "Geheugen Totaal",
        disk_usage: "Schijf Gebruik",
        gpu: "GPU Sensoren",
        network: "Netwerk Sensoren",
        battery: "Batterij Sensoren",
        os_version: "OS Versie",
        hostname: "Hostnaam",
        motherboard: "Moederbord",
        bios_version: "BIOS Versie",
        bios_vendor: "BIOS-leverancier",
        bios_date: "BIOS-datum",
        swap_usage: "Wisselgeheugengebruik",
        system_uptime: "Systeem-uptime",
        process_count: "Aantal processen",
        last_boot: "Laatste opstarttijd",
        logged_in_user: "Aangemelde gebruiker",
        display: "Beeldscherm",

        // Messages
        error_server_url: "Voer een geldige server URL in",
        error_token: "Voer een toegangstoken in",
        error_connection: "Kan geen verbinding maken met Home Assistant",
        dashboard_load_failed: "Verbonden, maar het Home Assistant-dashboard kon niet worden geopend",
        error_update_interval: "Het update-interval moet tussen 5 en 3600 seconden liggen",
        settings_save_failed: "Instellingen opslaan mislukt",
        settings_load_failed: "Instellingen konden niet worden geladen. Sluit en open Instellingen opnieuw om het te proberen.",
        settings_startup_failed: "Opgeslagen instellingen konden niet worden gelezen. De verbinding is gepauzeerd om de identiteit van dit apparaat te beschermen. Controleer het app-log en het lokale instellingenbestand en start de app opnieuw.",
        sensor_list_failed: "De sensorlijst kon niet worden geladen. Open Instellingen opnieuw om het te proberen.",
        error_generic: "Fout",
        success_saved: "Instellingen opgeslagen",
        success_registered: "Apparaat succesvol geregistreerd",
    },
};

let currentLanguage = "en";

/**
 * Get translated string for key
 */
function t(key) {
    const lang = translations[currentLanguage] || translations.en;
    return lang[key] || translations.en[key] || key;
}

/**
 * Set current language and update all UI elements
 */
function setLanguage(lang) {
    if (!translations[lang]) {
        console.warn(`Language '${lang}' not supported, falling back to 'en'`);
        lang = "en";
    }
    currentLanguage = lang;
    document.documentElement.lang = lang;
    updateUITranslations();
}

/**
 * Update all elements with data-i18n attribute
 */
function updateUITranslations() {
    document.querySelectorAll("[data-i18n]").forEach((el) => {
        const key = el.getAttribute("data-i18n");
        const translated = t(key);
        if (el.tagName === "INPUT" && el.type !== "checkbox") {
            // Don't overwrite input values
        } else if (el.tagName === "LABEL" || el.tagName === "SPAN" || el.tagName === "H1" ||
            el.tagName === "H2" || el.tagName === "H3" || el.tagName === "P" ||
            el.tagName === "BUTTON" || el.tagName === "SMALL") {
            el.textContent = translated;
        }
    });
    document.querySelectorAll("[data-sensor-key]").forEach((el) => {
        const key = el.getAttribute("data-sensor-key");
        el.textContent = translations[currentLanguage][key] || translations.en[key]
            || el.getAttribute("data-sensor-name") || key;
    });
    document.querySelectorAll("[data-i18n-aria-label]").forEach((el) => {
        el.setAttribute("aria-label", t(el.getAttribute("data-i18n-aria-label")));
    });
}

"""The Desktop App integration."""

from __future__ import annotations

import logging
from typing import Any

from homeassistant.config_entries import ConfigEntry
from homeassistant.const import EVENT_HOMEASSISTANT_START, Platform
from homeassistant.core import HomeAssistant, callback
from homeassistant.helpers import device_registry as dr
from homeassistant.helpers.event import async_call_later
from homeassistant.helpers.storage import Store

from .const import (
    ATTR_DEVICE_ID,
    ATTR_DEVICE_NAME,
    ATTR_MANUFACTURER,
    ATTR_MODEL,
    ATTR_APP_VERSION,
    ATTR_WEBHOOK_ID,
    DATA_CONFIG_ENTRIES,
    DATA_DEVICES,
    DATA_DELETED_IDS,
    DATA_PENDING_UPDATES,
    DATA_STORE,
    DOMAIN,
    PLATFORMS,
    STORAGE_KEY,
    STORAGE_VERSION,
)
from .helpers import get_device_info
from .http_api import DesktopAppRegistrationView
from .webhook import handle_webhook

_LOGGER = logging.getLogger(__name__)


def _register_api_view(hass: HomeAssistant) -> bool:
    """Register the registration API view. Returns True if registered."""
    if getattr(hass, "http", None) is None:
        return False
    try:
        hass.http.register_view(DesktopAppRegistrationView())
        _LOGGER.info(
            "Registered Desktop App registration endpoint at /api/desktop_app/registrations"
        )
        return True
    except Exception as e:  # noqa: BLE001
        _LOGGER.warning("Failed to register Desktop App API view: %s", e)
        return False


async def async_setup(hass: HomeAssistant, config: dict[str, Any]) -> bool:
    """Set up the Desktop App integration."""
    _LOGGER.info("Desktop App integration loading (registration API: /api/desktop_app/registrations)")

    store = Store(hass, STORAGE_VERSION, STORAGE_KEY)
    stored_data = await store.async_load() or {}

    hass.data[DOMAIN] = {
        DATA_CONFIG_ENTRIES: stored_data.get(DATA_CONFIG_ENTRIES, {}),
        DATA_DEVICES: stored_data.get(DATA_DEVICES, {}),
        DATA_DELETED_IDS: stored_data.get(DATA_DELETED_IDS, []),
        DATA_PENDING_UPDATES: {},
        DATA_STORE: store,
    }

    # Register the HTTP registration endpoint so the desktop app can register.
    # Try now; if hass.http is not ready (load order), try on HA start and once after delay.
    view_registered = _register_api_view(hass)

    if not view_registered:

        @callback
        def _on_start(_):
            nonlocal view_registered
            if view_registered:
                return
            view_registered = _register_api_view(hass)
            if not view_registered and getattr(hass, "http", None) is None:
                _LOGGER.error(
                    "Desktop App: hass.http not available at start; registration API will not work"
                )

        hass.bus.async_listen_once(EVENT_HOMEASSISTANT_START, _on_start)

        @callback
        def _delayed_register(_):
            nonlocal view_registered
            if view_registered:
                return
            view_registered = _register_api_view(hass)
            if not view_registered:
                _LOGGER.error(
                    "Desktop App: registration API could not be registered. "
                    "Check that the 'http' integration is loaded and restart Home Assistant."
                )

        async_call_later(hass, 5.0, _delayed_register)

    return True


async def async_setup_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Set up Desktop App from a config entry."""
    registration = entry.data
    device_id = registration[ATTR_DEVICE_ID]
    webhook_id = registration[ATTR_WEBHOOK_ID]

    # Store config entry data
    hass.data[DOMAIN][DATA_CONFIG_ENTRIES][entry.entry_id] = dict(registration)

    # Register device in device registry
    dev_reg = dr.async_get(hass)
    dev_reg.async_get_or_create(
        config_entry_id=entry.entry_id,
        identifiers={(DOMAIN, device_id)},
        name=registration.get(ATTR_DEVICE_NAME, "Desktop App"),
        manufacturer=registration.get(ATTR_MANUFACTURER, "Unknown"),
        model=registration.get(ATTR_MODEL, "Desktop"),
        sw_version=registration.get(ATTR_APP_VERSION),
    )

    # Register webhook handler
    hass.components.webhook.async_register(
        DOMAIN,
        f"Desktop App ({registration.get(ATTR_DEVICE_NAME, device_id)})",
        webhook_id,
        handle_webhook,
        allowed_methods=["POST"],
    )

    # Initialize pending updates dict for this entry
    hass.data[DOMAIN][DATA_PENDING_UPDATES][webhook_id] = {}

    # Forward setup to sensor and binary_sensor platforms
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)

    _LOGGER.info("Desktop App entry set up for device: %s", device_id)

    # Save store
    await _async_save_store(hass)

    return True


async def async_unload_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Unload a Desktop App config entry."""
    registration = entry.data
    webhook_id = registration.get(ATTR_WEBHOOK_ID)

    # Unregister webhook
    if webhook_id:
        hass.components.webhook.async_unregister(webhook_id)
        hass.data[DOMAIN][DATA_PENDING_UPDATES].pop(webhook_id, None)

    # Remove config entry data
    hass.data[DOMAIN][DATA_CONFIG_ENTRIES].pop(entry.entry_id, None)

    # Unload platforms
    unloaded = await hass.config_entries.async_unload_platforms(entry, PLATFORMS)

    if unloaded:
        await _async_save_store(hass)

    return unloaded


async def async_remove_entry(hass: HomeAssistant, entry: ConfigEntry) -> None:
    """Remove a Desktop App config entry."""
    device_id = entry.data.get(ATTR_DEVICE_ID)
    if device_id:
        deleted_ids = hass.data[DOMAIN][DATA_DELETED_IDS]
        if device_id not in deleted_ids:
            deleted_ids.append(device_id)
        await _async_save_store(hass)


async def _async_save_store(hass: HomeAssistant) -> None:
    """Save data to store."""
    store: Store = hass.data[DOMAIN][DATA_STORE]
    await store.async_save(
        {
            DATA_CONFIG_ENTRIES: hass.data[DOMAIN][DATA_CONFIG_ENTRIES],
            DATA_DEVICES: hass.data[DOMAIN][DATA_DEVICES],
            DATA_DELETED_IDS: hass.data[DOMAIN][DATA_DELETED_IDS],
        }
    )

"""Webhook handlers for the Desktop App integration."""

from __future__ import annotations

import logging
from typing import Any, Callable, Coroutine

from aiohttp.web import Request, Response

from homeassistant.core import HomeAssistant
from homeassistant.helpers.dispatcher import async_dispatcher_send

from .const import (
    ATTR_DEVICE_ID,
    ATTR_SENSOR_ATTRIBUTES,
    ATTR_SENSOR_DEVICE_CLASS,
    ATTR_SENSOR_ENTITY_CATEGORY,
    ATTR_SENSOR_ICON,
    ATTR_SENSOR_NAME,
    ATTR_SENSOR_STATE,
    ATTR_SENSOR_STATE_CLASS,
    ATTR_SENSOR_TYPE,
    ATTR_SENSOR_UNIQUE_ID,
    ATTR_SENSOR_UNIT_OF_MEASUREMENT,
    ATTR_WEBHOOK_ID,
    COMMAND_REGISTER_SENSOR,
    COMMAND_UPDATE_REGISTRATION,
    COMMAND_UPDATE_SENSOR_STATES,
    DATA_CONFIG_ENTRIES,
    DATA_PENDING_UPDATES,
    DOMAIN,
    SIGNAL_SENSOR_REGISTER,
    SIGNAL_SENSOR_UPDATE,
)
from .helpers import error_response, webhook_response

_LOGGER = logging.getLogger(__name__)

# Registry of webhook command handlers
WEBHOOK_COMMANDS: dict[
    str, Callable[[HomeAssistant, dict, str, dict], Coroutine[Any, Any, Response]]
] = {}


def webhook_command(command_type: str):
    """Decorator to register a webhook command handler."""

    def decorator(func):
        WEBHOOK_COMMANDS[command_type] = func
        return func

    return decorator


async def handle_webhook(
    hass: HomeAssistant, webhook_id: str, request: Request
) -> Response:
    """Handle incoming webhook requests from the Desktop App."""
    try:
        data: dict[str, Any] = await request.json()
    except ValueError:
        return error_response("Invalid JSON", status=400)

    command_type = data.get("type")
    if not command_type:
        return error_response("Missing 'type' field", status=400)

    handler = WEBHOOK_COMMANDS.get(command_type)
    if handler is None:
        _LOGGER.warning("Unknown webhook command type: %s", command_type)
        return error_response(f"Unknown command type: {command_type}", status=400)

    # Find the config entry for this webhook
    config_entry = None
    for entry_id, entry_data in hass.data[DOMAIN][DATA_CONFIG_ENTRIES].items():
        if entry_data.get(ATTR_WEBHOOK_ID) == webhook_id:
            config_entry = entry_data
            break

    if config_entry is None:
        return error_response("Device not registered", status=410)

    _LOGGER.debug(
        "Handling webhook command '%s' for device %s",
        command_type,
        config_entry.get(ATTR_DEVICE_ID, "unknown"),
    )

    return await handler(hass, config_entry, webhook_id, data.get("data", {}))


@webhook_command(COMMAND_REGISTER_SENSOR)
async def handle_register_sensor(
    hass: HomeAssistant,
    config_entry: dict[str, Any],
    webhook_id: str,
    data: dict[str, Any],
) -> Response:
    """Register a new sensor entity."""
    required_fields = [ATTR_SENSOR_UNIQUE_ID, ATTR_SENSOR_NAME, ATTR_SENSOR_TYPE]
    for field in required_fields:
        if field not in data:
            return error_response(f"Missing required field: {field}", status=400)

    sensor_type = data[ATTR_SENSOR_TYPE]
    if sensor_type not in ("sensor", "binary_sensor"):
        return error_response(
            f"Invalid sensor type: {sensor_type}. Must be 'sensor' or 'binary_sensor'.",
            status=400,
        )

    device_id = config_entry[ATTR_DEVICE_ID]
    sensor_unique_id = data[ATTR_SENSOR_UNIQUE_ID]
    unique_store_key = f"{device_id}_{sensor_unique_id}"

    sensor_data = {
        ATTR_SENSOR_UNIQUE_ID: sensor_unique_id,
        ATTR_SENSOR_NAME: data[ATTR_SENSOR_NAME],
        ATTR_SENSOR_TYPE: sensor_type,
        ATTR_SENSOR_STATE: data.get(ATTR_SENSOR_STATE),
        ATTR_SENSOR_ICON: data.get(ATTR_SENSOR_ICON),
        ATTR_SENSOR_DEVICE_CLASS: data.get(ATTR_SENSOR_DEVICE_CLASS),
        ATTR_SENSOR_UNIT_OF_MEASUREMENT: data.get(ATTR_SENSOR_UNIT_OF_MEASUREMENT),
        ATTR_SENSOR_STATE_CLASS: data.get(ATTR_SENSOR_STATE_CLASS),
        ATTR_SENSOR_ENTITY_CATEGORY: data.get(ATTR_SENSOR_ENTITY_CATEGORY),
        ATTR_SENSOR_ATTRIBUTES: data.get(ATTR_SENSOR_ATTRIBUTES, {}),
        "unique_store_key": unique_store_key,
        ATTR_DEVICE_ID: device_id,
    }

    # Store sensor registration
    devices = hass.data[DOMAIN].setdefault("registered_sensors", {})
    devices[unique_store_key] = sensor_data

    # Dispatch signal for dynamic entity creation
    signal = SIGNAL_SENSOR_REGISTER.format(device_id, sensor_type)
    async_dispatcher_send(hass, signal, sensor_data)

    _LOGGER.info(
        "Registered sensor '%s' (%s) for device %s",
        data[ATTR_SENSOR_NAME],
        sensor_type,
        device_id,
    )

    return webhook_response({"success": True})


@webhook_command(COMMAND_UPDATE_SENSOR_STATES)
async def handle_update_sensor_states(
    hass: HomeAssistant,
    config_entry: dict[str, Any],
    webhook_id: str,
    data: dict[str, Any],
) -> Response:
    """Handle batch sensor state updates."""
    sensor_states = data.get("sensors", [])
    if not isinstance(sensor_states, list):
        return error_response("'sensors' must be a list", status=400)

    device_id = config_entry[ATTR_DEVICE_ID]
    pending = hass.data[DOMAIN][DATA_PENDING_UPDATES].setdefault(webhook_id, {})

    for sensor_update in sensor_states:
        sensor_unique_id = sensor_update.get(ATTR_SENSOR_UNIQUE_ID)
        if not sensor_unique_id:
            continue

        unique_store_key = f"{device_id}_{sensor_unique_id}"

        update_data = {
            ATTR_SENSOR_STATE: sensor_update.get(ATTR_SENSOR_STATE),
            ATTR_SENSOR_ICON: sensor_update.get(ATTR_SENSOR_ICON),
            ATTR_SENSOR_ATTRIBUTES: sensor_update.get(ATTR_SENSOR_ATTRIBUTES, {}),
        }

        # Buffer in pending updates
        pending[unique_store_key] = update_data

        # Dispatch signal to individual entity
        signal = SIGNAL_SENSOR_UPDATE.format(device_id, sensor_unique_id)
        async_dispatcher_send(hass, signal, update_data)

    _LOGGER.debug(
        "Updated %d sensor states for device %s",
        len(sensor_states),
        device_id,
    )

    return webhook_response({"success": True})


@webhook_command(COMMAND_UPDATE_REGISTRATION)
async def handle_update_registration(
    hass: HomeAssistant,
    config_entry: dict[str, Any],
    webhook_id: str,
    data: dict[str, Any],
) -> Response:
    """Update device registration info."""
    device_id = config_entry[ATTR_DEVICE_ID]

    # Update allowed fields
    updatable_fields = ["os_version", "app_version", "device_name"]
    for field in updatable_fields:
        if field in data:
            config_entry[field] = data[field]

    # Save store
    from . import _async_save_store

    await _async_save_store(hass)

    _LOGGER.info("Updated registration for device %s", device_id)

    return webhook_response({"success": True})

"""HTTP API for Desktop App device registration."""

from __future__ import annotations

import logging
import secrets
from typing import Any

from aiohttp.web import Request, Response

from homeassistant.core import HomeAssistant
from homeassistant.helpers.http import HomeAssistantView

from .const import (
    ATTR_APP_VERSION,
    ATTR_DEVICE_ID,
    ATTR_DEVICE_NAME,
    ATTR_MANUFACTURER,
    ATTR_MODEL,
    ATTR_OS_NAME,
    ATTR_OS_VERSION,
    ATTR_WEBHOOK_ID,
    DOMAIN,
)
from .helpers import error_response, registration_response

_LOGGER = logging.getLogger(__name__)

REGISTRATION_SCHEMA_REQUIRED = [ATTR_DEVICE_ID, ATTR_DEVICE_NAME]
REGISTRATION_SCHEMA_OPTIONAL = [
    ATTR_MANUFACTURER,
    ATTR_MODEL,
    ATTR_OS_NAME,
    ATTR_OS_VERSION,
    ATTR_APP_VERSION,
]


class DesktopAppRegistrationView(HomeAssistantView):
    """Handle Desktop App device registrations."""

    url = "/api/desktop_app/registrations"
    name = "api:desktop_app:registrations"
    requires_auth = True

    async def get(self, request: Request) -> Response:
        """Allow checking that the registration endpoint exists (returns 401 without auth)."""
        return self.json_message(
            "Desktop App registration API; use POST with device_id and device_name"
        )

    async def post(self, request: Request) -> Response:
        """Handle device registration."""
        hass: HomeAssistant = request.app["hass"]

        try:
            data: dict[str, Any] = await request.json()
        except ValueError:
            return error_response("Invalid JSON", status=400)

        # Validate required fields
        for field in REGISTRATION_SCHEMA_REQUIRED:
            if field not in data:
                return error_response(f"Missing required field: {field}", status=400)

        device_id = data[ATTR_DEVICE_ID]

        # Check if device is already registered
        existing_entries = hass.data.get(DOMAIN, {}).get("config_entries", {})
        for entry_id, entry_data in existing_entries.items():
            if entry_data.get(ATTR_DEVICE_ID) == device_id:
                # Device already registered, return existing webhook_id
                _LOGGER.info(
                    "Device %s already registered, returning existing webhook_id",
                    device_id,
                )
                return registration_response(entry_data[ATTR_WEBHOOK_ID])

        # Generate webhook_id
        webhook_id = secrets.token_hex(32)

        # Build registration data
        registration = {
            ATTR_DEVICE_ID: device_id,
            ATTR_DEVICE_NAME: data[ATTR_DEVICE_NAME],
            ATTR_WEBHOOK_ID: webhook_id,
        }

        # Add optional fields
        for field in REGISTRATION_SCHEMA_OPTIONAL:
            if field in data:
                registration[field] = data[field]

        _LOGGER.info("Registering new Desktop App device: %s", device_id)

        # Start config flow with registration source
        result = await hass.config_entries.flow.async_init(
            DOMAIN,
            context={"source": "registration"},
            data=registration,
        )

        if result.get("type") == "create_entry":
            _LOGGER.info("Device %s registered successfully", device_id)
            return registration_response(webhook_id)

        _LOGGER.error("Failed to create config entry for device %s", device_id)
        return error_response("Failed to register device", status=500)

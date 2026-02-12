"""Helper utilities for the Desktop App integration."""

from __future__ import annotations

from typing import Any

from aiohttp.web import Response, json_response

from homeassistant.core import HomeAssistant
from homeassistant.helpers import device_registry as dr

from .const import (
    ATTR_APP_VERSION,
    ATTR_DEVICE_ID,
    ATTR_DEVICE_NAME,
    ATTR_MANUFACTURER,
    ATTR_MODEL,
    ATTR_OS_NAME,
    ATTR_OS_VERSION,
    DOMAIN,
)


def webhook_response(data: dict[str, Any] | None = None, status: int = 200) -> Response:
    """Create a webhook response."""
    if data is None:
        data = {}
    return json_response(data, status=status)


def error_response(message: str, status: int = 400) -> Response:
    """Create an error response."""
    return json_response({"success": False, "error": message}, status=status)


def registration_response(webhook_id: str) -> Response:
    """Create a registration success response."""
    return json_response(
        {
            "success": True,
            "webhook_id": webhook_id,
        }
    )


def get_device_info(registration: dict[str, Any]) -> dr.DeviceInfo:
    """Build device info dict from registration data."""
    return dr.DeviceInfo(
        identifiers={(DOMAIN, registration[ATTR_DEVICE_ID])},
        name=registration.get(ATTR_DEVICE_NAME, "Desktop App"),
        manufacturer=registration.get(ATTR_MANUFACTURER, "Unknown"),
        model=registration.get(ATTR_MODEL, "Desktop"),
        sw_version=registration.get(ATTR_APP_VERSION),
    )


def get_device_name(registration: dict[str, Any]) -> str:
    """Get device name from registration data."""
    return registration.get(ATTR_DEVICE_NAME, "Desktop App")

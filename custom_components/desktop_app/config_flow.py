"""Config flow for Desktop App integration."""

from __future__ import annotations

import logging
from typing import Any

from homeassistant.config_entries import ConfigFlow, ConfigFlowResult

from .const import (
    ATTR_DEVICE_ID,
    ATTR_DEVICE_NAME,
    DOMAIN,
)

_LOGGER = logging.getLogger(__name__)


class DesktopAppConfigFlow(ConfigFlow, domain=DOMAIN):
    """Handle a config flow for Desktop App."""

    VERSION = 1

    async def async_step_user(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        """Handle manual setup (show info message)."""
        if user_input is not None:
            return self.async_abort(reason="use_companion_app")

        return self.async_show_form(
            step_id="user",
            description_placeholders={},
        )

    async def async_step_registration(
        self, registration_data: dict[str, Any]
    ) -> ConfigFlowResult:
        """Handle registration from the desktop app."""
        device_id = registration_data[ATTR_DEVICE_ID]
        device_name = registration_data.get(ATTR_DEVICE_NAME, "Desktop App")

        # Set unique ID based on device_id to prevent duplicates
        await self.async_set_unique_id(device_id)
        self._abort_if_unique_id_configured(updates=registration_data)

        _LOGGER.info(
            "Creating config entry for device: %s (%s)",
            device_name,
            device_id,
        )

        return self.async_create_entry(
            title=device_name,
            data=registration_data,
        )

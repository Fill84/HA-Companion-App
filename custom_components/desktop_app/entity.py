"""Base entity for the Desktop App integration."""

from __future__ import annotations

import logging
from typing import Any

from homeassistant.core import HomeAssistant, callback
from homeassistant.helpers.dispatcher import async_dispatcher_connect
from homeassistant.helpers.restore_state import RestoreEntity

from .const import (
    ATTR_DEVICE_ID,
    ATTR_SENSOR_ATTRIBUTES,
    ATTR_SENSOR_DEVICE_CLASS,
    ATTR_SENSOR_ENTITY_CATEGORY,
    ATTR_SENSOR_ICON,
    ATTR_SENSOR_NAME,
    ATTR_SENSOR_STATE,
    ATTR_SENSOR_STATE_CLASS,
    ATTR_SENSOR_UNIQUE_ID,
    ATTR_SENSOR_UNIT_OF_MEASUREMENT,
    ATTR_WEBHOOK_ID,
    DATA_PENDING_UPDATES,
    DOMAIN,
    SIGNAL_SENSOR_UPDATE,
)

_LOGGER = logging.getLogger(__name__)


class DesktopAppEntity(RestoreEntity):
    """Base class for Desktop App entities."""

    _attr_should_poll = False
    _attr_has_entity_name = True

    def __init__(
        self,
        hass: HomeAssistant,
        config_entry_data: dict[str, Any],
        sensor_data: dict[str, Any],
    ) -> None:
        """Initialize the entity."""
        self._config_entry_data = config_entry_data
        self._sensor_data = sensor_data

        device_id = config_entry_data[ATTR_DEVICE_ID]
        sensor_unique_id = sensor_data[ATTR_SENSOR_UNIQUE_ID]

        self._attr_unique_id = f"{device_id}_{sensor_unique_id}"
        self._attr_name = sensor_data.get(ATTR_SENSOR_NAME, sensor_unique_id)
        self._device_id = device_id
        self._sensor_unique_id = sensor_unique_id
        self._webhook_id = config_entry_data.get(ATTR_WEBHOOK_ID)

        # Set optional attributes (default icon for desktop app entities)
        self._attr_icon = sensor_data.get(ATTR_SENSOR_ICON) or "mdi:desktop-tower-monitor"

        if device_class := sensor_data.get(ATTR_SENSOR_DEVICE_CLASS):
            self._attr_device_class = device_class

        if unit := sensor_data.get(ATTR_SENSOR_UNIT_OF_MEASUREMENT):
            self._attr_native_unit_of_measurement = unit

        if state_class := sensor_data.get(ATTR_SENSOR_STATE_CLASS):
            self._attr_state_class = state_class

        if entity_category := sensor_data.get(ATTR_SENSOR_ENTITY_CATEGORY):
            self._attr_entity_category = entity_category

        self._attr_extra_state_attributes = sensor_data.get(
            ATTR_SENSOR_ATTRIBUTES, {}
        )

    @property
    def device_info(self):
        """Return device info linking to the registered device."""
        return {
            "identifiers": {(DOMAIN, self._device_id)},
        }

    async def async_added_to_hass(self) -> None:
        """Handle entity added to hass."""
        await super().async_added_to_hass()

        # Restore previous state if available
        if (last_state := await self.async_get_last_state()) is not None:
            self._handle_restore(last_state)

        # Connect dispatcher listener for updates
        signal = SIGNAL_SENSOR_UPDATE.format(
            self._device_id, self._sensor_unique_id
        )
        self.async_on_remove(
            async_dispatcher_connect(self.hass, signal, self._handle_update)
        )

        # Apply any pending updates
        pending = self.hass.data[DOMAIN][DATA_PENDING_UPDATES].get(
            self._webhook_id, {}
        )
        unique_store_key = f"{self._device_id}_{self._sensor_unique_id}"
        if unique_store_key in pending:
            self._handle_update(pending.pop(unique_store_key))

    @callback
    def _handle_update(self, update_data: dict[str, Any]) -> None:
        """Handle a sensor state update."""
        if ATTR_SENSOR_STATE in update_data:
            self._update_state(update_data[ATTR_SENSOR_STATE])

        if ATTR_SENSOR_ICON in update_data and update_data[ATTR_SENSOR_ICON]:
            self._attr_icon = update_data[ATTR_SENSOR_ICON]

        if ATTR_SENSOR_ATTRIBUTES in update_data:
            self._attr_extra_state_attributes = update_data[ATTR_SENSOR_ATTRIBUTES]

        self.async_write_ha_state()

    def _update_state(self, state: Any) -> None:
        """Update the entity state. Override in subclasses."""
        pass

    def _handle_restore(self, last_state) -> None:
        """Handle state restore. Override in subclasses."""
        pass

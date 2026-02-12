"""Binary sensor platform for the Desktop App integration."""

from __future__ import annotations

import logging
from typing import Any

from homeassistant.components.binary_sensor import BinarySensorEntity
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant, callback
from homeassistant.helpers.dispatcher import async_dispatcher_connect
from homeassistant.helpers.entity_platform import AddEntitiesCallback
from homeassistant.helpers.entity_registry import async_get as async_get_entity_registry

from .const import (
    ATTR_DEVICE_ID,
    ATTR_SENSOR_STATE,
    ATTR_SENSOR_TYPE,
    ATTR_SENSOR_UNIQUE_ID,
    DOMAIN,
    SIGNAL_SENSOR_REGISTER,
)
from .entity import DesktopAppEntity

_LOGGER = logging.getLogger(__name__)


class DesktopAppBinarySensor(DesktopAppEntity, BinarySensorEntity):
    """Representation of a Desktop App binary sensor."""

    def _update_state(self, state: Any) -> None:
        """Update binary sensor state."""
        if isinstance(state, bool):
            self._attr_is_on = state
        elif isinstance(state, str):
            self._attr_is_on = state.lower() in ("true", "on", "1", "yes")
        else:
            self._attr_is_on = bool(state)

    def _handle_restore(self, last_state) -> None:
        """Restore binary sensor state."""
        if last_state.state not in (None, "unknown", "unavailable"):
            self._attr_is_on = last_state.state == "on"


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddEntitiesCallback,
) -> None:
    """Set up Desktop App binary sensor platform."""
    registration = entry.data
    device_id = registration[ATTR_DEVICE_ID]

    # Restore existing binary sensor entities from entity registry
    entity_registry = async_get_entity_registry(hass)
    existing_entities = []
    registered_sensors = hass.data[DOMAIN].get("registered_sensors", {})

    for entity_entry in entity_registry.entities.get_entries_for_config_entry_id(
        entry.entry_id
    ):
        if entity_entry.domain != "binary_sensor":
            continue

        unique_id = entity_entry.unique_id
        if unique_id in registered_sensors:
            sensor_data = registered_sensors[unique_id]
            existing_entities.append(
                DesktopAppBinarySensor(hass, registration, sensor_data)
            )
            _LOGGER.debug("Restoring binary sensor entity: %s", unique_id)

    if existing_entities:
        async_add_entities(existing_entities)

    # Listen for new binary sensor registrations
    @callback
    def _handle_sensor_register(sensor_data: dict[str, Any]) -> None:
        """Handle new binary sensor registration."""
        if sensor_data.get(ATTR_SENSOR_TYPE) != "binary_sensor":
            return

        _LOGGER.info(
            "Adding new binary sensor: %s",
            sensor_data.get(ATTR_SENSOR_UNIQUE_ID),
        )
        async_add_entities(
            [DesktopAppBinarySensor(hass, registration, sensor_data)]
        )

    signal = SIGNAL_SENSOR_REGISTER.format(device_id, "binary_sensor")
    entry.async_on_unload(
        async_dispatcher_connect(hass, signal, _handle_sensor_register)
    )

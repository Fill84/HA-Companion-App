"""Sensor platform for the Desktop App integration."""

from __future__ import annotations

import logging
from typing import Any

from homeassistant.components.sensor import SensorEntity
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


class DesktopAppSensor(DesktopAppEntity, SensorEntity):
    """Representation of a Desktop App sensor."""

    def _update_state(self, state: Any) -> None:
        """Update sensor state."""
        self._attr_native_value = state

    def _handle_restore(self, last_state) -> None:
        """Restore sensor state."""
        if last_state.state not in (None, "unknown", "unavailable"):
            self._attr_native_value = last_state.state


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddEntitiesCallback,
) -> None:
    """Set up Desktop App sensor platform."""
    registration = entry.data
    device_id = registration[ATTR_DEVICE_ID]

    # Restore existing sensor entities from entity registry
    entity_registry = async_get_entity_registry(hass)
    existing_entities = []
    registered_sensors = hass.data[DOMAIN].get("registered_sensors", {})

    for entity_entry in entity_registry.entities.get_entries_for_config_entry_id(
        entry.entry_id
    ):
        if entity_entry.domain != "sensor":
            continue

        unique_id = entity_entry.unique_id
        # Check if we have sensor data stored
        if unique_id in registered_sensors:
            sensor_data = registered_sensors[unique_id]
            existing_entities.append(
                DesktopAppSensor(hass, registration, sensor_data)
            )
            _LOGGER.debug("Restoring sensor entity: %s", unique_id)

    if existing_entities:
        async_add_entities(existing_entities)

    # Listen for new sensor registrations
    @callback
    def _handle_sensor_register(sensor_data: dict[str, Any]) -> None:
        """Handle new sensor registration."""
        if sensor_data.get(ATTR_SENSOR_TYPE) != "sensor":
            return

        _LOGGER.info(
            "Adding new sensor: %s",
            sensor_data.get(ATTR_SENSOR_UNIQUE_ID),
        )
        async_add_entities(
            [DesktopAppSensor(hass, registration, sensor_data)]
        )

    signal = SIGNAL_SENSOR_REGISTER.format(device_id, "sensor")
    entry.async_on_unload(
        async_dispatcher_connect(hass, signal, _handle_sensor_register)
    )

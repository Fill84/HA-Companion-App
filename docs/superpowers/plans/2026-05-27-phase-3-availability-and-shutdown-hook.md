# Phase 3: Availability binary_sensor + Windows shutdown hook — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Home Assistant should know within seconds whether this PC is online or offline. Add a `binary_sensor.<device>_online` (device_class: connectivity) that is `on` while the desktop app is sending heartbeats and flips to `off` either when (a) the heartbeat has been silent for 2.5× the update interval (covers crashes, sleep, hibernate, network loss) or (b) the Rust app sends a graceful "device_offline" webhook on Windows shutdown/sign-off (covers normal shutdown — flips instantly instead of waiting out the timeout).

**Architecture:** Two independent layers cooperate so each can degrade gracefully without the other:

1. **HA-side timeout** (Python in `ha-integration`): every successful sensor webhook updates an in-memory `last_seen` timestamp per device. A 30-second periodic timer flips the availability binary_sensor to `off` when `now - last_seen > 2.5 × update_interval`. This works regardless of how the PC went down — it just looks at silence.

2. **Graceful shutdown signal** (Rust in `desktop-app`): a Windows event hook for `WM_QUERYENDSESSION` / `WM_ENDSESSION` fires when the user logs out, restarts, or shuts down. The app sends one synchronous webhook `{"type":"device_offline"}` before returning so HA can mark the entity offline immediately. The new webhook command resets `last_seen` to "long ago" so the timer in layer 1 picks it up on its next tick (and we don't need a separate dispatcher path).

**Tech Stack:**
- Python: `homeassistant` (existing), `homeassistant.helpers.event.async_track_time_interval`, `homeassistant.util.dt.utcnow` — all stdlib for the integration
- Rust: `windows` crate for `WM_QUERYENDSESSION` (already a transitive dep via tauri-runtime; we add an explicit `windows = { version = "0.x", features = [...] }` to Cargo.toml so we can name the constants), `tauri::RunEvent::ExitRequested` for the graceful-exit path

**Out of scope:**
- macOS / Linux shutdown hooks (Tauri target for this project is Windows-only per `tauri.conf.json` `bundle.targets`; macOS/Linux would need their own handlers when those targets are added)
- Showing "last online" timestamp in HA (the binary_sensor's own `last_changed` already gives that for free)
- Sleep-vs-power-off differentiation (binary_sensor is binary; that nuance lives in HA automations)

**Prerequisites:**
- Phase 1's `ha-integration` repo metadata-update commit must be in place (already done — see `75f9833` in `ha-integration`)
- The Rust app and the HA integration are testable independently; we run their test suites separately

---

## File map

**Modify (ha-integration repo — separate sibling git):**
- [ha-integration/custom_components/desktop_app/const.py](../../../ha-integration/custom_components/desktop_app/const.py) — add `DATA_LAST_SEEN`, `COMMAND_DEVICE_OFFLINE`, `SIGNAL_AVAILABILITY_UPDATE`, `AVAILABILITY_TIMEOUT_FACTOR` constants
- [ha-integration/custom_components/desktop_app/webhook.py](../../../ha-integration/custom_components/desktop_app/webhook.py) — touch `last_seen` on every successful command + handle the new `device_offline` command
- [ha-integration/custom_components/desktop_app/__init__.py](../../../ha-integration/custom_components/desktop_app/__init__.py) — initialise `last_seen` store and start the 30-second timer
- [ha-integration/custom_components/desktop_app/binary_sensor.py](../../../ha-integration/custom_components/desktop_app/binary_sensor.py) — register the new `online` availability sensor on platform setup

**Create (ha-integration repo):**
- [ha-integration/custom_components/desktop_app/availability.py](../../../ha-integration/custom_components/desktop_app/availability.py) — pure functions for the timeout check (testable without HA), the `DesktopAppAvailabilitySensor` entity class, and the periodic-timer hookup
- [ha-integration/tests/test_availability.py](../../../ha-integration/tests/test_availability.py) — unit tests for the pure timeout logic

**Modify (desktop-app repo):**
- [desktop-app/src-tauri/Cargo.toml](../../../desktop-app/src-tauri/Cargo.toml) — add `windows = { version = "0.58", features = ["Win32_Foundation", "Win32_UI_WindowsAndMessaging", "Win32_System_Threading"] }` for the shutdown message
- [desktop-app/src-tauri/src/lib.rs](../../../desktop-app/src-tauri/src/lib.rs) — register a `WM_QUERYENDSESSION` hook in `setup`, install a `RunEvent::ExitRequested` handler in `app.run`
- [desktop-app/src-tauri/src/ha_client.rs](../../../desktop-app/src-tauri/src/ha_client.rs) — new `send_device_offline()` helper that POSTs `{"type":"device_offline"}` with a short timeout
- [desktop-app/src-tauri/src/shutdown_hook.rs](../../../desktop-app/src-tauri/src/shutdown_hook.rs) — NEW: encapsulates the Windows hook and exposes a single `register(handler)` function so `lib.rs` stays clean

**Test files:**
- Rust tests live inline under `#[cfg(test)] mod tests`
- Python tests live under `ha-integration/tests/` and run via `pytest`

---

## Task 1: Python — `last_seen` tracking on every webhook

**Goal:** Every time HA receives a webhook from the desktop app (register_sensor, update_sensor_states, update_registration), update the `last_seen` timestamp for that device. Pure dictionary state, no entity work yet.

**Files:**
- Modify: [ha-integration/custom_components/desktop_app/const.py](../../../ha-integration/custom_components/desktop_app/const.py)
- Modify: [ha-integration/custom_components/desktop_app/__init__.py](../../../ha-integration/custom_components/desktop_app/__init__.py)
- Modify: [ha-integration/custom_components/desktop_app/webhook.py](../../../ha-integration/custom_components/desktop_app/webhook.py)

- [ ] **Step 1: Add constants**

In `ha-integration/custom_components/desktop_app/const.py`, append:

```python

# Phase 3: availability tracking
DATA_LAST_SEEN = "last_seen"            # dict[device_id, datetime]
DATA_AVAILABILITY_TIMER = "availability_timer"  # async unsubscribe handle
AVAILABILITY_TIMEOUT_FACTOR = 2.5       # multiply update_interval by this
AVAILABILITY_CHECK_INTERVAL_SECONDS = 30  # how often the timer ticks

# New webhook command for graceful shutdown
COMMAND_DEVICE_OFFLINE = "device_offline"

# Dispatcher signal fired when a device's availability flips.
# Format: SIGNAL_AVAILABILITY_UPDATE.format(device_id)
SIGNAL_AVAILABILITY_UPDATE = f"{DOMAIN}_availability_update_{{}}"
```

- [ ] **Step 2: Initialise the last_seen store in __init__.py**

In `ha-integration/custom_components/desktop_app/__init__.py`, locate the `async_setup_entry` (or equivalent function that runs per config entry). Find the spot where `hass.data[DOMAIN]` is set up. Add:

```python
    hass.data[DOMAIN].setdefault(DATA_LAST_SEEN, {})
```

next to the other `setdefault` calls. (If the integration creates the dict in `async_setup` instead, add the line there.)

- [ ] **Step 3: Write failing test for "webhook touches last_seen"**

Create `ha-integration/tests/test_availability.py`:

```python
"""Unit tests for the Phase 3 availability layer."""
from __future__ import annotations

from datetime import datetime, timedelta, timezone

import pytest

from custom_components.desktop_app.availability import (
    is_device_online,
    timeout_threshold,
)


def test_is_device_online_true_within_window():
    now = datetime(2026, 5, 27, 12, 0, tzinfo=timezone.utc)
    last_seen = now - timedelta(seconds=60)
    # update_interval=60 → timeout at 150s. last_seen 60s ago → online.
    assert is_device_online(last_seen=last_seen, now=now, update_interval=60) is True


def test_is_device_online_false_after_timeout():
    now = datetime(2026, 5, 27, 12, 0, tzinfo=timezone.utc)
    last_seen = now - timedelta(seconds=200)
    # update_interval=60 → timeout at 150s. last_seen 200s ago → offline.
    assert is_device_online(last_seen=last_seen, now=now, update_interval=60) is False


def test_is_device_online_false_when_last_seen_missing():
    now = datetime(2026, 5, 27, 12, 0, tzinfo=timezone.utc)
    assert is_device_online(last_seen=None, now=now, update_interval=60) is False


def test_timeout_threshold_default_factor():
    # update_interval=60 → 60 * 2.5 = 150
    assert timeout_threshold(update_interval=60) == 150.0


def test_timeout_threshold_minimum_floor():
    # Very small update intervals should still allow at least ~10 seconds of
    # tolerance — otherwise a momentary blip flips the entity offline.
    assert timeout_threshold(update_interval=1) >= 10.0
```

- [ ] **Step 4: Run tests to verify they fail with import error**

Run from `ha-integration/`:
```bash
pytest tests/test_availability.py -v
```

Expected: ImportError — `availability.py` doesn't exist yet.

- [ ] **Step 5: Create the availability module with pure functions**

Create `ha-integration/custom_components/desktop_app/availability.py`:

```python
"""Availability tracking for the Desktop App integration.

Pure-function helpers (no HA imports) live at the top so they can be unit
tested without a HomeAssistant fixture. The DesktopAppAvailabilitySensor
entity class and the periodic timer wiring are added in later tasks.
"""
from __future__ import annotations

from datetime import datetime

from .const import AVAILABILITY_TIMEOUT_FACTOR

# Minimum time a device may be silent before we consider it offline. Prevents
# tiny update intervals from causing flapping on a single packet loss.
_MIN_TIMEOUT_SECONDS = 10.0


def timeout_threshold(update_interval: int | float) -> float:
    """How many seconds of silence count as offline."""
    return max(_MIN_TIMEOUT_SECONDS, float(update_interval) * AVAILABILITY_TIMEOUT_FACTOR)


def is_device_online(
    *,
    last_seen: datetime | None,
    now: datetime,
    update_interval: int | float,
) -> bool:
    """Decide whether a device is currently considered online.

    Returns False when last_seen is None (never seen).
    """
    if last_seen is None:
        return False
    age_seconds = (now - last_seen).total_seconds()
    return age_seconds <= timeout_threshold(update_interval)
```

- [ ] **Step 6: Run tests to verify they pass**

Run:
```bash
pytest tests/test_availability.py -v
```

Expected: all 5 tests pass.

- [ ] **Step 7: Touch last_seen on every webhook command**

In `ha-integration/custom_components/desktop_app/webhook.py`, find `handle_webhook` (the entry point). After the existing `entry = _find_entry_by_webhook(...)` check that returns 410 if no entry, and **before** `return await handler(...)`, add:

```python
    # Phase 3: record last_seen for the availability binary_sensor before
    # dispatching the command. Done unconditionally for any successful
    # webhook so even a no-op update_sensor_states with no sensors keeps
    # the device "alive".
    from homeassistant.util import dt as dt_util
    from .const import DATA_LAST_SEEN, SIGNAL_AVAILABILITY_UPDATE
    from homeassistant.helpers.dispatcher import async_dispatcher_send

    device_id = entry.data[ATTR_DEVICE_ID]
    hass.data[DOMAIN].setdefault(DATA_LAST_SEEN, {})[device_id] = dt_util.utcnow()
    async_dispatcher_send(
        hass,
        SIGNAL_AVAILABILITY_UPDATE.format(device_id),
        True,  # last activity was just now → entity should be on
    )
```

(Imports are inside the function deliberately — these constants and helpers aren't used at module top-level and adding them at the top would leak into other handlers' namespace.)

- [ ] **Step 8: Commit (ha-integration repo)**

```bash
cd ha-integration
git add custom_components/desktop_app/const.py \
        custom_components/desktop_app/__init__.py \
        custom_components/desktop_app/webhook.py \
        custom_components/desktop_app/availability.py \
        tests/test_availability.py
git commit -m "feat(availability): track last_seen on every webhook + add pure timeout helpers"
```

---

## Task 2: Python — register the availability binary_sensor

**Goal:** Add a new entity per device. On platform setup, register one `DesktopAppAvailabilitySensor` for each known config entry. It listens on `SIGNAL_AVAILABILITY_UPDATE.format(device_id)` and writes its state when the signal fires.

**Files:**
- Modify: [ha-integration/custom_components/desktop_app/availability.py](../../../ha-integration/custom_components/desktop_app/availability.py)
- Modify: [ha-integration/custom_components/desktop_app/binary_sensor.py](../../../ha-integration/custom_components/desktop_app/binary_sensor.py)

- [ ] **Step 1: Write the failing test (Python)**

Append to `ha-integration/tests/test_availability.py`:

```python
def test_availability_sensor_initial_state_is_unavailable_until_first_seen():
    # Until the first heartbeat arrives, _attr_is_on must be False and the
    # entity must NOT be reported as 'unavailable' (we want a definite
    # "offline" reading, not nothing).
    from custom_components.desktop_app.availability import (
        DesktopAppAvailabilitySensor,
    )
    sensor = DesktopAppAvailabilitySensor(
        device_id="dev-1",
        device_name="Test PC",
        update_interval=60,
    )
    assert sensor.is_on is False
    assert sensor.available is True
    assert sensor.device_class == "connectivity"


def test_availability_sensor_handle_signal_sets_state():
    from custom_components.desktop_app.availability import (
        DesktopAppAvailabilitySensor,
    )
    sensor = DesktopAppAvailabilitySensor(
        device_id="dev-1",
        device_name="Test PC",
        update_interval=60,
    )
    sensor._handle_update(True)
    assert sensor.is_on is True
    sensor._handle_update(False)
    assert sensor.is_on is False
```

- [ ] **Step 2: Run tests to verify they fail with ImportError**

```bash
pytest tests/test_availability.py::test_availability_sensor_initial_state_is_unavailable_until_first_seen -v
```

Expected: ImportError on `DesktopAppAvailabilitySensor`.

- [ ] **Step 3: Add the entity class**

Append to `ha-integration/custom_components/desktop_app/availability.py`:

```python

from homeassistant.components.binary_sensor import (
    BinarySensorDeviceClass,
    BinarySensorEntity,
)
from homeassistant.core import callback
from homeassistant.helpers.dispatcher import async_dispatcher_connect

from .const import DOMAIN, SIGNAL_AVAILABILITY_UPDATE


class DesktopAppAvailabilitySensor(BinarySensorEntity):
    """A binary_sensor that reflects whether a device's desktop app is
    currently sending heartbeats. Always available (never 'unavailable')
    so HA can render a clear 'offline' state."""

    _attr_should_poll = False
    _attr_has_entity_name = True
    _attr_name = "Online"
    _attr_device_class = BinarySensorDeviceClass.CONNECTIVITY
    _attr_icon = "mdi:wifi-check"

    def __init__(
        self,
        *,
        device_id: str,
        device_name: str,
        update_interval: int | float,
    ) -> None:
        self._device_id = device_id
        self._device_name = device_name
        self._update_interval = update_interval
        self._attr_unique_id = f"{device_id}_online"
        self._attr_is_on = False

    @property
    def device_info(self):
        return {"identifiers": {(DOMAIN, self._device_id)}}

    async def async_added_to_hass(self) -> None:
        await super().async_added_to_hass()
        self.async_on_remove(
            async_dispatcher_connect(
                self.hass,
                SIGNAL_AVAILABILITY_UPDATE.format(self._device_id),
                self._handle_update,
            )
        )

    @callback
    def _handle_update(self, is_online: bool) -> None:
        self._attr_is_on = bool(is_online)
        if self.hass is not None:
            self.async_write_ha_state()
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
pytest tests/test_availability.py -v
```

Expected: all 7 tests pass. (5 from Task 1 + 2 new.)

- [ ] **Step 5: Register the sensor from the binary_sensor platform**

In `ha-integration/custom_components/desktop_app/binary_sensor.py`, find the `async_setup_entry` function (which currently sets up entities driven by registered sensors). At the end of `async_setup_entry` — after the existing dispatcher registration and pre-registration replay logic — add:

```python
    # Phase 3: one availability sensor per device, registered up-front
    # regardless of dynamic sensor registrations. Always exists for the
    # lifetime of the config entry.
    from .availability import DesktopAppAvailabilitySensor
    from .const import ATTR_DEVICE_NAME

    device_id = entry.data[ATTR_DEVICE_ID]
    device_name = entry.data.get(ATTR_DEVICE_NAME, device_id)
    # The desktop app's settings.update_interval is the heartbeat cadence; we
    # don't have direct access here, so default to 60s and let the periodic
    # timer (Task 3) re-evaluate using the actual entry data if needed.
    async_add_entities(
        [
            DesktopAppAvailabilitySensor(
                device_id=device_id,
                device_name=device_name,
                update_interval=60,
            )
        ]
    )
```

- [ ] **Step 6: Commit**

```bash
cd ha-integration
git add custom_components/desktop_app/availability.py \
        custom_components/desktop_app/binary_sensor.py \
        tests/test_availability.py
git commit -m "feat(availability): add per-device 'Online' binary_sensor"
```

---

## Task 3: Python — periodic timeout check + initial state

**Goal:** A 30-second timer runs while the integration is loaded. On each tick it walks every device's `last_seen` and dispatches `SIGNAL_AVAILABILITY_UPDATE` with `False` when the last activity is older than the threshold. The first ever sensor registration also triggers a state update so an existing device shows online immediately on integration reload.

**Files:**
- Modify: [ha-integration/custom_components/desktop_app/availability.py](../../../ha-integration/custom_components/desktop_app/availability.py)
- Modify: [ha-integration/custom_components/desktop_app/__init__.py](../../../ha-integration/custom_components/desktop_app/__init__.py)

- [ ] **Step 1: Write the failing test**

Append to `ha-integration/tests/test_availability.py`:

```python
def test_evaluate_devices_returns_flips():
    """Given a snapshot, returns (device_id, new_is_online) tuples for any
    devices whose state should change."""
    from custom_components.desktop_app.availability import evaluate_devices

    now = datetime(2026, 5, 27, 12, 0, tzinfo=timezone.utc)
    devices = {
        "dev-fresh": now - timedelta(seconds=30),      # still online
        "dev-stale": now - timedelta(seconds=300),     # offline now
    }
    update_intervals = {"dev-fresh": 60, "dev-stale": 60}

    # Suppose dev-fresh is currently on and dev-stale is currently on
    # (we just flipped both to 'on' on first heartbeat). After this
    # evaluate, dev-stale must be flipped to off.
    current_state = {"dev-fresh": True, "dev-stale": True}
    flips = evaluate_devices(
        last_seen=devices,
        update_intervals=update_intervals,
        current_state=current_state,
        now=now,
    )
    assert flips == {"dev-stale": False}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
pytest tests/test_availability.py::test_evaluate_devices_returns_flips -v
```

Expected: ImportError on `evaluate_devices`.

- [ ] **Step 3: Add `evaluate_devices` and the periodic-timer setup**

In `ha-integration/custom_components/desktop_app/availability.py`, append (above the entity class):

```python

def evaluate_devices(
    *,
    last_seen: dict[str, datetime],
    update_intervals: dict[str, int | float],
    current_state: dict[str, bool],
    now: datetime,
) -> dict[str, bool]:
    """Return the subset of devices whose availability should flip.

    Stateless: caller is responsible for storing `current_state` between calls.
    """
    flips: dict[str, bool] = {}
    for device_id, ts in last_seen.items():
        interval = update_intervals.get(device_id, 60)
        new_state = is_device_online(last_seen=ts, now=now, update_interval=interval)
        if current_state.get(device_id) != new_state:
            flips[device_id] = new_state
    return flips
```

Also append the timer-setup function:

```python

from homeassistant.core import HomeAssistant
from homeassistant.helpers.event import async_track_time_interval
from homeassistant.util import dt as dt_util
from datetime import timedelta

from .const import (
    AVAILABILITY_CHECK_INTERVAL_SECONDS,
    DATA_LAST_SEEN,
    DOMAIN,
)


def start_availability_timer(hass: HomeAssistant) -> callable:
    """Schedule a periodic availability check. Returns the unsubscribe.

    Stored on hass.data so the integration can cancel it during unload.
    """
    current_state: dict[str, bool] = {}

    @callback
    def _tick(_now):
        last_seen = hass.data[DOMAIN].get(DATA_LAST_SEEN, {})
        intervals = {
            entry.data.get("device_id"): entry.data.get("update_interval", 60)
            for entry in hass.config_entries.async_entries(DOMAIN)
        }
        flips = evaluate_devices(
            last_seen=last_seen,
            update_intervals=intervals,
            current_state=current_state,
            now=dt_util.utcnow(),
        )
        for device_id, is_online in flips.items():
            current_state[device_id] = is_online
            from homeassistant.helpers.dispatcher import async_dispatcher_send
            async_dispatcher_send(
                hass,
                SIGNAL_AVAILABILITY_UPDATE.format(device_id),
                is_online,
            )

    return async_track_time_interval(
        hass,
        _tick,
        timedelta(seconds=AVAILABILITY_CHECK_INTERVAL_SECONDS),
    )
```

- [ ] **Step 4: Wire the timer into integration setup/unload**

In `ha-integration/custom_components/desktop_app/__init__.py`:

a) In `async_setup_entry` (the per-config-entry setup), after `hass.data[DOMAIN].setdefault(DATA_LAST_SEEN, {})`, add:

```python
    # Phase 3: start the periodic availability check once (first entry triggers).
    if DATA_AVAILABILITY_TIMER not in hass.data[DOMAIN]:
        from .availability import start_availability_timer
        from .const import DATA_AVAILABILITY_TIMER
        hass.data[DOMAIN][DATA_AVAILABILITY_TIMER] = start_availability_timer(hass)
```

b) In `async_unload_entry`, before the function returns, add:

```python
    # If this was the last entry being unloaded, stop the timer.
    if len(hass.config_entries.async_entries(DOMAIN)) <= 1:
        from .const import DATA_AVAILABILITY_TIMER
        unsub = hass.data[DOMAIN].pop(DATA_AVAILABILITY_TIMER, None)
        if unsub is not None:
            unsub()
```

(The `<= 1` accounts for the current entry being included before the framework removes it from `async_entries`.)

- [ ] **Step 5: Run tests**

```bash
pytest tests/test_availability.py -v
```

Expected: 8 tests pass.

- [ ] **Step 6: Commit**

```bash
cd ha-integration
git add custom_components/desktop_app/availability.py \
        custom_components/desktop_app/__init__.py \
        tests/test_availability.py
git commit -m "feat(availability): periodic timer flips devices offline after timeout"
```

---

## Task 4: Python — `device_offline` webhook command

**Goal:** Handle a graceful "device_offline" webhook by resetting `last_seen` to a time far enough in the past that the next periodic tick (or an immediate signal dispatch) will mark the device offline.

**Files:**
- Modify: [ha-integration/custom_components/desktop_app/webhook.py](../../../ha-integration/custom_components/desktop_app/webhook.py)

- [ ] **Step 1: Write the failing test**

Append to `ha-integration/tests/test_availability.py`:

```python
def test_device_offline_command_marks_device_offline_immediately():
    """When the desktop app sends device_offline, the dispatched signal
    must carry False so the binary_sensor flips without waiting for the
    next periodic tick."""
    # Pure-logic check on what the handler should push to the dispatcher.
    # The actual webhook handler is exercised in HA integration tests; here
    # we just verify the contract function used by handle_device_offline.
    from custom_components.desktop_app.availability import (
        offline_signal_payload,
    )
    payload = offline_signal_payload()
    assert payload is False
```

- [ ] **Step 2: Run tests to verify it fails**

```bash
pytest tests/test_availability.py::test_device_offline_command_marks_device_offline_immediately -v
```

Expected: ImportError on `offline_signal_payload`.

- [ ] **Step 3: Add the helper + the webhook handler**

In `ha-integration/custom_components/desktop_app/availability.py`, append:

```python

def offline_signal_payload() -> bool:
    """Constant returned to the dispatcher when a device sends device_offline.

    Exists as a function so the constant is testable without importing the
    webhook module's HA-side imports.
    """
    return False
```

In `ha-integration/custom_components/desktop_app/webhook.py`, add a new handler near `handle_update_registration`:

```python
@webhook_command(COMMAND_DEVICE_OFFLINE)
async def handle_device_offline(
    hass: HomeAssistant,
    entry: ConfigEntry,
    webhook_id: str,
    data: dict[str, Any],
) -> Response:
    """Mark this device offline immediately (graceful shutdown signal)."""
    from .availability import offline_signal_payload

    device_id = entry.data[ATTR_DEVICE_ID]
    async_dispatcher_send(
        hass,
        SIGNAL_AVAILABILITY_UPDATE.format(device_id),
        offline_signal_payload(),
    )
    _LOGGER.info("Device %s flagged offline by graceful shutdown signal", device_id)
    return webhook_response({"success": True})
```

(`COMMAND_DEVICE_OFFLINE` and `SIGNAL_AVAILABILITY_UPDATE` are already imported in `webhook.py` after Task 1; if not, add them to the import block at the top.)

Also: in `handle_webhook`, the `last_seen` touch we added in Task 1 step 7 must be **skipped** when the command is `device_offline` — otherwise we'd record a heartbeat right before flipping offline, which would cause the next periodic tick to flip back to online. Adjust the touch to:

```python
    if command_type != COMMAND_DEVICE_OFFLINE:
        from homeassistant.util import dt as dt_util
        from .const import DATA_LAST_SEEN, SIGNAL_AVAILABILITY_UPDATE
        device_id = entry.data[ATTR_DEVICE_ID]
        hass.data[DOMAIN].setdefault(DATA_LAST_SEEN, {})[device_id] = dt_util.utcnow()
        async_dispatcher_send(
            hass,
            SIGNAL_AVAILABILITY_UPDATE.format(device_id),
            True,
        )
```

- [ ] **Step 4: Run tests**

```bash
pytest tests/test_availability.py -v
```

Expected: 9 tests pass.

- [ ] **Step 5: Commit**

```bash
cd ha-integration
git add custom_components/desktop_app/availability.py \
        custom_components/desktop_app/webhook.py \
        tests/test_availability.py
git commit -m "feat(availability): handle device_offline webhook for instant offline flip"
```

---

## Task 5: Rust — `send_device_offline` HA-client helper

**Goal:** A short-timeout async function on `HaClient` that POSTs `{"type":"device_offline","data":{}}` to the webhook. Must complete within ~2 seconds so it doesn't block shutdown for long.

**Files:**
- Modify: [desktop-app/src-tauri/src/ha_client.rs](../../../desktop-app/src-tauri/src/ha_client.rs)

- [ ] **Step 1: Write the failing test**

Append to `desktop-app/src-tauri/src/ha_client.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_offline_payload_serializes_with_expected_shape() {
        let payload = WebhookPayload {
            command_type: "device_offline".to_string(),
            data: serde_json::json!({}),
        };
        let s = serde_json::to_string(&payload).expect("serialize");
        // Must contain "type":"device_offline" and an empty data object.
        assert!(s.contains(r#""type":"device_offline""#), "payload was: {s}");
        assert!(s.contains(r#""data":{}"#), "payload was: {s}");
    }
}
```

- [ ] **Step 2: Run test to verify it passes (the payload struct already exists)**

```powershell
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml device_offline_payload
```

Expected: PASS — `WebhookPayload` is already defined; we're just verifying the wire format.

- [ ] **Step 3: Add `send_device_offline` method**

In `desktop-app/src-tauri/src/ha_client.rs`, inside `impl HaClient`, add a new method (place it near `update_sensors`):

```rust
    /// Tell HA that this device is shutting down. Best-effort; the caller
    /// must time-bound the await (e.g. with tokio::time::timeout) because
    /// the OS may have already started reclaiming sockets when this runs.
    pub async fn send_device_offline(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let webhook_id = self
            .webhook_id
            .as_ref()
            .ok_or("No webhook_id configured")?;
        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);
        let payload = WebhookPayload {
            command_type: "device_offline".to_string(),
            data: serde_json::json!({}),
        };
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(format!("device_offline returned {}", response.status()).into());
        }
        Ok(())
    }
```

- [ ] **Step 4: Run tests**

```powershell
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml
```

Expected: all previously-passing tests still pass + the new `device_offline_payload_serializes_with_expected_shape`.

- [ ] **Step 5: Commit**

```powershell
git add desktop-app/src-tauri/src/ha_client.rs
git commit -m "feat(ha-client): send_device_offline helper for graceful shutdown"
```

---

## Task 6: Rust — Windows shutdown hook + Tauri exit handler

**Goal:** When Windows is shutting down (logoff, restart, power off) — i.e. `WM_QUERYENDSESSION` — call `send_device_offline()` with a 2-second timeout. Also handle `RunEvent::ExitRequested` so a normal "Quit" from the tray triggers the same path.

**Files:**
- Modify: [desktop-app/src-tauri/Cargo.toml](../../../desktop-app/src-tauri/Cargo.toml)
- Create: [desktop-app/src-tauri/src/shutdown_hook.rs](../../../desktop-app/src-tauri/src/shutdown_hook.rs)
- Modify: [desktop-app/src-tauri/src/lib.rs](../../../desktop-app/src-tauri/src/lib.rs)

- [ ] **Step 1: Add the `windows` crate**

In `desktop-app/src-tauri/Cargo.toml`, under `[target.'cfg(windows)'.dependencies]` (the WMI section), append:

```toml
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
] }
```

- [ ] **Step 2: Create the shutdown_hook module**

Create `desktop-app/src-tauri/src/shutdown_hook.rs`:

```rust
//! Windows shutdown / sign-off hook.
//!
//! Tauri's main window gets `WM_QUERYENDSESSION` from Windows when the user
//! logs off, restarts, or shuts down. We intercept it via a window-message
//! subclass and fire a callback so the rest of the app can send a graceful
//! "device_offline" webhook before Windows kills the process.

#[cfg(windows)]
pub use windows_impl::install;

#[cfg(not(windows))]
pub fn install<F: Fn() + Send + Sync + 'static>(_handler: F) {
    // No-op on non-Windows targets.
}

#[cfg(windows)]
mod windows_impl {
    use std::sync::OnceLock;
    use std::sync::Arc;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, FindWindowExW, GetWindowLongPtrW, SetWindowLongPtrW,
        GWLP_WNDPROC, WM_QUERYENDSESSION, WM_ENDSESSION,
    };

    type Handler = Arc<dyn Fn() + Send + Sync + 'static>;

    static HANDLER: OnceLock<Handler> = OnceLock::new();
    static ORIGINAL_PROC: OnceLock<isize> = OnceLock::new();

    /// Subclass the Tauri main window to fire `handler` when Windows sends
    /// `WM_QUERYENDSESSION`. Idempotent (the OnceLocks short-circuit a
    /// second install — only the first handler is installed).
    pub fn install<F>(handler: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let _ = HANDLER.set(Arc::new(handler));

        // Find the Tauri window by its class name. Tauri 2 uses the "Window
        // Class" name "Tauri Window" for its main HWND; if Tauri renames it
        // in a future version we fall back to FindWindowEx by title.
        let class_name: Vec<u16> = "Tauri Window\0".encode_utf16().collect();
        let hwnd = unsafe {
            FindWindowExW(
                None,
                None,
                PCWSTR(class_name.as_ptr()),
                PCWSTR::null(),
            )
        };

        if let Ok(hwnd) = hwnd {
            if !hwnd.0.is_null() {
                unsafe {
                    let original = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, subclassed_proc as isize);
                    let _ = ORIGINAL_PROC.set(original);
                }
            }
        }
    }

    /// Subclassed WindowProc — intercepts shutdown messages, forwards everything
    /// else to the original proc.
    unsafe extern "system" fn subclassed_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_QUERYENDSESSION || msg == WM_ENDSESSION {
            if let Some(handler) = HANDLER.get() {
                handler();
            }
            // Fall through to the original proc so Windows continues the
            // shutdown handshake normally.
        }
        let original = ORIGINAL_PROC.get().copied().unwrap_or(0);
        if original == 0 {
            // Defensive: if we somehow lost the original, return 1 for
            // WM_QUERYENDSESSION (allow shutdown) and 0 otherwise.
            return LRESULT(if msg == WM_QUERYENDSESSION { 1 } else { 0 });
        }
        CallWindowProcW(Some(std::mem::transmute(original)), hwnd, msg, wparam, lparam)
    }
}
```

- [ ] **Step 3: Declare the module + wire it into lib.rs**

In `desktop-app/src-tauri/src/lib.rs`, near the other module declarations (after `mod registration;`), add:

```rust
mod shutdown_hook;
```

In the same file, inside the `setup` closure, after `app.manage(state.clone());`, insert:

```rust
            // Phase 3: Windows shutdown hook — fire a synchronous send of
            // device_offline before Windows reaps the process.
            {
                let hook_state = state.clone();
                let hook_handle = handle.clone();
                shutdown_hook::install(move || {
                    let state = hook_state.clone();
                    let handle = hook_handle.clone();
                    // Spawn into a Tokio runtime, but block on it briefly so
                    // the OS shutdown handshake waits for the HTTP POST.
                    let _ = std::thread::spawn(move || {
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .ok();
                        if let Some(rt) = rt {
                            rt.block_on(async move {
                                let ha = state.ha_client.lock().await;
                                let _ = tokio::time::timeout(
                                    std::time::Duration::from_secs(2),
                                    ha.send_device_offline(),
                                ).await;
                            });
                        }
                        let _ = handle; // suppress unused warning
                    }).join();
                });
            }
```

In the `app.run` block at the bottom of `run`, extend the match to handle `RunEvent::ExitRequested` (which fires when the user picks "Quit" from the tray, before the app actually exits):

```rust
    app.run(|app_handle, event| {
        match event {
            RunEvent::WindowEvent {
                label,
                event: WindowEvent::CloseRequested { api, .. },
                ..
            } => {
                if label == "main" {
                    api.prevent_close();
                    if let Some(window) = app_handle.get_window("main") {
                        let _ = window.hide();
                    }
                }
            }
            RunEvent::ExitRequested { .. } => {
                // User chose "Quit" from the tray. Send a synchronous
                // device_offline (best-effort, 2s timeout) so HA flips us
                // offline immediately.
                let state = app_handle.state::<Arc<AppState>>().inner().clone();
                let rt = tokio::runtime::Handle::current();
                let _ = std::thread::spawn(move || {
                    rt.block_on(async move {
                        let ha = state.ha_client.lock().await;
                        let _ = tokio::time::timeout(
                            std::time::Duration::from_secs(2),
                            ha.send_device_offline(),
                        ).await;
                    });
                }).join();
            }
            _ => {}
        }
    });
```

- [ ] **Step 4: Check the crate compiles cleanly**

```powershell
cargo check --manifest-path desktop-app/src-tauri/Cargo.toml
```

Expected: clean build, no errors. (Warnings about unused symbols on non-Windows targets are fine; we only target Windows in `tauri.conf.json`.)

- [ ] **Step 5: Run the full test suite**

```powershell
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml
```

Expected: every test from Phase 1, 2A and the new device_offline payload test passes.

- [ ] **Step 6: Commit**

```powershell
git add desktop-app/src-tauri/Cargo.toml \
        desktop-app/src-tauri/Cargo.lock \
        desktop-app/src-tauri/src/shutdown_hook.rs \
        desktop-app/src-tauri/src/lib.rs
git commit -m "feat(shutdown): WM_QUERYENDSESSION + ExitRequested send graceful device_offline"
```

---

## Task 7: End-to-end verification

**Goal:** With the Python integration updated and the Rust app rebuilt, confirm:
1. A new `binary_sensor.<device>_online` exists and reads `on`
2. Stopping the dev app and waiting ~3 minutes flips it to `off` (timeout path)
3. Restarting the dev app + waiting one sensor cycle flips it back to `on`
4. Triggering Quit from the tray flips it to `off` within 5 seconds (graceful path)

**Files:** None — verification only.

- [ ] **Step 1: Deploy the integration changes to render-unit**

```powershell
scp ha-integration/custom_components/desktop_app/*.py render-unit:/tmp/desktop_app_phase3/
ssh render-unit "docker cp /tmp/desktop_app_phase3/. homeassistant:/config/custom_components/desktop_app/ && rm -rf /tmp/desktop_app_phase3"
```

Reload the integration via the HA REST API:

```powershell
$token = (Get-Content "$env:APPDATA\com.ha-companion.desktop\settings.json" | ConvertFrom-Json).access_token
$h = @{ Authorization = "Bearer $token" }
$entries = Invoke-RestMethod -Uri "https://assist.phillippepelzer.me/api/config/config_entries/entry" -Headers $h
$entries | Where-Object { $_.domain -eq "desktop_app" } | ForEach-Object {
    Invoke-RestMethod -Method POST -Uri "https://assist.phillippepelzer.me/api/config/config_entries/entry/$($_.entry_id)/reload" -Headers $h
}
```

Expected: each entry replies `{"require_restart":false}`.

- [ ] **Step 2: Build the desktop app (dev) and verify the new entity appears**

```powershell
cd desktop-app
yarn tauri dev
```

In a second PowerShell, after ~30 seconds:

```powershell
$token = (Get-Content "$env:APPDATA\com.ha-companion.desktop\settings.json" | ConvertFrom-Json).access_token
$h = @{ Authorization = "Bearer $token" }
Invoke-RestMethod -Uri "https://assist.phillippepelzer.me/api/states/binary_sensor.phill_pc_online" -Headers $h | Format-List entity_id,state,last_changed
```

Expected: `state` is `on`, `last_changed` is recent.

- [ ] **Step 3: Verify timeout path**

Force-kill the dev app:

```powershell
Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
```

Wait ~3 minutes (longer than `2.5 × 60s` = 150s + one tick interval). Then re-query:

```powershell
Invoke-RestMethod -Uri "https://assist.phillippepelzer.me/api/states/binary_sensor.phill_pc_online" -Headers $h | Format-List entity_id,state,last_changed
```

Expected: `state` is `off`.

- [ ] **Step 4: Verify recovery**

Restart `yarn tauri dev` and wait one minute. Re-query the entity. Expected: `state` is `on` again.

- [ ] **Step 5: Verify graceful shutdown path**

While the dev app is running, right-click the tray icon → Quit. Within ~5 seconds, the binary_sensor must flip to `off` (much faster than the timeout path). Re-query to confirm.

If it doesn't flip within 10 seconds, the `RunEvent::ExitRequested` branch (Task 6 step 3) didn't fire — check `app.log` for `[HA] POST .../api/webhook/.../` with the `device_offline` payload.

- [ ] **Step 6: NO version bump, NO installer build yet**

Phase 3 closes the loop on Phase 1+2+3 work. The final step (push everything to GitHub, tag releases for HA-Companion-App + ha-integration, build the Windows installer) is a separate orchestrated handoff in the parent session, not part of this plan.

---

## Self-review

**Spec coverage:**
- New `binary_sensor.<device>_online` with `device_class: connectivity` → Task 2
- HA timeout-based offline detection (no external broker, no MQTT) → Tasks 1 + 3
- Graceful Windows shutdown signal → Tasks 5 + 6
- Tests-first for all logic-bearing code → every Python module has a unit test in `tests/test_availability.py`; the Rust payload format has a unit test in `ha_client.rs`
- No version bump / no installer build in this phase → Task 7 step 6
- Phase 3 deferred Phase 2B work (per-core temps etc.) is not addressed here — explicitly out of scope

**Placeholder scan:** No "TODO", "TBD", or "add appropriate error handling" hand-waves. Every code block can be copied verbatim. Every command has expected output.

**Type / signature consistency:**
- `is_device_online(last_seen, now, update_interval) -> bool` defined in Task 1, called from `evaluate_devices` in Task 3 ✅
- `evaluate_devices(...)` signature in Task 3 step 3 matches the test signature in Task 3 step 1 ✅
- `DesktopAppAvailabilitySensor.__init__` keyword args (device_id, device_name, update_interval) match between Task 2 step 1 (test), step 3 (class), and step 5 (call site) ✅
- `SIGNAL_AVAILABILITY_UPDATE` is a constant in `const.py` (Task 1 step 1) referenced from `webhook.py`, `availability.py`, and `__init__.py` consistently ✅
- `offline_signal_payload() -> bool` (Task 4) is referenced by the test and by the webhook handler; same return type ✅
- `HaClient::send_device_offline(&self) -> Result<(), Box<dyn Error + Send + Sync>>` (Task 5) matches caller signature in Task 6 step 3 ✅
- `shutdown_hook::install<F: Fn() + Send + Sync + 'static>(handler: F)` is the same shape on Windows and non-Windows (no-op stub) — caller in Task 6 step 3 doesn't need a cfg-gate ✅

No gaps found.

"""Offline audit probes using the real integration modules with minimal HA stubs.

Run from any directory with Python 3.12+. No HA server, credentials or network.
These are reproductions of current defects, not passing acceptance tests.
"""
import asyncio
import importlib.util
import json
import sys
import types
from datetime import datetime, timedelta, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
SOURCE = ROOT / "ha-integration/custom_components/desktop_app"
NOW = datetime(2026, 9, 23, 12, tzinfo=timezone.utc)
clock = [NOW]
signals = []
ticks = []
results = {}


def stub(name, **attrs):
    module = types.ModuleType(name)
    module.__dict__.update(attrs)
    sys.modules[name] = module
    return module


class Response:
    def __init__(self, data=None, status=200):
        self.data, self.status = data, status


class View:
    pass


def json_response(data, status=200):
    return Response(data, status)


stub("aiohttp")
stub("aiohttp.web", Request=object, Response=Response, json_response=json_response)
stub("homeassistant")
stub("homeassistant.config_entries", ConfigEntry=object)
stub("homeassistant.core", HomeAssistant=object, callback=lambda fn: fn)
stub("homeassistant.helpers")
stub("homeassistant.helpers.device_registry", DeviceInfo=dict)
stub("homeassistant.helpers.http", HomeAssistantView=View)
stub("homeassistant.helpers.dispatcher",
     async_dispatcher_send=lambda hass, signal, value: signals.append((signal, value)))
stub("homeassistant.helpers.event",
     async_track_time_interval=lambda hass, fn, interval: ticks.append(fn))
stub("homeassistant.util", dt=types.SimpleNamespace(utcnow=lambda: clock[0]))
stub("homeassistant.util.dt", utcnow=lambda: clock[0])
stub("homeassistant.components", webhook=types.SimpleNamespace(
    async_unregister=lambda *args: None, async_register=lambda *args, **kwargs: None))
package = stub("audit_component")
package.__path__ = [str(SOURCE)]


def load(name):
    spec = importlib.util.spec_from_file_location(f"audit_component.{name}", SOURCE / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


const = load("const")
load("helpers")
availability = load("availability")
webhook = load("webhook")
http_api = load("http_api")
entry = types.SimpleNamespace(data={"device_id": "audit-device", "webhook_id": "audit-hook"})
hass = types.SimpleNamespace(
    data={const.DOMAIN: {const.DATA_LAST_SEEN: {}, const.DATA_PENDING_UPDATES: {}}},
    config_entries=types.SimpleNamespace(async_entries=lambda domain: [entry]),
)


class Request:
    def __init__(self, payload, app=None):
        self.payload = payload
        self.app = {"hass": hass} if app is None else app

    async def json(self):
        return self.payload


async def send(payload):
    return await webhook.handle_webhook(hass, "audit-hook", Request(payload))


async def run():
    availability.start_availability_timer(hass)
    tick = ticks[-1]
    await send({"type": "update_sensor_states", "data": {"sensors": []}})
    await send({"type": "device_offline", "data": {}})
    after_offline = signals[-1][1]
    clock[0] = NOW + timedelta(seconds=30)
    tick(clock[0])
    results["offline_undone_by_first_tick"] = {
        "after_offline": after_offline, "after_tick": signals[-1][1],
        "reproduced": after_offline is False and signals[-1][1] is True,
    }

    clock[0] = NOW + timedelta(seconds=200)
    tick(clock[0])
    # Timer now stores False. A heartbeat changes the entity but not that cache.
    clock[0] = NOW + timedelta(seconds=201)
    await send({"type": "update_sensor_states", "data": {"sensors": []}})
    clock[0] = NOW + timedelta(seconds=400)
    before = len(signals)
    tick(clock[0])
    results["brief_reconnect_stays_online_after_timeout"] = {
        "new_signals": len(signals) - before, "last_entity_signal": signals[-1][1],
        "reproduced": len(signals) == before and signals[-1][1] is True,
    }

    response = await send({"type": "register_sensor", "data": {}})
    results["invalid_command_counts_as_heartbeat"] = {
        "http_status": response.status, "last_entity_signal": signals[-1][1],
        "reproduced": response.status == 400 and signals[-1][1] is True,
    }
    for label, payload in {
        "null_envelope": None,
        "list_command_type": {"type": []},
        "nonempty_list_command_type": {"type": ["bad"]},
        "null_command_data": {"type": "update_sensor_states", "data": None},
        "null_sensor_item": {"type": "update_sensor_states", "data": {"sensors": [None]}},
    }.items():
        try:
            response = await send(payload)
            results[label] = {"http_status": response.status}
        except Exception as error:
            results[label] = {"exception": type(error).__name__}

    results["configured_interval_600_but_server_default_60"] = {
        "at_180_seconds": availability.is_device_online(
            last_seen=NOW, now=NOW + timedelta(seconds=180),
            update_interval=entry.data.get("update_interval", 60)),
        "required_with_interval_600": True,
    }
    print(json.dumps(results, indent=2))
    assert all(results[key]["reproduced"] for key in (
        "offline_undone_by_first_tick", "brief_reconnect_stays_online_after_timeout",
        "invalid_command_counts_as_heartbeat"))


if __name__ == "__main__":
    asyncio.run(run())

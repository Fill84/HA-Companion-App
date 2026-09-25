# Sensor diagnostics in Settings

## Goal

For each discovered measurement, show its current local reading, last valid reading, measurement source, time of the last valid local read, time of the last Home Assistant webhook acknowledgement, and a useful reason when the current reading is unavailable. Keep existing sensor identifiers and payload contracts unchanged.

## Design

The ordinary collection path updates an in-memory diagnostic cache keyed by the existing sensor `unique_id`. Opening Settings still discovers readings, but discovery does not update diagnostic timestamps or pretend that data reached Home Assistant. Only an acknowledged `update_sensor_states` webhook call updates the HA confirmation time. A failed send leaves that time untouched. A full snapshot marks previously observed missing readings unavailable; a dynamic snapshot does this only for dynamic readings. Last valid values remain explicitly historical and are never presented as current.

The Settings list has an expandable diagnostics section for each reading, including a one-reading group. It polls only the small in-memory cache while open; it does not poll hardware or HA. The text is localized in English and Dutch. Disabled preferences are shown separately from provider failures. Provider status codes are translated where available; for a missing reading without a provider status the UI says that absence or lack of support cannot be distinguished. A source is reported at the specificity actually known to the collector, without guessing a physical device.

The cache is process-local. After restart it says that no observation has been recorded until the normal sensor cycle runs. This avoids storing potentially sensitive sensor values on disk. No Home Assistant integration or entity migration is needed.

## Verification

Check known-to-unknown transitions, missing sensors, acknowledgement after success only, stale acknowledgement races, Settings rendering and localization, local IPC permissions, Rust unit tests, frontend tests, formatter, and lint. The feature does not substitute for live testing on a second machine before the next public release.

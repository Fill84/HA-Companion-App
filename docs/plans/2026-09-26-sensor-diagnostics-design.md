# Sensor diagnostics in Settings

## Goal

For each discovered measurement, show its current local reading, last valid reading, measurement source, time of the last valid local read, time of the last Home Assistant webhook acknowledgement, and a useful reason when the current reading is unavailable. Keep existing sensor identifiers and payload contracts unchanged.

## Design

The ordinary collection path updates an in-memory diagnostic cache keyed by the existing sensor `unique_id`. Opening Settings still discovers readings, but discovery does not update diagnostic timestamps or pretend that data reached Home Assistant. Only an acknowledged `update_sensor_states` webhook call updates the HA confirmation time. A failed send leaves that time untouched. A full snapshot marks previously observed missing readings unavailable; a dynamic snapshot does this only for dynamic readings. Last valid values remain explicitly historical and are never presented as current.

The Settings list has an expandable diagnostics section for each reading, including a one-reading group. It polls only the small in-memory cache while open; it does not poll hardware or HA. The text is localized in English and Dutch. Disabled preferences are shown separately from provider failures. Provider status codes are translated where available; for a missing reading without a provider status the UI says that absence or lack of support cannot be distinguished. A source is reported at the specificity actually known to the collector, without guessing a physical device.

The cache is process-local. After restart it says that no observation has been recorded until the normal sensor cycle runs. This avoids storing potentially sensitive sensor values on disk. No Home Assistant integration or entity migration is needed.

## Verification

Check known-to-unknown transitions, missing sensors, acknowledgement after success only, stale acknowledgement races, Settings rendering and localization, local IPC permissions, Rust unit tests, frontend tests, formatter, and lint. The feature does not substitute for live testing on a second machine before the next public release.

## Local Windows acceptance, 2026-09-26

The unpublished 1.0.5 NSIS build from commit `a262a38` (installer SHA-256 `D083CD8550614671EBEE5AB8436EBB12AE6C8F5CD916B61D0828D60099EFF9FF`) was installed on `phill-pc` and `beast-unit`. Both installers returned exit code 0, and the installed executables have the same SHA-256 `7B66A7D8B2AFBCD3C2853E2FBAE663A93C0B5082496D411DB6DB9581CF267166`. Both apps run in interactive user sessions, and both Windows sensor services and PawnIO are running. On `phill-pc`, Settings visibly shows CPU Temperature diagnostics with a valid reading, the `pawnio/2.2/intel-msr` source, separate local and HA acknowledgement times, and the existing 1.0.5/1.0.11 version labels. Its settings file remained byte-identical through installation. On `beast-unit`, the pre-upgrade settings backup was verified; device ID, webhook ID, server URL, sensor choices and settings fields remain equal. The file was reserialized by the app before the installer started, and remained byte-identical during the installer run. The read-only HA recorder showed fresh numeric values on the existing `sensor.beast_unit_cpu_usage` and `sensor.beast_unit_cpu_temperature` entities after restart. The remote Settings UI was not visually inspected. This local build was not pushed or published.

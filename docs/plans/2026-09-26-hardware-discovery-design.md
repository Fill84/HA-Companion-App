# Hardware discovery and sensor capabilities

Status: implementation in progress, 2026-09-26. The user wants automatic discovery of existing and new hardware readings on Windows, Linux, and macOS, with per-reading selection and a refresh after a relevant hardware or driver change. Windows may use the bundled PawnIO driver; the other platforms use native interfaces. Existing Home Assistant unique IDs must remain stable.

## Feasibility boundary

No operating system or PawnIO module exposes every physical measurement on every device. A reading is supported only when a documented, read-only source can identify its hardware, unit, interpretation, and validity. Firmware-hidden or unsupported readings remain `unknown`; the app must not substitute an unrelated thermal zone or a plausible-looking number. PawnIO modules are chip-specific building blocks, not a universal sensor catalog. This distinction must be visible in Settings and diagnostics.

## Selected architecture

1. At startup, discover devices and supported channels once. Build a capability inventory keyed by stable physical device ID, channel ID, provider, measurement kind, and unit. Persist HA ID assignments separately from the transient inventory. Do not use enumeration order as identity.
2. Sample only enabled channels at the configured interval. Reuse provider handles and discovered paths. Static metadata is collected at discovery and refreshed when its source changes. The Settings list reads the inventory, not a second full sensor probe.
3. Invalidate and rebuild the relevant portion of the inventory after a platform hardware event, resume, driver/provider error that indicates a topology change, or an explicit refresh. A low-cost reconciliation check may be used where an OS does not provide a reliable event, with bounded frequency and timeouts. Removed readings retain their HA IDs and become `unknown`; no reassignment to an ambiguous replacement.
4. Preserve the existing `cpu_temperature`, GPU, disk, network, battery, and other HA unique IDs. New readings receive deterministic IDs from a stable device/channel identifier. Existing per-reading enablement remains effective.
5. One snapshot is sent to Home Assistant per update; the collector may need multiple native or driver calls internally. A failed provider must not block the other providers. Collection time, errors, and source are recorded per reading.

## Provider coverage

| Platform | Discovery and sample sources | Restrictions |
| --- | --- | --- |
| Windows | Existing OS APIs/sysinfo for usage and topology; NVML for NVIDIA; bundled PawnIO service for verified Intel/AMD and supported motherboard/EC/SMBus modules | Add each module only after validating the exact chip/channel conversion and safe read behavior. GPU counters may need vendor/Windows APIs. Windows WMI GPU metadata does not provide live AMD/Intel temperature or utilization. |
| Linux | Existing sysinfo, DRM and NVML plus the kernel `hwmon`/`sysfs` ABI for temperatures, fans, voltage and power channels | Never assume a `tempN` label is a CPU sensor. Identify chip and channel, apply documented scaling, and respect absent/invalid files. |
| macOS | Public OS APIs and existing supported components; static graphics metadata may come from `system_profiler` at discovery | No documented public universal API for all CPU, GPU, fan and board sensors. Do not introduce undocumented SMC access as a guaranteed source. |

The [Linux hwmon ABI](https://docs.kernel.org/hwmon/sysfs-interface.html) explicitly warns that board sensor wiring and voltage scaling differ. The [official PawnIO module repository](https://github.com/namazso/PawnIO.Modules) lists hardware-specific modules, including Intel MSR, AMD family, embedded-controller, LPC, and SMBus modules.

## Alternatives considered

- Keep probing all sources every interval: simple, but repeats WMI, `system_profiler`, enumeration and opening provider handles; it does not give a trustworthy capability inventory.
- Route every reading through PawnIO: impossible on Linux/macOS and wrong for OS-owned measurements such as process count or network traffic. It also makes chip-specific low-level access a bottleneck and increases risk.
- Use a discovery inventory with native sampling adapters: selected because it separates hardware identity and availability from sampling, while preserving the current HA contract.

## Delivery and verification

Implement in slices: (1) inventory and no duplicate Settings probe; (2) targeted reuse of existing GPU/disk/network/battery providers and topology invalidation; (3) generic validated Linux hwmon channels; (4) Windows hardware modules per tested chip family; (5) macOS public-source coverage; (6) HA migration and cross-platform build checks. Verify on phill-pc and beast-unit without publishing a release. Track unsupported chip families and untested platforms explicitly; a green build is not evidence of hardware coverage.

### Current implementation status

- Startup discovery caches the Settings reading list, including disabled groups. A later observed change in dynamic reading IDs invalidates the cache. Tauri's event-loop resume requests a full topology refresh at the next update. A full snapshot every ten update intervals also reconciles disk/network/GPU topology; this is a bounded fallback, not OS hotplug-event detection.
- Windows WMI and macOS `system_profiler` GPU metadata are cached between topology refreshes. Disk and network provider state is reused between updates. Their full device lists refresh on the existing full-snapshot cadence.
- Linux `hwmon` discovers per-chip temperature, fan, voltage, current and power channels from the kernel ABI, samples discovered paths, validates units/ranges and optional `fault`/`enable` flags, and assigns a deterministic UUID from the stable device path and channel. This code has unit coverage on Windows but no Linux build/runtime evidence yet.
- Still open: native device hotplug events and proof that the resume event fires on each supported OS; Windows AMD, motherboard/EC, fan, voltage and storage-temperature adapters with chip-specific verification; public macOS sensor source coverage; physical hardware validation, collection impact measurements and full HA migration test for the new channels. Existing Windows Intel CPU temperature and other pre-existing readings remain in place.
- Hardware availability: `phill-pc` has an ASUS ROG STRIX Z490-E GAMING and Intel i7-10700K; `beast-unit` has an Intel X99 board and Intel CPU. The user confirmed that no AMD Windows test host is currently available. A non-elevated query for Windows `MSFT_StorageReliabilityCounter` on `phill-pc` was denied, while `beast-unit` returned no readings, so this is not a validated general storage-temperature provider.

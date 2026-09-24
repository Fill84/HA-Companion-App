# Home Assistant Companion desktop app

This directory contains the Tauri desktop application. User installation and upgrade steps are in the [project README](../README.md) and the [integration guide](https://github.com/Fill84/ha-integration/blob/main/docs/INSTALLATION.md). The integration lives in a separate repository for HACS; publishing a desktop release does not publish an integration update.

To build locally, install the locked Yarn dependencies with `corepack yarn install --frozen-lockfile`, then run `corepack yarn tauri:build`. On Windows this builds an NSIS installer under `src-tauri/target/release/bundle/nsis/`. A local build is a test artifact until its installer, signing policy, and release checks have been completed.

Windows CPU temperature uses the optional, bundled PawnIO driver with the app's own Rust sensor service. No separate PawnIO application is needed. Without this optional provider, unsupported measurements remain unavailable. See the [provider manifest](src-tauri/resources/pawnio/MANIFEST.md) for pinned driver provenance.

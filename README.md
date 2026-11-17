# opencad-hydrocomplete-plugin

> **Unsupported (retired 2026-11-17).** This plugin was built for Open CAD Studio v2026.42 (plugin API 10). Open CAD Studio has since moved to plugin API 23 and replaced the drawing library the plugin depends on, so it does not load in current releases and will not be updated. The source stays available under GPL-3.0. For the same storm-drain design workflow, use [HydroComplete](https://hydrocomplete.com).

**HydroComplete** hydrology/hydraulics add-on for [Open CAD Studio](https://github.com/HakanSeven12/OpenCADStudio), mirroring the [`HydroComplete.Civil3D`](https://github.com/mf4633/hydrocomplete-civil3d) command set and engine for gravity storm-drain design.

Distributed as a prebuilt dynamic library via GitHub Releases (same pattern as [opencad-storm-sewer-plugin](https://github.com/mf4633/opencad-storm-sewer-plugin)).

## Status (v0.4.0)

| Area | Status |
|------|--------|
| Engine (`hydrocomplete` + `stormsewer`) | Full `NetworkAnalysisPipeline`, BMP/WQV, GVF, culvert, SSURGO soil lookup, KaTeX HTML + Pro PDF reports |
| Command registry | All `HC_*` commands from Civil 3D 1.4 registered + ribbon |
| Working now | Full analysis (`HC_ANALYZE`, `HC_REVIEW`), HTML/PDF reports (`HC_REPORT` / `HC_REPORT_PDF`), Pro licensing (`HC_ACTIVATE`, `HC_LICENSE`), NOAA Atlas 14 presets + live PFDS (`HC_ATLAS14`, `HC_PARAMS PRESET`/`LIVE`), detention/BMP/WQV, `HC_NETWORK_EDIT`, `HC_BACKGROUND`, `HC_SOIL`, placement, LandXML |

OpenCAD uses **XDATA on entities** instead of Civil 3D pipe networks — see [PLUGIN.md](PLUGIN.md).

## Repo layout

```
opencad-hydrocomplete-plugin/
├── Cargo.toml
├── plugin.toml
├── crates/
│   ├── stormsewer/       # network Rational + Manning + HGL (shared engine)
│   └── hydrocomplete/    # Civil3D engine parity layer (box/arch, SCS, …)
└── src/
    ├── lib.rs            # BuiltinPlugin + ribbon
    ├── commands.rs       # HC_* command output (mirrors Civil3D)
    ├── dispatch.rs       # command routing
    └── data.rs           # HYDROCOMPLETE_* XDATA bridge
```

## Install

**Plugin Manager → Add repository →** `mf4633/opencad-hydrocomplete-plugin`, pick **v0.4.3+**, **Install**, restart OCS.

Requires Open CAD Studio **v2026.36** (plugin API 5). The host refuses a plugin whose rustc/acadrust fingerprint differs from its own, so each plugin release is built for one OCS release (`ocs-host.lock` records which; the `host-drift` workflow checks the latest OCS daily and opens an issue when a rebuild is needed); v0.4.x (API 2) loads on the current host but never answers a command.

## Build

```bash
cargo build --release
cargo test
```

Produces `opencad_hydrocomplete_plugin.dll` (Windows) / `libopencad_hydrocomplete_plugin.so` (Linux) / `libopencad_hydrocomplete_plugin.dylib` (macOS). Ship beside `plugin.toml`.

## Release

Tag `v0.4.x` — CI attaches per-platform `opencad.hydrocomplete-*` binaries + `plugin.toml` to the GitHub Release for Plugin Manager.

OCS `--serve` automation notes: `docs/OCS_SERVE.md`.

See [CHANGELOG.md](CHANGELOG.md) for version history.

## Related

- Civil 3D source: `dev/hydrocomplete-civil3d`
- Storm Sewer plugin (predecessor): `dev/opencad-storm-sewer-plugin`

## License

GPL-3.0-only
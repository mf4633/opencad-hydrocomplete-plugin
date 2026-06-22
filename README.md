# opencad-hydrocomplete-plugin

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

**Plugin Manager → Add repository →** `mf4633/opencad-hydrocomplete-plugin`, pick a **v0.4.1+** release, **Install**, restart OCS.

Requires Open CAD Studio **v0.6.0+** (API v2, interactive commands).

## Build

```bash
cargo build --release
cargo test
```

Produces `opencad_hydrocomplete_plugin.dll` (Windows) / `libopencad_hydrocomplete_plugin.so` (Linux) / `libopencad_hydrocomplete_plugin.dylib` (macOS). Ship beside `plugin.toml`.

## Release

Tag `v0.4.1` — CI attaches per-platform `opencad.hydrocomplete-*` binaries + `plugin.toml` to the GitHub Release for Plugin Manager.

OCS `--serve` automation notes: `docs/OCS_SERVE.md`.

See [CHANGELOG.md](CHANGELOG.md) for version history.

## Related

- Civil 3D source: `dev/hydrocomplete-civil3d`
- Storm Sewer plugin (predecessor): `dev/opencad-storm-sewer-plugin`

## License

GPL-3.0-only
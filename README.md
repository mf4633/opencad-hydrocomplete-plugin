# opencad-hydrocomplete-plugin

**HydroComplete** hydrology/hydraulics add-on for [Open CAD Studio](https://github.com/HakanSeven12/OpenCADStudio), mirroring the [`HydroComplete.Civil3D`](https://github.com/mf4633/hydrocomplete-civil3d) command set and engine for gravity storm-drain design.

Distributed as a prebuilt dynamic library via GitHub Releases (same pattern as [opencad-storm-sewer-plugin](https://github.com/mf4633/opencad-storm-sewer-plugin)).

## Status (v0.2.0)

| Area | Status |
|------|--------|
| Engine (`hydrocomplete` + `stormsewer`) | Circular/box/arch Manning, SCS runoff, network Rational/HGL, KaTeX HTML engine (not yet wired to `HC_REPORT`) |
| Command registry | All `HC_*` commands from Civil 3D 1.4 registered + ribbon |
| Working now | `HC_ABOUT`, `HC_NETWORK`, `HC_PIPES`, `HC_PIPES_WRITE`, `HC_CAPACITY`, `HC_CAPACITY_WRITE`, `HC_VALIDATE`, `HC_ANALYZE` (+ surcharge/flood styling), `HC_SIZE`, `HC_HGL`, `HC_PROFILE`, `HC_REPORT` (text), `HC_MULTIRP`, `HC_RATIONAL`, `HC_SCS`, `HC_ATLAS14` (embedded presets), placement (`HC_INLET`… interactive + coordinates), `HC_EDIT`, LandXML import |
| Planned (stubs) | KaTeX HTML report file export, Atlas 14 live PFDS fetch, detention, BMP/WQV, GVF, culvert, SSURGO, Pro licensing |

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

**Plugin Manager → Add repository →** `mf4633/opencad-hydrocomplete-plugin`, pick a **v0.2.0+** release, **Install**, restart OCS.

Requires Open CAD Studio **v0.6.0+** (API v2, interactive commands).

## Build

```bash
cargo build --release
cargo test
```

Produces `opencad_hydrocomplete_plugin.dll` (Windows) / `libopencad_hydrocomplete_plugin.so` (Linux) / `libopencad_hydrocomplete_plugin.dylib` (macOS). Ship beside `plugin.toml`.

## Release

Tag `v0.2.0` — CI attaches per-platform `opencad.hydrocomplete-*` binaries + `plugin.toml` to the GitHub Release for Plugin Manager.

See [CHANGELOG.md](CHANGELOG.md) for version history.

## Related

- Civil 3D source: `dev/hydrocomplete-civil3d`
- Storm Sewer plugin (predecessor): `dev/opencad-storm-sewer-plugin`

## License

GPL-3.0-only
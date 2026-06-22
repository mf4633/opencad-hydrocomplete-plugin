# Changelog

All notable changes to **opencad-hydrocomplete-plugin** are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0] - 2026-06-22

### Added

- `HC_ANALYZE` — full-network analysis with surcharge/flood color styling on pipes and structures
- `HC_VALIDATE` — design-criteria review (velocity, cover, slope, capacity, size progression, surface flooding) in addition to integrity checks
- `HC_PIPES_WRITE` / `HC_CAPACITY_WRITE` — MText capacity labels on layer `HC-CAPACITY`
- Interactive placement — click-to-place `HC_INLET`, `HC_JUNCTION`, `HC_OUTFALL`, and two-pick `HC_PIPE` (OpenCAD API v2)
- Coordinate/handle placement for automation and `--serve` workflows
- `HC_EDIT` — edit structure and pipe XDATA fields from the command line
- `HC_LANDXML_IMPORT` — import LandXML 1.2 pipe networks (ribbon file dialog or path argument)
- `HC_SCS` — SCS curve-number runoff from tagged catchments
- `hydrocomplete` engine crate — box/arch Manning conduits and SCS runoff helpers atop `stormsewer`
- Headless integration tests — XDATA round-trip, design-review cover flag, Tc apply map
- GitHub Actions release workflow — per-platform `opencad.hydrocomplete-*` binaries + `plugin.toml`

### Changed

- `HC_PIPES` / `HC_CAPACITY` — box and arch conduit shapes via `hydrocomplete::manning`
- Ribbon groups aligned with HydroComplete.Civil3D 1.4 command families

### Planned (stubs remain)

- KaTeX HTML report export (`HC_REPORT` currently emits text; `stormsewer::report_html` engine ready)
- NOAA Atlas 14 live PFDS fetch (`HC_ATLAS14` lists embedded presets)
- Detention, BMP/WQV, GVF, culvert, SSURGO, Pro licensing (`HC_REPORT_PDF`, `HC_ACTIVATE`, …)

## [0.1.0] - 2026-06-09

### Added

- Initial Open CAD Studio external plugin (`opencad.hydrocomplete`) — cdylib + `plugin.toml` manifest
- XDATA schemas: `HYDROCOMPLETE_STRUCT`, `HYDROCOMPLETE_PIPE`, `HYDROCOMPLETE_CATCHMENT`
- Core commands: `HC_ABOUT`, `HC_NETWORK`, `HC_PIPES`, `HC_CAPACITY`, `HC_SIZE`, `HC_HGL`, `HC_PROFILE`, `HC_REPORT`, `HC_MULTIRP`, `HC_RATIONAL`, `HC_ATLAS14`, `HC_PARAMS`
- Coordinate placement: `HC_INLET`, `HC_JUNCTION`, `HC_OUTFALL`, `HC_PIPE`
- `stormsewer` engine crate — Rational method, Manning (circular), HGL, IDF, LandXML parser, design review
- Ribbon module with Network / Analysis / Stormwater / More groups
- GPL-3.0-only license

[0.2.0]: https://github.com/mf4633/opencad-hydrocomplete-plugin/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/mf4633/opencad-hydrocomplete-plugin/releases/tag/v0.1.0
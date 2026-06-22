# Changelog

All notable changes to **opencad-hydrocomplete-plugin** are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0] - 2026-06-22

### Added

**Full analysis (mirrors `NetworkAnalysisPipeline`)**
- `HC_ANALYZE` — hydrology, routing, capacity, HGL, sediment, WQV, compliance, design review
- `HC_REVIEW` — design review + state regulatory compliance table
- `hydrocomplete` engine: `network_analysis`, `catchment_flow_router`, `compliance`, `state_compliance` (53 jurisdictions), `sediment`, `water_quality`, `rational`, `trace`

**HTML reports**
- `HC_REPORT` — KaTeX HTML export to `Documents/HydroComplete/` (Manning + capacity + HGL)
- `HC_REPORT_PDF` — Pro stub with free HTML fallback

**Stormwater / BMP**
- `HC_DETENTION`, `HC_BMP_SIZE`, `HC_WQ_TRAIN`, `HC_SEDIMENT_BASIN`, `HC_WQV`, `HC_SEDIMENT`, `HC_UNIT_HYDRO`
- `HC_PREPOST`, `HC_OPTIMIZE`, `HC_BIORETENTION`, `HC_WETLAND`

**Advanced hydraulics**
- `HC_GVF`, `HC_CULVERT`, `HC_TC`, `HC_INLETS`, `HC_HYDROGRAPH`, `HC_ROUTE_HYDRO`
- `HC_PUMP`, `HC_COST`, `HC_PROFILE_DXF`, `HC_NETWORK_DIAGRAM` (HTML/SVG), `HC_LANDXML` export

**Network / placement**
- Interactive placement (`HC_INLET`, `HC_JUNCTION`, `HC_OUTFALL`, `HC_PIPE`)
- `HC_EDIT`, `HC_LANDXML_IMPORT`, `HC_PIPES_WRITE`, `HC_CAPACITY_WRITE`
- 120 workspace tests (hydrocomplete 44, plugin 27, stormsewer 48)

### Changed

- `HC_PIPES` / `HC_CAPACITY` — circular, box, and arch Manning via `hydrocomplete::manning`
- Ribbon groups aligned with HydroComplete.Civil3D 1.4

### Planned (stubs remain)

- `HC_NETWORK_EDIT`, `HC_BACKGROUND`, `HC_SOIL` (SSURGO live fetch)
- NOAA Atlas 14 live PFDS fetch
- Pro licensing (`HC_ACTIVATE`)

## [0.1.0] - 2026-06-09

### Added

- Initial Open CAD Studio external plugin (`opencad.hydrocomplete`)
- XDATA schemas: `HYDROCOMPLETE_STRUCT`, `HYDROCOMPLETE_PIPE`, `HYDROCOMPLETE_CATCHMENT`
- Core commands: `HC_ABOUT`, `HC_NETWORK`, `HC_PIPES`, `HC_CAPACITY`, `HC_SIZE`, `HC_HGL`, `HC_MULTIRP`, `HC_RATIONAL`, `HC_ATLAS14`
- `stormsewer` engine crate — Rational, Manning, HGL, IDF, LandXML, design review
- GPL-3.0-only license

[0.2.0]: https://github.com/mf4633/opencad-hydrocomplete-plugin/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/mf4633/opencad-hydrocomplete-plugin/releases/tag/v0.1.0
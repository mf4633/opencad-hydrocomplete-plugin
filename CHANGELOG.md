# Changelog

All notable changes to **opencad-hydrocomplete-plugin** are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.5.1] - 2026-09-08

### Added

- **Periodic online re-validation.** A cached Pro license is re-checked with the
  server every 7 days (silently, 4 s timeout, at most one attempt per day when
  offline). Server rejection deletes the cached license; unreachable server =
  30-day offline grace after the last good check, then `HC_ACTIVATE` again.

### Fixed

- 401/403 from the server is now classified as rejection, not a network error.
- The license file stores the `hc_live_*` key itself. Earlier builds stored the
  server access token, which cannot be re-validated; legacy files are unwrapped
  transparently on the first re-check.

### Verified

- On OCS v2026.36: stale rejected key → license removed, PDF blocked; freshly
  issued key aged 10 days → refreshed on the next `HC_REPORT_PDF`, PDF written.

## [0.5.0] - 2026-09-08

### Fixed

- **Plugin did not work on current Open CAD Studio (v2026.36).** Every v0.4.x
  release declared plugin API 2; the host still accepts it but the ribbon reply
  no longer decodes across the June→September API drift, so the plugin never
  finished loading and every `HC_*` command returned "Unknown command".
  Rebuilt against the host's `ocs_plugin_api` tag commit, `acadrust`
  (cadcodec @5b56571) and rustc 1.98.1; `plugin.toml` now declares
  `api_version = 5` and the `rustc_version` fingerprint (filled in by CI).
- Per-tab parameters no longer use the host's `ensure_plugin_state`, which is
  in-process only and panics under the out-of-process runner; they live in a
  plugin-owned map keyed by tab id.
- `ribbon_groups()` returns a cached slice per the current `CadModule` trait.
- DWG round-trip tests let the document allocate handles (forced low handles
  collide with reserved table handles in the newer acadrust).

### Verified

- Headless on OCS v2026.36 via `--serve`: about/license, params, LandXML
  import, network, validate, analyze, pipes, capacity, HGL, HTML report,
  multi-RP, PDF report (with a cached Pro license), save, reopen, re-analyze.

## [0.4.11] - 2026-06-23

### Added

- **DWG XDATA persistence** — `xdata_persist` syncs `records` ↔ `raw_dwg_eed` + APPID registration; structures survive save/reopen
- `scripts/test_xdata_save_reopen.ps1` — OCS save/reopen regression for 24-145 Civil import
- **E2E nightly workflow** — `.github/workflows/e2e-nightly.yml` + `scripts/run_nightly_e2e.ps1`
- Playwright DAG palette DOM assertions (`.pnode` / `.cat-hdr` after TDZ fix)

### Fixed

- `run_24145_full_workflow.ps1` — rejects stale HTML/PDF; verifies `%PDF` magic and size > 500 bytes
- `install_dev_plugin.ps1` copies `plugin.toml` for fresh CI runners

## [0.4.10] - 2026-06-23

### Added

- **E2E test suite** — `scripts/run_e2e_suite.ps1` orchestrates Rust, OCS automation, 24-145, Playwright, and Civil 3D CI/GUI smoke
- **Playwright front-end tests** — `tests/frontend/` validates KaTeX HTML reports and DAG WASM palette (`scripts/run_frontend_tests.ps1`)
- **CI workflow** — Linux Rust build + Windows Playwright job (`scripts/run_ci_tests.ps1`)

### Fixed

- `test_slope_report.ps1` KaTeX assertions (`Q_{\text{full}}` result lines)
- PowerShell runners tolerate `cargo`/`npm` stderr without false failures

## [0.4.7] - 2026-06-22

### Added

- **Separate Open CAD Studio license SKU** — `product: opencad` validation; licenses stored in `opencad-license.json` (independent from Civil 3D keys)
- `scripts/install_dev_plugin.ps1` — debug DLL install for local OCS automation

### Changed

- `HC_ACTIVATE` / `HC_LICENSE` point to `https://hydrocomplete.com/opencad`; Civil 3D listed as a separate SKU
- Release builds require online license validation; offline stub and `HYDROCOMPLETE_PRO` bypass are debug-only
- Demo scripts tolerate Cargo warnings on stderr; use debug plugin for `HYDROCOMPLETE_PRO` in automation

### Fixed

- `Cargo.toml` / `plugin.toml` / ribbon manifest versions synced to 0.4.7

## [0.4.6] - 2026-06-22

### Fixed

- Release `plugin.toml` version now matches crate tag (v0.4.5 shipped with `0.4.4` in the release asset; root `plugin.toml` synced)

## [0.4.5] - 2026-06-22

### Added

- **Civil label import** — `HC_CIVIL_IMPORT` reads structure **MText** (`RIM=`, `INV.IN=`, `INV.OUT=`) and pipe **Text** (`~8"`, `~15"`, slope %) on the sewer layer; matches labels to nearest block/line; per-pipe diameter from plan labels
- `scripts/extract_24145_hydrology.py` + `scripts/run_24145_full_workflow.ps1` — 24-145 worksheet → `HC_EDIT` batch + analyze/report

### Changed

- Structure kind detection uses Civil plan labels (`CB-3`, `UG DET OUT`, `EX MH`) when MText is present
- Downstream invert stepping skipped when structure invert came from a label

## [0.4.4] - 2026-06-22

### Added

- **Drawing params persistence** — `HYDROCOMPLETE_PARAMS` XDATA marker stores IDF/hydraulics/sizing with the DWG; restored on open so `HC_REPORT` works without re-running `HC_PARAMS`
- `scripts/open_demo_reports.ps1` — open saved demos, verify Charlotte `a=81.2` and non-zero Q
- **Headwater inlet auto-pick** — `HC_CIVIL_IMPORT ... area <ac> c <rv> tc <min>` applies catchment to dendritic head; `HC_PRIMARY_INLET` reports handle for `HC_EDIT`

### Changed

- `HC_PARAMS` writes params marker to the drawing (hidden `HC-META` MText)

## [0.4.3] - 2026-06-22

### Added

- `HC_CIVIL_IMPORT` — bridge Civil 3D `I-SEWER-NETWORK` structure blocks (e.g. SPT65) and pipe lines into HC XDATA; segment-proximity matching (120 ft); auto inlet/outfall from topology; optional `force`, `d##`, `n##` args
- Ribbon **Import Civil** tool; 155 workspace tests

## [0.4.2] - 2026-06-22

### Added

- Ribbon: **Pipe Args** (`HC_PIPE_ARGS`) and **PDF Report** (`HC_REPORT_PDF`) tools
- PDF report: Manning + HGL + structure tables with surcharge/flat-invert status, multi-page overflow, disclaimer
- CI test workflow on push/PR; `scripts/run_all_demos.ps1`; `HC_PIPE_ARGS` slope regression test

### Changed

- PDF export mirrors HTML report logic (`manning_slope*`, `report_surcharged`, adverse/N/A labels)
- 150 workspace tests

## [0.4.1] - 2026-06-22

### Fixed

- **Hydraulic reports** — flat inverts no longer show zero slope / false surcharge everywhere; Manning uses `min_slope` when bed slope is flat; adverse slope labeled `ADVERSE SLOPE — capacity N/A` (not flat)
- **`HC_PIPE` handle parsing** — hex handles (`2B`, `2C`); `43 44 1.25 0.013` no longer misparsed as coordinates
- **Pipe placement** — lines snap to structure centers; downstream invert auto-step on flat runs
- **OCS `--serve`** — `HC_PIPE_ARGS 2B 2C d15 n13` serve-safe pipe placement (workaround for OCS `run_headless` interactive split; see `docs/OCS_SERVE.md`)

### Added

- `HC_PIPE_ARGS` — non-interactive pipe command for automation (`d##` inches, `n##` milli-n)
- `HC_NETWORK` — per-structure inverts and per-pipe diameter/slope in summary
- Demo scripts: `build_hydro_demo.ps1`, `build_manual_pipe_demo.ps1`, `test_slope_report.ps1`
- 143 workspace tests

## [0.4.0] - 2026-06-22

### Added

- **Pro licensing** — `HC_ACTIVATE` (online validate + offline `hc_live_*` stub), `HC_LICENSE` status, `HC_REPORT_PDF` PDF export via `printpdf` (Pro gate; `HYDROCOMPLETE_PRO=1` dev bypass)
- **NOAA Atlas 14** — 24 embedded city presets (`atlas14_presets`), live PFDS fetch + 30-day cache (`atlas14_fetcher`)
- `HC_ATLAS14 LIVE <lat> <lon> [rp]` and `HC_ATLAS14 APPLY <key> [rp]`
- `HC_PARAMS PRESET <key> [rp]` and `HC_PARAMS LIVE <lat> <lon> [rp]` apply IDF to tab state
- Engine: `license`, `pdf_report`, `atlas14_presets`, `atlas14_fetcher`

### Changed

- `HC_ATLAS14` lists full preset table with multi-RP i@10m labels (mirrors Civil3D `IdfPrompts`)
- 134 workspace tests (hydrocomplete 57, plugin 29, stormsewer 48)

## [0.3.0] - 2026-06-22

### Added

- `HC_NETWORK_EDIT` — per-drawing pipe Q and Manning n overrides (JSON in `%APPDATA%/HydroComplete/overrides/`); applied in `HC_ANALYZE` / `HC_REVIEW`
- `HC_BACKGROUND` — attach georeferenced raster on layer `HC-BACKGROUND` (interactive or `HC_BACKGROUND <path> <x> <y> <width>`)
- `HC_SOIL` — embedded soil table (50+ map units) + live SSURGO via USDA SDA with cache and regional fallback; BMP suitability screening
- Engine: `hydrocomplete::soil_database`, `hydrocomplete::ssurgo` (6 new tests)

### Changed

- 128 workspace tests (hydrocomplete 50, plugin 29, stormsewer 48)

### Planned (stubs remain)

- NOAA Atlas 14 live PFDS fetch
- Pro licensing (`HC_ACTIVATE`, `HC_REPORT_PDF`)

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

## [0.1.0] - 2026-06-09

### Added

- Initial Open CAD Studio external plugin (`opencad.hydrocomplete`)
- XDATA schemas: `HYDROCOMPLETE_STRUCT`, `HYDROCOMPLETE_PIPE`, `HYDROCOMPLETE_CATCHMENT`
- Core commands: `HC_ABOUT`, `HC_NETWORK`, `HC_PIPES`, `HC_CAPACITY`, `HC_SIZE`, `HC_HGL`, `HC_MULTIRP`, `HC_RATIONAL`, `HC_ATLAS14`
- `stormsewer` engine crate — Rational, Manning, HGL, IDF, LandXML, design review
- GPL-3.0-only license

[0.4.2]: https://github.com/mf4633/opencad-hydrocomplete-plugin/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/mf4633/opencad-hydrocomplete-plugin/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/mf4633/opencad-hydrocomplete-plugin/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/mf4633/opencad-hydrocomplete-plugin/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/mf4633/opencad-hydrocomplete-plugin/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/mf4633/opencad-hydrocomplete-plugin/releases/tag/v0.1.0
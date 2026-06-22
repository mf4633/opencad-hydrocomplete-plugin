# HydroComplete (`opencad.hydrocomplete`)

Open CAD Studio add-on mirroring **HydroComplete for Civil 3D** — stormwater hydrology/hydraulics with formula-transparent output.

## Data model (OpenCAD)

Civil 3D reads native pipe networks; OpenCAD stores hydraulics on drawing entities via XDATA:

### `HYDROCOMPLETE_STRUCT` (on `CIRCLE`)

| Index | Field | Type |
|-------|-------|------|
| 0 | kind | string (`inlet` / `junction` / `outfall`) |
| 1 | invert | real |
| 2 | rim | real |
| 3 | area | real (acres) |
| 4 | C | real |
| 5 | tc | real (minutes) |

### `HYDROCOMPLETE_PIPE` (on `LINE`)

| Index | Field | Type |
|-------|-------|------|
| 0 | diameter | real (feet) |
| 1 | n | real |
| 2 | from_handle | handle |
| 3 | to_handle | handle |

### `HYDROCOMPLETE_CATCHMENT` (on closed `LWPOLYLINE`)

| Index | Field | Type |
|-------|-------|------|
| 0 | C | real |
| 1 | length_ft | real |
| 2 | slope | real |
| 3 | inlet_handle | handle (0 = auto) |

## Commands (parity with Civil 3D)

| Command | Since | Description |
|---------|-------|-------------|
| `HC_ABOUT` | 0.1 | List all commands |
| `HC_NETWORK` | 0.1 | Per-network summary |
| `HC_PIPES` | 0.1 | Manning Qfull/Vfull (circular; box/arch via engine) |
| `HC_CAPACITY` | 0.1 | Design Q vs Q_full, d/D, surcharge |
| `HC_SIZE` | 0.1 | Standard pipe sizing |
| `HC_HGL` / `HC_PROFILE` | 0.1 | HGL long-section profile |
| `HC_REPORT` | 0.2 | Formula-transparent HTML report (Manning + HGL + capacity) → `Documents/HydroComplete/` |
| `HC_REPORT_PDF` | stub | Pro-only PDF export (free alternative: `HC_REPORT` HTML) |
| `HC_MULTIRP` | 0.1 | Multi return-period table |
| `HC_RATIONAL` | 0.1 | Rational Q from catchments |
| `HC_ATLAS14` | 0.1 | Embedded IDF preset list (live PFDS fetch planned) |
| `HC_PARAMS` | 0.1 | Storm analysis parameters (IDF, hydraulics) |
| `HC_PIPES_WRITE` | 0.2 | Label Qfull/Vfull on layer `HC-CAPACITY` |
| `HC_CAPACITY_WRITE` | 0.2 | Overload labels on layer `HC-CAPACITY` |
| `HC_VALIDATE` | 0.2 | Integrity + design-criteria review |
| `HC_ANALYZE` | 0.2 | Full-network analysis + surcharge/flood styling |
| `HC_SCS` | 0.2 | SCS CN runoff (default P=3 in) |
| `HC_LANDXML_IMPORT` | 0.2 | Import LandXML network (ribbon file dialog or path) |
| `HC_INLET` / `HC_JUNCTION` / `HC_OUTFALL` / `HC_PIPE` | 0.1 / 0.2 | Coordinate placement (0.1); interactive click/pick (0.2) |
| `HC_EDIT` | 0.2 | Edit XDATA fields |
| `HC_REVIEW`, `HC_DETENTION`, `HC_BMP_*`, … | — | Stubs — port from `HydroComplete.Engine` |

### `HC_VALIDATE` checks

Two passes, reported as warnings (info) and errors:

**Integrity** — rim ≤ invert, zero contributing area, runoff C out of range,
pipe diameter ≤ 0 / Manning n ≤ 0, dangling pipe handles, incomplete/malformed
XDATA, no structures, structures-without-pipes.

**Design criteria** (on the analyzed network, default municipal thresholds):

| Check | Default | Severity |
|-------|---------|----------|
| Adverse (uphill) slope | slope < 0 | error |
| Suspiciously flat slope | slope < 0.0005 ft/ft | warning |
| Surcharge | design Q > open-channel capacity | error |
| Near capacity | design Q > 85% of full | warning |
| Self-cleansing velocity | V < 2.0 ft/s | warning |
| Scour velocity | V > 10.0 ft/s | warning |
| Minimum cover | rim − (invert + diameter) < 1.0 ft | warning |
| Pipe size reduces downstream | downstream Ø < upstream Ø at a node | warning |
| Surface flooding | HGL above rim | error |

### Example workflow (v0.2)

```
HC_INLET 0,0 104 110 1.0 0.7
HC_OUTFALL 200,0 100 106
HC_PIPE 1 2 1.5 0.013
HC_VALIDATE
HC_ANALYZE
HC_PIPES
HC_CAPACITY
HC_HGL
HC_REPORT
```

## Parity roadmap

Engine modules are ported from `HydroComplete.Engine` (405 unit tests in Civil 3D) into `crates/hydrocomplete`:

1. **v0.1** — scaffold + core network commands
2. **v0.2** — full analysis + design validation, label writes, interactive placement, LandXML import, box/arch conduits, SCS runoff *(this release)*
3. **v0.3** — KaTeX HTML report export, Atlas 14 live PFDS fetch, detention, BMP/WQV, state compliance, Pro licensing
4. **v0.4** — GVF, culvert HDS-5, hydrograph routing, network diagram SVG
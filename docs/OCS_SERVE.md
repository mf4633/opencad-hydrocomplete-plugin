# OpenCAD Studio `--serve` and plugin commands

## Symptom

In `OpenCADStudio --serve`, commands like:

```json
{"op":"run","cmd":"HC_PIPE 2B 2C 1.25 0.013"}
```

create a pipe with the **default** diameter (1.50 ft) and **ignore** `1.25` and `0.013`, even though the JSON echoes the full `cmd` string.

`HC_EDIT 2E diameter 1.25 n 0.013` works — decimals are preserved on non-interactive commands.

## Root cause (OCS v0.6.0)

`src/app/automation.rs` → `run_headless`:

1. Splits the command on whitespace.
2. Dispatches **only the first token** (`HC_PIPE`), which starts HydroComplete’s **interactive** pipe tool.
3. Feeds remaining tokens (`2B`, `2C`, `1.25`, `0.013`) as entity picks / coordinates.
4. Never calls the plugin’s `HC_PIPE <args>` handler with the full argument string.
5. Diameter/n tokens arrive **after** the interactive command has already committed with defaults.

Reference: [OpenCADStudio `automation.rs` `run_headless`](https://github.com/HakanSeven12/OpenCADStudio/blob/main/src/app/automation.rs) (lines ~380–410).

## Recommended OCS fix

Before splitting for interactive point-feeding, try dispatching the **full command line** when the first-token dispatch did not start an interactive tool, or when the command prefix is registered as a plugin inline command:

```rust
// Pseudocode
if tokens.len() > 1 {
    self.dispatch_command(cmd);
    if self.tabs[i].active_cmd.is_none() {
        return;
    }
    // else fall through to interactive token feeding...
}
```

## HydroComplete workaround (v0.4.1+)

Use **`HC_PIPE_ARGS`** — no bare/interactive handler, so `--serve` dispatches the full line:

```
HC_PIPE_ARGS 2B 2C d15 n13
HC_PIPE_ARGS 2C 2D d18 n13
```

| Token | Meaning |
|-------|---------|
| `d15` | Diameter **15 inches** (1.25 ft) |
| `d18` | Diameter **18 inches** (1.50 ft) |
| `n13` | Manning **n = 0.013** (milli-n) |
| `d15n13` | Combined dia + n in one token |

Interactive GUI placement is unchanged: `HC_PIPE` (click/pick) or `HC_PIPE 2B 2C 1.25 0.013` in the command line (full-line dispatch).

## Automation scripts

| Script | Path |
|--------|------|
| LandXML demo (recommended) | `scripts/build_hydro_demo.ps1` |
| Manual pipes + Charlotte IDF | `scripts/build_manual_pipe_demo.ps1` |
| Slope / capacity report checks | `scripts/test_slope_report.ps1` |
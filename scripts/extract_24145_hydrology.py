"""Map 24-145 DRAINAGE WORKSHEET hydrology to HC_EDIT commands by structure position."""
import json
import math
import re
import sys
from pathlib import Path

import pandas as pd

WORKSHEET = Path(
    r"C:\Users\michael.flynn\Downloads\24-145-20260617T201018Z-3-001\24-145\HD\24-145 DRAINAGE WORKSHEET.xlsx"
)

# Post-developed watersheds (College Street at Creekside)
WATERSHEDS = [
    {"id": "SWS-1", "acres": 1.7602, "c": 0.78, "tc": 11.8, "x_max": 939330.0},
    {"id": "SWS-2", "acres": 2.23, "c": 0.76, "tc": 14.7, "x_max": 939480.0},
    {"id": "SWS-3", "acres": 1.47, "c": 0.57, "tc": 14.7, "x_max": math.inf},
]


def parse_structure_label(text: str):
    lines = [ln.strip() for ln in text.splitlines() if ln.strip()]
    if not lines:
        return None
    name = lines[0]
    rim = inv = None
    for line in lines[1:]:
        u = line.upper()
        if "RIM=" in u:
            m = re.search(r"=\s*([\d.]+)", line)
            if m:
                rim = float(m.group(1))
        elif "INV.OUT" in u or "INV.IN" in u:
            m = re.search(r"=\s*([\d.]+)", line)
            if m:
                v = float(m.group(1))
                inv = v if inv is None else min(inv, v)
    return {"name": name, "rim": rim, "invert": inv}


def watershed_for_x(x: float):
    for ws in WATERSHEDS:
        if x <= ws["x_max"]:
            return ws
    return WATERSHEDS[-1]


def is_inlet(name: str) -> bool:
    u = name.upper()
    return "CB" in u or " HW" in u or u.endswith(" HW") or u.startswith("HW")


def main():
    if len(sys.argv) < 2:
        print("usage: extract_24145_hydrology.py <structures.json>", file=sys.stderr)
        sys.exit(1)

    structs = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8-sig"))
    labels_path = Path(sys.argv[2]) if len(sys.argv) > 2 else None
    labels = []
    if labels_path and labels_path.exists():
        raw = json.loads(labels_path.read_text(encoding="utf-8-sig"))
        for ent in raw.get("entities", []):
            parsed = parse_structure_label(ent.get("value", ""))
            if not parsed:
                continue
            pos = ent.get("position") or ent.get("center") or [0, 0]
            labels.append({**parsed, "x": pos[0], "y": pos[1]})

    matched = []
    for s in structs:
        sx, sy = s["x"], s["y"]
        best = None
        best_d = 1e18
        for lab in labels:
            d = (lab["x"] - sx) ** 2 + (lab["y"] - sy) ** 2
            if d < best_d:
                best_d = d
                best = lab
        name = best["name"] if best and best_d <= 35.0**2 else s.get("name", "SPT65")
        rim = best.get("rim") if best and best_d <= 35.0**2 else None
        inv = best.get("invert") if best and best_d <= 35.0**2 else None
        ws = watershed_for_x(sx)
        matched.append(
            {
                "handle": s["handle"],
                "name": name,
                "x": sx,
                "y": sy,
                "kind": s.get("kind", "junction"),
                "rim": rim,
                "invert": inv,
                "watershed": ws["id"],
            }
        )

    inlets = [m for m in matched if is_inlet(m["name"]) or m["kind"] == "inlet"]
    ws_inlet_count = {}
    for m in inlets:
        ws_inlet_count[m["watershed"]] = ws_inlet_count.get(m["watershed"], 0) + 1

    edits = []
    for m in matched:
        parts = [f"HC_EDIT {m['handle']}"]
        if m.get("rim") is not None:
            parts += ["rim", f"{m['rim']:.2f}"]
        if m.get("invert") is not None:
            parts += ["invert", f"{m['invert']:.2f}"]
        if is_inlet(m["name"]) or m["kind"] == "inlet":
            ws = watershed_for_x(m["x"])
            n = max(ws_inlet_count.get(ws["id"], 1), 1)
            area_ac = ws["acres"] / n
            parts += ["area", f"{area_ac:.3f}", "c", f"{ws['c']:.2f}", "tc", f"{ws['tc']:.1f}"]
        if len(parts) > 2:
            edits.append(" ".join(parts))

    out = {
        "project": "College Street at Creekside (24-145)",
        "worksheet": str(WORKSHEET),
        "structures": len(matched),
        "inlets": len(inlets),
        "edits": edits,
        "watersheds": WATERSHEDS,
    }
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
#!/usr/bin/env python3
"""
Structure-aware tuner for the new climate precipitation model.

It tunes selected constants in rust/src/sim/climate/surface.rs by repeatedly
running `pnpm run bench --suite climate_solo`.
"""

from __future__ import annotations

import argparse
import itertools
import json
import math
import random
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Tuple


METRIC_RHO_PATTERN = re.compile(
    r"^\s*(temperature|precipitation|aridity|evapotranspiration|runoff):\s+rho=([-+]?\d+(?:\.\d+)?)\s*$",
    re.MULTILINE,
)
BAND_RHO_PATTERN = re.compile(
    r"^\s*\[([a-z_]+)\s+[-+]?\d+(?:\.\d+)?-[-+]?\d+(?:\.\d+)?\]\s+rho=([-+]?\d+(?:\.\d+)?)\s*$",
    re.MULTILINE,
)
PROCESS_LINE_PATTERN = re.compile(
    r"continental_reduction=([-+]?\d+(?:\.\d+)?)%\s+cap_reduction=([-+]?\d+(?:\.\d+)?)%\s+"
    r"depletion_reduction=([-+]?\d+(?:\.\d+)?)%\s+cold_coast_reduction=([-+]?\d+(?:\.\d+)?)%",
    re.MULTILINE,
)
PROCESS_LINE_2_PATTERN = re.compile(
    r"cap_hit_ratio=([-+]?\d+(?:\.\d+)?)%\s+mean_monsoon_boost_mm=([-+]?\d+(?:\.\d+)?)",
    re.MULTILINE,
)

DEFAULT_GRID = {
    "MONSOON_GAIN_MM": [520.0, 620.0, 760.0],
    "LAT_ITCZ_GAIN_MM": [1650.0, 1750.0, 1850.0],
    "LAT_SUBTROPICAL_DRY_GAIN_MM": [680.0, 760.0, 860.0],
    "CONTINENTAL_RELAX_MAX": [0.60, 0.70, 0.78],
    "CAP_DYNAMIC_MAX": [6.2, 6.8, 7.4],
    "COLD_RELAX_MAX": [0.60, 0.75],
}


@dataclass
class TrialResult:
    index: int
    values: Dict[str, float]
    metrics: Dict[str, float]
    objective: float
    feasible: bool
    elapsed_sec: float


def run(cmd: List[str], cwd: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd,
        cwd=str(cwd),
        text=True,
        capture_output=True,
        check=True,
    )


def parse_metrics(output: str) -> Dict[str, float]:
    found: Dict[str, float] = {}
    for name, value in METRIC_RHO_PATTERN.findall(output):
        found[name] = float(value)
    required = {"temperature", "precipitation", "aridity", "evapotranspiration", "runoff"}
    missing = required - set(found.keys())
    if missing:
        raise RuntimeError(f"missing metrics in bench output: {sorted(missing)}")

    for band, value in BAND_RHO_PATTERN.findall(output):
        found[f"{band}_precipitation_band"] = float(value)

    line_1 = PROCESS_LINE_PATTERN.search(output)
    line_2 = PROCESS_LINE_2_PATTERN.search(output)
    if line_1:
        found["continental_reduction_pct"] = float(line_1.group(1))
        found["cap_reduction_pct"] = float(line_1.group(2))
        found["depletion_reduction_pct"] = float(line_1.group(3))
        found["cold_coast_reduction_pct"] = float(line_1.group(4))
    if line_2:
        found["cap_hit_ratio_pct"] = float(line_2.group(1))
        found["mean_monsoon_boost_mm"] = float(line_2.group(2))

    found.setdefault("subtropics_precipitation_band", float("nan"))
    return found


def format_const_value(value: float) -> str:
    return f"{value:.6f}"


def set_const(text: str, const_name: str, value: float) -> str:
    pattern = re.compile(
        rf"^(const\s+{re.escape(const_name)}\s*:\s*f32\s*=\s*)([-+]?\d[\d_]*(?:\.\d+)?(?:f32)?);$",
        re.MULTILINE,
    )
    replaced, count = pattern.subn(rf"\g<1>{format_const_value(value)};", text, count=1)
    if count != 1:
        raise RuntimeError(f"failed to update const {const_name}")
    return replaced


def apply_values(surface_path: Path, values: Dict[str, float]) -> None:
    text = surface_path.read_text()
    for key, value in values.items():
        text = set_const(text, key, value)
    surface_path.write_text(text)


def run_bench(repo: Path) -> Dict[str, float]:
    completed = run(["pnpm", "run", "bench", "--suite", "climate_solo"], repo)
    output = f"{completed.stdout}\n{completed.stderr}"
    return parse_metrics(output)


def objective_score(metrics: Dict[str, float], baseline: Dict[str, float], args: argparse.Namespace) -> Tuple[bool, float]:
    feasible = (
        metrics["temperature"] >= baseline["temperature"] - args.max_temp_drop
        and metrics["precipitation"] >= baseline["precipitation"] - args.max_precip_drop
        and metrics["aridity"] >= baseline["aridity"] - args.max_aridity_drop
        and metrics["runoff"] >= baseline["runoff"] - args.max_runoff_drop
        and metrics["evapotranspiration"] >= baseline["evapotranspiration"] - args.max_et_drop
    )
    if not feasible:
        return False, -math.inf

    subtropics = metrics.get("subtropics_precipitation_band", float("nan"))
    subtropics_score = 0.0 if math.isnan(subtropics) else subtropics
    score = (
        0.45 * metrics["precipitation"]
        + 0.25 * metrics["aridity"]
        + 0.15 * metrics["evapotranspiration"]
        + 0.10 * metrics["runoff"]
        + 0.05 * subtropics_score
    )
    return True, score


def trial_grid(grid: Dict[str, List[float]], max_runs: int, seed: int) -> List[Dict[str, float]]:
    keys = list(grid.keys())
    product = list(itertools.product(*(grid[key] for key in keys)))
    trials = [dict(zip(keys, values)) for values in product]
    rng = random.Random(seed)
    rng.shuffle(trials)
    if max_runs > 0:
        return trials[:max_runs]
    return trials


def write_jsonl(path: Path, obj: Dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(obj, ensure_ascii=False) + "\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--surface-path", default="rust/src/sim/climate/surface.rs")
    parser.add_argument(
        "--output",
        default="benches/results/climate_tuning/runs/climate_structure_tuning_runs.jsonl",
    )
    parser.add_argument("--grid-json", default="", help="Optional JSON file for tuning grid.")
    parser.add_argument("--max-runs", type=int, default=24, help="0 means full grid.")
    parser.add_argument("--seed", type=int, default=42, help="Shuffle seed.")
    parser.add_argument("--max-temp-drop", type=float, default=0.002)
    parser.add_argument("--max-precip-drop", type=float, default=0.006)
    parser.add_argument("--max-aridity-drop", type=float, default=0.006)
    parser.add_argument("--max-runoff-drop", type=float, default=0.020)
    parser.add_argument("--max-et-drop", type=float, default=0.015)
    return parser.parse_args()


def load_grid(repo: Path, grid_json: str) -> Dict[str, List[float]]:
    if not grid_json:
        return DEFAULT_GRID
    path = (repo / grid_json).resolve()
    data = json.loads(path.read_text())
    if not isinstance(data, dict):
        raise RuntimeError("grid-json must be an object")
    return {str(k): [float(v) for v in values] for k, values in data.items()}


def main() -> int:
    args = parse_args()
    repo = Path(args.repo_root).resolve()
    surface_path = (repo / args.surface_path).resolve()
    output_path = (repo / args.output).resolve()
    if not surface_path.exists():
        print(json.dumps({"error": f"surface file not found: {surface_path}"}))
        return 1

    grid = load_grid(repo, args.grid_json)
    candidates = trial_grid(grid, args.max_runs, args.seed)
    if not candidates:
        print(json.dumps({"error": "no candidates"}))
        return 1

    original = surface_path.read_text()
    baseline_metrics = run_bench(repo)
    best: TrialResult | None = None
    results: List[TrialResult] = []

    try:
        for index, values in enumerate(candidates, start=1):
            started = time.time()
            apply_values(surface_path, values)
            metrics = run_bench(repo)
            feasible, objective = objective_score(metrics, baseline_metrics, args)
            elapsed = time.time() - started
            trial = TrialResult(
                index=index,
                values=values,
                metrics=metrics,
                objective=objective,
                feasible=feasible,
                elapsed_sec=elapsed,
            )
            results.append(trial)
            write_jsonl(
                output_path,
                {
                    "trial": index,
                    "values": values,
                    "metrics": metrics,
                    "feasible": feasible,
                    "objective_score": objective,
                    "elapsed_sec": elapsed,
                },
            )
            if feasible and (best is None or objective > best.objective):
                best = trial
    finally:
        surface_path.write_text(original)
        if best is not None:
            apply_values(surface_path, best.values)

    summary = {
        "search_space_size": math.prod(len(v) for v in grid.values()),
        "evaluated_runs": len(results),
        "baseline": baseline_metrics,
        "constraints": {
            "max_temp_drop": args.max_temp_drop,
            "max_precip_drop": args.max_precip_drop,
            "max_aridity_drop": args.max_aridity_drop,
            "max_runoff_drop": args.max_runoff_drop,
            "max_et_drop": args.max_et_drop,
        },
        "best": None
        if best is None
        else {
            "trial": best.index,
            "values": best.values,
            "metrics": best.metrics,
            "objective_score": best.objective,
        },
        "output_jsonl": str(output_path),
    }
    print(json.dumps(summary, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())

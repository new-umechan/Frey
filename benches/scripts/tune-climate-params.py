#!/usr/bin/env python3
"""
Climate parameter tuner for climate_solo benchmark.

This script performs exhaustive search over a finite parameter grid and reports
the best precipitation rho under constraints.

By design, "theoretical maximum" means the maximum within the explicitly
defined discrete search space.
"""

from __future__ import annotations

import argparse
import itertools
import json
import math
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Tuple


RHO_PATTERN = re.compile(r"^\s*([a-z_]+):\s+rho=([-+]?\d+(?:\.\d+)?)\s*$", re.MULTILINE)

DEFAULT_GRID = {
    "precipitation.hadley_anomaly_gain": [0.35, 0.45, 0.55],
    "precipitation.continentality_gain": [0.36, 0.44, 0.52],
    "precipitation.moisture_convergence_gain": [32000.0, 38000.0, 44000.0],
    "precipitation.convergence_blend": [0.18, 0.24, 0.30],
    "precipitation.cap_from_moisture": [2.2, 2.8, 3.4],
    "orography.uplift_gain_mm": [280.0, 360.0, 440.0],
    "orography.rain_shadow_gain": [1.4, 1.7, 2.0],
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


def parse_metrics(stdout: str) -> Dict[str, float]:
    found = {}
    for name, value in RHO_PATTERN.findall(stdout):
        found[name] = float(value)
    required = {"temperature", "precipitation", "aridity", "evapotranspiration", "runoff"}
    missing = required - set(found.keys())
    if missing:
        raise RuntimeError(f"failed to parse rho metrics, missing={sorted(missing)}")
    return found


def set_scalar(yaml_text: str, dotted_key: str, value: float) -> str:
    key = dotted_key.split(".")[-1]
    pattern = re.compile(
        rf"^(\s*{re.escape(key)}:\s*)([-+]?\d+(?:\.\d+)?)\s*$",
        re.MULTILINE,
    )
    replaced, count = pattern.subn(rf"\g<1>{value}", yaml_text, count=1)
    if count != 1:
        raise RuntimeError(f"failed to update key={dotted_key}")
    return replaced


def apply_values(yaml_path: Path, values: Dict[str, float]) -> None:
    text = yaml_path.read_text()
    for key, value in values.items():
        text = set_scalar(text, key, value)
    yaml_path.write_text(text)


def sync_climate_params(repo: Path) -> None:
    run(["pnpm", "run", "climate:sync"], repo)


def run_bench(repo: Path) -> Dict[str, float]:
    completed = run(["pnpm", "run", "bench", "--", "--suite", "climate_solo"], repo)
    output = f"{completed.stdout}\n{completed.stderr}"
    return parse_metrics(output)


def objective_score(
    metrics: Dict[str, float],
    baseline_temperature: float,
    baseline_runoff: float,
    min_aridity: float,
    max_temp_drop: float,
    max_runoff_drop: float,
) -> Tuple[bool, float]:
    feasible = (
        metrics["aridity"] >= min_aridity
        and metrics["temperature"] >= (baseline_temperature - max_temp_drop)
        and metrics["runoff"] >= (baseline_runoff - max_runoff_drop)
    )
    if not feasible:
        return False, -math.inf
    return True, metrics["precipitation"]


def trial_grid(grid: Dict[str, List[float]]) -> List[Dict[str, float]]:
    keys = list(grid.keys())
    product = itertools.product(*(grid[key] for key in keys))
    return [dict(zip(keys, values)) for values in product]


def write_jsonl(path: Path, obj: Dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(obj, ensure_ascii=False) + "\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--config-path", default="config/climate.yaml")
    parser.add_argument("--output", default="benches/results/climate_tuning_runs.jsonl")
    parser.add_argument("--min-aridity", type=float, default=0.34)
    parser.add_argument("--max-temp-drop", type=float, default=0.01)
    parser.add_argument("--max-runoff-drop", type=float, default=0.01)
    parser.add_argument(
        "--max-runs",
        type=int,
        default=0,
        help="0 means exhaustive over the full grid.",
    )
    parser.add_argument(
        "--grid-json",
        default="",
        help="Optional JSON file containing { dotted_key: [values...] }",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = Path(args.repo_root).resolve()
    yaml_path = (repo / args.config_path).resolve()
    output_path = (repo / args.output).resolve()

    if not yaml_path.exists():
        print(json.dumps({"error": f"config not found: {yaml_path}"}))
        return 1

    original_yaml = yaml_path.read_text()
    if args.grid_json:
        grid_path = (repo / args.grid_json).resolve()
        grid = json.loads(grid_path.read_text())
    else:
        grid = DEFAULT_GRID

    candidates = trial_grid(grid)
    if args.max_runs > 0:
        candidates = candidates[: args.max_runs]

    if not candidates:
        print(json.dumps({"error": "no candidates"}))
        return 1

    baseline_metrics = run_bench(repo)
    baseline_temperature = baseline_metrics["temperature"]
    baseline_runoff = baseline_metrics["runoff"]
    best: TrialResult | None = None
    results: List[TrialResult] = []

    try:
        for idx, values in enumerate(candidates, start=1):
            started = time.time()
            apply_values(yaml_path, values)
            sync_climate_params(repo)
            metrics = run_bench(repo)
            feasible, score = objective_score(
                metrics,
                baseline_temperature,
                baseline_runoff,
                args.min_aridity,
                args.max_temp_drop,
                args.max_runoff_drop,
            )
            elapsed = time.time() - started
            trial = TrialResult(
                index=idx,
                values=values,
                metrics=metrics,
                objective=score,
                feasible=feasible,
                elapsed_sec=elapsed,
            )
            results.append(trial)
            write_jsonl(
                output_path,
                {
                    "trial": idx,
                    "values": values,
                    "metrics": metrics,
                    "feasible": feasible,
                    "objective_precipitation": score,
                    "elapsed_sec": elapsed,
                },
            )
            if feasible and (best is None or score > best.objective):
                best = trial
    finally:
        yaml_path.write_text(original_yaml)
        if best is not None:
            apply_values(yaml_path, best.values)
        sync_climate_params(repo)

    summary = {
        "search_space_size": len(trial_grid(grid)),
        "evaluated_runs": len(results),
        "baseline": baseline_metrics,
        "constraints": {
            "min_aridity": args.min_aridity,
            "max_temp_drop": args.max_temp_drop,
            "max_runoff_drop": args.max_runoff_drop,
            "baseline_temperature": baseline_temperature,
            "baseline_runoff": baseline_runoff,
        },
        "best": None
        if best is None
        else {
            "trial": best.index,
            "values": best.values,
            "metrics": best.metrics,
            "objective_precipitation": best.objective,
        },
        "output_jsonl": str(output_path),
    }
    print(json.dumps(summary, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())

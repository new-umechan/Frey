#!/usr/bin/env python3
"""
Hydrology parameter tuner for hydrology_solo benchmark.

This script performs exhaustive search over a finite parameter grid and reports
the best objective score under constraints.

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


# Hydrology-specific parameters to tune
# These are the key parameters affecting river_flow and is_lake accuracy
RHO_PATTERN = re.compile(r"^\s*([a-z_]+):\s+rho=([-+]?\d+(?:\.\d+)?)\s*$", re.MULTILINE)
LAKE_METRIC_PATTERN = re.compile(
    r"^\s*precision=([-+]?\d+(?:\.\d+)?)\s+recall=([-+]?\d+(?:\.\d+)?)\s+f1=([-+]?\d+(?:\.\d+)?)\s*$",
    re.MULTILINE,
)

DEFAULT_GRID = {
    # River network formation parameters
    "river_accumulation_threshold": [0.008, 0.012, 0.016],
    "river_inertia_gain": [0.15, 0.25, 0.35],
    "river_curvature_penalty": [0.08, 0.12, 0.16],
    
    # Sink/lake parameters
    "sink_local_rebuild_radius": [3, 4, 5],
    "sink_overflow_hysteresis": [0.18, 0.24, 0.30],
    "sink_min_capacity": [0.08, 0.12, 0.16],
    
    # Baseflow parameters
    "baseflow_infiltration_rate": [0.22, 0.28, 0.34],
    "baseflow_release_rate": [0.08, 0.12, 0.16],
    "baseflow_storage_cap": [180.0, 240.0, 300.0],
    
    # Erosion/deposition (affects long-term river behavior)
    "hydraulic_erosion_rate": [0.012, 0.018, 0.024],
    "hydraulic_deposit_rate": [0.008, 0.012, 0.016],
    "sediment_capacity_gain": [0.35, 0.45, 0.55],
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
    
    # Parse river_flow rho
    for match in re.finditer(r"river_flow:\s+rho=([-+]?\d+(?:\.\d+)?)", stdout):
        found["river_flow"] = float(match.group(1))
    
    # Parse lake metrics (format: "precision=0.000  recall=0.000  f1=0.000")
    lake_match = re.search(r"precision=([-+]?\d+(?:\.\d+)?)\s+recall=([-+]?\d+(?:\.\d+)?)\s+f1=([-+]?\d+(?:\.\d+)?)", stdout)
    if lake_match:
        found["lake_precision"] = float(lake_match.group(1))
        found["lake_recall"] = float(lake_match.group(2))
        found["lake_f1"] = float(lake_match.group(3))
    
    required = {"river_flow", "lake_f1"}
    missing = required - set(found.keys())
    if missing:
        raise RuntimeError(f"failed to parse hydrology metrics, missing={sorted(missing)}")
    
    return found


def set_scalar(yaml_text: str, dotted_key: str, value: float) -> str:
    """Update a scalar value in YAML text by key name."""
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
    """Apply parameter values to the YAML config file."""
    text = yaml_path.read_text()
    for key, value in values.items():
        text = set_scalar(text, key, value)
    yaml_path.write_text(text)


def sync_terrain_params(repo: Path) -> None:
    """Sync terrain parameters to WASM build."""
    run(["pnpm", "run", "terrain:sync"], repo)


def run_bench(repo: Path) -> Dict[str, float]:
    """Run the hydrology_solo benchmark and parse metrics."""
    completed = run(["pnpm", "run", "bench", "--suite", "hydrology_solo"], repo)
    output = f"{completed.stdout}\n{completed.stderr}"
    return parse_metrics(output)


def objective_score(
    metrics: Dict[str, float],
    baseline_flow_rho: float,
    baseline_lake_f1: float,
    min_flow_rho: float,
    min_lake_f1: float,
) -> Tuple[bool, float]:
    """
    Compute the objective score for a trial.
    
    Returns (feasible, score) where:
    - feasible: True if constraints are satisfied
    - score: Weighted combination of river_flow rho and lake_f1
    """
    feasible = (
        metrics["river_flow"] >= min_flow_rho
        and metrics["lake_f1"] >= min_lake_f1
    )
    if not feasible:
        return False, -math.inf
    
    # Objective: weighted combination of river flow accuracy and lake detection
    # River flow is primary (60%), lake detection is secondary (40%)
    return True, 0.6 * metrics["river_flow"] + 0.4 * metrics["lake_f1"]


def trial_grid(grid: Dict[str, List[float]]) -> List[Dict[str, float]]:
    """Generate all combinations of parameter values."""
    keys = list(grid.keys())
    product = itertools.product(*(grid[key] for key in keys))
    return [dict(zip(keys, values)) for values in product]


def write_jsonl(path: Path, obj: Dict) -> None:
    """Append a JSON object to a JSONL file."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(obj, ensure_ascii=False) + "\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--config-path", default="config/terrain.yaml")
    parser.add_argument(
        "--output",
        default="benches/results/hydrology_tuning/runs/hydrology_tuning_runs.jsonl",
    )
    # Note: 1 tick ベンチでは river_flow rho は 0.1-0.3 程度、lake_f1 は 0.0-0.1 程度が現状
    # 制約は「ベースラインからどれだけ改善するか」を見るために緩めに設定
    parser.add_argument("--min-flow-rho", type=float, default=0.10)
    parser.add_argument("--min-lake-f1", type=float, default=0.0)
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

    # Get baseline metrics
    baseline_metrics = run_bench(repo)
    baseline_flow_rho = baseline_metrics["river_flow"]
    baseline_lake_f1 = baseline_metrics["lake_f1"]
    
    best: TrialResult | None = None
    results: List[TrialResult] = []

    try:
        for idx, values in enumerate(candidates, start=1):
            started = time.time()
            apply_values(yaml_path, values)
            sync_terrain_params(repo)
            metrics = run_bench(repo)
            feasible, score = objective_score(
                metrics,
                baseline_flow_rho,
                baseline_lake_f1,
                args.min_flow_rho,
                args.min_lake_f1,
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
                    "objective_score": score,
                    "elapsed_sec": elapsed,
                },
            )
            if feasible and (best is None or score > best.objective):
                best = trial
    finally:
        yaml_path.write_text(original_yaml)
        if best is not None:
            apply_values(yaml_path, best.values)
        sync_terrain_params(repo)

    summary = {
        "search_space_size": len(trial_grid(grid)),
        "evaluated_runs": len(results),
        "baseline": baseline_metrics,
        "constraints": {
            "min_flow_rho": args.min_flow_rho,
            "min_lake_f1": args.min_lake_f1,
            "baseline_flow_rho": baseline_flow_rho,
            "baseline_lake_f1": baseline_lake_f1,
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

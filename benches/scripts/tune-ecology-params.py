#!/usr/bin/env python3
"""
Ecology parameter tuner for ecology_solo benchmark.

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


# Ecology-specific parameters to tune
# These are the key parameters affecting tree_cover, ground_cover, and biome accuracy
TREE_RHO_PATTERN = re.compile(r"^\s*tree_cover:\s+rho=([-+]?\d+(?:\.\d+)?)\s*$", re.MULTILINE)
GROUND_RHO_PATTERN = re.compile(r"^\s*ground_cover:\s+rho=([-+]?\d+(?:\.\d+)?)\s*$", re.MULTILINE)
BIOME_F1_PATTERN = re.compile(r"^\s*biome:\s+macro_f1=([-+]?\d+(?:\.\d+)?)\s+accuracy=([-+]?\d+(?:\.\d+)?)\s*$", re.MULTILINE)

DEFAULT_GRID = {
    # Tree cover dynamics
    "tree_growth_rate": [0.14, 0.18, 0.22],
    "tree_decline_rate": [0.06, 0.08, 0.10],

    # Ground cover dynamics
    "ground_growth_rate": [0.12, 0.16, 0.20],
    "ground_decline_rate": [0.06, 0.08, 0.10],

    # Disturbance dynamics
    "disturbance_up_rate": [0.18, 0.22, 0.26],
    "disturbance_down_rate": [0.08, 0.10, 0.12],

    # Biome classification thresholds
    "alpine_threshold": [0.68, 0.72, 0.76],
    "tundra_threshold": [-3.0, -2.5, -2.0],
    "desert_threshold": [180.0, 220.0, 260.0],
    "wetland_threshold": [0.52, 0.58, 0.64],
    "wetland_tree_threshold": [0.50, 0.55, 0.60],
    "tropical_temp_threshold": [20.0, 22.0, 24.0],
    "boreal_temp_threshold": [4.0, 6.0, 8.0],
    "forest_threshold": [0.52, 0.58, 0.64],
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

    # Parse tree_cover rho
    tree_match = TREE_RHO_PATTERN.search(stdout)
    if tree_match:
        found["tree_cover"] = float(tree_match.group(1))

    # Parse ground_cover rho
    ground_match = GROUND_RHO_PATTERN.search(stdout)
    if ground_match:
        found["ground_cover"] = float(ground_match.group(1))

    # Parse biome macro_f1 and accuracy
    biome_match = BIOME_F1_PATTERN.search(stdout)
    if biome_match:
        found["biome_macro_f1"] = float(biome_match.group(1))
        found["biome_accuracy"] = float(biome_match.group(2))

    required = {"tree_cover", "ground_cover", "biome_macro_f1"}
    missing = required - set(found.keys())
    if missing:
        raise RuntimeError(f"failed to parse ecology metrics, missing={sorted(missing)}")

    return found


def set_ecology_param(rust_path: Path, param_name: str, value: float) -> None:
    """Update an ecology parameter in the Rust source file."""
    text = rust_path.read_text()

    # Pattern to match: const PARAM_NAME: f32 = <value>;
    pattern = re.compile(
        rf"^(\s*const\s+{re.escape(param_name)}:\s+f32\s*=\s*)([-+]?\d+(?:\.\d+)?)\s*;",
        re.MULTILINE,
    )

    replaced, count = pattern.subn(rf"\g<1>{value};", text, count=1)
    if count != 1:
        raise RuntimeError(f"failed to update param={param_name}")

    rust_path.write_text(replaced)


def apply_values(rust_path: Path, values: Dict[str, float]) -> None:
    """Apply parameter values to the Rust source file."""
    for key, value in values.items():
        # Convert snake_case to UPPER_SNAKE_CASE for Rust constants
        param_name = key.upper()
        set_ecology_param(rust_path, param_name, value)


def sync_terrain_params(repo: Path) -> None:
    """Sync terrain parameters (includes ecology build)."""
    run(["pnpm", "run", "terrain:sync"], repo)


def run_bench(repo: Path) -> Dict[str, float]:
    """Run the ecology_solo benchmark and parse metrics."""
    completed = run(["pnpm", "run", "bench", "--suite", "ecology_solo"], repo)
    output = f"{completed.stdout}\n{completed.stderr}"
    return parse_metrics(output)


def objective_score(
    metrics: Dict[str, float],
    baseline_tree_rho: float,
    baseline_ground_rho: float,
    baseline_biome_f1: float,
    min_tree_rho: float,
    min_ground_rho: float,
    min_biome_f1: float,
) -> Tuple[bool, float]:
    """
    Compute the objective score for a trial.

    Returns (feasible, score) where:
    - feasible: True if constraints are satisfied
    - score: Weighted combination of tree_cover rho, ground_cover rho, and biome macro_f1
    """
    feasible = (
        metrics["tree_cover"] >= min_tree_rho
        and metrics["ground_cover"] >= min_ground_rho
        and metrics["biome_macro_f1"] >= min_biome_f1
    )
    if not feasible:
        return False, -math.inf

    # Objective: weighted combination
    # tree_cover (35%), ground_cover (25%), biome macro_f1 (40%)
    return True, 0.35 * metrics["tree_cover"] + 0.25 * metrics["ground_cover"] + 0.40 * metrics["biome_macro_f1"]


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
    parser.add_argument("--rust-path", default="rust/src/sim/ecology/mod.rs")
    parser.add_argument(
        "--output",
        default="benches/results/ecology_tuning/runs/ecology_tuning_runs.jsonl",
    )
    # Note: ecology_solo ベンチでは tree_cover rho は 0.5-0.7 程度、
    # ground_cover rho は 0.4-0.6 程度、biome macro_f1 は 0.3-0.5 程度が現状
    parser.add_argument("--min-tree-rho", type=float, default=0.30)
    parser.add_argument("--min-ground-rho", type=float, default=0.20)
    parser.add_argument("--min-biome-f1", type=float, default=0.10)
    parser.add_argument(
        "--max-runs",
        type=int,
        default=0,
        help="0 means exhaustive over the full grid.",
    )
    parser.add_argument(
        "--grid-json",
        default="",
        help="Optional JSON file containing { param_name: [values...] }",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = Path(args.repo_root).resolve()
    rust_path = (repo / args.rust_path).resolve()
    output_path = (repo / args.output).resolve()

    if not rust_path.exists():
        print(json.dumps({"error": f"rust source not found: {rust_path}"}))
        return 1

    original_rust = rust_path.read_text()
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
    baseline_tree_rho = baseline_metrics["tree_cover"]
    baseline_ground_rho = baseline_metrics["ground_cover"]
    baseline_biome_f1 = baseline_metrics["biome_macro_f1"]

    best: TrialResult | None = None
    results: List[TrialResult] = []

    try:
        for idx, values in enumerate(candidates, start=1):
            started = time.time()
            apply_values(rust_path, values)
            sync_terrain_params(repo)
            metrics = run_bench(repo)
            feasible, score = objective_score(
                metrics,
                baseline_tree_rho,
                baseline_ground_rho,
                baseline_biome_f1,
                args.min_tree_rho,
                args.min_ground_rho,
                args.min_biome_f1,
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
        rust_path.write_text(original_rust)
        if best is not None:
            apply_values(rust_path, best.values)
        sync_terrain_params(repo)

    summary = {
        "search_space_size": len(trial_grid(grid)),
        "evaluated_runs": len(results),
        "baseline": {
            "tree_cover": baseline_tree_rho,
            "ground_cover": baseline_ground_rho,
            "biome_macro_f1": baseline_biome_f1,
        },
        "constraints": {
            "min_tree_rho": args.min_tree_rho,
            "min_ground_rho": args.min_ground_rho,
            "min_biome_f1": args.min_biome_f1,
            "baseline_tree_rho": baseline_tree_rho,
            "baseline_ground_rho": baseline_ground_rho,
            "baseline_biome_f1": baseline_biome_f1,
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

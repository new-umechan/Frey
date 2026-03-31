#!/usr/bin/env python3
"""
Climate parameter tuner for climate_solo benchmark.

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


RHO_PATTERN = re.compile(r"^\s*([a-z_]+):\s+rho=([-+]?\d+(?:\.\d+)?)\s*$", re.MULTILINE)
BAND_RHO_PATTERN = re.compile(
    r"^\s*\[([a-z_]+)\s+[-+]?\d+(?:\.\d+)?-[-+]?\d+(?:\.\d+)?\]\s+rho=([-+]?\d+(?:\.\d+)?)\s*$",
    re.MULTILINE,
)

DEFAULT_GRID = {
    "precipitation.hadley_anomaly_gain": [0.35, 0.45, 0.55],
    "precipitation.continentality_gain": [0.30, 0.36, 0.42],
    "precipitation.cap_from_moisture": [2.0, 2.4, 2.8],
    "core.moisture_transport_gain": [0.12, 0.16, 0.20],
    "core.condense_excess_gain": [0.45, 0.55, 0.65],
    "core.orographic_condense_gain": [0.08, 0.12, 0.16],
    "core.ocean_evaporation_gain": [0.20, 0.26, 0.32],
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
    for band, value in BAND_RHO_PATTERN.findall(stdout):
        found[f"{band}_precipitation_band"] = float(value)
    required = {"temperature", "precipitation", "aridity", "evapotranspiration", "runoff"}
    missing = required - set(found.keys())
    if missing:
        raise RuntimeError(f"failed to parse rho metrics, missing={sorted(missing)}")
    found.setdefault("subtropics_precipitation_band", float("nan"))
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


def get_scalar(yaml_text: str, dotted_key: str) -> float:
    key = dotted_key.split(".")[-1]
    pattern = re.compile(
        rf"^\s*{re.escape(key)}:\s*([-+]?\d+(?:\.\d+)?)\s*$",
        re.MULTILINE,
    )
    match = pattern.search(yaml_text)
    if not match:
        raise RuntimeError(f"failed to read key={dotted_key}")
    return float(match.group(1))


def apply_values(yaml_path: Path, values: Dict[str, float]) -> None:
    text = yaml_path.read_text()
    for key, value in values.items():
        text = set_scalar(text, key, value)
    yaml_path.write_text(text)


def sync_climate_params(repo: Path) -> None:
    run(["pnpm", "run", "climate:sync"], repo)


def run_bench(repo: Path) -> Dict[str, float]:
    completed = run(["pnpm", "run", "bench", "--suite", "climate_solo"], repo)
    output = f"{completed.stdout}\n{completed.stderr}"
    return parse_metrics(output)


def objective_score(
    metrics: Dict[str, float],
    baseline_temperature: float,
    baseline_runoff: float,
    min_aridity: float,
    max_temp_drop: float,
    max_runoff_drop: float,
    weight_precip: float,
    weight_subtropics: float,
    weight_aridity: float,
) -> Tuple[bool, float]:
    feasible = (
        metrics["aridity"] >= min_aridity
        and metrics["temperature"] >= (baseline_temperature - max_temp_drop)
        and metrics["runoff"] >= (baseline_runoff - max_runoff_drop)
    )
    if not feasible:
        return False, -math.inf
    subtropics = metrics.get("subtropics_precipitation_band", float("nan"))
    subtropics_term = 0.0 if math.isnan(subtropics) else subtropics
    return True, (
        weight_precip * metrics["precipitation"]
        + weight_subtropics * subtropics_term
        + weight_aridity * metrics["aridity"]
    )


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
    parser.add_argument(
        "--output",
        default="benches/results/climate_tuning/runs/climate_tuning_runs.jsonl",
    )
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
    parser.add_argument(
        "--mode",
        choices=["exhaustive", "coordinate"],
        default="coordinate",
        help="Search mode. coordinate is faster for iterative local tuning.",
    )
    parser.add_argument(
        "--rounds",
        type=int,
        default=2,
        help="Rounds for coordinate mode.",
    )
    parser.add_argument("--weight-precip", type=float, default=0.60)
    parser.add_argument("--weight-subtropics", type=float, default=0.20)
    parser.add_argument("--weight-aridity", type=float, default=0.20)
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

    if not grid:
        print(json.dumps({"error": "empty grid"}))
        return 1

    baseline_metrics = run_bench(repo)
    baseline_temperature = baseline_metrics["temperature"]
    baseline_runoff = baseline_metrics["runoff"]
    best: TrialResult | None = None
    results: List[TrialResult] = []
    trial_index = 0

    def evaluate(values: Dict[str, float]) -> TrialResult:
        nonlocal trial_index, best
        trial_index += 1
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
            args.weight_precip,
            args.weight_subtropics,
            args.weight_aridity,
        )
        elapsed = time.time() - started
        trial = TrialResult(
            index=trial_index,
            values=dict(values),
            metrics=metrics,
            objective=score,
            feasible=feasible,
            elapsed_sec=elapsed,
        )
        results.append(trial)
        write_jsonl(
            output_path,
            {
                "trial": trial.index,
                "values": trial.values,
                "metrics": trial.metrics,
                "feasible": trial.feasible,
                "objective_score": trial.objective,
                "elapsed_sec": trial.elapsed_sec,
            },
        )
        if feasible and (best is None or score > best.objective):
            best = trial
        return trial

    try:
        if args.mode == "exhaustive":
            candidates = trial_grid(grid)
            if args.max_runs > 0:
                candidates = candidates[: args.max_runs]
            if not candidates:
                print(json.dumps({"error": "no candidates"}))
                return 1
            for values in candidates:
                evaluate(values)
        else:
            current_yaml = yaml_path.read_text()
            current = {
                key: get_scalar(current_yaml, key)
                for key in grid.keys()
            }
            current_trial = evaluate(current)
            current_score = current_trial.objective if current_trial.feasible else -math.inf
            budget_limit = args.max_runs if args.max_runs > 0 else 10**9

            for _ in range(max(1, args.rounds)):
                improved = False
                for key, choices in grid.items():
                    if trial_index >= budget_limit:
                        break
                    unique_choices = sorted({float(v) for v in choices}, key=lambda v: abs(v - current[key]))
                    best_local_value = current[key]
                    best_local_score = current_score
                    for value in unique_choices:
                        if abs(value - current[key]) <= 1e-9:
                            continue
                        if trial_index >= budget_limit:
                            break
                        candidate = dict(current)
                        candidate[key] = value
                        trial = evaluate(candidate)
                        if trial.feasible and trial.objective > best_local_score:
                            best_local_score = trial.objective
                            best_local_value = value
                    if abs(best_local_value - current[key]) > 1e-9:
                        current[key] = best_local_value
                        current_score = best_local_score
                        improved = True
                if trial_index >= budget_limit or not improved:
                    break
    finally:
        yaml_path.write_text(original_yaml)
        if best is not None:
            apply_values(yaml_path, best.values)
        sync_climate_params(repo)

    summary = {
        "search_space_size": len(trial_grid(grid)),
        "mode": args.mode,
        "evaluated_runs": len(results),
        "baseline": baseline_metrics,
        "constraints": {
            "min_aridity": args.min_aridity,
            "max_temp_drop": args.max_temp_drop,
            "max_runoff_drop": args.max_runoff_drop,
            "baseline_temperature": baseline_temperature,
            "baseline_runoff": baseline_runoff,
        },
        "weights": {
            "precipitation": args.weight_precip,
            "subtropics": args.weight_subtropics,
            "aridity": args.weight_aridity,
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

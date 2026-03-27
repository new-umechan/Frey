#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
import rasterio


PROPS = ("bdod", "cec", "phh2o", "soc")
DEPTH_TAGS = ("0_5", "5_15", "15_30")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Aggregate 3 SoilGrids depth rasters into 0-30cm weighted rasters. "
            "Input rasters are expected as 0.1deg products."
        )
    )
    parser.add_argument(
        "--in-dir",
        default="benches/raw/ecology/soilgrids",
        help="Directory containing 12 SoilGrids depth rasters.",
    )
    parser.add_argument(
        "--out-dir",
        default="benches/raw/ecology/soilgrids",
        help="Directory to write aggregated 0-30cm rasters.",
    )
    parser.add_argument(
        "--suffix",
        default="0p1deg",
        help="Filename suffix used by input and output rasters.",
    )
    parser.add_argument("--w-0-5", type=float, default=5.0, help="Weight for 0-5cm.")
    parser.add_argument("--w-5-15", type=float, default=3.5, help="Weight for 5-15cm.")
    parser.add_argument("--w-15-30", type=float, default=1.5, help="Weight for 15-30cm.")
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Overwrite output files if they already exist.",
    )
    return parser.parse_args()


def input_path(base: Path, prop: str, depth_tag: str, suffix: str) -> Path:
    return base / f"{prop}_{depth_tag}cm_mean_{suffix}.tif"


def output_path(base: Path, prop: str, suffix: str) -> Path:
    return base / f"{prop}_0_30cm_mean_{suffix}.tif"


def ensure_compatible(profiles: list[dict]) -> None:
    first = profiles[0]
    keys = ("width", "height", "crs", "transform")
    for idx, profile in enumerate(profiles[1:], start=1):
        for key in keys:
            if profile[key] != first[key]:
                raise ValueError(f"incompatible raster profile at index {idx}: key={key}")


def read_as_float(path: Path) -> tuple[np.ndarray, dict]:
    with rasterio.open(path) as src:
        data = src.read(1).astype(np.float32, copy=False)
        nodata = src.nodata
        if nodata is not None:
            data = np.where(data == nodata, np.nan, data)
        profile = src.profile.copy()
    return data, profile


def aggregate_depths(
    arr_0_5: np.ndarray,
    arr_5_15: np.ndarray,
    arr_15_30: np.ndarray,
    w_0_5: float,
    w_5_15: float,
    w_15_30: float,
) -> np.ndarray:
    denom = w_0_5 + w_5_15 + w_15_30
    if denom <= 0.0:
        raise ValueError("weights must sum to > 0")
    weighted = arr_0_5 * w_0_5 + arr_5_15 * w_5_15 + arr_15_30 * w_15_30
    finite_mask = np.isfinite(arr_0_5) & np.isfinite(arr_5_15) & np.isfinite(arr_15_30)
    out = np.full(arr_0_5.shape, np.nan, dtype=np.float32)
    out[finite_mask] = (weighted[finite_mask] / denom).astype(np.float32, copy=False)
    return out


def main() -> None:
    args = parse_args()
    in_dir = Path(args.in_dir)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    for prop in PROPS:
        paths = [input_path(in_dir, prop, depth, args.suffix) for depth in DEPTH_TAGS]
        for path in paths:
            if not path.exists():
                raise FileNotFoundError(f"missing input raster: {path}")

        out_path = output_path(out_dir, prop, args.suffix)
        if out_path.exists() and not args.overwrite:
            print(f"SKIP existing output: {out_path}")
            continue

        arrays: list[np.ndarray] = []
        profiles: list[dict] = []
        for path in paths:
            data, profile = read_as_float(path)
            arrays.append(data)
            profiles.append(profile)
        ensure_compatible(profiles)

        agg = aggregate_depths(
            arrays[0],
            arrays[1],
            arrays[2],
            float(args.w_0_5),
            float(args.w_5_15),
            float(args.w_15_30),
        )
        out_profile = profiles[0].copy()
        out_profile.update(
            {
                "dtype": "float32",
                "count": 1,
                "nodata": np.nan,
                "compress": "lzw",
                "tiled": True,
            }
        )
        with rasterio.open(out_path, "w", **out_profile) as dst:
            dst.write(agg, 1)
        print(
            f"WROTE {out_path} "
            f"(weights={args.w_0_5:.3f},{args.w_5_15:.3f},{args.w_15_30:.3f})"
        )


if __name__ == "__main__":
    main()

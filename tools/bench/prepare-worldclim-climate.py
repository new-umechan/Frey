#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Aggregate WorldClim monthly tavg/prec into annual climatology GeoTIFFs."
    )
    parser.add_argument(
        "--input-dir",
        default="data/raw/climate",
        help="Directory containing wc2.1_30s_tavg_01..12.tif and wc2.1_30s_prec_01..12.tif",
    )
    parser.add_argument(
        "--out-temperature",
        default="data/raw/climate/worldclim_tavg_annual_c.tif",
        help="Output annual mean temperature (degC).",
    )
    parser.add_argument(
        "--out-precipitation",
        default="data/raw/climate/worldclim_prec_annual_mm.tif",
        help="Output annual total precipitation (mm/year).",
    )
    parser.add_argument(
        "--tavg-scale",
        type=float,
        default=1.0,
        help="Scale divisor for tavg values (default: 1.0 for WorldClim v2.1).",
    )
    return parser.parse_args()


def monthly_paths(base: Path, prefix: str) -> list[Path]:
    return [base / f"wc2.1_30s_{prefix}_{month:02d}.tif" for month in range(1, 13)]


def validate_inputs(paths: list[Path]) -> None:
    for path in paths:
        if not path.exists():
            raise FileNotFoundError(f"missing input file: {path}")


def prepare_output_profile(meta: dict) -> dict:
    profile = meta.copy()
    profile.update(
        {
            "count": 1,
            "dtype": "float32",
            "nodata": np.float32(np.nan),
            "compress": "deflate",
        }
    )
    return profile


def aggregate_blockwise(
    tavg_paths: list[Path],
    prec_paths: list[Path],
    out_t: Path,
    out_p: Path,
    tavg_scale: float,
) -> None:
    try:
        import rasterio
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("rasterio is required. Install with: pip install rasterio") from exc

    out_t.parent.mkdir(parents=True, exist_ok=True)
    out_p.parent.mkdir(parents=True, exist_ok=True)

    with rasterio.open(tavg_paths[0]) as t0, rasterio.open(prec_paths[0]) as p0:
        if t0.width != p0.width or t0.height != p0.height:
            raise ValueError("tavg/prec raster size mismatch")
        profile_t = prepare_output_profile(t0.meta)
        profile_p = prepare_output_profile(p0.meta)

        # streaming stats
        temp_valid = 0
        temp_sum = 0.0
        temp_min = np.inf
        temp_max = -np.inf
        prec_valid = 0
        prec_sum = 0.0
        prec_min = np.inf
        prec_max = -np.inf

        with rasterio.open(out_t, "w", **profile_t) as dst_t, rasterio.open(
            out_p, "w", **profile_p
        ) as dst_p:
            t_sources = [rasterio.open(path) for path in tavg_paths]
            p_sources = [rasterio.open(path) for path in prec_paths]
            try:
                for _, window in t0.block_windows(1):
                    t_block_sum = None
                    t_block_count = None
                    p_block_sum = None
                    p_block_count = None

                    for src in t_sources:
                        block = src.read(1, window=window).astype(np.float64)
                        nodata = src.nodata
                        if nodata is not None:
                            block[np.isclose(block, nodata)] = np.nan
                        block = block / tavg_scale
                        finite = np.isfinite(block)
                        if t_block_sum is None:
                            t_block_sum = np.zeros(block.shape, dtype=np.float64)
                            t_block_count = np.zeros(block.shape, dtype=np.uint8)
                        t_block_sum[finite] += block[finite]
                        t_block_count[finite] += 1

                    for src in p_sources:
                        block = src.read(1, window=window).astype(np.float64)
                        nodata = src.nodata
                        if nodata is not None:
                            block[np.isclose(block, nodata)] = np.nan
                        finite = np.isfinite(block)
                        if p_block_sum is None:
                            p_block_sum = np.zeros(block.shape, dtype=np.float64)
                            p_block_count = np.zeros(block.shape, dtype=np.uint8)
                        p_block_sum[finite] += block[finite]
                        p_block_count[finite] += 1

                    t_out = np.full(t_block_sum.shape, np.nan, dtype=np.float32)  # type: ignore[union-attr]
                    valid_t = t_block_count > 0  # type: ignore[operator]
                    t_out[valid_t] = (t_block_sum[valid_t] / t_block_count[valid_t]).astype(np.float32)  # type: ignore[index]

                    p_out = np.full(p_block_sum.shape, np.nan, dtype=np.float32)  # type: ignore[union-attr]
                    valid_p = p_block_count > 0  # type: ignore[operator]
                    p_out[valid_p] = p_block_sum[valid_p].astype(np.float32)  # type: ignore[index]

                    dst_t.write(t_out, 1, window=window)
                    dst_p.write(p_out, 1, window=window)

                    if np.any(valid_t):
                        block_vals = t_out[valid_t].astype(np.float64)
                        temp_valid += block_vals.size
                        temp_sum += float(np.sum(block_vals))
                        temp_min = min(temp_min, float(np.min(block_vals)))
                        temp_max = max(temp_max, float(np.max(block_vals)))
                    if np.any(valid_p):
                        block_vals = p_out[valid_p].astype(np.float64)
                        prec_valid += block_vals.size
                        prec_sum += float(np.sum(block_vals))
                        prec_min = min(prec_min, float(np.min(block_vals)))
                        prec_max = max(prec_max, float(np.max(block_vals)))
            finally:
                for src in t_sources + p_sources:
                    src.close()

    if temp_valid == 0 or prec_valid == 0:
        raise RuntimeError("aggregation produced no valid pixels")

    print(f"temperature: valid={temp_valid} min={temp_min:.3f} max={temp_max:.3f} mean={temp_sum / temp_valid:.3f}")
    print(f"precipitation: valid={prec_valid} min={prec_min:.3f} max={prec_max:.3f} mean={prec_sum / prec_valid:.3f}")


def main() -> None:
    args = parse_args()
    base = Path(args.input_dir)
    tavg_paths = monthly_paths(base, "tavg")
    prec_paths = monthly_paths(base, "prec")
    validate_inputs(tavg_paths)
    validate_inputs(prec_paths)

    out_t = Path(args.out_temperature)
    out_p = Path(args.out_precipitation)
    aggregate_blockwise(
        tavg_paths=tavg_paths,
        prec_paths=prec_paths,
        out_t=out_t,
        out_p=out_p,
        tavg_scale=float(args.tavg_scale),
    )

    print(f"OUTPUT_TEMPERATURE {out_t}")
    print(f"OUTPUT_PRECIPITATION {out_p}")


if __name__ == "__main__":
    main()

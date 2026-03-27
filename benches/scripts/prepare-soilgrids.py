#!/usr/bin/env python3
from __future__ import annotations

import argparse
import shutil
import subprocess
from pathlib import Path


PROPERTIES = ("bdod", "cec", "phh2o", "soc")
DEPTHS = ("0-5", "5-15", "15-30")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Fetch SoilGrids layers through remote VRT (/vsicurl/) and save local "
            "GeoTIFFs at fixed target resolution."
        )
    )
    parser.add_argument(
        "--out-dir",
        default="benches/raw/ecology/soilgrids",
        help="Output directory for projected GeoTIFF files.",
    )
    parser.add_argument(
        "--resolution-deg",
        type=float,
        default=0.1,
        help="Target output resolution in degrees for EPSG:4326 (default: 0.1).",
    )
    parser.add_argument(
        "--resampling",
        choices=("nearest", "bilinear", "cubic"),
        default="bilinear",
        help="Resampling method for gdalwarp.",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Overwrite output files if they already exist.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print commands without executing gdalwarp.",
    )
    parser.add_argument(
        "--gdalwarp",
        default=None,
        help="Optional explicit path to gdalwarp binary.",
    )
    return parser.parse_args()


def format_resolution_tag(value: float) -> str:
    text = f"{value:.4f}".rstrip("0").rstrip(".")
    return text.replace(".", "p")


def resolve_gdalwarp(explicit_path: str | None) -> str:
    if explicit_path:
        path = Path(explicit_path)
        if path.exists():
            return str(path)
        raise RuntimeError(f"gdalwarp not found at: {explicit_path}")

    found = shutil.which("gdalwarp")
    if found:
        return found

    fallback = Path("/opt/homebrew/Caskroom/miniforge/base/bin/gdalwarp")
    if fallback.exists():
        return str(fallback)

    raise RuntimeError("required command not found: gdalwarp")


def build_remote_vrt(prop: str, depth: str) -> str:
    base = "https://files.isric.org/soilgrids/latest/data"
    vrt_name = f"{prop}_{depth}cm_mean.vrt"
    return f"/vsicurl/{base}/{prop}/{vrt_name}"


def build_output_path(out_dir: Path, prop: str, depth: str, resolution_tag: str) -> Path:
    depth_tag = depth.replace("-", "_")
    return out_dir / f"{prop}_{depth_tag}cm_mean_{resolution_tag}deg.tif"


def run_gdalwarp(
    gdalwarp_bin: str,
    source_vrt: str,
    out_path: Path,
    resolution: float,
    resampling: str,
    overwrite: bool,
    dry_run: bool,
) -> None:
    command = [
        gdalwarp_bin,
        "-overwrite" if overwrite else "",
        "-t_srs",
        "EPSG:4326",
        "-tr",
        str(resolution),
        str(resolution),
        "-r",
        resampling,
        "-co",
        "COMPRESS=LZW",
        "-co",
        "TILED=YES",
        "-co",
        "BIGTIFF=IF_SAFER",
        source_vrt,
        str(out_path),
    ]
    command = [token for token in command if token]
    print("CMD", " ".join(command))
    if dry_run:
        return
    subprocess.run(command, check=True)


def main() -> None:
    args = parse_args()
    gdalwarp_bin = resolve_gdalwarp(args.gdalwarp)

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    resolution = float(args.resolution_deg)
    resolution_tag = format_resolution_tag(resolution)

    print(f"OUT_DIR {out_dir}")
    print(f"RESOLUTION_DEG {resolution}")
    print(f"RESAMPLING {args.resampling}")
    print(f"LAYERS {len(PROPERTIES) * len(DEPTHS)}")

    for prop in PROPERTIES:
        for depth in DEPTHS:
            source_vrt = build_remote_vrt(prop, depth)
            out_path = build_output_path(out_dir, prop, depth, resolution_tag)
            if out_path.exists() and not args.overwrite:
                print(f"SKIP existing output: {out_path}")
                continue
            run_gdalwarp(
                gdalwarp_bin=gdalwarp_bin,
                source_vrt=source_vrt,
                out_path=out_path,
                resolution=resolution,
                resampling=args.resampling,
                overwrite=bool(args.overwrite),
                dry_run=bool(args.dry_run),
            )
            if not args.dry_run:
                print(f"WROTE {out_path}")


if __name__ == "__main__":
    main()

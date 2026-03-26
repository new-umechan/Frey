#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path
from typing import Iterable

import numpy as np


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Extract required SDS layers from MOD44B/MCD12Q1 HDF tiles and build "
            "canonical global GeoTIFFs for ecology benchmark."
        )
    )
    parser.add_argument(
        "--mod44b-dir",
        default="data/raw/ecology/MOD44B",
        help="Directory containing MOD44B HDF tiles.",
    )
    parser.add_argument(
        "--mcd12q1-dir",
        default="data/raw/ecology/MCD12Q1",
        help="Directory containing MCD12Q1 HDF tiles.",
    )
    parser.add_argument(
        "--year",
        type=int,
        default=2019,
        help="Acquisition year to use (default: 2019).",
    )
    parser.add_argument(
        "--out-tree-cover",
        default="data/raw/ecology/mod44b_tree_cover.tif",
        help="Canonical output path for MOD44B Percent_Tree_Cover.",
    )
    parser.add_argument(
        "--out-non-tree-cover",
        default="data/raw/ecology/mod44b_non_tree_cover.tif",
        help="Canonical output path for MOD44B Percent_NonTree_Vegetation.",
    )
    parser.add_argument(
        "--out-non-vegetated",
        default="data/raw/ecology/mod44b_non_vegetated.tif",
        help="Canonical output path for MOD44B Percent_NonVegetated.",
    )
    parser.add_argument(
        "--out-lc-type1",
        default="data/raw/ecology/mcd12q1_lc_type1.tif",
        help="Canonical output path for MCD12Q1 LC_Type1.",
    )
    parser.add_argument(
        "--out-lc-prop2",
        default="data/raw/ecology/mcd12q1_lc_prop2.tif",
        help="Canonical output path for MCD12Q1 LC_Prop2.",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Overwrite output files if they already exist.",
    )
    return parser.parse_args()


def collect_hdf_tiles(base_dir: Path, product: str, year: int) -> list[Path]:
    if not base_dir.exists():
        raise FileNotFoundError(f"missing directory: {base_dir}")
    pattern = f"{product}.A{year}*.hdf"
    files = sorted(base_dir.glob(pattern))
    if not files:
        raise FileNotFoundError(f"no files matched {pattern} under {base_dir}")
    return files


def pick_subdataset_uri(
    hdf_path: Path,
    required_name: str,
    fallback_names: Iterable[str] = (),
) -> str:
    try:
        import rasterio
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("rasterio is required. Install with: pip install rasterio") from exc

    try:
        with rasterio.open(hdf_path) as src:
            subdatasets = list(src.subdatasets)
    except Exception as exc:
        raise RuntimeError(
            "failed to open HDF tile. This environment likely lacks GDAL HDF4 support. "
            "Install GDAL/rasterio with HDF4 enabled, or pre-convert HDF tiles to GeoTIFF."
            f" file={hdf_path}"
        ) from exc
    if not subdatasets:
        raise ValueError(f"no subdatasets found in {hdf_path}")

    candidate_names = [required_name, *fallback_names]
    for name in candidate_names:
        for uri in subdatasets:
            if uri.endswith(name) or f":{name}" in uri:
                return uri
    raise ValueError(
        f"required SDS not found in {hdf_path}: {required_name} (fallbacks={list(fallback_names)})"
    )


def build_canonical_mosaic(
    hdf_paths: list[Path],
    required_name: str,
    out_path: Path,
    overwrite: bool,
    fallback_names: Iterable[str] = (),
) -> None:
    try:
        import rasterio
        from rasterio.merge import merge
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("rasterio is required. Install with: pip install rasterio") from exc

    if out_path.exists() and not overwrite:
        print(f"SKIP existing output: {out_path}")
        return
    out_path.parent.mkdir(parents=True, exist_ok=True)

    subdataset_uris = [
        pick_subdataset_uri(path, required_name, fallback_names=fallback_names)
        for path in hdf_paths
    ]

    srcs = [rasterio.open(uri) for uri in subdataset_uris]
    try:
        mosaic, transform = merge(srcs, method="first")
        reference = srcs[0]
        out_meta = reference.meta.copy()
        out_meta.update(
            {
                "driver": "GTiff",
                "height": mosaic.shape[1],
                "width": mosaic.shape[2],
                "transform": transform,
                "count": 1,
                "compress": "deflate",
            }
        )

        dtype = np.dtype(out_meta["dtype"])
        band = mosaic[0]
        if np.issubdtype(dtype, np.integer):
            info = np.iinfo(dtype)
            band = np.clip(band, info.min, info.max).astype(dtype, copy=False)
        else:
            band = band.astype(dtype, copy=False)

        with rasterio.open(out_path, "w", **out_meta) as dst:
            dst.write(band, 1)
    finally:
        for src in srcs:
            src.close()

    print(f"WROTE {out_path}")
    print(f"SDS {required_name}")
    print(f"TILES {len(hdf_paths)}")


def main() -> None:
    args = parse_args()

    mod44b_dir = Path(args.mod44b_dir)
    mcd12q1_dir = Path(args.mcd12q1_dir)
    year = int(args.year)

    mod44b_tiles = collect_hdf_tiles(mod44b_dir, "MOD44B", year)
    mcd12q1_tiles = collect_hdf_tiles(mcd12q1_dir, "MCD12Q1", year)

    print(f"YEAR {year}")
    print(f"MOD44B_TILES {len(mod44b_tiles)}")
    print(f"MCD12Q1_TILES {len(mcd12q1_tiles)}")

    build_canonical_mosaic(
        mod44b_tiles,
        required_name="Percent_Tree_Cover",
        out_path=Path(args.out_tree_cover),
        overwrite=bool(args.overwrite),
    )
    build_canonical_mosaic(
        mod44b_tiles,
        required_name="Percent_NonTree_Vegetation",
        out_path=Path(args.out_non_tree_cover),
        overwrite=bool(args.overwrite),
        fallback_names=("Percent_NonTree_Vegetation", "Percent_NonTree_Vegetation_Cover"),
    )
    build_canonical_mosaic(
        mod44b_tiles,
        required_name="Percent_NonVegetated",
        out_path=Path(args.out_non_vegetated),
        overwrite=bool(args.overwrite),
    )
    build_canonical_mosaic(
        mcd12q1_tiles,
        required_name="LC_Type1",
        out_path=Path(args.out_lc_type1),
        overwrite=bool(args.overwrite),
    )
    build_canonical_mosaic(
        mcd12q1_tiles,
        required_name="LC_Prop2",
        out_path=Path(args.out_lc_prop2),
        overwrite=bool(args.overwrite),
    )


if __name__ == "__main__":
    main()

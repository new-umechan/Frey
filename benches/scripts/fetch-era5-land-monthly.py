#!/usr/bin/env python3
from __future__ import annotations

import argparse
import tempfile
from pathlib import Path
from typing import List


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Download ERA5-Land monthly means (runoff + total_evaporation) "
            "with year-wise resume support."
        )
    )
    parser.add_argument(
        "--start-year",
        type=int,
        default=1970,
        help="First year (default: 1970).",
    )
    parser.add_argument(
        "--end-year",
        type=int,
        default=2000,
        help="Last year (default: 2000).",
    )
    parser.add_argument(
        "--out",
        default="benches/raw/climate/era5_land_monthly_1970_2000.nc",
        help="Merged monthly NetCDF output path.",
    )
    parser.add_argument(
        "--yearly-dir",
        default="benches/raw/climate/era5_land_monthly_yearly",
        help="Directory to cache per-year NetCDF files.",
    )
    parser.add_argument(
        "--no-merge",
        action="store_true",
        help="Only fetch yearly files and skip merged output generation.",
    )
    parser.add_argument(
        "--force-year",
        action="store_true",
        help="Re-download yearly files even if cache exists.",
    )
    parser.add_argument(
        "--dataset",
        default="reanalysis-era5-land-monthly-means",
        help="CDS dataset name.",
    )
    return parser.parse_args()


def _has_time_coord(dataset) -> bool:
    return "time" in dataset.coords or "valid_time" in dataset.coords


def _year_file_valid(path: Path) -> bool:
    if not path.exists() or path.stat().st_size == 0:
        return False
    try:
        import xarray as xr
    except Exception:
        return True
    try:
        with xr.open_dataset(path) as ds:
            return _has_time_coord(ds)
    except Exception:
        return False


def _download_year(client, dataset: str, year: int, out_file: Path) -> None:
    request = {
        "product_type": ["monthly_averaged_reanalysis"],
        "variable": [
            "runoff",
            "total_evaporation",
        ],
        "year": [str(year)],
        "month": [f"{month:02d}" for month in range(1, 13)],
        "time": ["00:00"],
        "data_format": "netcdf",
        "download_format": "unarchived",
    }
    out_file.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        suffix=".nc", prefix=f"era5_{year}_", dir=out_file.parent, delete=False
    ) as tf:
        tmp_path = Path(tf.name)
    try:
        client.retrieve(dataset, request).download(str(tmp_path))
        if not _year_file_valid(tmp_path):
            raise RuntimeError(f"downloaded file is invalid: {tmp_path}")
        tmp_path.replace(out_file)
    except Exception:
        if tmp_path.exists():
            tmp_path.unlink()
        raise


def _merge_year_files(year_files: List[Path], out_path: Path) -> None:
    import xarray as xr

    datasets = []
    merged = None
    tmp_path = out_path.with_suffix(f"{out_path.suffix}.tmp")
    try:
        for path in year_files:
            datasets.append(xr.open_dataset(path))
        if not datasets:
            raise ValueError("no yearly files to merge")
        time_name = "time" if "time" in datasets[0].coords else "valid_time"
        merged = xr.concat(datasets, dim=time_name)
        if time_name in merged.coords:
            merged = merged.sortby(time_name)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        merged.to_netcdf(tmp_path)
        tmp_path.replace(out_path)
    finally:
        if merged is not None:
            merged.close()
        for ds in datasets:
            ds.close()
        if tmp_path.exists():
            tmp_path.unlink()


def main() -> None:
    args = parse_args()
    if args.start_year > args.end_year:
        raise ValueError("--start-year must be <= --end-year")

    try:
        import cdsapi
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("cdsapi is required. Install with: pip install cdsapi") from exc

    out_path = Path(args.out)
    yearly_dir = Path(args.yearly_dir)
    yearly_dir.mkdir(parents=True, exist_ok=True)

    years = list(range(args.start_year, args.end_year + 1))
    year_files = [yearly_dir / f"era5_land_monthly_{year}.nc" for year in years]

    print(f"REQUEST_DATASET {args.dataset}")
    print(f"REQUEST_YEARS {years[0]}-{years[-1]} ({len(years)} years)")
    print(f"YEARLY_CACHE_DIR {yearly_dir}")
    if not args.no_merge:
        print(f"MERGED_OUTPUT {out_path}")

    client = cdsapi.Client()
    downloaded = 0
    skipped = 0
    for year, year_file in zip(years, year_files):
        if not args.force_year and _year_file_valid(year_file):
            skipped += 1
            print(f"SKIP_YEAR {year} {year_file}")
            continue
        print(f"FETCH_YEAR {year} {year_file}")
        _download_year(client, args.dataset, year, year_file)
        downloaded += 1

    if not args.no_merge:
        _merge_year_files(year_files, out_path)
        print(f"MERGED_WRITTEN {out_path}")

    print(f"DOWNLOADED_YEARS {downloaded}")
    print(f"SKIPPED_YEARS {skipped}")
    print("DONE")


if __name__ == "__main__":
    main()

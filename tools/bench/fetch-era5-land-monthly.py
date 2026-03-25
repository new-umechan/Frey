#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Download ERA5-Land monthly means (runoff + total_evaporation) via cdsapi."
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
        default="data/raw/climate/era5_land_monthly_1970_2000.zip",
        help="Output ZIP path.",
    )
    parser.add_argument(
        "--dataset",
        default="reanalysis-era5-land-monthly-means",
        help="CDS dataset name.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.start_year > args.end_year:
        raise ValueError("--start-year must be <= --end-year")

    try:
        import cdsapi
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("cdsapi is required. Install with: pip install cdsapi") from exc

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    years = [str(year) for year in range(args.start_year, args.end_year + 1)]
    request = {
        "product_type": ["monthly_averaged_reanalysis"],
        "variable": [
            "runoff",
            "total_evaporation",
        ],
        "year": years,
        "month": [f"{month:02d}" for month in range(1, 13)],
        "time": ["00:00"],
        "data_format": "netcdf",
        "download_format": "zip",
    }

    print(f"REQUEST_DATASET {args.dataset}")
    print(f"REQUEST_YEARS {years[0]}-{years[-1]} ({len(years)} years)")
    print(f"DOWNLOAD_TO {out_path}")

    client = cdsapi.Client()
    client.retrieve(args.dataset, request).download(str(out_path))

    print("DONE")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import signal
import shutil
import zipfile
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Download GloFAS historical river discharge in monthly chunks via cdsapi."
    )
    parser.add_argument(
        "--start-year",
        type=int,
        default=1979,
        help="First year to download (default: 1979).",
    )
    parser.add_argument(
        "--end-year",
        type=int,
        default=2000,
        help="Last year to download (default: 2000).",
    )
    parser.add_argument(
        "--out-dir",
        default="data/raw/hydrology/glofas_raw",
        help="Directory to store extracted monthly NetCDF files.",
    )
    parser.add_argument(
        "--dataset",
        default="cems-glofas-historical",
        help="EWDS dataset name.",
    )
    parser.add_argument(
        "--system-version",
        default="version_4_0",
        help="System version to request (default: version_4_0).",
    )
    parser.add_argument(
        "--hydrological-model",
        default="lisflood",
        help="Hydrological model to request (default: lisflood).",
    )
    parser.add_argument(
        "--product-type",
        default="consolidated",
        help="Product type to request (default: consolidated).",
    )
    parser.add_argument(
        "--variable",
        default="river_discharge_in_the_last_24_hours",
        help="Variable to request (default: river_discharge_in_the_last_24_hours).",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Redownload files even if the extracted monthly NetCDF already exists.",
    )
    parser.add_argument(
        "--days",
        default="01,08,15,22,29",
        help="Comma-separated day list to request (default: 01,08,15,22,29).",
    )
    parser.add_argument(
        "--chunk-timeout-sec",
        type=int,
        default=900,
        help="Per-chunk timeout in seconds (default: 900).",
    )
    parser.add_argument(
        "--keep-zip",
        action="store_true",
        help="Keep downloaded ZIP files in the _tmp directory.",
    )
    return parser.parse_args()


class ChunkTimeoutError(TimeoutError):
    pass


def timestamp() -> str:
    return dt.datetime.now().strftime("%Y-%m-%d %H:%M:%S")


def log(message: str) -> None:
    print(f"[{timestamp()}] {message}", flush=True)


@contextlib.contextmanager
def chunk_timeout(timeout_sec: int):
    if timeout_sec <= 0:
        yield
        return

    def handler(_signum, _frame):
        raise ChunkTimeoutError(f"chunk timed out after {timeout_sec} seconds")

    previous = signal.signal(signal.SIGALRM, handler)
    signal.alarm(timeout_sec)
    try:
        yield
    finally:
        signal.alarm(0)
        signal.signal(signal.SIGALRM, previous)


def extract_single_netcdf(zip_path: Path, out_path: Path) -> None:
    with zipfile.ZipFile(zip_path) as zf:
        members = [member for member in zf.namelist() if member.lower().endswith(".nc")]
        if len(members) != 1:
            raise ValueError(
                f"expected exactly one NetCDF file in {zip_path}, found {len(members)}"
            )
        with zf.open(members[0]) as src, out_path.open("wb") as dst:
            shutil.copyfileobj(src, dst)


def main() -> None:
    args = parse_args()
    if args.start_year > args.end_year:
        raise ValueError("--start-year must be <= --end-year")

    try:
        import cdsapi
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("cdsapi is required. Install with: pip install cdsapi") from exc

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    tmp_dir = out_dir / "_tmp"
    tmp_dir.mkdir(parents=True, exist_ok=True)
    day_values = [day.strip() for day in args.days.split(",") if day.strip()]
    if not day_values:
        raise ValueError("--days must contain at least one day")

    client = cdsapi.Client()
    total = 0
    skipped = 0

    for year in range(args.start_year, args.end_year + 1):
        for month in range(1, 13):
            nc_path = out_dir / f"glofas_{year}_{month:02d}.nc"
            zip_path = tmp_dir / f"glofas_{year}_{month:02d}.zip"
            if nc_path.exists() and not args.overwrite:
                print(f"SKIP {nc_path}")
                skipped += 1
                continue

            request = {
                "system_version": [args.system_version],
                "hydrological_model": [args.hydrological_model],
                "product_type": [args.product_type],
                "variable": [args.variable],
                "hyear": [str(year)],
                "hmonth": [f"{month:02d}"],
                "hday": day_values,
                "data_format": "netcdf",
                "download_format": "zip",
            }

            log(f"REQUEST_DATASET {args.dataset}")
            log(f"REQUEST_CHUNK {year}-{month:02d}")
            log(f"REQUEST_TARGET {zip_path}")
            try:
                with chunk_timeout(args.chunk_timeout_sec):
                    client.retrieve(args.dataset, request, str(zip_path))
            except ChunkTimeoutError as exc:
                log(f"TIMEOUT {year}-{month:02d} {exc}")
                raise
            except Exception as exc:
                log(f"ERROR {year}-{month:02d} {type(exc).__name__}: {exc}")
                raise
            extract_single_netcdf(zip_path, nc_path)
            log(f"WROTE {nc_path}")
            if zip_path.exists() and not args.keep_zip:
                zip_path.unlink()
            total += 1

    log(f"DONE downloaded={total} skipped={skipped} out_dir={out_dir}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
from __future__ import annotations

import argparse
import tempfile
import zipfile
from pathlib import Path

import numpy as np


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Prepare ERA5-Land monthly means to annual climatology (mm/year)."
    )
    parser.add_argument(
        "--input",
        default="benches/raw/climate/era5_land_monthly_1970_2000.zip",
        help="Input ZIP or NetCDF from CDS.",
    )
    parser.add_argument(
        "--start-year",
        type=int,
        default=1970,
        help="First year to include (default: 1970).",
    )
    parser.add_argument(
        "--end-year",
        type=int,
        default=2000,
        help="Last year to include (default: 2000).",
    )
    parser.add_argument(
        "--out",
        default="benches/raw/climate/era5_land_annual_1970_2000.nc",
        help="Output NetCDF path (contains runoff_mm_yr and evapotranspiration_mm_yr).",
    )
    parser.add_argument(
        "--evap-sign",
        choices=["auto_abs", "as_is", "negate"],
        default="auto_abs",
        help="How to handle evaporation sign (default: auto_abs).",
    )
    return parser.parse_args()


def open_dataset(path: Path):
    try:
        import xarray as xr
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("xarray is required. Install with: pip install xarray netCDF4") from exc

    suffix = path.suffix.lower()
    if suffix in {".nc", ".nc4", ".netcdf"}:
        return xr.open_dataset(path), None
    if suffix == ".zip":
        with zipfile.ZipFile(path, "r") as zf:
            nc_members = [
                name for name in zf.namelist() if name.lower().endswith((".nc", ".nc4", ".netcdf"))
            ]
            if not nc_members:
                raise ValueError(f"no NetCDF found in ZIP: {path}")
            # extract first NetCDF to temp file and open
            member = nc_members[0]
            tmp_dir = tempfile.TemporaryDirectory()
            extracted = Path(zf.extract(member, path=tmp_dir.name))
            ds = xr.open_dataset(extracted)
            return ds, tmp_dir
    raise ValueError(f"unsupported input extension: {path}")


def find_time_name(ds) -> str:
    for candidate in ("time", "valid_time"):
        if candidate in ds.coords:
            return candidate
    raise ValueError("time coordinate not found")


def find_var_name(ds, candidates):
    for name in candidates:
        if name in ds.data_vars:
            return name
    for name in ds.data_vars:
        lowered = name.lower()
        for c in candidates:
            if c in lowered:
                return name
    raise ValueError(f"variable not found. candidates={candidates}, available={list(ds.data_vars)}")


def compute_annual_mm_per_year(data, time_coord):
    # data: monthly means of daily values (m/day) for accumulation-type variables.
    # Convert to monthly totals: m/day * days_in_month, then to mm and annual sum.
    monthly_mm = data * time_coord.dt.days_in_month * 1000.0
    annual_mm = monthly_mm.groupby(time_coord.dt.year).sum(time_coord.name, skipna=True)
    climatology_mm = annual_mm.mean("year", skipna=True)
    return climatology_mm


def main() -> None:
    args = parse_args()
    if args.start_year > args.end_year:
        raise ValueError("--start-year must be <= --end-year")

    input_path = Path(args.input)
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    ds, tmp_dir = open_dataset(input_path)
    try:
        time_name = find_time_name(ds)
        time_coord = ds[time_name]

        runoff_name = find_var_name(ds, ["runoff", "ro"])
        evap_name = find_var_name(ds, ["total_evaporation", "evaporation", "e"])

        ds_subset = ds.sel({time_name: slice(f"{args.start_year}-01-01", f"{args.end_year}-12-31")})
        if ds_subset[time_name].size == 0:
            raise ValueError("selected year range produced empty dataset")

        runoff_data = ds_subset[runoff_name]
        evap_data = ds_subset[evap_name]

        runoff_mm_yr = compute_annual_mm_per_year(runoff_data, ds_subset[time_name]).astype(np.float32)
        evap_mm_yr = compute_annual_mm_per_year(evap_data, ds_subset[time_name]).astype(np.float32)

        if args.evap_sign == "negate":
            evap_mm_yr = -evap_mm_yr
        elif args.evap_sign == "auto_abs":
            evap_mm_yr = np.abs(evap_mm_yr)

        runoff_mm_yr = np.abs(runoff_mm_yr)

        out_ds = runoff_mm_yr.to_dataset(name="runoff_mm_yr")
        out_ds["evapotranspiration_mm_yr"] = evap_mm_yr
        out_ds["runoff_mm_yr"].attrs.update(
            {
                "long_name": "annual runoff climatology",
                "units": "mm/year",
                "source_variable": runoff_name,
                "source_dataset": "ERA5-Land monthly averaged reanalysis",
                "years": f"{args.start_year}-{args.end_year}",
            }
        )
        out_ds["evapotranspiration_mm_yr"].attrs.update(
            {
                "long_name": "annual evapotranspiration climatology",
                "units": "mm/year",
                "source_variable": evap_name,
                "source_dataset": "ERA5-Land monthly averaged reanalysis",
                "years": f"{args.start_year}-{args.end_year}",
                "evap_sign_mode": args.evap_sign,
            }
        )
        out_ds.to_netcdf(out_path)

        print(f"INPUT {input_path}")
        print(f"RUNOFF_VAR {runoff_name}")
        print(f"EVAP_VAR {evap_name}")
        print(f"OUTPUT {out_path}")
        print(f"YEARS {args.start_year}-{args.end_year}")
    finally:
        ds.close()
        if tmp_dir is not None:
            tmp_dir.cleanup()


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Aggregate GloFAS river discharge data into a single annual-mean NetCDF."
    )
    parser.add_argument(
        "--input",
        default=None,
        help="Input GloFAS NetCDF path.",
    )
    parser.add_argument(
        "--input-dir",
        default=None,
        help="Directory containing monthly GloFAS NetCDF chunks.",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Output annual-mean NetCDF path.",
    )
    parser.add_argument(
        "--var-name",
        default=None,
        help="Optional input variable name. If omitted, auto-detect a lat/lon variable.",
    )
    parser.add_argument(
        "--output-var-name",
        default="river_flow_m3s",
        help="Variable name to store in the output NetCDF.",
    )
    return parser.parse_args()


def detect_lat_lon_names(ds) -> tuple[str, str]:
    lat_candidates = ["lat", "latitude", "y"]
    lon_candidates = ["lon", "longitude", "x"]
    lat_name = next((name for name in lat_candidates if name in ds.coords), None)
    lon_name = next((name for name in lon_candidates if name in ds.coords), None)
    if lat_name is None or lon_name is None:
        raise ValueError("failed to detect lat/lon coordinate names in NetCDF")
    return lat_name, lon_name


def detect_var_name(ds, lat_name: str, lon_name: str) -> str:
    preferred = [
        "dis24",
        "discharge",
        "mean_discharge_in_the_last_24_hours",
        "river_discharge_in_the_last_24_hours",
    ]
    for name in preferred:
        if name in ds.data_vars:
            dims = set(ds[name].dims)
            if lat_name in dims and lon_name in dims:
                return name
    for name, value in ds.data_vars.items():
        dims = set(value.dims)
        if lat_name in dims and lon_name in dims:
            return name
    raise ValueError("failed to detect suitable river flow variable in NetCDF")


def main() -> None:
    args = parse_args()
    if not args.input and not args.input_dir:
        raise ValueError("either --input or --input-dir is required")
    if args.input and args.input_dir:
        raise ValueError("use either --input or --input-dir, not both")

    try:
        import xarray as xr
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("xarray is required to process GloFAS NetCDF input") from exc

    output_path = Path(args.output)
    source_label = None

    if args.input:
        input_path = Path(args.input)
        ds = xr.open_dataset(input_path)
        try:
            lat_name, lon_name = detect_lat_lon_names(ds)
            var_name = args.var_name or detect_var_name(ds, lat_name, lon_name)
            data = ds[var_name]
            reduce_dims = [dim for dim in data.dims if dim not in (lat_name, lon_name)]
            if reduce_dims:
                data = data.mean(dim=reduce_dims, skipna=True)
            data = data.astype("float32").rename(args.output_var_name)
            out = data.to_dataset()
            out[lat_name] = ds[lat_name]
            out[lon_name] = ds[lon_name]
            source_label = str(input_path)
        finally:
            ds.close()
    else:
        input_dir = Path(args.input_dir)
        input_files = sorted(input_dir.glob("*.nc"))
        if not input_files:
            raise ValueError(f"no NetCDF files found in {input_dir}")
        datasets = [xr.open_dataset(path) for path in input_files]
        try:
            lat_name, lon_name = detect_lat_lon_names(datasets[0])
            var_name = args.var_name or detect_var_name(datasets[0], lat_name, lon_name)
            reduced = []
            for ds in datasets:
                data = ds[var_name]
                reduce_dims = [dim for dim in data.dims if dim not in (lat_name, lon_name)]
                if reduce_dims:
                    data = data.mean(dim=reduce_dims, skipna=True)
                reduced.append(data.astype("float32"))
            data = xr.concat(reduced, dim="chunk_index").mean(dim="chunk_index", skipna=True)
            data = data.rename(args.output_var_name)
            out = data.to_dataset()
            out[lat_name] = datasets[0][lat_name]
            out[lon_name] = datasets[0][lon_name]
            source_label = str(input_dir)
            out.attrs["sampling_note"] = (
                "Mean over available NetCDF chunks. If the source directory was fetched with "
                "subsampled hday values, this represents an approximate multi-year mean."
            )
        finally:
            for ds in datasets:
                ds.close()

    output_path.parent.mkdir(parents=True, exist_ok=True)
    out.to_netcdf(output_path)
    print(f"INPUT {source_label}")
    print(f"VAR {args.var_name or var_name}")
    print(f"OUTPUT {output_path}")
    print(f"OUTPUT_VAR {args.output_var_name}")


if __name__ == "__main__":
    main()

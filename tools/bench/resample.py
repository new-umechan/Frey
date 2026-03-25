#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, Tuple

import numpy as np

CLIMATE_MAGIC = b"CLIMREF1"
TERRAIN_MAGIC = b"TERRREF1"
VERSION = 1
CLIMATE_VARIABLES = [
    "temperature",
    "precipitation",
    "evapotranspiration",
    "runoff",
    "aridity",
]


@dataclass
class GridData:
    lat: np.ndarray
    lon: np.ndarray
    values: np.ndarray


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Resample reference rasters onto CellStore centroids."
    )
    parser.add_argument("--module", required=True, choices=["climate", "terrain"])
    parser.add_argument(
        "--centroids",
        default="bench/data/cell_centroids.csv",
        help="Path to centroid CSV (cell_id,latitude,longitude).",
    )
    parser.add_argument(
        "--output",
        default=None,
        help="Output binary cache path.",
    )
    parser.add_argument(
        "--method",
        choices=["bilinear", "nearest"],
        default="bilinear",
        help="Resampling method.",
    )
    parser.add_argument("--temperature", help="Input raster/NetCDF for temperature.")
    parser.add_argument("--precipitation", help="Input raster/NetCDF for precipitation.")
    parser.add_argument(
        "--evapotranspiration",
        help="Input raster/NetCDF for evapotranspiration.",
    )
    parser.add_argument("--runoff", help="Input raster/NetCDF for runoff.")
    parser.add_argument("--aridity", help="Input raster/NetCDF for aridity.")
    parser.add_argument(
        "--aridity-source",
        choices=["pet_over_precip", "precip_over_pet_x10000"],
        default="precip_over_pet_x10000",
        help=(
            "Definition of aridity input. "
            "pet_over_precip=already dry-high (no transform), "
            "precip_over_pet_x10000=AI*10000 wet-high (converted to PET/P dry-high)."
        ),
    )
    parser.add_argument(
        "--var-name",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help="Optional NetCDF variable mapping, e.g. temperature=tavg.",
    )
    parser.add_argument("--height", help="Input raster/NetCDF for terrain height.")
    parser.add_argument(
        "--height-var-name",
        default=None,
        help="Optional NetCDF variable name for terrain height.",
    )
    parser.add_argument(
        "--height-source",
        choices=["meters", "normalized"],
        default="meters",
        help="terrain height source unit (default: meters).",
    )
    parser.add_argument(
        "--sea-level-m",
        type=float,
        default=0.0,
        help="Sea level for meters input (default: 0.0).",
    )
    parser.add_argument(
        "--height-to-meters",
        type=float,
        default=6000.0,
        help="Model conversion scale; internal_height * height_to_meters = meters.",
    )
    parser.add_argument(
        "--height-ocean-fill",
        type=float,
        default=-0.01,
        help="Fallback internal height for NaN terrain samples (default: -0.01).",
    )
    parser.add_argument(
        "--height-clip-min",
        type=float,
        default=-1.5,
        help="Minimum internal terrain height after conversion.",
    )
    parser.add_argument(
        "--height-clip-max",
        type=float,
        default=1.5,
        help="Maximum internal terrain height after conversion.",
    )
    return parser.parse_args()


def load_centroids(path: Path) -> Tuple[np.ndarray, np.ndarray]:
    latitudes = []
    longitudes = []
    with path.open("r", encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle)
        required = {"latitude", "longitude"}
        if not required.issubset(set(reader.fieldnames or [])):
            raise ValueError(
                f"centroid csv must contain {sorted(required)} columns: {path}"
            )
        for row in reader:
            latitudes.append(float(row["latitude"]))
            longitudes.append(float(row["longitude"]))
    if not latitudes:
        raise ValueError(f"centroid csv is empty: {path}")
    return np.asarray(latitudes, dtype=np.float64), np.asarray(longitudes, dtype=np.float64)


def parse_var_map(raw_items: Iterable[str]) -> Dict[str, str]:
    mapping: Dict[str, str] = {}
    for raw in raw_items:
        if "=" not in raw:
            raise ValueError(f"--var-name must be KEY=VALUE format: {raw}")
        key, value = raw.split("=", 1)
        key = key.strip()
        value = value.strip()
        if key not in CLIMATE_VARIABLES:
            raise ValueError(f"unknown var key for --var-name: {key}")
        if not value:
            raise ValueError(f"empty variable name for key: {key}")
        mapping[key] = value
    return mapping


def load_input_grid(path: Path, preferred_var: str | None) -> GridData:
    suffix = path.suffix.lower()
    if suffix in {".tif", ".tiff"}:
        return load_geotiff(path)
    if suffix in {".nc", ".nc4", ".netcdf"}:
        return load_netcdf(path, preferred_var)
    raise ValueError(
        f"unsupported input extension for {path}. Use GeoTIFF(.tif/.tiff) or NetCDF(.nc/.nc4)."
    )


def load_geotiff(path: Path) -> GridData:
    try:
        import rasterio
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("rasterio is required to read GeoTIFF inputs") from exc

    with rasterio.open(path) as ds:
        if ds.count < 1:
            raise ValueError(f"GeoTIFF has no bands: {path}")
        crs_str = str(ds.crs or "")
        # Accept common geographic CRS encodings for global DEM/Climate rasters.
        if "4326" not in crs_str and "9518" not in crs_str:
            raise ValueError(
                f"GeoTIFF CRS must be geographic (e.g. EPSG:4326/9518): {path} (crs={crs_str})"
            )
        values = ds.read(1).astype(np.float64)
        nodata = ds.nodata
        if nodata is not None:
            values[np.isclose(values, nodata)] = np.nan
        transform = ds.transform
        cols = np.arange(ds.width, dtype=np.float64) + 0.5
        rows = np.arange(ds.height, dtype=np.float64) + 0.5
        lon = transform.c + cols * transform.a
        lat = transform.f + rows * transform.e
    return normalize_grid_axes(lat=lat, lon=lon, values=values)


def load_netcdf(path: Path, preferred_var: str | None) -> GridData:
    try:
        import xarray as xr
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("xarray is required to read NetCDF inputs") from exc

    ds = xr.open_dataset(path)
    try:
        lat_name, lon_name = detect_lat_lon_names(ds)
        var_name = preferred_var or detect_data_var_name(ds, lat_name, lon_name)
        data = ds[var_name]
        if lat_name not in data.dims or lon_name not in data.dims:
            raise ValueError(
                f"selected variable '{var_name}' does not include lat/lon dims in {path}"
            )
        reduce_dims = [dim for dim in data.dims if dim not in (lat_name, lon_name)]
        if reduce_dims:
            data = data.mean(dim=reduce_dims, skipna=True)
        lat = np.asarray(ds[lat_name].values, dtype=np.float64)
        lon = np.asarray(ds[lon_name].values, dtype=np.float64)
        values = np.asarray(data.values, dtype=np.float64)
    finally:
        ds.close()
    return normalize_grid_axes(lat=lat, lon=lon, values=values)


def detect_lat_lon_names(ds) -> Tuple[str, str]:
    lat_candidates = ["lat", "latitude", "y"]
    lon_candidates = ["lon", "longitude", "x"]
    lat_name = next((name for name in lat_candidates if name in ds.coords), None)
    lon_name = next((name for name in lon_candidates if name in ds.coords), None)
    if lat_name is None or lon_name is None:
        raise ValueError("failed to detect lat/lon coordinate names in NetCDF")
    return lat_name, lon_name


def detect_data_var_name(ds, lat_name: str, lon_name: str) -> str:
    for name, value in ds.data_vars.items():
        dims = set(value.dims)
        if lat_name in dims and lon_name in dims:
            return name
    raise ValueError("failed to detect suitable data variable in NetCDF")


def normalize_grid_axes(lat: np.ndarray, lon: np.ndarray, values: np.ndarray) -> GridData:
    lat = np.asarray(lat, dtype=np.float64).reshape(-1)
    lon = np.asarray(lon, dtype=np.float64).reshape(-1)
    values = np.asarray(values, dtype=np.float64)
    if values.ndim != 2:
        raise ValueError(f"grid must be 2D, got shape={values.shape}")
    if values.shape != (lat.size, lon.size):
        raise ValueError(
            f"grid shape mismatch: values={values.shape}, lat={lat.size}, lon={lon.size}"
        )

    if lat.size >= 2 and lat[0] > lat[-1]:
        lat = lat[::-1]
        values = values[::-1, :]
    if lon.size >= 2 and lon[0] > lon[-1]:
        lon = lon[::-1]
        values = values[:, ::-1]

    if np.nanmin(lon) >= 0.0 and np.nanmax(lon) > 180.0:
        lon = np.mod(lon, 360.0)
        order = np.argsort(lon)
        lon = lon[order]
        values = values[:, order]
    else:
        lon = np.asarray([normalize_lon(v) for v in lon], dtype=np.float64)
        order = np.argsort(lon)
        lon = lon[order]
        values = values[:, order]

    return GridData(lat=lat, lon=lon, values=values)


def normalize_lon(lon: float) -> float:
    value = lon
    while value <= -180.0:
        value += 360.0
    while value > 180.0:
        value -= 360.0
    return value


def interpolate_grid(
    grid: GridData, query_lat: np.ndarray, query_lon: np.ndarray, method: str
) -> np.ndarray:
    if method not in {"bilinear", "nearest"}:
        raise ValueError(f"unknown interpolation method: {method}")
    if query_lat.shape != query_lon.shape:
        raise ValueError("query latitude/longitude arrays must have the same shape")

    if np.nanmin(grid.lon) >= 0.0 and np.nanmax(grid.lon) > 180.0:
        query_lon_norm = np.mod(query_lon, 360.0)
    else:
        query_lon_norm = np.asarray([normalize_lon(v) for v in query_lon], dtype=np.float64)

    if method == "nearest":
        return interpolate_nearest(grid, query_lat, query_lon_norm)
    return interpolate_bilinear(grid, query_lat, query_lon_norm)


def interpolate_nearest(grid: GridData, query_lat: np.ndarray, query_lon: np.ndarray) -> np.ndarray:
    lat_idx = np.searchsorted(grid.lat, query_lat, side="left")
    lon_idx = np.searchsorted(grid.lon, query_lon, side="left")

    lat_idx = np.clip(lat_idx, 0, grid.lat.size - 1)
    lon_idx = np.clip(lon_idx, 0, grid.lon.size - 1)

    left_lat = np.maximum(lat_idx - 1, 0)
    left_lon = np.maximum(lon_idx - 1, 0)
    choose_left_lat = np.abs(query_lat - grid.lat[left_lat]) <= np.abs(
        query_lat - grid.lat[lat_idx]
    )
    choose_left_lon = np.abs(query_lon - grid.lon[left_lon]) <= np.abs(
        query_lon - grid.lon[lon_idx]
    )
    lat_idx = np.where(choose_left_lat, left_lat, lat_idx)
    lon_idx = np.where(choose_left_lon, left_lon, lon_idx)

    return grid.values[lat_idx, lon_idx]


def interpolate_bilinear(grid: GridData, query_lat: np.ndarray, query_lon: np.ndarray) -> np.ndarray:
    lat_hi = np.searchsorted(grid.lat, query_lat, side="right")
    lon_hi = np.searchsorted(grid.lon, query_lon, side="right")
    lat_lo = lat_hi - 1
    lon_lo = lon_hi - 1

    valid = (
        (lat_lo >= 0)
        & (lat_hi < grid.lat.size)
        & (lon_lo >= 0)
        & (lon_hi < grid.lon.size)
    )

    out = np.full(query_lat.shape, np.nan, dtype=np.float64)
    if not np.any(valid):
        return out

    q_lat = query_lat[valid]
    q_lon = query_lon[valid]
    lo_lat = lat_lo[valid]
    hi_lat = lat_hi[valid]
    lo_lon = lon_lo[valid]
    hi_lon = lon_hi[valid]

    lat0 = grid.lat[lo_lat]
    lat1 = grid.lat[hi_lat]
    lon0 = grid.lon[lo_lon]
    lon1 = grid.lon[hi_lon]

    with np.errstate(invalid="ignore", divide="ignore"):
        ty = (q_lat - lat0) / (lat1 - lat0)
        tx = (q_lon - lon0) / (lon1 - lon0)

    v00 = grid.values[lo_lat, lo_lon]
    v01 = grid.values[lo_lat, hi_lon]
    v10 = grid.values[hi_lat, lo_lon]
    v11 = grid.values[hi_lat, hi_lon]

    bilinear = (
        v00 * (1.0 - tx) * (1.0 - ty)
        + v01 * tx * (1.0 - ty)
        + v10 * (1.0 - tx) * ty
        + v11 * tx * ty
    )
    invalid_data = np.isnan(v00) | np.isnan(v01) | np.isnan(v10) | np.isnan(v11)
    if np.any(invalid_data):
        # For masked rasters (e.g., land-only climate data), coastal cells can have
        # NaNs in one or more bilinear corners. Fall back to nearest finite corner.
        fallback = np.full(bilinear.shape, np.nan, dtype=np.float64)
        d00 = (q_lat - lat0) ** 2 + (q_lon - lon0) ** 2
        d01 = (q_lat - lat0) ** 2 + (q_lon - lon1) ** 2
        d10 = (q_lat - lat1) ** 2 + (q_lon - lon0) ** 2
        d11 = (q_lat - lat1) ** 2 + (q_lon - lon1) ** 2
        distances = np.stack([d00, d01, d10, d11], axis=0)
        candidates = np.stack([v00, v01, v10, v11], axis=0)
        valid_corner = np.isfinite(candidates)
        distances[~valid_corner] = np.inf
        nearest_idx = np.argmin(distances, axis=0)
        has_valid = np.any(valid_corner, axis=0)
        cols = np.arange(nearest_idx.size)
        fallback[has_valid] = candidates[nearest_idx[has_valid], cols[has_valid]]
        bilinear[invalid_data] = fallback[invalid_data]
    out[valid] = bilinear
    return out


def write_climate_ref_bin(path: Path, vectors: Dict[str, np.ndarray]) -> None:
    length = len(vectors["temperature"])
    for key in CLIMATE_VARIABLES:
        if len(vectors[key]) != length:
            raise ValueError(f"length mismatch for {key}: expected {length}")

    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as handle:
        handle.write(CLIMATE_MAGIC)
        handle.write(struct.pack("<I", VERSION))
        handle.write(struct.pack("<Q", length))
        for key in CLIMATE_VARIABLES:
            data = np.asarray(vectors[key], dtype="<f4")
            handle.write(data.tobytes(order="C"))


def transform_aridity(values: np.ndarray, source: str) -> np.ndarray:
    if source == "pet_over_precip":
        return values
    if source == "precip_over_pet_x10000":
        out = np.full(values.shape, np.nan, dtype=np.float64)
        valid = np.isfinite(values) & (values > 0.0)
        out[valid] = 10000.0 / values[valid]
        return out
    raise ValueError(f"unknown aridity source: {source}")


def transform_terrain_height(values: np.ndarray, args: argparse.Namespace) -> np.ndarray:
    if args.height_source == "meters":
        if args.height_to_meters <= 0.0:
            raise ValueError("--height-to-meters must be > 0")
        transformed = (values - float(args.sea_level_m)) / float(args.height_to_meters)
    else:
        transformed = values.copy()

    non_finite = ~np.isfinite(transformed)
    if np.any(non_finite):
        transformed[non_finite] = float(args.height_ocean_fill)
    transformed = np.clip(
        transformed,
        float(args.height_clip_min),
        float(args.height_clip_max),
    )
    return transformed


def write_terrain_ref_bin(path: Path, heights: np.ndarray) -> None:
    values = np.asarray(heights, dtype="<f4")
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as handle:
        handle.write(TERRAIN_MAGIC)
        handle.write(struct.pack("<I", VERSION))
        handle.write(struct.pack("<Q", int(values.size)))
        handle.write(values.tobytes(order="C"))


def summarize(name: str, values: np.ndarray) -> str:
    valid = np.isfinite(values)
    valid_count = int(np.sum(valid))
    total = int(values.size)
    if valid_count == 0:
        return f"{name}: valid=0/{total}"
    selected = values[valid]
    return (
        f"{name}: valid={valid_count}/{total} "
        f"min={np.nanmin(selected):.3f} max={np.nanmax(selected):.3f} "
        f"mean={np.nanmean(selected):.3f}"
    )


def main() -> None:
    args = parse_args()

    centroids_path = Path(args.centroids)
    if args.output is None:
        if args.module == "climate":
            output_path = Path("bench/data/climate_ref.bin")
        else:
            output_path = Path("bench/data/terrain_ref.bin")
    else:
        output_path = Path(args.output)
    centroid_lat, centroid_lon = load_centroids(centroids_path)

    if args.module == "climate":
        var_map = parse_var_map(args.var_name)
        required_climate = {
            "temperature": args.temperature,
            "precipitation": args.precipitation,
            "evapotranspiration": args.evapotranspiration,
            "runoff": args.runoff,
            "aridity": args.aridity,
        }
        missing = [name for name, value in required_climate.items() if not value]
        if missing:
            raise ValueError(f"missing required args for climate module: {missing}")

        input_paths = {key: Path(value) for key, value in required_climate.items()}
        output_vectors: Dict[str, np.ndarray] = {}
        for key in CLIMATE_VARIABLES:
            source_path = input_paths[key]
            preferred_var = var_map.get(key)
            grid = load_input_grid(source_path, preferred_var)
            sampled = interpolate_grid(
                grid=grid,
                query_lat=centroid_lat,
                query_lon=centroid_lon,
                method=args.method,
            )
            if key == "aridity":
                sampled = transform_aridity(sampled, args.aridity_source)
            output_vectors[key] = sampled.astype(np.float32, copy=False)
            print(summarize(key, sampled))

        write_climate_ref_bin(output_path, output_vectors)
        print(f"WROTE {output_path}")
        print(f"CELL_COUNT {len(output_vectors['temperature'])}")
        return

    if not args.height:
        raise ValueError("missing required arg for terrain module: --height")
    grid = load_input_grid(Path(args.height), args.height_var_name)
    sampled = interpolate_grid(
        grid=grid,
        query_lat=centroid_lat,
        query_lon=centroid_lon,
        method=args.method,
    )
    terrain_height = transform_terrain_height(sampled, args).astype(np.float32, copy=False)
    print(summarize("terrain_height", terrain_height))
    write_terrain_ref_bin(output_path, terrain_height)
    print(f"WROTE {output_path}")
    print(f"CELL_COUNT {len(terrain_height)}")


if __name__ == "__main__":
    main()

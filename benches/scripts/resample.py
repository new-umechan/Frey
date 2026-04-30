#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, Tuple

import numpy as np

CLIMATE_MAGIC = b"CLIMREF1"
TERRAIN_MAGIC = b"TERRREF1"
GEOLOGY_AGE_MAGIC = b"GEOAG001"
GEOLOGY_RIDGE_MAGIC = b"GEORIDG1"
GEOLOGY_CONTINENTAL_MASK_MAGIC = b"GEOCNTL1"
HYDRO_INPUT_MAGIC = b"HYDINPUT1"
HYDRO_REF_MAGIC = b"HYDROREF1"
GLOSEM_REF_MAGIC = b"GLOSEM01"
ECOLOGY_REF_MAGIC = b"ECOREF01"
GLACIOLOGY_REF_MAGIC = b"GLACREF1"
DOMESTICATES_REF_MAGIC = b"DOMEREF2"
VERSION = 1
CLIMATE_VARIABLES = [
    "temperature",
    "precipitation",
    "evapotranspiration",
    "runoff",
    "aridity",
]
HYDRO_INPUT_VARIABLES = ["runoff"]
DOMESTICATES_CROP_NAMES = [
    "Wheat",
    "Rice",
    "Maize",
    "Millet",
    "Potato",
    "Cassava",
    "Sorghum",
]
DOMESTICATES_LIVESTOCK_NAMES = ["Cattle", "Horse", "Sheep", "Pig"]


@dataclass
class GridData:
    lat: np.ndarray
    lon: np.ndarray
    values: np.ndarray


def is_geographic_crs_string(crs_str: str) -> bool:
    return ("4326" in crs_str) or ("9518" in crs_str)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Resample reference rasters onto CellStore centroids."
    )
    parser.add_argument(
        "--module",
        required=True,
        choices=[
            "climate",
            "terrain",
            "geology-age",
            "plate-boundary",
            "continental-mask",
            "hydro-input",
            "hydro-ref",
            "glosem-ref",
            "ecology-ref",
            "domesticates-ref",
            "glaciology-ref",
        ],
    )
    parser.add_argument(
        "--centroids",
        default="benches/data/cell_centroids.csv",
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
    parser.add_argument(
        "--precipitation", help="Input raster/NetCDF for precipitation."
    )
    parser.add_argument(
        "--evapotranspiration",
        help="Input raster/NetCDF for evapotranspiration.",
    )
    parser.add_argument("--runoff", help="Input raster/NetCDF for runoff.")
    parser.add_argument("--river-flow", help="Input raster/NetCDF for river flow.")
    parser.add_argument("--lakes", help="Input shapefile for lake polygons.")
    parser.add_argument("--soil-loss", help="Input raster/NetCDF for GloSEM soil loss.")
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
    parser.add_argument("--age", help="Input raster/NetCDF for oceanic crust age.")
    parser.add_argument(
        "--ridges",
        help="Input GeoJSON file for spreading ridge line features.",
    )
    parser.add_argument(
        "--polygons",
        help="Input shapefile for continental polygons.",
    )
    parser.add_argument(
        "--age-var-name",
        default=None,
        help="Optional NetCDF variable name for oceanic crust age.",
    )
    parser.add_argument("--tree-cover", help="Input raster for MOD44B tree cover.")
    parser.add_argument(
        "--non-tree-cover",
        help="Input raster for MOD44B non-tree vegetation cover.",
    )
    parser.add_argument(
        "--bare-cover",
        help="Input raster for MOD44B non-vegetated cover.",
    )
    parser.add_argument("--landcover", help="Input raster for MCD12Q1 LC_Type1.")
    parser.add_argument("--landuse", help="Input raster for MCD12Q1 LC_Prop2.")
    parser.add_argument(
        "--soil-soc",
        help="Optional input raster for SoilGrids SOC (0-30cm aggregated).",
    )
    parser.add_argument(
        "--soil-cec",
        help="Optional input raster for SoilGrids CEC (0-30cm aggregated).",
    )
    parser.add_argument(
        "--soil-ph",
        help="Optional input raster for SoilGrids pH(H2O) (0-30cm aggregated).",
    )
    parser.add_argument(
        "--soil-bdod",
        help="Optional input raster for SoilGrids bulk density (0-30cm aggregated).",
    )
    parser.add_argument(
        "--soil-dir",
        help=(
            "Optional directory containing 12 SoilGrids depth rasters "
            "(prop_{0_5,5_15,15_30}cm_mean_{suffix}.tif)."
        ),
    )
    parser.add_argument(
        "--soil-suffix",
        default="0p1deg",
        help="Suffix for depth rasters under --soil-dir (default: 0p1deg).",
    )
    parser.add_argument(
        "--soil-w-0-5",
        type=float,
        default=5.0,
        help="Depth weight for 0-5cm when using --soil-dir.",
    )
    parser.add_argument(
        "--soil-w-5-15",
        type=float,
        default=3.5,
        help="Depth weight for 5-15cm when using --soil-dir.",
    )
    parser.add_argument(
        "--soil-w-15-30",
        type=float,
        default=1.5,
        help="Depth weight for 15-30cm when using --soil-dir.",
    )
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
        "--ice-thickness", help="Input raster/NetCDF for ice thickness."
    )
    parser.add_argument(
        "--height-clip-max",
        type=float,
        default=1.5,
        help="Maximum internal terrain height after conversion.",
    )
    parser.add_argument(
        "--manifest",
        help="Manifest JSON for domesticates-ref generation.",
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
    return np.asarray(latitudes, dtype=np.float64), np.asarray(
        longitudes, dtype=np.float64
    )


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


def sample_ridge_distance_at_centroids(
    path: Path, query_lat: np.ndarray, query_lon: np.ndarray
) -> np.ndarray:
    ridge_segments = load_ridge_segments(path)
    if not ridge_segments:
        raise ValueError(f"no line segments found in ridge file: {path}")

    lat_index = build_ridge_lat_index(ridge_segments)
    distances = np.full(query_lat.shape, np.inf, dtype=np.float64)
    for idx in range(query_lat.size):
        lat = float(query_lat[idx])
        lon = float(query_lon[idx])
        best = np.inf
        candidate_indexes = candidate_ridge_segments(lat, lat_index)
        if not candidate_indexes:
            candidate_indexes = list(range(len(ridge_segments)))
        for segment_index in candidate_indexes:
            segment = ridge_segments[segment_index]
            best = min(best, point_to_segment_distance_km(lat, lon, segment[0], segment[1]))
        distances[idx] = best
    return distances


def load_ridge_segments(path: Path) -> list[tuple[tuple[float, float], tuple[float, float]]]:
    with path.open("r", encoding="utf-8") as handle:
        if path.suffix.lower() == ".xy":
            return line_strings_to_segments(load_xy_line_strings(handle))
        data = json.load(handle)

    geometries = []

    def collect_geometry(geometry: dict | None) -> None:
        if not geometry:
            return
        geom_type = geometry.get("type")
        coordinates = geometry.get("coordinates")
        if geom_type == "LineString" and coordinates:
            geometries.append([(float(lon), float(lat)) for lon, lat in coordinates])
        elif geom_type == "MultiLineString" and coordinates:
            for line in coordinates:
                geometries.append([(float(lon), float(lat)) for lon, lat in line])
        elif geom_type == "GeometryCollection":
            for child in geometry.get("geometries", []) or []:
                collect_geometry(child)

    if data.get("type") == "FeatureCollection":
        for feature in data.get("features", []) or []:
            collect_geometry(feature.get("geometry"))
    elif data.get("type") in {"LineString", "MultiLineString", "GeometryCollection"}:
        collect_geometry(data)
    else:
        raise ValueError(f"unsupported GeoJSON type for ridge file: {data.get('type')}")

    return line_strings_to_segments([line for line in geometries if len(line) >= 2])


def load_xy_line_strings(handle) -> list[list[tuple[float, float]]]:
    lines: list[list[tuple[float, float]]] = []
    current: list[tuple[float, float]] = []
    keep_current = False
    awaiting_numeric_header = False

    for raw_line in handle:
        line = raw_line.strip()
        if not line:
            continue
        if line.startswith(">"):
            if line.startswith("> "):
                header_parts = line[1:].split()
                if len(header_parts) >= 2:
                    try:
                        keep_current = abs(float(header_parts[1])) <= 1e-6
                    except ValueError:
                        keep_current = True
                else:
                    keep_current = True
                awaiting_numeric_header = False
                continue

            if len(current) >= 2 and keep_current:
                lines.append(current)
            current = []
            keep_current = False
            awaiting_numeric_header = True
            continue
        if awaiting_numeric_header:
            continue
        if not keep_current:
            continue
        parts = line.split()
        if len(parts) < 2:
            continue
        try:
            lon = float(parts[0])
            lat = float(parts[1])
        except ValueError:
            continue
        current.append((lon, lat))

    if len(current) >= 2:
        lines.append(current)

    return lines


def line_strings_to_segments(
    lines: list[list[tuple[float, float]]],
) -> list[tuple[tuple[float, float], tuple[float, float]]]:
    segments: list[tuple[tuple[float, float], tuple[float, float]]] = []
    for line in lines:
        for start_index in range(0, max(0, len(line) - 1), 4):
            start = line[start_index]
            end = line[min(start_index + 4, len(line) - 1)]
            segments.append((start, end))
    return segments


def build_ridge_lat_index(
    segments: list[tuple[tuple[float, float], tuple[float, float]]],
    bin_size_deg: float = 5.0,
) -> dict[int, list[int]]:
    index: dict[int, list[int]] = {}
    for segment_index, (start, end) in enumerate(segments):
        lat_min = min(start[1], end[1])
        lat_max = max(start[1], end[1])
        start_bin = int(np.floor((lat_min + 90.0) / bin_size_deg))
        end_bin = int(np.floor((lat_max + 90.0) / bin_size_deg))
        for bin_index in range(start_bin, end_bin + 1):
            index.setdefault(bin_index, []).append(segment_index)
    return index


def candidate_ridge_segments(
    lat: float, index: dict[int, list[int]], bin_size_deg: float = 5.0
) -> list[int]:
    lat_bin = int(np.floor((lat + 90.0) / bin_size_deg))
    candidate_indexes: list[int] = []
    seen = set()
    for bin_index in range(lat_bin - 2, lat_bin + 3):
        for segment_index in index.get(bin_index, []):
            if segment_index not in seen:
                seen.add(segment_index)
                candidate_indexes.append(segment_index)
    return candidate_indexes


def point_to_segment_distance_km(
    lat: float,
    lon: float,
    start: tuple[float, float],
    end: tuple[float, float],
) -> float:
    radius_km = 6371.0
    lat_rad = np.deg2rad(lat)
    cos_lat = np.cos(lat_rad)

    def project(point: tuple[float, float]) -> tuple[float, float]:
        lon_pt, lat_pt = point
        dlon = lon_pt - lon
        while dlon <= -180.0:
            dlon += 360.0
        while dlon > 180.0:
            dlon -= 360.0
        x = np.deg2rad(dlon) * radius_km * cos_lat
        y = np.deg2rad(lat_pt - lat) * radius_km
        return x, y

    px, py = 0.0, 0.0
    ax, ay = project(start)
    bx, by = project(end)
    vx = bx - ax
    vy = by - ay
    wx = px - ax
    wy = py - ay
    denom = vx * vx + vy * vy
    if denom <= 1e-12:
        return float(np.hypot(wx, wy))
    t = max(0.0, min(1.0, (wx * vx + wy * vy) / denom))
    cx = ax + t * vx
    cy = ay + t * vy
    return float(np.hypot(px - cx, py - cy))


def load_geotiff(path: Path) -> GridData:
    try:
        import rasterio
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("rasterio is required to read GeoTIFF inputs") from exc

    with rasterio.open(path) as ds:
        if ds.count < 1:
            raise ValueError(f"GeoTIFF has no bands: {path}")
        crs_str = str(ds.crs or "")
        # Accept common geographic CRS encodings for global rasters.
        if not is_geographic_crs_string(crs_str):
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


def sample_geotiff_projected_nearest(
    path: Path, query_lat: np.ndarray, query_lon: np.ndarray
) -> np.ndarray:
    try:
        import rasterio
        from rasterio.warp import transform
    except Exception as exc:  # pragma: no cover
        raise RuntimeError(
            "rasterio is required to read projected GeoTIFF inputs"
        ) from exc

    with rasterio.open(path) as ds:
        crs = ds.crs
        if crs is None:
            raise ValueError(f"GeoTIFF CRS is missing: {path}")
        x_vals, y_vals = transform(
            "EPSG:4326",
            crs,
            query_lon.astype(float).tolist(),
            query_lat.astype(float).tolist(),
        )
        coords = list(zip(x_vals, y_vals))
        sampled = np.array([v[0] for v in ds.sample(coords)], dtype=np.float64)
        nodata = ds.nodata
        if nodata is not None:
            sampled[np.isclose(sampled, nodata)] = np.nan
        return sampled


def sample_input_at_centroids(
    path: Path,
    preferred_var: str | None,
    query_lat: np.ndarray,
    query_lon: np.ndarray,
    method: str,
) -> np.ndarray:
    suffix = path.suffix.lower()
    if suffix in {".nc", ".nc4", ".netcdf"}:
        grid = load_netcdf(path, preferred_var)
        return interpolate_grid(grid, query_lat, query_lon, method)
    if suffix not in {".tif", ".tiff"}:
        raise ValueError(
            f"unsupported input extension for {path}. Use GeoTIFF(.tif/.tiff) or NetCDF(.nc/.nc4)."
        )

    try:
        import rasterio
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("rasterio is required to read GeoTIFF inputs") from exc

    with rasterio.open(path) as ds:
        crs_str = str(ds.crs or "")
    if is_geographic_crs_string(crs_str):
        grid = load_geotiff(path)
        return interpolate_grid(grid, query_lat, query_lon, method)

    if method != "nearest":
        print(
            f"WARNING: projected raster sampled with nearest instead of {method}: {path}"
        )
    return sample_geotiff_projected_nearest(path, query_lat, query_lon)


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


def normalize_grid_axes(
    lat: np.ndarray, lon: np.ndarray, values: np.ndarray
) -> GridData:
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
        query_lon_norm = np.asarray(
            [normalize_lon(v) for v in query_lon], dtype=np.float64
        )

    if method == "nearest":
        return interpolate_nearest(grid, query_lat, query_lon_norm)
    return interpolate_bilinear(grid, query_lat, query_lon_norm)


def interpolate_nearest(
    grid: GridData, query_lat: np.ndarray, query_lon: np.ndarray
) -> np.ndarray:
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


def interpolate_bilinear(
    grid: GridData, query_lat: np.ndarray, query_lon: np.ndarray
) -> np.ndarray:
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


def write_hydro_input_bin(path: Path, runoff: np.ndarray) -> None:
    values = np.asarray(runoff, dtype="<f4")
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as handle:
        handle.write(HYDRO_INPUT_MAGIC)
        handle.write(struct.pack("<I", VERSION))
        handle.write(struct.pack("<Q", int(values.size)))
        handle.write(values.tobytes(order="C"))


def write_hydro_ref_bin(
    path: Path, river_flow: np.ndarray, is_lake: np.ndarray
) -> None:
    flow_values = np.asarray(river_flow, dtype="<f4")
    lake_values = np.asarray(is_lake, dtype=np.uint8)
    if flow_values.size != lake_values.size:
        raise ValueError(
            "hydro ref arrays must have the same length: "
            f"river_flow={flow_values.size}, is_lake={lake_values.size}"
        )
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as handle:
        handle.write(HYDRO_REF_MAGIC)
        handle.write(struct.pack("<I", VERSION))
        handle.write(struct.pack("<Q", int(flow_values.size)))
        handle.write(flow_values.tobytes(order="C"))
        handle.write(lake_values.tobytes(order="C"))


def write_glosem_ref_bin(path: Path, erosion_rate: np.ndarray) -> None:
    values = np.asarray(erosion_rate, dtype="<f4")
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as handle:
        handle.write(GLOSEM_REF_MAGIC)
        handle.write(struct.pack("<I", VERSION))
        handle.write(struct.pack("<Q", int(values.size)))
        handle.write(values.tobytes(order="C"))


def write_ecology_ref_bin(
    path: Path,
    tree_cover: np.ndarray,
    ground_cover: np.ndarray,
    soil_fertility: np.ndarray,
    biome: np.ndarray,
    natural_mask: np.ndarray,
    open_canopy_mask: np.ndarray,
) -> None:
    tree_values = np.asarray(tree_cover, dtype="<f4")
    ground_values = np.asarray(ground_cover, dtype="<f4")
    soil_values = np.asarray(soil_fertility, dtype="<f4")
    biome_values = np.asarray(biome, dtype=np.uint8)
    natural_values = np.asarray(natural_mask, dtype=np.uint8)
    open_canopy_values = np.asarray(open_canopy_mask, dtype=np.uint8)

    length = int(tree_values.size)
    for name, values in (
        ("ground_cover", ground_values),
        ("soil_fertility", soil_values),
        ("biome", biome_values),
        ("natural_mask", natural_values),
        ("open_canopy_mask", open_canopy_values),
    ):
        if int(values.size) != length:
            raise ValueError(
                f"length mismatch for {name}: expected {length}, got {values.size}"
            )

    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as handle:
        handle.write(ECOLOGY_REF_MAGIC)
        handle.write(struct.pack("<I", VERSION))
        handle.write(struct.pack("<Q", length))
        handle.write(tree_values.tobytes(order="C"))
        handle.write(ground_values.tobytes(order="C"))
        handle.write(soil_values.tobytes(order="C"))
        handle.write(biome_values.tobytes(order="C"))
        handle.write(natural_values.tobytes(order="C"))
        handle.write(open_canopy_values.tobytes(order="C"))


def signed_ring_area(points: list[tuple[float, float]]) -> float:
    area = 0.0
    count = len(points)
    if count < 3:
        return 0.0
    for index in range(count):
        x0, y0 = points[index]
        x1, y1 = points[(index + 1) % count]
        area += (x0 * y1) - (x1 * y0)
    return area * 0.5


def point_in_ring(lon: float, lat: float, ring: list[tuple[float, float]]) -> bool:
    inside = False
    count = len(ring)
    if count < 3:
        return False
    for index in range(count):
        x0, y0 = ring[index]
        x1, y1 = ring[(index + 1) % count]
        if (y0 > lat) == (y1 > lat):
            continue
        if y1 == y0:
            continue
        cross_lon = x0 + (lat - y0) * (x1 - x0) / (y1 - y0)
        if cross_lon == lon:
            return True
        if cross_lon > lon:
            inside = not inside
    return inside


def shape_rings(shape) -> list[list[tuple[float, float]]]:
    points = [(float(lon), float(lat)) for lon, lat in shape.points]
    part_starts = list(shape.parts) + [len(points)]
    rings = []
    for start, end in zip(part_starts, part_starts[1:]):
        ring = points[start:end]
        if len(ring) >= 3:
            rings.append(ring)
    return rings


def shape_contains_point(lon: float, lat: float, shape) -> bool:
    outer_rings = []
    hole_rings = []
    for ring in shape_rings(shape):
        if signed_ring_area(ring) < 0.0:
            outer_rings.append(ring)
        else:
            hole_rings.append(ring)
    if not outer_rings:
        outer_rings = shape_rings(shape)
    if not any(point_in_ring(lon, lat, ring) for ring in outer_rings):
        return False
    return not any(point_in_ring(lon, lat, ring) for ring in hole_rings)


def rasterize_lake_polygons(
    path: Path, centroid_lat: np.ndarray, centroid_lon: np.ndarray
) -> np.ndarray:
    try:
        import shapefile
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("pyshp is required to read HydroLAKES polygons") from exc

    reader = shapefile.Reader(str(path), encoding="latin1")
    fields = [field[0] for field in reader.fields[1:]]
    area_column = next(
        (name for name in ("Lake_area", "Lake_area_km2", "area_km2") if name in fields),
        None,
    )
    if area_column is None:
        raise ValueError(
            "lake shapefile must include an area column such as Lake_area or Lake_area_km2"
        )
    area_index = fields.index(area_column)
    if reader.numRecords == 0:
        return np.zeros(centroid_lat.shape, dtype=np.uint8)

    is_lake = np.zeros(centroid_lat.shape, dtype=np.uint8)
    for shape_record in reader.iterShapeRecords():
        area_value = shape_record.record[area_index]
        if area_value is None or float(area_value) < 1500.0:
            continue
        min_lon, min_lat, max_lon, max_lat = shape_record.shape.bbox
        candidates = np.where(
            (is_lake == 0)
            & (centroid_lon >= min_lon)
            & (centroid_lon <= max_lon)
            & (centroid_lat >= min_lat)
            & (centroid_lat <= max_lat)
        )[0]
        if candidates.size == 0:
            continue
        for index in candidates:
            if shape_contains_point(
                float(centroid_lon[index]),
                float(centroid_lat[index]),
                shape_record.shape,
            ):
                is_lake[index] = 1
    return is_lake


def rasterize_continental_polygons(
    path: Path, centroid_lat: np.ndarray, centroid_lon: np.ndarray
) -> np.ndarray:
    try:
        import shapefile
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("pyshp is required to read continental polygons") from exc

    reader = shapefile.Reader(str(path), encoding="latin1")
    if reader.numRecords == 0:
        return np.zeros(centroid_lat.shape, dtype=np.uint8)

    is_continental = np.zeros(centroid_lat.shape, dtype=np.uint8)
    for shape_record in reader.iterShapeRecords():
        min_lon, min_lat, max_lon, max_lat = shape_record.shape.bbox
        candidates = np.where(
            (is_continental == 0)
            & (centroid_lon >= min_lon)
            & (centroid_lon <= max_lon)
            & (centroid_lat >= min_lat)
            & (centroid_lat <= max_lat)
        )[0]
        if candidates.size == 0:
            continue
        for index in candidates:
            if shape_contains_point(
                float(centroid_lon[index]),
                float(centroid_lat[index]),
                shape_record.shape,
            ):
                is_continental[index] = 1
    return is_continental


def transform_aridity(values: np.ndarray, source: str) -> np.ndarray:
    if source == "pet_over_precip":
        return values
    if source == "precip_over_pet_x10000":
        out = np.full(values.shape, np.nan, dtype=np.float64)
        valid = np.isfinite(values) & (values > 0.0)
        out[valid] = 10000.0 / values[valid]
        return out
    raise ValueError(f"unknown aridity source: {source}")


def transform_terrain_height(
    values: np.ndarray, args: argparse.Namespace
) -> np.ndarray:
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


def write_geology_age_ref_bin(path: Path, ages: np.ndarray) -> None:
    values = np.asarray(ages, dtype="<f4")
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as handle:
        handle.write(GEOLOGY_AGE_MAGIC)
        handle.write(struct.pack("<I", VERSION))
        handle.write(struct.pack("<Q", int(values.size)))
        handle.write(values.tobytes(order="C"))


def write_geology_ridge_ref_bin(path: Path, ridge_distance_km: np.ndarray) -> None:
    values = np.asarray(ridge_distance_km, dtype="<f4")
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as handle:
        handle.write(GEOLOGY_RIDGE_MAGIC)
        handle.write(struct.pack("<I", VERSION))
        handle.write(struct.pack("<Q", int(values.size)))
        handle.write(values.tobytes(order="C"))


def write_geology_continental_mask_ref_bin(path: Path, mask: np.ndarray) -> None:
    values = np.asarray(mask, dtype=np.uint8)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as handle:
        handle.write(GEOLOGY_CONTINENTAL_MASK_MAGIC)
        handle.write(struct.pack("<I", VERSION))
        handle.write(struct.pack("<Q", int(values.size)))
        handle.write(values.tobytes(order="C"))


def write_glaciology_ref_bin(path: Path, ice_thickness: np.ndarray) -> None:
    values = np.asarray(ice_thickness, dtype="<f4")
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as handle:
        handle.write(GLACIOLOGY_REF_MAGIC)
        handle.write(struct.pack("<I", VERSION))
        handle.write(struct.pack("<Q", int(values.size)))
        handle.write(values.tobytes(order="C"))


def write_domesticates_ref_bin(
    path: Path,
    crop_observed_intensity: np.ndarray,
    livestock_observed_intensity: np.ndarray,
    crop_observed_presence: np.ndarray,
    livestock_observed_presence: np.ndarray,
    crop_eval_mask: np.ndarray,
    livestock_eval_mask: np.ndarray,
) -> None:
    crop_intensity = np.asarray(crop_observed_intensity, dtype="<f4")
    livestock_intensity = np.asarray(livestock_observed_intensity, dtype="<f4")
    crop_presence = np.asarray(crop_observed_presence, dtype=np.uint8)
    livestock_presence = np.asarray(livestock_observed_presence, dtype=np.uint8)
    crop_mask = np.asarray(crop_eval_mask, dtype=np.uint8)
    livestock_mask = np.asarray(livestock_eval_mask, dtype=np.uint8)

    if crop_intensity.ndim != 2 or crop_intensity.shape[1] != len(DOMESTICATES_CROP_NAMES):
        raise ValueError("crop observed intensity must be [cell_count, 7]")
    if (
        livestock_intensity.ndim != 2
        or livestock_intensity.shape[1] != len(DOMESTICATES_LIVESTOCK_NAMES)
    ):
        raise ValueError("livestock observed intensity must be [cell_count, 4]")

    cell_count = int(crop_intensity.shape[0])
    if int(livestock_intensity.shape[0]) != cell_count:
        raise ValueError("crop/livestock intensity cell counts must match")
    if crop_presence.shape != (cell_count,):
        raise ValueError("crop observed presence must be [cell_count]")
    if livestock_presence.shape != (cell_count,):
        raise ValueError("livestock observed presence must be [cell_count]")
    if crop_mask.shape != crop_intensity.shape:
        raise ValueError("crop eval mask shape must match crop intensity")
    if livestock_mask.shape != livestock_intensity.shape:
        raise ValueError("livestock eval mask shape must match livestock intensity")

    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as handle:
        handle.write(DOMESTICATES_REF_MAGIC)
        handle.write(struct.pack("<I", VERSION))
        handle.write(struct.pack("<Q", cell_count))
        handle.write(crop_intensity.tobytes(order="C"))
        handle.write(livestock_intensity.tobytes(order="C"))
        handle.write(crop_presence.tobytes(order="C"))
        handle.write(livestock_presence.tobytes(order="C"))
        handle.write(crop_mask.tobytes(order="C"))
        handle.write(livestock_mask.tobytes(order="C"))


def to_fraction_cover(values: np.ndarray) -> np.ndarray:
    out = np.asarray(values, dtype=np.float64).copy()
    invalid = ~np.isfinite(out) | (out >= 200.0) | (out < 0.0)
    out[invalid] = np.nan
    out = np.clip(out / 100.0, 0.0, 1.0)
    return out


def to_height_m(values: np.ndarray, args: argparse.Namespace) -> np.ndarray:
    out = np.asarray(values, dtype=np.float64).copy()
    if args.height_source == "meters":
        out = out - float(args.sea_level_m)
    else:
        out = out * float(args.height_to_meters)
    out[~np.isfinite(out)] = np.nan
    return out


def percentile_rank(values: np.ndarray) -> np.ndarray:
    arr = np.asarray(values, dtype=np.float64)
    out = np.full(arr.shape, np.nan, dtype=np.float64)
    valid = np.isfinite(arr)
    if not np.any(valid):
        return out
    valid_values = arr[valid]
    order = np.argsort(valid_values, kind="mergesort")
    ranks = np.empty_like(order, dtype=np.float64)
    ranks[order] = np.arange(order.size, dtype=np.float64)
    if order.size > 1:
        ranks /= float(order.size - 1)
    else:
        ranks.fill(0.5)
    out[valid] = ranks
    return out


def normalize_observed_intensity(values: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    arr = np.asarray(values, dtype=np.float64)
    out = np.full(arr.shape, np.nan, dtype=np.float64)
    eval_mask = np.zeros(arr.shape, dtype=np.uint8)
    valid = np.isfinite(arr) & (arr >= 0.0)
    if not np.any(valid):
        return out, eval_mask
    transformed = np.log1p(arr[valid])
    lo = np.nanquantile(transformed, 0.01)
    hi = np.nanquantile(transformed, 0.99)
    if not np.isfinite(lo) or not np.isfinite(hi):
        return out, eval_mask
    if hi <= lo:
        out[valid] = 0.0
        return out, eval_mask
    clipped = np.clip(transformed, lo, hi)
    out[valid] = (clipped - lo) / (hi - lo)
    eval_mask[valid] = 1
    return np.clip(out, 0.0, 1.0), eval_mask


def load_domesticates_manifest(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)
    if not isinstance(payload, dict):
        raise ValueError("domesticates manifest must be a JSON object")
    entries = payload.get("entries")
    if not isinstance(entries, list):
        raise ValueError("domesticates manifest must include entries[]")
    return payload


def resolve_manifest_entry(
    entries: list[dict], kind: str, name: str
) -> dict:
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        if entry.get("kind") == kind and entry.get("name") == name:
            return entry
    raise ValueError(f"manifest entry missing for {kind}:{name}")


def sample_domesticates_entry(
    entry: dict,
    centroid_lat: np.ndarray,
    centroid_lon: np.ndarray,
    method: str,
    cell_count: int,
    base_dir: Path,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    mode = str(entry.get("mode", "raster")).strip()
    if mode != "raster":
        raise ValueError(
            f"domesticates-ref only supports mode=raster in resample, got {mode}"
        )

    local_path = entry.get("local_path")
    if not isinstance(local_path, str) or not local_path.strip():
        raise ValueError("manifest entry requires local_path")
    source_path = Path(local_path)
    if not source_path.is_absolute():
        source_path = (base_dir / source_path).resolve()
    if not source_path.exists():
        raise FileNotFoundError(f"missing domesticates raster: {source_path}")

    preferred_var = entry.get("var_name")
    sampled = sample_input_at_centroids(
        source_path,
        str(preferred_var) if isinstance(preferred_var, str) else None,
        centroid_lat,
        centroid_lon,
        method,
    )
    normalized, eval_mask = normalize_observed_intensity(sampled)
    threshold = float(entry.get("presence_threshold", 0.2))
    presence = np.zeros((cell_count,), dtype=np.uint8)
    valid_presence = (eval_mask == 1) & (normalized >= threshold)
    presence[valid_presence] = 1
    return normalized.astype(np.float32), presence, eval_mask


def ph_suitability(values: np.ndarray) -> np.ndarray:
    ph = np.asarray(values, dtype=np.float64)
    out = np.full(ph.shape, np.nan, dtype=np.float64)
    valid = np.isfinite(ph)
    if not np.any(valid):
        return out
    pv = ph[valid]
    score = np.zeros_like(pv)
    score[(pv >= 6.0) & (pv <= 7.5)] = 1.0
    left = (pv >= 4.0) & (pv < 6.0)
    score[left] = (pv[left] - 4.0) / 2.0
    right = (pv > 7.5) & (pv <= 9.0)
    score[right] = (9.0 - pv[right]) / 1.5
    out[valid] = np.clip(score, 0.0, 1.0)
    return out


def load_weighted_soil_from_depths(
    soil_dir: Path,
    prop: str,
    suffix: str,
    centroid_lat: np.ndarray,
    centroid_lon: np.ndarray,
    method: str,
    w_0_5: float,
    w_5_15: float,
    w_15_30: float,
) -> np.ndarray:
    paths = [
        soil_dir / f"{prop}_0_5cm_mean_{suffix}.tif",
        soil_dir / f"{prop}_5_15cm_mean_{suffix}.tif",
        soil_dir / f"{prop}_15_30cm_mean_{suffix}.tif",
    ]
    for path in paths:
        if not path.exists():
            raise FileNotFoundError(f"missing SoilGrids depth raster: {path}")

    depth_0_5 = sample_input_at_centroids(
        paths[0], None, centroid_lat, centroid_lon, method
    )
    depth_5_15 = sample_input_at_centroids(
        paths[1], None, centroid_lat, centroid_lon, method
    )
    depth_15_30 = sample_input_at_centroids(
        paths[2], None, centroid_lat, centroid_lon, method
    )

    denom = w_0_5 + w_5_15 + w_15_30
    if denom <= 0.0:
        raise ValueError("soil depth weights must sum to > 0")
    weighted = depth_0_5 * w_0_5 + depth_5_15 * w_5_15 + depth_15_30 * w_15_30
    out = np.full(depth_0_5.shape, np.nan, dtype=np.float64)
    finite = np.isfinite(depth_0_5) & np.isfinite(depth_5_15) & np.isfinite(depth_15_30)
    out[finite] = weighted[finite] / denom
    return out


def build_natural_mask(lc_type1: np.ndarray, lc_prop2: np.ndarray | None) -> np.ndarray:
    lc = np.asarray(lc_type1, dtype=np.float64)
    mask = np.isfinite(lc)
    excluded_type1 = {0, 12, 13, 14, 15, 17, 254, 255}
    for code in excluded_type1:
        mask &= lc != float(code)
    if lc_prop2 is not None:
        prop2 = np.asarray(lc_prop2, dtype=np.float64)
        # Keep this conservative: only mark obvious no-data/fill values as excluded.
        mask &= np.isfinite(prop2)
        mask &= prop2 != 255.0
    return mask.astype(np.uint8)


def classify_biome_ref(
    tree_cover: np.ndarray,
    bare_cover: np.ndarray,
    temperature: np.ndarray,
    precipitation: np.ndarray,
    river_flow: np.ndarray,
    height_m: np.ndarray,
    lc_type1: np.ndarray,
    natural_mask: np.ndarray,
) -> np.ndarray:
    biome = np.full(tree_cover.shape, 255, dtype=np.uint8)
    natural = natural_mask == 1
    if not np.any(natural):
        return biome

    river_valid = np.isfinite(river_flow)
    river_q98 = (
        np.nanquantile(river_flow[river_valid], 0.98) if np.any(river_valid) else np.nan
    )

    tundra = (
        natural
        & np.isfinite(temperature)
        & np.isfinite(tree_cover)
        & (temperature <= -2.0)
        & (tree_cover < 0.25)
    )
    alpine = (
        natural
        & np.isfinite(height_m)
        & np.isfinite(tree_cover)
        & (height_m >= 2500.0)
        & (tree_cover < 0.20)
    )
    desert = (
        natural
        & np.isfinite(bare_cover)
        & np.isfinite(precipitation)
        & (bare_cover >= 0.60)
        & (precipitation < 300.0)
    )
    wetland = natural & np.isfinite(lc_type1) & (lc_type1 == 11.0)
    if np.isfinite(river_q98):
        lowland = np.isfinite(height_m) & (height_m <= 300.0)
        high_flow = np.isfinite(river_flow) & (river_flow >= river_q98)
        wetland = wetland | (natural & lowland & high_flow)

    tropical_forest = (
        natural
        & np.isfinite(temperature)
        & np.isfinite(tree_cover)
        & (temperature >= 22.0)
        & (tree_cover >= 0.60)
    )
    savanna = (
        natural
        & np.isfinite(temperature)
        & np.isfinite(tree_cover)
        & (temperature >= 22.0)
        & (tree_cover >= 0.10)
    )
    temperate_forest = (
        natural
        & np.isfinite(temperature)
        & np.isfinite(tree_cover)
        & (temperature >= 6.0)
        & (tree_cover >= 0.55)
    )
    boreal_forest = (
        natural
        & np.isfinite(temperature)
        & np.isfinite(tree_cover)
        & (temperature < 6.0)
        & (tree_cover >= 0.35)
    )

    # Biome encoding aligned with rust enum:
    # 0 TropicalForest, 1 Savanna, 2 Desert, 3 Grassland, 4 TemperateForest,
    # 5 BorealForest, 6 Tundra, 7 Wetland, 8 Alpine, 255 excluded.
    biome[grassland := natural] = 3
    biome[boreal_forest] = 5
    biome[temperate_forest] = 4
    biome[savanna] = 1
    biome[tropical_forest] = 0
    biome[wetland] = 7
    biome[desert] = 2
    biome[tundra] = 6
    biome[alpine] = 8
    return biome


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
            output_path = Path("benches/data/climate_ref.bin")
        elif args.module == "geology-age":
            output_path = Path("benches/data/oceanic_crust_age_ref.bin")
        elif args.module == "plate-boundary":
            output_path = Path("benches/data/plate_boundary_ref.bin")
        elif args.module == "continental-mask":
            output_path = Path("benches/data/continental_mask_ref.bin")
        elif args.module == "hydro-input":
            output_path = Path("benches/data/hydro_input.bin")
        elif args.module == "hydro-ref":
            output_path = Path("benches/data/hydro_ref.bin")
        elif args.module == "ecology-ref":
            output_path = Path("benches/data/ecology_ref.bin")
        elif args.module == "domesticates-ref":
            output_path = Path("benches/data/domesticates_ref.bin")
        else:
            output_path = Path("benches/data/glaciology_ref.bin")
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

    if args.module == "geology-age":
        if not args.age:
            raise ValueError("missing required arg for geology-age module: --age")

        grid = load_input_grid(Path(args.age), args.age_var_name)
        ages = interpolate_grid(
            grid=grid,
            query_lat=centroid_lat,
            query_lon=centroid_lon,
            method=args.method,
        ).astype(np.float32, copy=False)
        print(summarize("oceanic_age", ages))
        write_geology_age_ref_bin(output_path, ages)
        print(f"WROTE {output_path}")
        print(f"CELL_COUNT {len(ages)}")
        return

    if args.module == "plate-boundary":
        if not args.ridges:
            raise ValueError(
                "missing required arg for plate-boundary module: --ridges"
            )

        ridge_distance_km = sample_ridge_distance_at_centroids(
            Path(args.ridges), centroid_lat, centroid_lon
        ).astype(np.float32, copy=False)
        print(summarize("ridge_distance_km", ridge_distance_km))
        write_geology_ridge_ref_bin(output_path, ridge_distance_km)
        print(f"WROTE {output_path}")
        print(f"CELL_COUNT {len(ridge_distance_km)}")
        return

    if args.module == "continental-mask":
        if not args.polygons:
            raise ValueError(
                "missing required arg for continental-mask module: --polygons"
            )

        continental_mask = rasterize_continental_polygons(
            Path(args.polygons), centroid_lat, centroid_lon
        )
        print(
            f"continental_mask: valid={int(continental_mask.size)}/{int(continental_mask.size)} "
            f"true={int(np.count_nonzero(continental_mask))} "
            f"false={int(continental_mask.size - np.count_nonzero(continental_mask))}"
        )
        write_geology_continental_mask_ref_bin(output_path, continental_mask)
        print(f"WROTE {output_path}")
        print(f"CELL_COUNT {len(continental_mask)}")
        return

    if args.module == "hydro-input":
        var_map = parse_var_map(args.var_name)
        if not args.runoff:
            raise ValueError("missing required arg for hydro-input module: --runoff")

        grid = load_input_grid(Path(args.runoff), var_map.get("runoff"))
        runoff = interpolate_grid(
            grid=grid,
            query_lat=centroid_lat,
            query_lon=centroid_lon,
            method=args.method,
        ).astype(np.float32, copy=False)
        print(summarize("runoff", runoff))
        write_hydro_input_bin(output_path, runoff)
        print(f"WROTE {output_path}")
        print(f"CELL_COUNT {len(runoff)}")
        return

    if args.module == "hydro-ref":
        if not args.river_flow or not args.lakes:
            raise ValueError(
                "missing required args for hydro-ref module: --river-flow, --lakes"
            )

        flow_grid = load_input_grid(Path(args.river_flow), None)
        river_flow = interpolate_grid(
            grid=flow_grid,
            query_lat=centroid_lat,
            query_lon=centroid_lon,
            method=args.method,
        ).astype(np.float32, copy=False)
        is_lake = rasterize_lake_polygons(Path(args.lakes), centroid_lat, centroid_lon)
        print(summarize("river_flow", river_flow))
        print(
            f"is_lake: valid={int(is_lake.size)}/{int(is_lake.size)} "
            f"true={int(np.count_nonzero(is_lake))} false={int(is_lake.size - np.count_nonzero(is_lake))}"
        )
        write_hydro_ref_bin(output_path, river_flow, is_lake)
        print(f"WROTE {output_path}")
        print(f"CELL_COUNT {len(river_flow)}")
        return

    if args.module == "glosem-ref":
        if not args.soil_loss:
            raise ValueError("missing required arg for glosem-ref module: --soil-loss")

        grid = load_input_grid(Path(args.soil_loss), None)
        erosion_rate = interpolate_grid(
            grid=grid,
            query_lat=centroid_lat,
            query_lon=centroid_lon,
            method=args.method,
        ).astype(np.float32, copy=False)
        print(summarize("erosion_rate", erosion_rate))
        write_glosem_ref_bin(output_path, erosion_rate)
        print(f"WROTE {output_path}")
        print(f"CELL_COUNT {len(erosion_rate)}")
        return

    if args.module == "ecology-ref":
        required = {
            "tree_cover": args.tree_cover,
            "non_tree_cover": args.non_tree_cover,
            "bare_cover": args.bare_cover,
            "landcover": args.landcover,
            "temperature": args.temperature,
            "precipitation": args.precipitation,
            "river_flow": args.river_flow,
            "height": args.height,
        }
        missing = [name for name, value in required.items() if not value]
        if missing:
            raise ValueError(f"missing required args for ecology-ref module: {missing}")

        tree_cover = to_fraction_cover(
            sample_input_at_centroids(
                Path(args.tree_cover), None, centroid_lat, centroid_lon, args.method
            )
        )
        ground_cover = to_fraction_cover(
            sample_input_at_centroids(
                Path(args.non_tree_cover), None, centroid_lat, centroid_lon, args.method
            )
        )
        bare_cover = to_fraction_cover(
            sample_input_at_centroids(
                Path(args.bare_cover), None, centroid_lat, centroid_lon, args.method
            )
        )
        lc_type1 = sample_input_at_centroids(
            Path(args.landcover),
            None,
            centroid_lat,
            centroid_lon,
            "nearest",
        )
        lc_prop2 = None
        if args.landuse:
            lc_prop2 = sample_input_at_centroids(
                Path(args.landuse), None, centroid_lat, centroid_lon, "nearest"
            )

        temperature = sample_input_at_centroids(
            Path(args.temperature), None, centroid_lat, centroid_lon, args.method
        )
        precipitation = sample_input_at_centroids(
            Path(args.precipitation), None, centroid_lat, centroid_lon, args.method
        )
        river_flow = sample_input_at_centroids(
            Path(args.river_flow), None, centroid_lat, centroid_lon, args.method
        )
        height_m = to_height_m(
            sample_input_at_centroids(
                Path(args.height),
                args.height_var_name,
                centroid_lat,
                centroid_lon,
                args.method,
            ),
            args,
        )

        natural_mask = build_natural_mask(lc_type1, lc_prop2)
        open_canopy_mask = (
            (natural_mask == 1) & np.isfinite(tree_cover) & (tree_cover <= 0.40)
        ).astype(np.uint8)
        biome = classify_biome_ref(
            tree_cover=tree_cover,
            bare_cover=bare_cover,
            temperature=temperature,
            precipitation=precipitation,
            river_flow=river_flow,
            height_m=height_m,
            lc_type1=lc_type1,
            natural_mask=natural_mask,
        )

        soil_fertility = np.full(tree_cover.shape, np.nan, dtype=np.float64)
        if args.soil_soc and args.soil_cec and args.soil_ph and args.soil_bdod:
            soc = sample_input_at_centroids(
                Path(args.soil_soc),
                None,
                centroid_lat,
                centroid_lon,
                args.method,
            )
            cec = sample_input_at_centroids(
                Path(args.soil_cec),
                None,
                centroid_lat,
                centroid_lon,
                args.method,
            )
            sph = sample_input_at_centroids(
                Path(args.soil_ph),
                None,
                centroid_lat,
                centroid_lon,
                args.method,
            )
            bdod = sample_input_at_centroids(
                Path(args.soil_bdod),
                None,
                centroid_lat,
                centroid_lon,
                args.method,
            )
            soil_fertility = (
                0.45 * percentile_rank(soc)
                + 0.25 * percentile_rank(cec)
                + 0.20 * ph_suitability(sph)
                + 0.10 * (1.0 - percentile_rank(bdod))
            )
            soil_fertility = np.clip(soil_fertility, 0.0, 1.0)
        elif args.soil_dir:
            soil_dir = Path(args.soil_dir)
            w_0_5 = float(args.soil_w_0_5)
            w_5_15 = float(args.soil_w_5_15)
            w_15_30 = float(args.soil_w_15_30)
            soc = load_weighted_soil_from_depths(
                soil_dir=soil_dir,
                prop="soc",
                suffix=args.soil_suffix,
                centroid_lat=centroid_lat,
                centroid_lon=centroid_lon,
                method=args.method,
                w_0_5=w_0_5,
                w_5_15=w_5_15,
                w_15_30=w_15_30,
            )
            cec = load_weighted_soil_from_depths(
                soil_dir=soil_dir,
                prop="cec",
                suffix=args.soil_suffix,
                centroid_lat=centroid_lat,
                centroid_lon=centroid_lon,
                method=args.method,
                w_0_5=w_0_5,
                w_5_15=w_5_15,
                w_15_30=w_15_30,
            )
            sph = load_weighted_soil_from_depths(
                soil_dir=soil_dir,
                prop="phh2o",
                suffix=args.soil_suffix,
                centroid_lat=centroid_lat,
                centroid_lon=centroid_lon,
                method=args.method,
                w_0_5=w_0_5,
                w_5_15=w_5_15,
                w_15_30=w_15_30,
            )
            bdod = load_weighted_soil_from_depths(
                soil_dir=soil_dir,
                prop="bdod",
                suffix=args.soil_suffix,
                centroid_lat=centroid_lat,
                centroid_lon=centroid_lon,
                method=args.method,
                w_0_5=w_0_5,
                w_5_15=w_5_15,
                w_15_30=w_15_30,
            )
            print(
                "soil_depth_weights: "
                f"w0_5={w_0_5:.3f} w5_15={w_5_15:.3f} w15_30={w_15_30:.3f}"
            )
            soil_fertility = (
                0.45 * percentile_rank(soc)
                + 0.25 * percentile_rank(cec)
                + 0.20 * ph_suitability(sph)
                + 0.10 * (1.0 - percentile_rank(bdod))
            )
            soil_fertility = np.clip(soil_fertility, 0.0, 1.0)
        else:
            print("soil_fertility: no soil rasters provided, writing NaN")

        print(summarize("tree_cover", tree_cover))
        print(summarize("ground_cover", ground_cover))
        print(
            f"natural_mask: valid={int(natural_mask.size)}/{int(natural_mask.size)} "
            f"true={int(np.count_nonzero(natural_mask))} false={int(natural_mask.size - np.count_nonzero(natural_mask))}"
        )
        print(
            f"open_canopy_mask: valid={int(open_canopy_mask.size)}/{int(open_canopy_mask.size)} "
            f"true={int(np.count_nonzero(open_canopy_mask))} false={int(open_canopy_mask.size - np.count_nonzero(open_canopy_mask))}"
        )
        print(
            "biome_ref: "
            + ", ".join(
                [
                    f"{code}={int(np.count_nonzero(biome == code))}"
                    for code in [0, 1, 2, 3, 4, 5, 6, 7, 8, 255]
                ]
            )
        )
        print(summarize("soil_fertility", soil_fertility))

        write_ecology_ref_bin(
            output_path,
            tree_cover.astype(np.float32, copy=False),
            ground_cover.astype(np.float32, copy=False),
            soil_fertility.astype(np.float32, copy=False),
            biome,
            natural_mask,
            open_canopy_mask,
        )
        print(f"WROTE {output_path}")
        print(f"CELL_COUNT {len(tree_cover)}")
        return

    if args.module == "domesticates-ref":
        if not args.manifest:
            raise ValueError(
                "missing required arg for domesticates-ref module: --manifest"
            )
        manifest_path = Path(args.manifest)
        manifest = load_domesticates_manifest(manifest_path)
        entries = manifest["entries"]
        cell_count = int(centroid_lat.size)
        base_dir = manifest_path.parent

        crop_intensity = np.full(
            (cell_count, len(DOMESTICATES_CROP_NAMES)), np.nan, dtype=np.float32
        )
        livestock_intensity = np.full(
            (cell_count, len(DOMESTICATES_LIVESTOCK_NAMES)), np.nan, dtype=np.float32
        )
        crop_presence = np.zeros((cell_count,), dtype=np.uint8)
        livestock_presence = np.zeros((cell_count,), dtype=np.uint8)
        crop_eval_mask = np.zeros_like(crop_intensity, dtype=np.uint8)
        livestock_eval_mask = np.zeros_like(livestock_intensity, dtype=np.uint8)

        for species_idx, species_name in enumerate(DOMESTICATES_CROP_NAMES):
            entry = resolve_manifest_entry(entries, "crop", species_name)
            normalized, presence, eval_mask = sample_domesticates_entry(
                entry,
                centroid_lat,
                centroid_lon,
                args.method,
                cell_count,
                base_dir,
            )
            crop_intensity[:, species_idx] = normalized
            crop_eval_mask[:, species_idx] = eval_mask
            crop_presence |= presence << species_idx
            print(summarize(f"crop.{species_name}.intensity", normalized))
            print(
                f"crop.{species_name}.presence: "
                f"true={int(np.count_nonzero(presence))}/{cell_count} "
                f"threshold={float(entry.get('presence_threshold', 0.2)):.3f}"
            )

        for species_idx, species_name in enumerate(DOMESTICATES_LIVESTOCK_NAMES):
            entry = resolve_manifest_entry(entries, "livestock", species_name)
            normalized, presence, eval_mask = sample_domesticates_entry(
                entry,
                centroid_lat,
                centroid_lon,
                args.method,
                cell_count,
                base_dir,
            )
            livestock_intensity[:, species_idx] = normalized
            livestock_eval_mask[:, species_idx] = eval_mask
            livestock_presence |= presence << species_idx
            print(summarize(f"livestock.{species_name}.intensity", normalized))
            print(
                f"livestock.{species_name}.presence: "
                f"true={int(np.count_nonzero(presence))}/{cell_count} "
                f"threshold={float(entry.get('presence_threshold', 0.2)):.3f}"
            )

        write_domesticates_ref_bin(
            output_path,
            crop_intensity,
            livestock_intensity,
            crop_presence,
            livestock_presence,
            crop_eval_mask,
            livestock_eval_mask,
        )
        print(f"manifest: {manifest_path}")
        print(f"WROTE {output_path}")
        print(f"CELL_COUNT {cell_count}")
        return

    if args.module == "glaciology-ref":
        if not args.ice_thickness:
            raise ValueError(
                "missing required arg for glaciology-ref module: --ice-thickness"
            )

        grid = load_input_grid(Path(args.ice_thickness), None)
        ice_thickness = interpolate_grid(
            grid=grid,
            query_lat=centroid_lat,
            query_lon=centroid_lon,
            method=args.method,
        ).astype(np.float32, copy=False)
        print(summarize("ice_thickness", ice_thickness))
        write_glaciology_ref_bin(output_path, ice_thickness)
        print(f"WROTE {output_path}")
        print(f"CELL_COUNT {len(ice_thickness)}")
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
    terrain_height = transform_terrain_height(sampled, args).astype(
        np.float32, copy=False
    )
    print(summarize("terrain_height", terrain_height))
    write_terrain_ref_bin(output_path, terrain_height)
    print(f"WROTE {output_path}")
    print(f"CELL_COUNT {len(terrain_height)}")


if __name__ == "__main__":
    main()

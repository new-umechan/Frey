use std::env;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::sim::geology_types::{GeologyInternal, GeologyParams};
use frey_wasm::world;

#[derive(Debug, Clone)]
struct GlaciologyRef {
    ice_thickness: Vec<f32>,
}

#[derive(Debug, Clone)]
struct ClimateRef {
    temperature: Vec<f32>,
    precipitation: Vec<f32>,
}

#[derive(Debug, Clone)]
struct TerrainRef {
    height: Vec<f32>,
}

#[derive(Clone, Copy)]
struct RegionSpec {
    id: &'static str,
    lat: f32,
    lon: f32,
}

struct RegionMetric {
    region_id: &'static str,
    valid_cells: usize,
    model_mean: f32,
    reference_mean: f32,
    rmse: f32,
    rho: f32,
    sle_contrib: f32,
}

struct BenchRunMetadata {
    run_id: String,
    repeat_index: Option<u32>,
    repeat_total: Option<u32>,
    git_commit: Option<String>,
    cache_fingerprint: String,
}

const GLACIOLOGY_MAGIC: &[u8; 8] = b"GLACREF1";
const TERRAIN_MAGIC: &[u8; 8] = b"TERRREF1";
const REGION_RADIUS_KM: f32 = 450.0;

const REGIONS: &[RegionSpec] = &[
    RegionSpec {
        id: "alaska",
        lat: 63.0,
        lon: -151.0,
    },
    RegionSpec {
        id: "western_canada_usa",
        lat: 52.0,
        lon: -125.0,
    },
    RegionSpec {
        id: "arctic_canada_north",
        lat: 75.0,
        lon: -90.0,
    },
    RegionSpec {
        id: "arctic_canada_south",
        lat: 67.0,
        lon: -72.0,
    },
    RegionSpec {
        id: "greenland_periphery",
        lat: 72.0,
        lon: -40.0,
    },
    RegionSpec {
        id: "iceland",
        lat: 65.0,
        lon: -19.0,
    },
    RegionSpec {
        id: "svalbard",
        lat: 78.0,
        lon: 20.0,
    },
    RegionSpec {
        id: "antarctic_subantarctic",
        lat: -75.0,
        lon: 0.0,
    },
    RegionSpec {
        id: "new_zealand",
        lat: -43.0,
        lon: 170.0,
    },
    RegionSpec {
        id: "southern_andes",
        lat: -49.0,
        lon: -73.0,
    },
    RegionSpec {
        id: "low_latitudes",
        lat: 0.0,
        lon: -78.0,
    },
    RegionSpec {
        id: "central_south_asia",
        lat: 34.0,
        lon: 78.0,
    },
    RegionSpec {
        id: "caucasus_middle_east",
        lat: 42.0,
        lon: 44.0,
    },
    RegionSpec {
        id: "central_europe",
        lat: 46.0,
        lon: 10.0,
    },
    RegionSpec {
        id: "north_asia",
        lat: 57.0,
        lon: 110.0,
    },
    RegionSpec {
        id: "russian_arctic",
        lat: 73.0,
        lon: 80.0,
    },
    RegionSpec {
        id: "scandinavia",
        lat: 67.0,
        lon: 20.0,
    },
];

fn main() {
    let geology_params = GeologyParams {
        level: 6,
        ..Default::default()
    };
    let mesh_level = geology_params.level;

    let seed = "earth";
    let run_id = env::var("GLACIOLOGY_SERIES_RUN_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(default_run_id);
    let repeat_index = parse_env_u32("GLACIOLOGY_SERIES_REPEAT_INDEX");
    let repeat_total = parse_env_u32("GLACIOLOGY_SERIES_REPEAT_TOTAL");
    let git_commit = env::var("GLACIOLOGY_SERIES_GIT_COMMIT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(resolve_git_commit);
    let horizon = env::var("GLACIOLOGY_SERIES_HORIZON")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "short".to_string());
    let tick_count = parse_env_u32("GLACIOLOGY_SERIES_TICKS")
        .map(|value| value.max(1) as usize)
        .unwrap_or(64);

    let (mut terrain, positions, nbr_offsets, nbrs) =
        sim::build_geology_with_mesh(seed, geology_params);

    let cell_count = positions.len();
    let (terrain_ref_path, terrain_ref) = match find_terrain_ref_cache_path() {
        Some(path) => match load_terrain_ref(&path) {
            Ok(reference) => (path, reference),
            Err(error) => {
                println!("=== Glaciology Sea-Level Series Bench ===");
                println!("-- Terrain Input: ERROR --");
                println!("{}", error);
                return;
            }
        },
        None => {
            println!("=== Glaciology Sea-Level Series Bench ===");
            println!("-- Terrain Input: SKIPPED (benches/data/terrain_ref.bin not found) --");
            return;
        }
    };
    if terrain_ref.height.len() != cell_count {
        println!(
            "-- Terrain Input: ERROR (cell_count mismatch: mesh={}, terrain_ref={}) --",
            cell_count,
            terrain_ref.height.len()
        );
        return;
    }

    let (climate_ref_path, climate_ref) = match find_climate_ref_cache_path() {
        Some(path) => match load_climate_ref(&path) {
            Ok(reference) => (path, reference),
            Err(error) => {
                println!("=== Glaciology Sea-Level Series Bench ===");
                println!("-- Climate Input: ERROR --");
                println!("{}", error);
                return;
            }
        },
        None => {
            println!("=== Glaciology Sea-Level Series Bench ===");
            println!("-- Climate Input: SKIPPED (benches/data/climate_ref.bin not found) --");
            return;
        }
    };
    if climate_ref.temperature.len() != cell_count || climate_ref.precipitation.len() != cell_count
    {
        println!(
            "-- Climate Input: ERROR (cell_count mismatch: mesh={}, temperature={}, precipitation={}) --",
            cell_count,
            climate_ref.temperature.len(),
            climate_ref.precipitation.len(),
        );
        return;
    }

    let modern_ref_path = find_glaciology_modern_ref_path();
    let modern_ref = modern_ref_path
        .as_ref()
        .and_then(|path| load_glaciology_ref(path).ok());
    let paleo_ref_path = env::var("GLACIOLOGY_SERIES_PALEO_REF_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);

    terrain.height = terrain_ref.height;
    let plate_id = terrain.plate_id.clone();
    let geology = world::GeologyState {
        height: terrain.height,
        lake_depth: vec![0.0; cell_count],
        plate_id,
        erosion_rate: vec![0.0; cell_count],
        deposition_rate: vec![0.0; cell_count],
        volcanism: terrain.volcanism,
        vertex_buoyancy: terrain.vertex_buoyancy,
        geology_internal: vec![GeologyInternal::default(); cell_count],
        boundary_condition: vec![0.0; cell_count],
    };

    let mesh = world::WorldMesh {
        positions,
        nbr_offsets,
        nbrs,
    };

    let mut sim_world = world::World::new(mesh, geology);
    sim_world.clock.epoch = world::EraKind::Environment;
    sim_world.clock.real_years_per_tick = world::EraKind::Environment.real_years_per_tick();
    sim_world.clock.runtime_tick_ms = world::EraKind::Environment.runtime_tick_ms();
    sim_world.clock.budgets = world::EraKind::Environment.budgets();
    sim_world.state.climate.temperature = climate_ref.temperature;
    sim_world.state.climate.precipitation = climate_ref.precipitation;
    sim_world.state.ecology.tree_cover.fill(0.5);
    sim_world.state.ecology.ground_cover.fill(0.5);

    let glaciology_budget = sim_world.clock.budgets.climate;
    let mut step_ms_series = Vec::<f32>::with_capacity(tick_count);
    let mut sea_level_series = Vec::<f32>::with_capacity(tick_count);
    for _ in 0..tick_count {
        let started_at = Instant::now();
        sim::run_glaciology_step_for_bench(&mut sim_world, glaciology_budget);
        let step_ms = (started_at.elapsed().as_secs_f64() * 1000.0) as f32;
        step_ms_series.push(step_ms);
        sea_level_series.push(sim_world.runtime.sea_level_offset);
    }

    let step_median_ms = median(&step_ms_series).unwrap_or(f32::NAN);
    let step_p95_ms = percentile(&step_ms_series, 0.95).unwrap_or(f32::NAN);
    let sea_level_start = sea_level_series.first().copied().unwrap_or(0.0);
    let sea_level_end = sea_level_series.last().copied().unwrap_or(0.0);
    let sea_level_mean = mean(&sea_level_series).unwrap_or(f32::NAN);
    let sea_level_min = sea_level_series
        .iter()
        .copied()
        .fold(f32::INFINITY, f32::min);
    let sea_level_max = sea_level_series
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);

    let final_ice = &sim_world.state.glaciology.ice_thickness;
    let land_ice_volume_km3 = approximate_land_ice_volume_km3(final_ice, &sim_world.state.geology.height);

    let (grid_spearman, grid_rmse) = if let Some(reference) = modern_ref.as_ref() {
        (
            spearman_on_land(
                final_ice,
                &reference.ice_thickness,
                &sim_world.state.geology.height,
            )
            .unwrap_or(f32::NAN),
            weighted_rmse_on_land(
                final_ice,
                &reference.ice_thickness,
                &sim_world.state.geology.height,
                &sim_world.mesh.positions,
            )
            .unwrap_or(f32::NAN),
        )
    } else {
        (f32::NAN, f32::NAN)
    };

    let region_metrics = if let Some(reference) = modern_ref.as_ref() {
        build_region_metrics(
            &sim_world,
            &reference.ice_thickness,
            &sim_world.state.glaciology.ice_thickness,
            sea_level_end,
        )
    } else {
        Vec::new()
    };

    println!("=== Glaciology Sea-Level Series Bench ===");
    println!("-- Horizon: {} --", horizon);
    println!("-- Tick Count: {} --", tick_count);
    println!("-- Terrain Source: {} --", terrain_ref_path.display());
    println!("-- Climate Source: {} --", climate_ref_path.display());
    println!(
        "-- Modern Ref Source: {} --",
        modern_ref_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!(
        "-- Paleo Ref Source: {} --",
        paleo_ref_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!("-- Runtime: median_ms={:.3} p95_ms={:.3} --", step_median_ms, step_p95_ms);
    println!(
        "-- Sea Level Offset: start={:.6} end={:.6} mean={:.6} min={:.6} max={:.6} --",
        sea_level_start, sea_level_end, sea_level_mean, sea_level_min, sea_level_max
    );
    println!(
        "-- Land Ice Volume (proxy km3): {:.3} --",
        land_ice_volume_km3
    );
    println!(
        "-- Grid Metrics: spearman={:.4} rmse={:.4} --",
        grid_spearman, grid_rmse
    );

    let run_metadata = BenchRunMetadata {
        run_id,
        repeat_index,
        repeat_total,
        git_commit,
        cache_fingerprint: build_cache_fingerprint(
            Some(terrain_ref_path.as_path()),
            Some(climate_ref_path.as_path()),
            modern_ref_path.as_deref(),
            paleo_ref_path.as_deref(),
        ),
    };

    if let Err(error) = append_score_record_jsonl(
        &run_metadata,
        &horizon,
        tick_count,
        seed,
        mesh_level,
        cell_count,
        step_median_ms,
        step_p95_ms,
        sea_level_start,
        sea_level_end,
        sea_level_mean,
        sea_level_min,
        sea_level_max,
        land_ice_volume_km3,
        grid_spearman,
        grid_rmse,
        &region_metrics,
        modern_ref_path.as_deref(),
        paleo_ref_path.as_deref(),
    ) {
        println!("-- Score Save: ERROR ({}) --", error);
    } else {
        println!("-- Score Save: OK --");
    }
}

fn approximate_land_ice_volume_km3(ice: &[f32], height: &[f32]) -> f32 {
    let cell_count = ice.len().min(height.len());
    if cell_count == 0 {
        return 0.0;
    }
    // Level-6 icosphere rough cell area estimate on Earth.
    let earth_surface_km2 = 510_072_000.0_f32;
    let cell_area_km2 = earth_surface_km2 / cell_count as f32;
    let mut volume = 0.0_f32;
    for i in 0..cell_count {
        if height[i] <= 0.0 {
            continue;
        }
        let thickness_m = ice[i].max(0.0);
        volume += thickness_m * 1e-3 * cell_area_km2;
    }
    volume
}

fn build_region_metrics(
    world: &world::World,
    reference: &[f32],
    model: &[f32],
    sea_level_end: f32,
) -> Vec<RegionMetric> {
    let mut out = Vec::<RegionMetric>::with_capacity(REGIONS.len());
    for region in REGIONS {
        let mut model_values = Vec::<f32>::new();
        let mut ref_values = Vec::<f32>::new();
        for (idx, pos) in world.mesh.positions.iter().enumerate() {
            let cell_lat = pos[1].clamp(-1.0, 1.0).asin().to_degrees();
            let cell_lon = pos[2].atan2(pos[0]).to_degrees();
            if haversine_km(cell_lat, cell_lon, region.lat, region.lon) > REGION_RADIUS_KM {
                continue;
            }
            if idx >= world.state.geology.height.len() || world.state.geology.height[idx] <= 0.0 {
                continue;
            }
            let model_value = *model.get(idx).unwrap_or(&f32::NAN);
            let ref_value = *reference.get(idx).unwrap_or(&f32::NAN);
            if !model_value.is_finite() || !ref_value.is_finite() {
                continue;
            }
            model_values.push(model_value);
            ref_values.push(ref_value);
        }

        let valid_cells = model_values.len();
        let model_mean = mean(&model_values).unwrap_or(f32::NAN);
        let reference_mean = mean(&ref_values).unwrap_or(f32::NAN);
        let rmse = rmse(&model_values, &ref_values).unwrap_or(f32::NAN);
        let rho = spearman(&model_values, &ref_values).unwrap_or(f32::NAN);
        let sle_contrib = if valid_cells > 0 {
            sea_level_end * (valid_cells as f32 / world.mesh.positions.len().max(1) as f32)
        } else {
            0.0
        };

        out.push(RegionMetric {
            region_id: region.id,
            valid_cells,
            model_mean,
            reference_mean,
            rmse,
            rho,
            sle_contrib,
        });
    }
    out
}

fn rmse(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut sum = 0.0_f32;
    for i in 0..a.len() {
        let diff = a[i] - b[i];
        sum += diff * diff;
    }
    Some((sum / a.len() as f32).sqrt())
}

fn weighted_rmse_on_land(
    model_field: &[f32],
    ref_field: &[f32],
    geology_height: &[f32],
    positions: &[[f32; 3]],
) -> Option<f32> {
    let len = model_field
        .len()
        .min(ref_field.len())
        .min(geology_height.len())
        .min(positions.len());
    if len < 3 {
        return None;
    }

    let mut weighted_sq = 0.0_f32;
    let mut weight_sum = 0.0_f32;
    for i in 0..len {
        if geology_height[i] <= 0.0 {
            continue;
        }
        let m = model_field[i];
        let r = ref_field[i];
        if !m.is_finite() || !r.is_finite() {
            continue;
        }
        let lat_rad = positions[i][1].clamp(-1.0, 1.0).asin();
        let weight = lat_rad.cos().abs().max(1e-6);
        let diff = m - r;
        weighted_sq += weight * diff * diff;
        weight_sum += weight;
    }

    if weight_sum <= 0.0 {
        None
    } else {
        Some((weighted_sq / weight_sum).sqrt())
    }
}

fn percentile(values: &[f32], p: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let clamped = p.clamp(0.0, 1.0);
    let index = ((sorted.len() as f32 * clamped).ceil() as isize - 1)
        .clamp(0, sorted.len() as isize - 1) as usize;
    sorted.get(index).copied()
}

fn median(values: &[f32]) -> Option<f32> {
    percentile(values, 0.5)
}

fn mean(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().copied().sum::<f32>() / values.len() as f32)
}

fn spearman_on_land(model_field: &[f32], ref_field: &[f32], geology_height: &[f32]) -> Option<f32> {
    let len = model_field
        .len()
        .min(ref_field.len())
        .min(geology_height.len());
    if len < 3 {
        return None;
    }

    let mut model_values = Vec::with_capacity(len);
    let mut ref_values = Vec::with_capacity(len);
    for i in 0..len {
        if geology_height[i] <= 0.0 {
            continue;
        }
        let model = model_field[i];
        let reference = ref_field[i];
        if !model.is_finite() || !reference.is_finite() {
            continue;
        }
        model_values.push(model);
        ref_values.push(reference);
    }
    if model_values.len() < 3 {
        return None;
    }
    spearman(&model_values, &ref_values)
}

fn spearman(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.len() < 3 {
        return None;
    }
    let rank_a = rank_with_ties(a);
    let rank_b = rank_with_ties(b);
    pearson_corr(&rank_a, &rank_b)
}

fn rank_with_ties(values: &[f32]) -> Vec<f32> {
    let mut indexed = values
        .iter()
        .copied()
        .enumerate()
        .collect::<Vec<(usize, f32)>>();
    indexed.sort_by(|(_, left), (_, right)| {
        left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut ranks = vec![0.0_f32; values.len()];
    let mut i = 0usize;
    while i < indexed.len() {
        let start = i;
        let value = indexed[i].1;
        while i < indexed.len() && indexed[i].1 == value {
            i += 1;
        }
        let end = i;
        let avg_rank = (start + 1 + end) as f32 / 2.0;
        for j in start..end {
            ranks[indexed[j].0] = avg_rank;
        }
    }
    ranks
}

fn pearson_corr(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let n = a.len() as f32;
    let mean_a = a.iter().copied().sum::<f32>() / n;
    let mean_b = b.iter().copied().sum::<f32>() / n;

    let mut numerator = 0.0_f32;
    let mut denom_a = 0.0_f32;
    let mut denom_b = 0.0_f32;
    for i in 0..a.len() {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        numerator += da * db;
        denom_a += da * da;
        denom_b += db * db;
    }
    let denom = (denom_a * denom_b).sqrt();
    if denom <= 1e-12 {
        return None;
    }
    Some((numerator / denom).clamp(-1.0, 1.0))
}

fn find_glaciology_modern_ref_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("GLACIOLOGY_SERIES_MODERN_REF_PATH") {
        if !path.trim().is_empty() {
            let custom = PathBuf::from(path);
            if custom.exists() {
                return Some(custom);
            }
        }
    }

    let candidates = [
        Path::new("benches/data/glaciology_ref.bin"),
        Path::new("../benches/data/glaciology_ref.bin"),
    ];
    candidates
        .iter()
        .find(|path| path.exists())
        .map(|path| (*path).to_path_buf())
}

fn find_climate_ref_cache_path() -> Option<PathBuf> {
    let candidates = [
        Path::new("benches/data/climate_ref.bin"),
        Path::new("../benches/data/climate_ref.bin"),
    ];
    candidates
        .iter()
        .find(|path| path.exists())
        .map(|path| (*path).to_path_buf())
}

fn find_terrain_ref_cache_path() -> Option<PathBuf> {
    let candidates = [
        Path::new("benches/data/terrain_ref.bin"),
        Path::new("../benches/data/terrain_ref.bin"),
    ];
    candidates
        .iter()
        .find(|path| path.exists())
        .map(|path| (*path).to_path_buf())
}

fn load_glaciology_ref(path: &Path) -> Result<GlaciologyRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    decode_glaciology_ref_custom(&mut reader)
        .map_err(|error| format!("failed to decode {}: {}", path.display(), error))
}

fn load_climate_ref(path: &Path) -> Result<ClimateRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    decode_climate_ref_custom(&mut reader)
        .map_err(|error| format!("failed to decode {}: {}", path.display(), error))
}

fn load_terrain_ref(path: &Path) -> Result<TerrainRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    decode_terrain_ref_custom(&mut reader)
        .map_err(|error| format!("failed to decode {}: {}", path.display(), error))
}

fn decode_climate_ref_custom<R: Read>(reader: &mut R) -> Result<ClimateRef, String> {
    let mut magic = [0_u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read magic: {}", error))?;
    if &magic != b"CLIMREF1" {
        return Err("invalid magic (expected CLIMREF1)".to_string());
    }

    let version = read_u32_le(reader)?;
    if version != 1 {
        return Err(format!("unsupported version: {}", version));
    }

    let cell_count = read_u64_le(reader)? as usize;
    let temperature = read_f32_vec(reader, cell_count)?;
    let precipitation = read_f32_vec(reader, cell_count)?;
    let _evapotranspiration = read_f32_vec(reader, cell_count)?;
    let _runoff = read_f32_vec(reader, cell_count)?;
    let _aridity = read_f32_vec(reader, cell_count)?;

    Ok(ClimateRef {
        temperature,
        precipitation,
    })
}

fn decode_glaciology_ref_custom<R: Read>(reader: &mut R) -> Result<GlaciologyRef, String> {
    let mut magic = [0_u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read magic: {}", error))?;
    if &magic != GLACIOLOGY_MAGIC {
        return Err("invalid magic (expected GLACREF1)".to_string());
    }

    let version = read_u32_le(reader)?;
    if version != 1 {
        return Err(format!("unsupported version: {}", version));
    }

    let cell_count = read_u64_le(reader)? as usize;
    let ice_thickness = read_f32_vec(reader, cell_count)?;

    Ok(GlaciologyRef { ice_thickness })
}

fn decode_terrain_ref_custom<R: Read>(reader: &mut R) -> Result<TerrainRef, String> {
    let mut magic = [0_u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read magic: {}", error))?;
    if &magic != TERRAIN_MAGIC {
        return Err("invalid magic (expected TERRREF1)".to_string());
    }

    let version = read_u32_le(reader)?;
    if version != 1 {
        return Err(format!("unsupported version: {}", version));
    }

    let cell_count = read_u64_le(reader)? as usize;
    let height = read_f32_vec(reader, cell_count)?;
    Ok(TerrainRef { height })
}

fn read_u32_le<R: Read>(reader: &mut R) -> Result<u32, String> {
    let mut bytes = [0_u8; 4];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("failed to read u32: {}", error))?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64_le<R: Read>(reader: &mut R) -> Result<u64, String> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("failed to read u64: {}", error))?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_f32_vec<R: Read>(reader: &mut R, len: usize) -> Result<Vec<f32>, String> {
    let mut bytes = vec![0_u8; len.saturating_mul(4)];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("failed to read f32 vec: {}", error))?;
    let mut values = Vec::with_capacity(len);
    for chunk in bytes.chunks_exact(4) {
        values.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Ok(values)
}

fn haversine_km(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f32 {
    let r = 6371.0_f32;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat * 0.5).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon * 0.5).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    r * c
}

fn score_output_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().unwrap_or(manifest_dir.as_path());
    repo_root.join("benches/results/glaciology_sea_level_series_scores.jsonl")
}

fn json_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn format_json_number(value: f32) -> String {
    if value.is_finite() {
        format!("{:.6}", value)
    } else {
        "null".to_string()
    }
}

fn parse_env_u32(key: &str) -> Option<u32> {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
}

fn default_run_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("run-{}", millis)
}

fn resolve_git_commit() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
        .filter(|value| !value.is_empty())
}

fn file_fingerprint_component(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            let len = metadata.len();
            let modified = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            format!("{}:{}:{}", path.display(), len, modified)
        }
        Err(_) => format!("{}:missing", path.display()),
    }
}

fn build_cache_fingerprint(
    terrain_ref: Option<&Path>,
    climate_ref: Option<&Path>,
    modern_ref: Option<&Path>,
    paleo_ref: Option<&Path>,
) -> String {
    let mut parts = Vec::<String>::new();
    if let Some(path) = terrain_ref {
        parts.push(file_fingerprint_component(path));
    }
    if let Some(path) = climate_ref {
        parts.push(file_fingerprint_component(path));
    }
    if let Some(path) = modern_ref {
        parts.push(file_fingerprint_component(path));
    }
    if let Some(path) = paleo_ref {
        parts.push(file_fingerprint_component(path));
    }
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join("|")
    }
}

#[allow(clippy::too_many_arguments)]
fn append_score_record_jsonl(
    run_metadata: &BenchRunMetadata,
    horizon: &str,
    tick_count: usize,
    seed: &str,
    mesh_level: u32,
    cell_count: usize,
    runtime_median_ms: f32,
    runtime_p95_ms: f32,
    sea_level_start: f32,
    sea_level_end: f32,
    sea_level_mean: f32,
    sea_level_min: f32,
    sea_level_max: f32,
    land_ice_volume_km3: f32,
    grid_spearman: f32,
    grid_rmse: f32,
    region_metrics: &[RegionMetric],
    modern_ref_path: Option<&Path>,
    paleo_ref_path: Option<&Path>,
) -> Result<(), String> {
    let timestamp_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system time error: {}", error))?
        .as_millis();

    let regions_json = {
        let mut rows = Vec::<String>::with_capacity(region_metrics.len());
        for metric in region_metrics {
            rows.push(format!(
                "{{\"region_id\":\"{}\",\"valid_cells\":{},\"model_mean\":{},\"reference_mean\":{},\"rmse\":{},\"rho\":{},\"sle_contrib\":{}}}",
                json_escape(metric.region_id),
                metric.valid_cells,
                format_json_number(metric.model_mean),
                format_json_number(metric.reference_mean),
                format_json_number(metric.rmse),
                format_json_number(metric.rho),
                format_json_number(metric.sle_contrib),
            ));
        }
        format!("[{}]", rows.join(","))
    };

    let line = format!(
        "{{\"schema_version\":1,\"timestamp_unix_ms\":{},\"bench\":\"glaciology_sea_level_series\",\"run_id\":\"{}\",\"repeat_index\":{},\"repeat_total\":{},\"git_commit\":{},\"cache_fingerprint\":\"{}\",\"horizon\":\"{}\",\"tick_count\":{},\"seed\":\"{}\",\"mesh_level\":{},\"cell_count\":{},\"runtime\":{{\"glaciology_step_ms_median\":{},\"glaciology_step_ms_p95\":{}}},\"metrics\":{{\"sle_mm\":{},\"sle_start_mm\":{},\"sle_mean_mm\":{},\"sle_min_mm\":{},\"sle_max_mm\":{},\"land_ice_volume_km3\":{},\"grid_spearman\":{},\"grid_rmse\":{},\"region_metrics\":{}}},\"references\":{{\"modern\":{},\"paleo\":{}}}}}\n",
        timestamp_unix_ms,
        json_escape(&run_metadata.run_id),
        run_metadata
            .repeat_index
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string()),
        run_metadata
            .repeat_total
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string()),
        run_metadata
            .git_commit
            .as_ref()
            .map(|value| format!("\"{}\"", json_escape(value)))
            .unwrap_or_else(|| "null".to_string()),
        json_escape(&run_metadata.cache_fingerprint),
        json_escape(horizon),
        tick_count,
        json_escape(seed),
        mesh_level,
        cell_count,
        format_json_number(runtime_median_ms),
        format_json_number(runtime_p95_ms),
        format_json_number(sea_level_end),
        format_json_number(sea_level_start),
        format_json_number(sea_level_mean),
        format_json_number(sea_level_min),
        format_json_number(sea_level_max),
        format_json_number(land_ice_volume_km3),
        format_json_number(grid_spearman),
        format_json_number(grid_rmse),
        regions_json,
        modern_ref_path
            .map(|value| format!("\"{}\"", json_escape(&value.display().to_string())))
            .unwrap_or_else(|| "null".to_string()),
        paleo_ref_path
            .map(|value| format!("\"{}\"", json_escape(&value.display().to_string())))
            .unwrap_or_else(|| "null".to_string()),
    );

    let output_path = score_output_path();
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {}", parent.display(), error))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&output_path)
        .map_err(|error| format!("failed to open {}: {}", output_path.display(), error))?;
    file.write_all(line.as_bytes())
        .map_err(|error| format!("failed to write {}: {}", output_path.display(), error))
}

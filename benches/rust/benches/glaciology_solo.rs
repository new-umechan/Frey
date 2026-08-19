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

#[derive(Clone, Copy)]
struct Region {
    id: &'static str,
    lat: f32,
    lon: f32,
}

#[derive(Clone, Copy)]
struct Assertion {
    id: &'static str,
    left: &'static str,
    right: &'static str,
    known_hard: bool,
}

struct AssertionOutcome {
    id: &'static str,
    left: &'static str,
    right: &'static str,
    left_value: f32,
    right_value: f32,
    passed: bool,
    known_hard: bool,
}

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

struct Phase1MetricSummary {
    matched: usize,
    total: usize,
    excluded_known_hard: usize,
    coverage_ratio: f32,
}

struct Phase2MetricResult {
    name: &'static str,
    rho: f32,
}

enum Phase2State {
    Ready {
        reference_path: PathBuf,
        metrics: Vec<Phase2MetricResult>,
    },
    Skipped,
    Error(String),
}

struct BenchRunMetadata {
    run_id: String,
    repeat_index: Option<u32>,
    repeat_total: Option<u32>,
    git_commit: Option<String>,
    cache_fingerprint: String,
}

const REGIONS: &[Region] = &[
    Region {
        id: "greenland_center",
        lat: 75.0,
        lon: -40.0,
    },
    Region {
        id: "antarctica_inland",
        lat: -80.0,
        lon: 0.0,
    },
    Region {
        id: "patagonia",
        lat: -50.0,
        lon: -73.0,
    },
    Region {
        id: "alaska_range",
        lat: 63.0,
        lon: -150.0,
    },
    Region {
        id: "himalaya_core",
        lat: 28.0,
        lon: 86.0,
    },
    Region {
        id: "karakoram",
        lat: 36.0,
        lon: 76.0,
    },
    Region {
        id: "alps",
        lat: 46.5,
        lon: 8.0,
    },
    Region {
        id: "rockies",
        lat: 51.0,
        lon: -116.0,
    },
    Region {
        id: "andes_tropical",
        lat: -8.0,
        lon: -77.0,
    },
    Region {
        id: "sahara",
        lat: 23.0,
        lon: 13.0,
    },
];

const ICE_THICKNESS_ASSERTIONS: &[Assertion] = &[
    Assertion {
        id: "ICE-01",
        left: "greenland_center",
        right: "himalaya_core",
        known_hard: false,
    },
    Assertion {
        id: "ICE-02",
        left: "antarctica_inland",
        right: "patagonia",
        known_hard: false,
    },
    Assertion {
        id: "ICE-03",
        left: "alps",
        right: "andes_tropical",
        known_hard: false,
    },
    Assertion {
        id: "ICE-04",
        left: "himalaya_core",
        right: "alps",
        known_hard: false,
    },
    Assertion {
        id: "ICE-05",
        left: "alaska_range",
        right: "rockies",
        known_hard: false,
    },
    Assertion {
        id: "ICE-06",
        left: "patagonia",
        right: "alaska_range",
        known_hard: false,
    },
    Assertion {
        id: "ICE-07",
        left: "karakoram",
        right: "andes_tropical",
        known_hard: false,
    },
    Assertion {
        id: "ICE-08",
        left: "greenland_center",
        right: "alps",
        known_hard: false,
    },
    Assertion {
        id: "ICE-09",
        left: "antarctica_inland",
        right: "himalaya_core",
        known_hard: false,
    },
    Assertion {
        id: "ICE-10",
        left: "greenland_center",
        right: "sahara",
        known_hard: false,
    },
];

const MELT_RUNOFF_ASSERTIONS: &[Assertion] = &[
    Assertion {
        id: "MELT-01",
        left: "alps",
        right: "greenland_center",
        known_hard: true,
    },
    Assertion {
        id: "MELT-02",
        left: "andes_tropical",
        right: "antarctica_inland",
        known_hard: true,
    },
    Assertion {
        id: "MELT-03",
        left: "patagonia",
        right: "himalaya_core",
        known_hard: true,
    },
];

const GLACIOLOGY_MAGIC: &[u8; 8] = b"GLACREF1";
const TERRAIN_MAGIC: &[u8; 8] = b"TERRREF1";

fn main() {
    let geology_params = GeologyParams {
        level: 6,
        ..Default::default()
    };
    let mesh_level = geology_params.level;

    let seed = "earth";
    let run_id = env::var("GLACIOLOGY_BENCH_RUN_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(default_run_id);
    let repeat_index = parse_env_u32("GLACIOLOGY_BENCH_REPEAT_INDEX");
    let repeat_total = parse_env_u32("GLACIOLOGY_BENCH_REPEAT_TOTAL");
    let git_commit = env::var("GLACIOLOGY_BENCH_GIT_COMMIT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(resolve_git_commit);
    let (mut terrain, positions, nbr_offsets, nbrs) =
        sim::build_geology_with_mesh(seed, geology_params);

    let cell_count = positions.len();
    let (terrain_ref_path, terrain_ref) = match find_terrain_ref_cache_path() {
        Some(path) => match load_terrain_ref(&path) {
            Ok(reference) => (path, reference),
            Err(error) => {
                println!("=== Glaciology Solo Bench ===");
                println!();
                println!("-- Terrain Input: ERROR --");
                println!("{}", error);
                return;
            }
        },
        None => {
            println!("=== Glaciology Solo Bench ===");
            println!();
            println!("-- Terrain Input: SKIPPED (benches/data/terrain_ref.bin not found) --");
            println!("To generate:");
            println!("  1) pnpm bench:dump-centroids");
            println!("  2) pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif");
            return;
        }
    };
    if terrain_ref.height.len() != cell_count {
        println!("=== Glaciology Solo Bench ===");
        println!();
        println!(
            "-- Terrain Input: ERROR (cell_count mismatch: mesh={}, terrain_ref={}) --",
            cell_count,
            terrain_ref.height.len()
        );
        return;
    }
    terrain.height = terrain_ref.height;

    let (climate_ref_path, climate_ref) = match find_climate_ref_cache_path() {
        Some(path) => match load_climate_ref(&path) {
            Ok(reference) => (path, reference),
            Err(error) => {
                println!("=== Glaciology Solo Bench ===");
                println!();
                println!("-- Climate Input: ERROR --");
                println!("{}", error);
                return;
            }
        },
        None => {
            println!("=== Glaciology Solo Bench ===");
            println!();
            println!("-- Climate Input: SKIPPED (benches/data/climate_ref.bin not found) --");
            println!("To generate:");
            println!("  1) pnpm bench:dump-centroids");
            println!("  2) pnpm bench:resample:climate -- --temperature <path> --precipitation <path> --evapotranspiration <path> --runoff <path> --aridity <path>");
            return;
        }
    };
    if climate_ref.temperature.len() != cell_count || climate_ref.precipitation.len() != cell_count
    {
        println!("=== Glaciology Solo Bench ===");
        println!();
        println!(
            "-- Climate Input: ERROR (cell_count mismatch: mesh={}, temperature={}, precipitation={}) --",
            cell_count,
            climate_ref.temperature.len(),
            climate_ref.precipitation.len(),
        );
        return;
    }

    let plate_id = terrain.plate_id.clone();

    let geology = world::GeologyState {
        height: terrain.height,
        lake_depth: vec![0.0; cell_count],
        plate_id,
        plate_emergence_regime: terrain.plate_emergence_regime,
        plate_emergence_fallback: terrain.plate_emergence_fallback,
        initial_plate_kinematics: terrain.initial_plate_kinematics,
        volcanism: terrain.volcanism,
        vertex_buoyancy: terrain.vertex_buoyancy,
        geology_internal: vec![GeologyInternal::default(); cell_count],
        boundary_condition: vec![0.0; cell_count],
        smoothing_limited_cells_ratio: 0.0,
        mean_smoothing_factor: 1.0,
        zero_mean_adjusted_cells_ratio: 0.0,
        zero_mean_mean_abs_correction: 0.0,
        zero_mean_std_delta: 0.0,
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
    let glaciology_started_at = Instant::now();
    sim::run_glaciology_step_for_bench(&mut sim_world, glaciology_budget);
    let glaciology_step_ms = glaciology_started_at.elapsed().as_secs_f64() * 1000.0;

    println!("=== Glaciology Solo Bench ===");
    println!("-- Terrain Source: {} --", terrain_ref_path.display());
    println!("-- Climate Source: {} --", climate_ref_path.display());
    println!(
        "-- Runtime Diagnostics: glaciology_step_ms={:.3} --",
        glaciology_step_ms
    );
    println!();
    println!("-- Main Evaluation: Spearman Correlation (land cells only) --");

    let phase2_state = match find_glaciology_ref_cache_path() {
        Some(path) => match load_glaciology_ref(&path) {
            Ok(reference) => {
                let results = evaluate_phase2(&sim_world, &reference);
                for metric in &results {
                    println!("{:<16} rho={:.3}", format!("{}:", metric.name), metric.rho);
                }
                println!();
                println!(
                    "-- Main Evaluation Summary: metrics_reported={} --",
                    results.len()
                );
                Phase2State::Ready {
                    reference_path: path,
                    metrics: results,
                }
            }
            Err(error) => {
                println!("ERROR    ({})", error);
                Phase2State::Error(error)
            }
        },
        None => {
            println!("SKIPPED  (benches/data/glaciology_ref.bin not found)");
            println!("To generate:");
            println!("  1) pnpm bench:dump-centroids");
            println!("  2) pnpm bench:prepare:glaciology -- --ice-thickness <path>");
            Phase2State::Skipped
        }
    };

    let selection = build_region_selection(&sim_world.mesh().positions);
    let ice_thickness_results = run_assertions(
        &selection,
        &sim_world.state.glaciology.ice_thickness,
        ICE_THICKNESS_ASSERTIONS,
    );
    let melt_runoff_results = run_assertions(
        &selection,
        &sim_world.state.glaciology.glacial_melt_runoff,
        MELT_RUNOFF_ASSERTIONS,
    );

    println!();
    println!("-- Diagnostic Evaluation: Ranking Assertions --");
    print_assertion_summary("ice_thickness", &ice_thickness_results);
    print_assertion_summary("glacial_melt_runoff", &melt_runoff_results);

    let known_hard = melt_runoff_results
        .iter()
        .filter(|outcome| outcome.known_hard)
        .collect::<Vec<_>>();
    if !known_hard.is_empty() {
        println!();
        println!("-- Known-Hard Assertions (reference only, not counted) --");
        for outcome in known_hard {
            let relation = if outcome.passed { "match" } else { "mismatch" };
            println!(
                "{}  {} > {}:  {}  ({:.4} vs {:.4})",
                outcome.id,
                outcome.left,
                outcome.right,
                relation,
                outcome.left_value,
                outcome.right_value,
            );
        }
    }

    let ice_thickness_summary = summarize_phase1_metric(&ice_thickness_results);
    let melt_runoff_summary = summarize_phase1_metric(&melt_runoff_results);

    println!();
    let mean_coverage_ratio =
        (ice_thickness_summary.coverage_ratio + melt_runoff_summary.coverage_ratio) / 2.0;
    println!(
        "-- Diagnostic Evaluation Summary: metrics=2 mean_coverage_ratio={:.3} (excl. known-hard) --",
        mean_coverage_ratio
    );

    match &phase2_state {
        Phase2State::Ready { .. } => println!("-- Main Evaluation State: READY --"),
        Phase2State::Skipped => println!("-- Main Evaluation State: SKIPPED --"),
        Phase2State::Error(_) => println!("-- Main Evaluation State: ERROR --"),
    }

    let glaciology_ref_fallback = match &phase2_state {
        Phase2State::Ready { .. } => None,
        Phase2State::Skipped | Phase2State::Error(_) => find_glaciology_ref_cache_path(),
    };
    let glaciology_ref_for_fingerprint = match &phase2_state {
        Phase2State::Ready { reference_path, .. } => Some(reference_path.as_path()),
        Phase2State::Skipped | Phase2State::Error(_) => glaciology_ref_fallback.as_deref(),
    };
    let run_metadata = BenchRunMetadata {
        run_id,
        repeat_index,
        repeat_total,
        git_commit,
        cache_fingerprint: build_cache_fingerprint(
            Some(terrain_ref_path.as_path()),
            Some(climate_ref_path.as_path()),
            glaciology_ref_for_fingerprint,
        ),
    };

    if let Err(error) = append_score_record_jsonl(
        &phase2_state,
        &run_metadata,
        glaciology_step_ms as f32,
        seed,
        mesh_level,
        cell_count,
        &ice_thickness_summary,
        &melt_runoff_summary,
    ) {
        println!("-- Score Save: ERROR ({}) --", error);
    } else {
        println!("-- Score Save: OK --");
    }
}

fn evaluate_phase2(world: &world::World, reference: &GlaciologyRef) -> Vec<Phase2MetricResult> {
    let glaciology = &world.state.glaciology;
    let geology_height = &world.state.geology.height;

    vec![evaluate_phase2_metric(
        "ice_thickness",
        &glaciology.ice_thickness,
        &reference.ice_thickness,
        geology_height,
    )]
}

fn evaluate_phase2_metric(
    name: &'static str,
    model_field: &[f32],
    ref_field: &[f32],
    geology_height: &[f32],
) -> Phase2MetricResult {
    let rho = spearman_on_land(model_field, ref_field, geology_height).unwrap_or(f32::NAN);
    Phase2MetricResult { name, rho }
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

fn find_glaciology_ref_cache_path() -> Option<PathBuf> {
    let candidates = [
        Path::new("benches/data/glaciology_ref.bin"),
        Path::new("../benches/data/glaciology_ref.bin"),
        Path::new("../../benches/data/glaciology_ref.bin"),
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
        Path::new("../../benches/data/climate_ref.bin"),
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
        Path::new("../../benches/data/terrain_ref.bin"),
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

fn build_region_selection(positions: &[[f32; 3]]) -> Vec<(&'static str, usize)> {
    REGIONS
        .iter()
        .map(|region| (region.id, nearest_cell(positions, region.lat, region.lon)))
        .collect::<Vec<_>>()
}

fn nearest_cell(positions: &[[f32; 3]], lat: f32, lon: f32) -> usize {
    positions
        .iter()
        .enumerate()
        .map(|(index, pos)| {
            let cell_lat = pos[1].clamp(-1.0, 1.0).asin().to_degrees();
            let cell_lon = pos[2].atan2(pos[0]).to_degrees();
            let dist = haversine_km(cell_lat, cell_lon, lat, lon);
            (index, dist)
        })
        .min_by(|(_, left), (_, right)| {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn haversine_km(lat_a: f32, lon_a: f32, lat_b: f32, lon_b: f32) -> f32 {
    let earth_radius_km = 6_371.0_f32;
    let lat1 = lat_a.to_radians();
    let lon1 = lon_a.to_radians();
    let lat2 = lat_b.to_radians();
    let lon2 = lon_b.to_radians();
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;

    let sin_dlat = (dlat * 0.5).sin();
    let sin_dlon = (dlon * 0.5).sin();
    let a = sin_dlat * sin_dlat + lat1.cos() * lat2.cos() * sin_dlon * sin_dlon;
    let c = 2.0 * a.clamp(0.0, 1.0).sqrt().asin();
    earth_radius_km * c
}

fn run_assertions(
    selection: &[(&'static str, usize)],
    field: &[f32],
    assertions: &[Assertion],
) -> Vec<AssertionOutcome> {
    assertions
        .iter()
        .map(|assertion| {
            let left_index = lookup_index(selection, assertion.left);
            let right_index = lookup_index(selection, assertion.right);
            let left_value = field.get(left_index).copied().unwrap_or(f32::NAN);
            let right_value = field.get(right_index).copied().unwrap_or(f32::NAN);
            let passed = left_value > right_value;

            AssertionOutcome {
                id: assertion.id,
                left: assertion.left,
                right: assertion.right,
                left_value,
                right_value,
                passed,
                known_hard: assertion.known_hard,
            }
        })
        .collect::<Vec<_>>()
}

fn lookup_index(selection: &[(&'static str, usize)], id: &str) -> usize {
    selection
        .iter()
        .find(|(region_id, _)| *region_id == id)
        .map(|(_, index)| *index)
        .unwrap_or(0)
}

fn summarize_phase1_metric(outcomes: &[AssertionOutcome]) -> Phase1MetricSummary {
    let matched = outcomes
        .iter()
        .filter(|outcome| !outcome.known_hard && outcome.passed)
        .count();
    let total = outcomes
        .iter()
        .filter(|outcome| !outcome.known_hard)
        .count();
    let excluded_known_hard = outcomes.iter().filter(|outcome| outcome.known_hard).count();
    let coverage_ratio = if total > 0 {
        (matched as f32) / (total as f32)
    } else {
        0.0
    };
    Phase1MetricSummary {
        matched,
        total,
        excluded_known_hard,
        coverage_ratio,
    }
}

fn print_assertion_summary(name: &str, outcomes: &[AssertionOutcome]) {
    let summary = summarize_phase1_metric(outcomes);
    if summary.excluded_known_hard > 0 {
        println!(
            "[{}] matched={}/{}  coverage_ratio={:.3}  (excl. {} known-hard)",
            name,
            summary.matched,
            summary.total,
            summary.coverage_ratio,
            summary.excluded_known_hard
        );
    } else {
        println!(
            "[{}] matched={}/{}  coverage_ratio={:.3}",
            name, summary.matched, summary.total, summary.coverage_ratio
        );
    }
}

fn score_output_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().unwrap_or(manifest_dir.as_path());
    repo_root.join("benches/results/glaciology_main_scores.jsonl")
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
    glaciology_ref: Option<&Path>,
) -> String {
    let mut parts = Vec::<String>::new();
    if let Some(path) = terrain_ref {
        parts.push(file_fingerprint_component(path));
    }
    if let Some(path) = climate_ref {
        parts.push(file_fingerprint_component(path));
    }
    if let Some(path) = glaciology_ref {
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
    phase2_state: &Phase2State,
    run_metadata: &BenchRunMetadata,
    glaciology_step_ms: f32,
    seed: &str,
    mesh_level: u32,
    cell_count: usize,
    ice_thickness_summary: &Phase1MetricSummary,
    melt_runoff_summary: &Phase1MetricSummary,
) -> Result<(), String> {
    let timestamp_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system time error: {}", error))?
        .as_millis();

    let (phase2_state_label, phase2_ref_path, phase2_error, metrics_json) = match phase2_state {
        Phase2State::Ready {
            reference_path,
            metrics,
        } => {
            let metric_value = |name: &str| -> String {
                metrics
                    .iter()
                    .find(|metric| metric.name == name)
                    .map(|metric| format_json_number(metric.rho))
                    .unwrap_or_else(|| "null".to_string())
            };
            (
                "ready",
                Some(reference_path.display().to_string()),
                None,
                format!("{{\"ice_thickness\":{}}}", metric_value("ice_thickness"),),
            )
        }
        Phase2State::Skipped => (
            "skipped",
            None,
            None,
            "{\"ice_thickness\":null}".to_string(),
        ),
        Phase2State::Error(error) => (
            "error",
            None,
            Some(error.clone()),
            "{\"ice_thickness\":null}".to_string(),
        ),
    };

    let line = format!(
        "{{\"schema_version\":1,\"timestamp_unix_ms\":{},\"bench\":\"glaciology_solo\",\"run_id\":\"{}\",\"repeat_index\":{},\"repeat_total\":{},\"git_commit\":{},\"cache_fingerprint\":\"{}\",\"seed\":\"{}\",\"mesh_level\":{},\"cell_count\":{},\"runtime\":{{\"glaciology_step_ms\":{}}},\"runtime_stats\":{{\"count\":1,\"median_ms\":{},\"p95_ms\":{}}},\"phase2\":{{\"state\":\"{}\",\"ref_path\":{},\"error\":{},\"metrics\":{}}},\"phase1\":{{\"ice_thickness\":{{\"matched\":{},\"total\":{},\"excluded_known_hard\":{},\"coverage_ratio\":{}}},\"glacial_melt_runoff\":{{\"matched\":{},\"total\":{},\"excluded_known_hard\":{},\"coverage_ratio\":{}}}}}}}\n",
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
        json_escape(seed),
        mesh_level,
        cell_count,
        format_json_number(glaciology_step_ms),
        format_json_number(glaciology_step_ms),
        format_json_number(glaciology_step_ms),
        phase2_state_label,
        phase2_ref_path
            .map(|value| format!("\"{}\"", json_escape(&value)))
            .unwrap_or_else(|| "null".to_string()),
        phase2_error
            .map(|value| format!("\"{}\"", json_escape(&value)))
            .unwrap_or_else(|| "null".to_string()),
        metrics_json,
        ice_thickness_summary.matched,
        ice_thickness_summary.total,
        ice_thickness_summary.excluded_known_hard,
        format_json_number(ice_thickness_summary.coverage_ratio),
        melt_runoff_summary.matched,
        melt_runoff_summary.total,
        melt_runoff_summary.excluded_known_hard,
        format_json_number(melt_runoff_summary.coverage_ratio),
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

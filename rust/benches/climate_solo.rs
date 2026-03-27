use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::sim::geology_types::GeologyParams;
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
struct ClimateRef {
    temperature: Vec<f32>,
    precipitation: Vec<f32>,
    evapotranspiration: Vec<f32>,
    runoff: Vec<f32>,
    aridity: Vec<f32>,
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

const REGIONS: &[Region] = &[
    Region {
        id: "sahara",
        lat: 23.0,
        lon: 13.0,
    },
    Region {
        id: "arabia",
        lat: 23.0,
        lon: 45.0,
    },
    Region {
        id: "amazon",
        lat: -3.0,
        lon: -60.0,
    },
    Region {
        id: "congo",
        lat: -1.0,
        lon: 24.0,
    },
    Region {
        id: "mediterranean",
        lat: 40.0,
        lon: 0.0,
    },
    Region {
        id: "monsoon_india",
        lat: 20.0,
        lon: 77.0,
    },
    Region {
        id: "maritime_europe",
        lat: 47.0,
        lon: 2.0,
    },
    Region {
        id: "siberia",
        lat: 62.0,
        lon: 105.0,
    },
    Region {
        id: "tropics_maritime",
        lat: 5.0,
        lon: 160.0,
    },
    Region {
        id: "andes_high",
        lat: -15.0,
        lon: -70.0,
    },
    Region {
        id: "arctic",
        lat: 80.0,
        lon: 0.0,
    },
    Region {
        id: "equator_africa",
        lat: 0.0,
        lon: 37.0,
    },
];

const TEMP_ASSERTIONS: &[Assertion] = &[
    Assertion {
        id: "T-01",
        left: "amazon",
        right: "arctic",
        known_hard: false,
    },
    Assertion {
        id: "T-02",
        left: "sahara",
        right: "siberia",
        known_hard: false,
    },
    Assertion {
        id: "T-03",
        left: "congo",
        right: "mediterranean",
        known_hard: false,
    },
    Assertion {
        id: "T-04",
        left: "mediterranean",
        right: "siberia",
        known_hard: false,
    },
    Assertion {
        id: "T-05",
        left: "amazon",
        right: "andes_high",
        known_hard: false,
    },
    Assertion {
        id: "T-06",
        left: "amazon",
        right: "equator_africa",
        known_hard: false,
    },
    Assertion {
        id: "T-07",
        left: "sahara",
        right: "arctic",
        known_hard: false,
    },
];

const PRECIP_ASSERTIONS: &[Assertion] = &[
    Assertion {
        id: "P-01",
        left: "amazon",
        right: "sahara",
        known_hard: false,
    },
    Assertion {
        id: "P-02",
        left: "congo",
        right: "arabia",
        known_hard: false,
    },
    Assertion {
        id: "P-03",
        left: "tropics_maritime",
        right: "sahara",
        known_hard: false,
    },
    Assertion {
        id: "P-04",
        left: "amazon",
        right: "siberia",
        known_hard: false,
    },
    Assertion {
        id: "P-05",
        left: "congo",
        right: "mediterranean",
        known_hard: false,
    },
    Assertion {
        id: "P-06",
        left: "maritime_europe",
        right: "siberia",
        known_hard: true,
    },
    Assertion {
        id: "P-07",
        left: "monsoon_india",
        right: "arabia",
        known_hard: true,
    },
];

const ARIDITY_ASSERTIONS: &[Assertion] = &[
    Assertion {
        id: "A-01",
        left: "sahara",
        right: "amazon",
        known_hard: false,
    },
    Assertion {
        id: "A-02",
        left: "arabia",
        right: "congo",
        known_hard: false,
    },
    Assertion {
        id: "A-03",
        left: "siberia",
        right: "amazon",
        known_hard: false,
    },
    Assertion {
        id: "A-04",
        left: "sahara",
        right: "mediterranean",
        known_hard: false,
    },
    Assertion {
        id: "A-05",
        left: "arabia",
        right: "tropics_maritime",
        known_hard: false,
    },
];

fn main() {
    let geology_params = GeologyParams {
        level: 6,
        ..Default::default()
    };
    let mesh_level = geology_params.level;

    let seed = "earth";
    let (mut terrain, positions, nbr_offsets, nbrs) =
        sim::build_geology_with_mesh(seed, geology_params);

    let cell_count = positions.len();
    let (terrain_ref_path, terrain_ref) = match find_terrain_ref_cache_path() {
        Some(path) => match load_terrain_ref(&path) {
            Ok(reference) => (path, reference),
            Err(error) => {
                println!("=== Climate Solo Bench ===");
                println!();
                println!("-- Terrain Input: ERROR --");
                println!("{}", error);
                return;
            }
        },
        None => {
            println!("=== Climate Solo Bench ===");
            println!();
            println!("-- Terrain Input: SKIPPED (benches/data/terrain_ref.bin not found) --");
            println!("To generate:");
            println!("  1) npm run bench:dump-centroids");
            println!("  2) npm run bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif");
            return;
        }
    };
    if terrain_ref.height.len() != cell_count {
        println!("=== Climate Solo Bench ===");
        println!();
        println!(
            "-- Terrain Input: ERROR (cell_count mismatch: mesh={}, terrain_ref={}) --",
            cell_count,
            terrain_ref.height.len()
        );
        return;
    }
    terrain.height = terrain_ref.height;

    let plate_id = terrain
        .plate_id
        .iter()
        .copied()
        .map(world::PlateId)
        .collect::<Vec<_>>();

    let geology = world::GeologyState {
        height: terrain.height,
        plate_id,
        erosion_rate: vec![0.0; cell_count],
        deposition_rate: vec![0.0; cell_count],
        volcanism: terrain.volcanism,
        vertex_buoyancy: terrain.vertex_buoyancy,
        geology_internal: vec![world::GeologyInternal::default(); cell_count],
        boundary_condition: vec![0.0; cell_count],
    };

    let mesh = world::WorldMesh {
        positions,
        nbr_offsets,
        nbrs,
    };

    let mut sim_world = world::World::new(mesh, geology);
    sim_world.state.ecology.tree_cover.fill(0.5);
    sim_world.state.ecology.ground_cover.fill(0.5);

    let climate_budget = world::EraKind::Environment.budgets().climate;
    sim::run_climate_step_for_bench(&mut sim_world, climate_budget);

    println!("=== Climate Solo Bench ===");
    println!("-- Terrain Source: {} --", terrain_ref_path.display());
    println!();
    println!("-- Main Evaluation: Spearman Correlation (land cells only) --");

    let phase2_state = match find_climate_ref_cache_path() {
        Some(path) => match load_climate_ref(&path) {
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
            println!("SKIPPED  (benches/data/climate_ref.bin not found)");
            println!("To generate:");
            println!("  1) npm run bench:dump-centroids");
            println!("  2) npm run bench:resample:climate -- --temperature <path> --precipitation <path> --evapotranspiration <path> --runoff <path> --aridity <path>");
            Phase2State::Skipped
        }
    };

    let selection = build_region_selection(&sim_world.mesh.positions);
    let temperature_results = run_assertions(
        &selection,
        &sim_world.state.climate.temperature,
        TEMP_ASSERTIONS,
    );
    let precipitation_results = run_assertions(
        &selection,
        &sim_world.state.climate.precipitation,
        PRECIP_ASSERTIONS,
    );
    let aridity_results = run_assertions(
        &selection,
        &sim_world.state.climate.aridity,
        ARIDITY_ASSERTIONS,
    );

    println!();
    println!("-- Diagnostic Evaluation: Ranking Assertions --");
    print_assertion_summary("temperature", &temperature_results);
    print_assertion_summary("precipitation", &precipitation_results);
    print_assertion_summary("aridity", &aridity_results);

    let known_hard = precipitation_results
        .iter()
        .filter(|outcome| outcome.known_hard)
        .collect::<Vec<_>>();
    if !known_hard.is_empty() {
        println!();
        println!("-- Known-Hard Assertions (reference only, not counted) --");
        for outcome in known_hard {
            let relation = if outcome.passed { "match" } else { "mismatch" };
            println!(
                "{}  {} > {}:  {}  ({:.1} vs {:.1})",
                outcome.id,
                outcome.left,
                outcome.right,
                relation,
                outcome.left_value,
                outcome.right_value,
            );
        }
    }

    let temperature_summary = summarize_phase1_metric(&temperature_results);
    let precipitation_summary = summarize_phase1_metric(&precipitation_results);
    let aridity_summary = summarize_phase1_metric(&aridity_results);

    println!();
    let mean_coverage_ratio = (
        temperature_summary.coverage_ratio
            + precipitation_summary.coverage_ratio
            + aridity_summary.coverage_ratio
    ) / 3.0;
    println!(
        "-- Diagnostic Evaluation Summary: metrics=3 mean_coverage_ratio={:.3} (excl. known-hard) --",
        mean_coverage_ratio
    );

    match &phase2_state {
        Phase2State::Ready { .. } => println!("-- Main Evaluation State: READY --"),
        Phase2State::Skipped => println!("-- Main Evaluation State: SKIPPED --"),
        Phase2State::Error(_) => println!("-- Main Evaluation State: ERROR --"),
    }

    if let Err(error) = append_score_record_jsonl(
        &phase2_state,
        seed,
        mesh_level,
        cell_count,
        &temperature_summary,
        &precipitation_summary,
        &aridity_summary,
    ) {
        println!("-- Score Save: ERROR ({}) --", error);
    } else {
        println!("-- Score Save: OK --");
    }
}

fn evaluate_phase2(world: &world::World, reference: &ClimateRef) -> Vec<Phase2MetricResult> {
    let climate = &world.state.climate;
    let geology_height = &world.state.geology.height;

    vec![
        evaluate_phase2_metric(
            "temperature",
            &climate.temperature,
            &reference.temperature,
            geology_height,
        ),
        evaluate_phase2_metric(
            "precipitation",
            &climate.precipitation,
            &reference.precipitation,
            geology_height,
        ),
        evaluate_phase2_metric(
            "aridity",
            &climate.aridity,
            &reference.aridity,
            geology_height,
        ),
        evaluate_phase2_metric(
            "evapotranspiration",
            &climate.evapotranspiration,
            &reference.evapotranspiration,
            geology_height,
        ),
        evaluate_phase2_metric("runoff", &climate.runoff, &reference.runoff, geology_height),
    ]
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
    const MAGIC: &[u8; 8] = b"CLIMREF1";
    let mut magic = [0_u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read magic: {}", error))?;
    if &magic != MAGIC {
        return Err("invalid magic (expected CLIMREF1)".to_string());
    }

    let version = read_u32_le(reader)?;
    if version != 1 {
        return Err(format!("unsupported version: {}", version));
    }

    let cell_count = read_u64_le(reader)? as usize;
    let temperature = read_f32_vec(reader, cell_count)?;
    let precipitation = read_f32_vec(reader, cell_count)?;
    let evapotranspiration = read_f32_vec(reader, cell_count)?;
    let runoff = read_f32_vec(reader, cell_count)?;
    let aridity = read_f32_vec(reader, cell_count)?;

    Ok(ClimateRef {
        temperature,
        precipitation,
        evapotranspiration,
        runoff,
        aridity,
    })
}

fn decode_terrain_ref_custom<R: Read>(reader: &mut R) -> Result<TerrainRef, String> {
    const MAGIC: &[u8; 8] = b"TERRREF1";
    let mut magic = [0_u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read magic: {}", error))?;
    if &magic != MAGIC {
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
            summary.excluded_known_hard,
            summary.coverage_ratio
        );
    } else {
        println!(
            "[{}] matched={}/{}  coverage_ratio={:.3}",
            name,
            summary.matched,
            summary.total,
            summary.coverage_ratio
        );
    }
}

fn score_output_path() -> PathBuf {
    let candidates = [
        Path::new("benches/results/climate_main_scores.jsonl"),
        Path::new("../benches/results/climate_main_scores.jsonl"),
    ];
    for candidate in candidates {
        if let Some(parent) = candidate.parent() {
            if parent.exists() {
                return candidate.to_path_buf();
            }
        }
    }
    candidates[0].to_path_buf()
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

fn append_score_record_jsonl(
    phase2_state: &Phase2State,
    seed: &str,
    mesh_level: u32,
    cell_count: usize,
    temperature_summary: &Phase1MetricSummary,
    precipitation_summary: &Phase1MetricSummary,
    aridity_summary: &Phase1MetricSummary,
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
                format!(
                    "{{\"temperature\":{},\"precipitation\":{},\"aridity\":{},\"evapotranspiration\":{},\"runoff\":{}}}",
                    metric_value("temperature"),
                    metric_value("precipitation"),
                    metric_value("aridity"),
                    metric_value("evapotranspiration"),
                    metric_value("runoff"),
                ),
            )
        }
        Phase2State::Skipped => (
            "skipped",
            None,
            None,
            "{\"temperature\":null,\"precipitation\":null,\"aridity\":null,\"evapotranspiration\":null,\"runoff\":null}".to_string(),
        ),
        Phase2State::Error(error) => (
            "error",
            None,
            Some(error.clone()),
            "{\"temperature\":null,\"precipitation\":null,\"aridity\":null,\"evapotranspiration\":null,\"runoff\":null}".to_string(),
        ),
    };

    let line = format!(
        "{{\"timestamp_unix_ms\":{},\"bench\":\"climate_solo\",\"seed\":\"{}\",\"mesh_level\":{},\"cell_count\":{},\"phase2\":{{\"state\":\"{}\",\"ref_path\":{},\"error\":{},\"metrics\":{}}},\"phase1\":{{\"temperature\":{{\"matched\":{},\"total\":{},\"excluded_known_hard\":{},\"coverage_ratio\":{}}},\"precipitation\":{{\"matched\":{},\"total\":{},\"excluded_known_hard\":{},\"coverage_ratio\":{}}},\"aridity\":{{\"matched\":{},\"total\":{},\"excluded_known_hard\":{},\"coverage_ratio\":{}}}}}}}\n",
        timestamp_unix_ms,
        json_escape(seed),
        mesh_level,
        cell_count,
        phase2_state_label,
        phase2_ref_path
            .map(|value| format!("\"{}\"", json_escape(&value)))
            .unwrap_or_else(|| "null".to_string()),
        phase2_error
            .map(|value| format!("\"{}\"", json_escape(&value)))
            .unwrap_or_else(|| "null".to_string()),
        metrics_json,
        temperature_summary.matched,
        temperature_summary.total,
        temperature_summary.excluded_known_hard,
        format_json_number(temperature_summary.coverage_ratio),
        precipitation_summary.matched,
        precipitation_summary.total,
        precipitation_summary.excluded_known_hard,
        format_json_number(precipitation_summary.coverage_ratio),
        aridity_summary.matched,
        aridity_summary.total,
        aridity_summary.excluded_known_hard,
        format_json_number(aridity_summary.coverage_ratio),
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

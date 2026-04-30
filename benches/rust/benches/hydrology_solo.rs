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
}

struct AssertionOutcome {
    id: &'static str,
    left: &'static str,
    right: &'static str,
    left_value: f32,
    right_value: f32,
    passed: bool,
}

#[derive(Debug, Clone)]
struct TerrainRef {
    height: Vec<f32>,
}

#[derive(Debug, Clone)]
struct HydroInputRef {
    runoff: Vec<f32>,
}

#[derive(Debug, Clone)]
struct HydroRef {
    river_flow: Vec<f32>,
    is_lake: Vec<u8>,
}

struct DiagnosticSummary {
    matched: usize,
    total: usize,
    coverage_ratio: f32,
}

struct FillSpillDiagnosticSummary {
    active_sink_count: usize,
    overflow_active_ratio: f32,
    mean_sink_fill_ratio: f32,
    ponded_cell_count: usize,
}

struct FlowMetricResult {
    rho: f32,
}

struct LakeMetricResult {
    precision: f32,
    recall: f32,
    f1: f32,
}

enum MainEvaluationState {
    Ready {
        reference_path: PathBuf,
        flow: FlowMetricResult,
        lake: LakeMetricResult,
    },
    Skipped,
    Error(String),
}

struct ReferenceSummary {
    mean: f32,
    p50: f32,
    p95: f32,
}

struct BenchRunMetadata {
    run_id: String,
    repeat_index: Option<u32>,
    repeat_total: Option<u32>,
    git_commit: Option<String>,
}

const DEFAULT_STABILIZATION_TICKS: usize = 8;
const DEFAULT_SAMPLE_TICKS: usize = 3;

const REGIONS: &[Region] = &[
    Region {
        id: "amazon_mouth",
        lat: -1.5,
        lon: -51.5,
    },
    Region {
        id: "congo_mouth",
        lat: -6.0,
        lon: 12.5,
    },
    Region {
        id: "mississippi_mouth",
        lat: 29.0,
        lon: -89.5,
    },
    Region {
        id: "yangtze_mouth",
        lat: 31.5,
        lon: 121.5,
    },
    Region {
        id: "nile_mouth",
        lat: 31.5,
        lon: 31.0,
    },
    Region {
        id: "sahara_interior",
        lat: 23.0,
        lon: 13.0,
    },
    Region {
        id: "himalaya_foothills",
        lat: 27.0,
        lon: 85.0,
    },
    Region {
        id: "ganges_delta",
        lat: 22.5,
        lon: 89.5,
    },
];

const FLOW_ASSERTIONS: &[Assertion] = &[
    Assertion {
        id: "R-01",
        left: "amazon_mouth",
        right: "congo_mouth",
    },
    Assertion {
        id: "R-02",
        left: "congo_mouth",
        right: "mississippi_mouth",
    },
    Assertion {
        id: "R-03",
        left: "amazon_mouth",
        right: "nile_mouth",
    },
    Assertion {
        id: "R-04",
        left: "himalaya_foothills",
        right: "sahara_interior",
    },
    Assertion {
        id: "R-05",
        left: "ganges_delta",
        right: "sahara_interior",
    },
];

fn main() {
    let geology_params = GeologyParams {
        level: 6,
        ..Default::default()
    };
    let mesh_level = geology_params.level;
    let seed = "earth";

    let run_id = env::var("HYDROLOGY_BENCH_RUN_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(default_run_id);
    let repeat_index = parse_env_u32("HYDROLOGY_BENCH_REPEAT_INDEX");
    let repeat_total = parse_env_u32("HYDROLOGY_BENCH_REPEAT_TOTAL");
    let git_commit = env::var("HYDROLOGY_BENCH_GIT_COMMIT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(resolve_git_commit);
    let stabilization_ticks = parse_env_usize("HYDROLOGY_BENCH_STABILIZATION_TICKS")
        .unwrap_or(DEFAULT_STABILIZATION_TICKS);
    let sample_ticks = parse_env_usize("HYDROLOGY_BENCH_SAMPLE_TICKS")
        .unwrap_or(DEFAULT_SAMPLE_TICKS)
        .max(1);

    let (mut terrain, positions, nbr_offsets, nbrs) =
        sim::build_geology_with_mesh(seed, geology_params.clone());
    let cell_count = positions.len();

    let (terrain_ref_path, terrain_ref) = match find_terrain_ref_cache_path() {
        Some(path) => match load_terrain_ref(&path) {
            Ok(reference) => (path, reference),
            Err(error) => {
                println!("=== Hydrology Solo Bench ===");
                println!();
                println!("-- Terrain Input: ERROR --");
                println!("{}", error);
                return;
            }
        },
        None => {
            println!("=== Hydrology Solo Bench ===");
            println!();
            println!("-- Terrain Input: SKIPPED (benches/data/terrain_ref.bin not found) --");
            println!("To generate:");
            println!("  1) pnpm bench:dump-centroids");
            println!("  2) pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif");
            return;
        }
    };
    if terrain_ref.height.len() != cell_count {
        println!("=== Hydrology Solo Bench ===");
        println!();
        println!(
            "-- Terrain Input: ERROR (cell_count mismatch: mesh={}, terrain_ref={}) --",
            cell_count,
            terrain_ref.height.len()
        );
        return;
    }
    terrain.height = terrain_ref.height;

    let (hydro_input_path, hydro_input) = match find_hydro_input_cache_path() {
        Some(path) => match load_hydro_input_ref(&path) {
            Ok(reference) => (path, reference),
            Err(error) => {
                println!("=== Hydrology Solo Bench ===");
                println!();
                println!("-- Hydro Input: ERROR --");
                println!("{}", error);
                return;
            }
        },
        None => {
            println!("=== Hydrology Solo Bench ===");
            println!();
            println!("-- Hydro Input: SKIPPED (benches/data/hydro_input.bin not found) --");
            println!("To generate:");
            println!("  pnpm bench:resample:hydro-input -- --runoff <path>");
            return;
        }
    };
    if hydro_input.runoff.len() != cell_count {
        println!("=== Hydrology Solo Bench ===");
        println!();
        println!(
            "-- Hydro Input: ERROR (cell_count mismatch: mesh={}, hydro_input={}) --",
            cell_count,
            hydro_input.runoff.len()
        );
        return;
    }

    let plate_id = terrain.plate_id.clone();
    let geology = world::GeologyState {
        height: terrain.height,
        lake_depth: vec![0.0; cell_count],
        plate_id,
        volcanism: terrain.volcanism,
        vertex_buoyancy: terrain.vertex_buoyancy,
        geology_internal: vec![crate::sim::geology_types::GeologyInternal::default(); cell_count],
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
    sim_world.state.climate.runoff = hydro_input.runoff;
    sim_world.state.hydrology.river_flow = terrain.river_flux;
    sim_world.state.hydrology.river_next = terrain.river_next;
    let mut erosion_state = sim::build_hydrology_state_for_bench(&sim_world, geology_params);
    let geology_budget = sim_world.clock.budgets.geology;
    let mut sampled_runtime_ms = Vec::with_capacity(sample_ticks);
    let total_ticks = stabilization_ticks + sample_ticks;
    for tick_index in 0..total_ticks {
        let hydro_started_at = Instant::now();
        sim::run_hydrology_step_with_state_for_bench(
            &mut sim_world,
            &mut erosion_state,
            geology_budget,
            true,
        );
        let elapsed_ms = hydro_started_at.elapsed().as_secs_f64() * 1000.0;
        if tick_index >= stabilization_ticks {
            sampled_runtime_ms.push(elapsed_ms as f32);
        }
        sim_world.clock.tick = sim_world.clock.tick.saturating_add(1);
    }
    let hydrology_step_ms = percentile_in_place(&mut sampled_runtime_ms, 0.95);
    let fill_spill_diagnostics = summarize_fill_spill_diagnostics(&sim_world);

    println!("=== Hydrology Solo Bench ===");
    println!("-- Terrain Source: {} --", terrain_ref_path.display());
    println!("-- Hydro Input Source: {} --", hydro_input_path.display());
    println!(
        "-- Runtime Diagnostics: hydrology_step_ms={:.3} stabilization_ticks={} sample_ticks={} --",
        hydrology_step_ms, stabilization_ticks, sample_ticks
    );
    println!();
    println!("-- Main Evaluation 1-A: river_flow Spearman (log scale, land cells only) --");

    let main_eval_state = match find_hydro_ref_cache_path() {
        Some(path) => match load_hydro_ref(&path) {
            Ok(reference) => {
                let flow = evaluate_flow_metric(&sim_world, &reference);
                println!("river_flow:  rho={:.3}", flow.rho);
                println!();
                println!("-- Main Evaluation 1-B: is_lake F1 (land cells only) --");
                let lake = evaluate_lake_metric(&sim_world, &reference);
                println!(
                    "precision={:.3}  recall={:.3}  f1={:.3}",
                    lake.precision, lake.recall, lake.f1
                );
                println!();
                println!("-- Main Evaluation 1-A Summary: metrics_reported=1 --");
                println!("-- Main Evaluation 1-B Summary: metrics_reported=3 --");
                MainEvaluationState::Ready {
                    reference_path: path,
                    flow,
                    lake,
                }
            }
            Err(error) => {
                println!("ERROR    ({})", error);
                MainEvaluationState::Error(error)
            }
        },
        None => {
            println!("SKIPPED  (benches/data/hydro_ref.bin not found)");
            println!("To generate:");
            println!("  pnpm bench:resample:hydro-ref -- --river-flow <path> --lakes <path>");
            println!();
            MainEvaluationState::Skipped
        }
    };

    println!("-- Main Evaluation 1-C: Reference Only --");
    let erosion_summary = summarize_land_values(
        &sim_world.state.hydrology.erosion_rate,
        &sim_world.state.geology.height,
    );
    let deposition_summary = summarize_land_values(
        &sim_world.state.hydrology.deposition_rate,
        &sim_world.state.geology.height,
    );
    println!(
        "erosion_rate:    mean={:.4}  p50={:.4}  p95={:.4}",
        erosion_summary.mean, erosion_summary.p50, erosion_summary.p95
    );
    println!(
        "deposition_rate: mean={:.4}  p50={:.4}  p95={:.4}",
        deposition_summary.mean, deposition_summary.p50, deposition_summary.p95
    );

    let selection = build_region_selection(&sim_world.mesh().positions);
    let flow_assertions = run_assertions(
        &selection,
        &sim_world.state.hydrology.river_flow,
        FLOW_ASSERTIONS,
    );

    println!();
    println!("-- Diagnostic Evaluation 2-A: River Flow Ranking Assertions --");
    for outcome in &flow_assertions {
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

    println!();
    println!("-- Diagnostic Evaluation 2-B: Representative Cell Values --");
    for (region_id, index) in &selection {
        let value = sim_world
            .state
            .hydrology
            .river_flow
            .get(*index)
            .copied()
            .unwrap_or(f32::NAN);
        println!("{}: river_flow={:.1}", region_id, value);
    }

    let diagnostic_summary = summarize_diagnostics(&flow_assertions);
    println!();
    println!(
        "-- Diagnostic Evaluation 2-A Summary: matched={}/{} coverage_ratio={:.3} --",
        diagnostic_summary.matched, diagnostic_summary.total, diagnostic_summary.coverage_ratio
    );
    println!(
        "-- Diagnostic Evaluation 2-C: Fill-Spill active_sink_count={} overflow_active_ratio={:.3} mean_sink_fill_ratio={:.3} ponded_cell_count={} --",
        fill_spill_diagnostics.active_sink_count,
        fill_spill_diagnostics.overflow_active_ratio,
        fill_spill_diagnostics.mean_sink_fill_ratio,
        fill_spill_diagnostics.ponded_cell_count,
    );

    match &main_eval_state {
        MainEvaluationState::Ready { .. } => println!("-- Main Evaluation State: READY --"),
        MainEvaluationState::Skipped => println!("-- Main Evaluation State: SKIPPED --"),
        MainEvaluationState::Error(_) => println!("-- Main Evaluation State: ERROR --"),
    }

    let run_metadata = BenchRunMetadata {
        run_id,
        repeat_index,
        repeat_total,
        git_commit,
    };

    if let Err(error) = append_score_record_jsonl(
        &main_eval_state,
        &run_metadata,
        seed,
        mesh_level,
        cell_count,
        &diagnostic_summary,
        &fill_spill_diagnostics,
        hydrology_step_ms as f32,
    ) {
        println!("-- Score Save: ERROR ({}) --", error);
    } else {
        println!("-- Score Save: OK --");
    }
}

fn evaluate_flow_metric(world: &world::World, reference: &HydroRef) -> FlowMetricResult {
    let rho = spearman_log_flow_on_land(
        &world.state.hydrology.river_flow,
        &reference.river_flow,
        &world.state.geology.height,
    )
    .unwrap_or(f32::NAN);
    FlowMetricResult { rho }
}

fn evaluate_lake_metric(world: &world::World, reference: &HydroRef) -> LakeMetricResult {
    let len = world
        .state
        .hydrology
        .is_lake
        .len()
        .min(reference.is_lake.len())
        .min(world.state.geology.height.len());
    let mut tp = 0.0_f32;
    let mut fp = 0.0_f32;
    let mut fnn = 0.0_f32;
    for i in 0..len {
        if world.state.geology.height[i] <= 0.0 {
            continue;
        }
        let pred = world.state.hydrology.is_lake[i];
        let truth = reference.is_lake[i] != 0;
        if pred && truth {
            tp += 1.0;
        } else if pred && !truth {
            fp += 1.0;
        } else if !pred && truth {
            fnn += 1.0;
        }
    }
    let precision = if tp + fp > 0.0 { tp / (tp + fp) } else { 0.0 };
    let recall = if tp + fnn > 0.0 { tp / (tp + fnn) } else { 0.0 };
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    LakeMetricResult {
        precision,
        recall,
        f1,
    }
}

fn spearman_log_flow_on_land(
    model_field: &[f32],
    ref_field: &[f32],
    geology_height: &[f32],
) -> Option<f32> {
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
        if model <= 0.0 || reference <= 0.0 {
            continue;
        }
        if !model.is_finite() || !reference.is_finite() {
            continue;
        }
        model_values.push(model.ln());
        ref_values.push(reference.ln());
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

fn summarize_land_values(values: &[f32], geology_height: &[f32]) -> ReferenceSummary {
    let len = values.len().min(geology_height.len());
    let mut selected = Vec::with_capacity(len);
    for i in 0..len {
        if geology_height[i] > 0.0 && values[i].is_finite() {
            selected.push(values[i]);
        }
    }
    if selected.is_empty() {
        return ReferenceSummary {
            mean: f32::NAN,
            p50: f32::NAN,
            p95: f32::NAN,
        };
    }
    selected.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let mean = selected.iter().copied().sum::<f32>() / (selected.len() as f32);
    let p50 = percentile_sorted(&selected, 0.50);
    let p95 = percentile_sorted(&selected, 0.95);
    ReferenceSummary { mean, p50, p95 }
}

fn percentile_sorted(values: &[f32], quantile: f32) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    if values.len() == 1 {
        return values[0];
    }
    let q = quantile.clamp(0.0, 1.0);
    let pos = q * ((values.len() - 1) as f32);
    let lower = pos.floor() as usize;
    let upper = pos.ceil() as usize;
    if lower == upper {
        return values[lower];
    }
    let weight = pos - (lower as f32);
    values[lower] * (1.0 - weight) + values[upper] * weight
}

fn percentile_in_place(values: &mut [f32], quantile: f32) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let q = quantile.clamp(0.0, 1.0);
    let index = (((values.len() - 1) as f32) * q).round() as usize;
    values[index.min(values.len() - 1)]
}

fn find_terrain_ref_cache_path() -> Option<PathBuf> {
    let candidates = [
        Path::new("data/terrain_ref.bin"),
        Path::new("../data/terrain_ref.bin"),
        Path::new("benches/data/terrain_ref.bin"),
        Path::new("../benches/data/terrain_ref.bin"),
        Path::new("../../benches/data/terrain_ref.bin"),
    ];
    candidates
        .iter()
        .find(|path| path.exists())
        .map(|path| (*path).to_path_buf())
}

fn find_hydro_input_cache_path() -> Option<PathBuf> {
    let candidates = [
        Path::new("data/hydro_input.bin"),
        Path::new("../data/hydro_input.bin"),
        Path::new("benches/data/hydro_input.bin"),
        Path::new("../benches/data/hydro_input.bin"),
        Path::new("../../benches/data/hydro_input.bin"),
    ];
    candidates
        .iter()
        .find(|path| path.exists())
        .map(|path| (*path).to_path_buf())
}

fn find_hydro_ref_cache_path() -> Option<PathBuf> {
    let candidates = [
        Path::new("data/hydro_ref.bin"),
        Path::new("../data/hydro_ref.bin"),
        Path::new("benches/data/hydro_ref.bin"),
        Path::new("../benches/data/hydro_ref.bin"),
        Path::new("../../benches/data/hydro_ref.bin"),
    ];
    candidates
        .iter()
        .find(|path| path.exists())
        .map(|path| (*path).to_path_buf())
}

fn load_terrain_ref(path: &Path) -> Result<TerrainRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    decode_terrain_ref_custom(&mut reader)
        .map_err(|error| format!("failed to decode {}: {}", path.display(), error))
}

fn load_hydro_input_ref(path: &Path) -> Result<HydroInputRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    decode_hydro_input_ref_custom(&mut reader)
        .map_err(|error| format!("failed to decode {}: {}", path.display(), error))
}

fn load_hydro_ref(path: &Path) -> Result<HydroRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    decode_hydro_ref_custom(&mut reader)
        .map_err(|error| format!("failed to decode {}: {}", path.display(), error))
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

fn decode_hydro_input_ref_custom<R: Read>(reader: &mut R) -> Result<HydroInputRef, String> {
    const MAGIC: &[u8; 9] = b"HYDINPUT1";
    let mut magic = [0_u8; 9];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read magic: {}", error))?;
    if &magic != MAGIC {
        return Err("invalid magic (expected HYDINPUT1)".to_string());
    }

    let version = read_u32_le(reader)?;
    if version != 1 {
        return Err(format!("unsupported version: {}", version));
    }

    let cell_count = read_u64_le(reader)? as usize;
    let runoff = read_f32_vec(reader, cell_count)?;
    Ok(HydroInputRef { runoff })
}

fn decode_hydro_ref_custom<R: Read>(reader: &mut R) -> Result<HydroRef, String> {
    const MAGIC: &[u8; 9] = b"HYDROREF1";
    let mut magic = [0_u8; 9];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read magic: {}", error))?;
    if &magic != MAGIC {
        return Err("invalid magic (expected HYDROREF1)".to_string());
    }

    let version = read_u32_le(reader)?;
    if version != 1 {
        return Err(format!("unsupported version: {}", version));
    }

    let cell_count = read_u64_le(reader)? as usize;
    let river_flow = read_f32_vec(reader, cell_count)?;
    let is_lake = read_u8_vec(reader, cell_count)?;
    Ok(HydroRef {
        river_flow,
        is_lake,
    })
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

fn read_u8_vec<R: Read>(reader: &mut R, len: usize) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0_u8; len];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("failed to read u8 vec: {}", error))?;
    Ok(bytes)
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

fn summarize_diagnostics(outcomes: &[AssertionOutcome]) -> DiagnosticSummary {
    let matched = outcomes.iter().filter(|outcome| outcome.passed).count();
    let total = outcomes.len();
    let coverage_ratio = if total > 0 {
        (matched as f32) / (total as f32)
    } else {
        0.0
    };
    DiagnosticSummary {
        matched,
        total,
        coverage_ratio,
    }
}

fn summarize_fill_spill_diagnostics(world: &world::World) -> FillSpillDiagnosticSummary {
    let hydrology = &world.state.hydrology;
    let active_sink_count = hydrology
        .sink_spill_cell
        .iter()
        .copied()
        .filter(|spill_cell| *spill_cell >= 0)
        .count();
    let overflow_active_count = hydrology
        .sink_overflow_active
        .iter()
        .copied()
        .filter(|active| *active != 0)
        .count();
    let overflow_active_ratio = if active_sink_count > 0 {
        overflow_active_count as f32 / active_sink_count as f32
    } else {
        0.0
    };

    let mut fill_sum = 0.0f32;
    let mut fill_count = 0usize;
    for sid in 0..hydrology.sink_capacity_total.len() {
        let total = hydrology.sink_capacity_total[sid];
        if !total.is_finite() || total <= 1e-6 {
            continue;
        }
        let remain = hydrology
            .sink_capacity_remaining
            .get(sid)
            .copied()
            .unwrap_or(total)
            .clamp(0.0, total);
        fill_sum += (1.0 - remain / total).clamp(0.0, 1.0);
        fill_count += 1;
    }
    let mean_sink_fill_ratio = if fill_count > 0 {
        fill_sum / fill_count as f32
    } else {
        0.0
    };
    let ponded_cell_count = hydrology
        .is_lake
        .iter()
        .copied()
        .filter(|value| *value)
        .count();

    FillSpillDiagnosticSummary {
        active_sink_count,
        overflow_active_ratio,
        mean_sink_fill_ratio,
        ponded_cell_count,
    }
}

fn default_run_id() -> String {
    format!(
        "default-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    )
}

fn parse_env_u32(key: &str) -> Option<u32> {
    env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| value.parse::<u32>().ok())
}

fn parse_env_usize(key: &str) -> Option<usize> {
    env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| value.parse::<usize>().ok())
}

fn resolve_git_commit() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|commit| commit.trim().to_string())
}

fn score_output_path() -> PathBuf {
    let candidates = [
        Path::new("benches/results/hydrology_main_scores.jsonl"),
        Path::new("../benches/results/hydrology_main_scores.jsonl"),
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
    main_eval_state: &MainEvaluationState,
    run_metadata: &BenchRunMetadata,
    seed: &str,
    mesh_level: u32,
    cell_count: usize,
    diagnostic_summary: &DiagnosticSummary,
    fill_spill_diagnostics: &FillSpillDiagnosticSummary,
    hydrology_step_ms: f32,
) -> Result<(), String> {
    let timestamp_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system time error: {}", error))?
        .as_millis();

    let (main_state_label, main_ref_path, main_error, metrics_json) = match main_eval_state {
        MainEvaluationState::Ready {
            reference_path,
            flow,
            lake,
        } => (
            "ready",
            Some(reference_path.display().to_string()),
            None,
            format!(
                "{{\"river_flow_rho\":{},\"is_lake_precision\":{},\"is_lake_recall\":{},\"is_lake_f1\":{},\"erosion_rate_spearman\":null}}",
                format_json_number(flow.rho),
                format_json_number(lake.precision),
                format_json_number(lake.recall),
                format_json_number(lake.f1),
            ),
        ),
        MainEvaluationState::Skipped => (
            "skipped",
            None,
            None,
            "{\"river_flow_rho\":null,\"is_lake_precision\":null,\"is_lake_recall\":null,\"is_lake_f1\":null,\"erosion_rate_spearman\":null}".to_string(),
        ),
        MainEvaluationState::Error(error) => (
            "error",
            None,
            Some(error.clone()),
            "{\"river_flow_rho\":null,\"is_lake_precision\":null,\"is_lake_recall\":null,\"is_lake_f1\":null,\"erosion_rate_spearman\":null}".to_string(),
        ),
    };

    let line = format!(
        "{{\"schema_version\":2,\"timestamp_unix_ms\":{},\"bench\":\"hydrology_solo\",\"run_id\":\"{}\",\"repeat_index\":{},\"repeat_total\":{},\"git_commit\":{},\"seed\":\"{}\",\"mesh_level\":{},\"cell_count\":{},\"runtime\":{{\"hydrology_step_ms\":{}}},\"runtime_stats\":{{\"count\":1,\"median_ms\":{},\"p95_ms\":{}}},\"phase2\":{{\"state\":\"{}\",\"ref_path\":{},\"error\":{},\"metrics\":{}}},\"phase1\":{{\"river_flow_ranking\":{{\"matched\":{},\"total\":{},\"excluded_known_hard\":0,\"coverage_ratio\":{}}}}},\"diagnostics\":{{\"fill_spill_stats\":{{\"active_sink_count\":{},\"overflow_active_ratio\":{},\"mean_sink_fill_ratio\":{},\"ponded_cell_count\":{}}}}}}}\n",
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
        json_escape(seed),
        mesh_level,
        cell_count,
        format_json_number(hydrology_step_ms),
        format_json_number(hydrology_step_ms),
        format_json_number(hydrology_step_ms),
        main_state_label,
        main_ref_path
            .map(|value| format!("\"{}\"", json_escape(&value)))
            .unwrap_or_else(|| "null".to_string()),
        main_error
            .map(|value| format!("\"{}\"", json_escape(&value)))
            .unwrap_or_else(|| "null".to_string()),
        metrics_json,
        diagnostic_summary.matched,
        diagnostic_summary.total,
        format_json_number(diagnostic_summary.coverage_ratio),
        fill_spill_diagnostics.active_sink_count,
        format_json_number(fill_spill_diagnostics.overflow_active_ratio),
        format_json_number(fill_spill_diagnostics.mean_sink_fill_ratio),
        fill_spill_diagnostics.ponded_cell_count,
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

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::sim::geology_types::{GeologyInternal, GeologyParams};
use frey_wasm::world;

const DEFAULT_STABILIZATION_TICKS: usize = 12;
const DEFAULT_SAMPLE_TICKS: usize = 10;

struct BenchRunMetadata {
    run_id: String,
    repeat_index: Option<u32>,
    repeat_total: Option<u32>,
    git_commit: Option<String>,
}

struct Phase2Metrics {
    sediment_budget_ratio: Option<f32>,
    coastal_deposition_share: Option<f32>,
    low_slope_deposition_share: Option<f32>,
}

struct Diagnostics {
    open_boundary_export_fraction: Option<f32>,
    erosion_reference_coverage: Option<f32>,
    lake_deposition_share: Option<f32>,
}

fn main() {
    let geology_params = GeologyParams {
        level: 6,
        ..Default::default()
    };
    let mesh_level = geology_params.level;
    let seed = env::var("GEOLOGY_BENCH_SEED")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "earth".to_string());
    let stabilization_ticks =
        parse_env_usize("GEOLOGY_BENCH_STABILIZATION_TICKS").unwrap_or(DEFAULT_STABILIZATION_TICKS);
    let sample_ticks = parse_env_usize("GEOLOGY_BENCH_SAMPLE_TICKS")
        .unwrap_or(DEFAULT_SAMPLE_TICKS)
        .max(1);
    let run_id = env::var("GEOLOGY_BENCH_RUN_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(default_run_id);
    let repeat_index = parse_env_u32("GEOLOGY_BENCH_REPEAT_INDEX");
    let repeat_total = parse_env_u32("GEOLOGY_BENCH_REPEAT_TOTAL");
    let git_commit = env::var("GEOLOGY_BENCH_GIT_COMMIT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(resolve_git_commit);

    let (terrain, positions, nbr_offsets, nbrs) =
        sim::build_geology_with_mesh(seed.as_str(), geology_params.clone());
    let cell_count = positions.len();
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

    let geology_budget = sim_world.clock.budgets.geology;
    let total_ticks = stabilization_ticks + sample_ticks;
    let mut sampled_runtime_ms = Vec::with_capacity(sample_ticks);
    let mut geology_state: sim::GeologyExecState = None;

    for tick_index in 0..total_ticks {
        let started_at = Instant::now();
        sim::run_geology_step_with_state_for_bench(
            &mut sim_world,
            &mut geology_state,
            geology_budget,
        );
        let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
        if tick_index >= stabilization_ticks {
            sampled_runtime_ms.push(elapsed_ms as f32);
        }
        sim_world.clock.tick = sim_world.clock.tick.saturating_add(1);
    }
    sim_world.refresh_terrain_state();

    let mut p50_samples = sampled_runtime_ms.clone();
    let geology_step_p50_ms = percentile_in_place(&mut p50_samples, 0.50);
    let geology_step_p95_ms = percentile_in_place(&mut sampled_runtime_ms, 0.95);
    let metrics = geology_state
        .as_ref()
        .map(|state| state.cached_metrics)
        .unwrap_or_default();
    let phase2_metrics = compute_phase2_metrics(&sim_world);
    let diagnostics = compute_diagnostics(&sim_world);
    let run_metadata = BenchRunMetadata {
        run_id,
        repeat_index,
        repeat_total,
        git_commit,
    };

    println!("=== Geology Solo Bench ===");
    println!("seed={}", seed);
    println!(
        "runtime: geology_step_p50_ms={:.3} geology_step_p95_ms={:.3} stabilization_ticks={} sample_ticks={}",
        geology_step_p50_ms,
        geology_step_p95_ms,
        stabilization_ticks,
        sample_ticks,
    );
    println!(
        "metrics: geology_activity={:.5} boundary_activity={:.5} uplift_rate={:.5} subsidence_rate={:.5}",
        metrics.geology_activity,
        metrics.boundary_activity,
        metrics.uplift_rate,
        metrics.subsidence_rate,
    );
    println!(
        "phase2: sediment_budget_ratio={} coastal_deposition_share={} low_slope_deposition_share={}",
        format_option_number(phase2_metrics.sediment_budget_ratio),
        format_option_number(phase2_metrics.coastal_deposition_share),
        format_option_number(phase2_metrics.low_slope_deposition_share),
    );
    println!(
        "diagnostics: open_boundary_export_fraction={} lake_deposition_share={}",
        format_option_number(diagnostics.open_boundary_export_fraction),
        format_option_number(diagnostics.lake_deposition_share),
    );

    if let Err(error) = append_score_record_jsonl(
        &run_metadata,
        seed.as_str(),
        mesh_level,
        cell_count,
        stabilization_ticks,
        sample_ticks,
        geology_step_p50_ms,
        geology_step_p95_ms,
        &phase2_metrics,
        &diagnostics,
    ) {
        println!("score_save=ERROR ({})", error);
    } else {
        println!("score_save=OK");
    }
}

fn parse_env_usize(key: &str) -> Option<usize> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<usize>().ok())
}

fn parse_env_u32(key: &str) -> Option<u32> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u32>().ok())
}

fn percentile_in_place(values: &mut [f32], percentile: f32) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    percentile_sorted(values, percentile)
}

fn percentile_sorted(values: &[f32], percentile: f32) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    let q = percentile.clamp(0.0, 1.0);
    let max_index = values.len().saturating_sub(1);
    let rank = (max_index as f32) * q;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower >= values.len() {
        return values[max_index];
    }
    if upper >= values.len() || lower == upper {
        return values[lower];
    }
    let t = rank - (lower as f32);
    values[lower] + (values[upper] - values[lower]) * t
}

fn compute_phase2_metrics(world: &world::World) -> Phase2Metrics {
    let mut erosion_sum = 0.0_f32;
    let mut deposition_sum = 0.0_f32;
    let mut coastal_deposition_sum = 0.0_f32;
    let mut low_slope_deposition_sum = 0.0_f32;
    let coastal = world.coastal_flags();
    let height = &world.state.geology.height;
    let deposition = &world.state.geology.deposition_rate;
    let erosion = &world.state.geology.erosion_rate;
    let shallow_sea_floor = world.control.geology_params.shallow_sea_floor;
    let mesh = world.mesh();
    for i in 0..height.len().min(erosion.len()).min(deposition.len()) {
        let er = erosion[i];
        let dep = deposition[i];
        if er.is_finite() && er > 0.0 {
            erosion_sum += er;
        }
        if dep.is_finite() && dep > 0.0 {
            deposition_sum += dep;
            let is_coastal = coastal.get(i).copied().unwrap_or(false);
            let h = height[i];
            let is_shallow_marine = h <= 0.0 && h >= shallow_sea_floor;
            if is_coastal || is_shallow_marine {
                coastal_deposition_sum += dep;
            }
            if slope_proxy(mesh, height, i) < 0.015 {
                low_slope_deposition_sum += dep;
            }
        }
    }
    let sediment_budget_ratio = if erosion_sum > 0.0 {
        Some(deposition_sum / erosion_sum)
    } else {
        None
    };
    let coastal_deposition_share = if deposition_sum > 0.0 {
        Some(coastal_deposition_sum / deposition_sum)
    } else {
        None
    };
    Phase2Metrics {
        sediment_budget_ratio,
        coastal_deposition_share,
        low_slope_deposition_share: if deposition_sum > 0.0 {
            Some(low_slope_deposition_sum / deposition_sum)
        } else {
            None
        },
    }
}

fn compute_diagnostics(world: &world::World) -> Diagnostics {
    let mut total_deposition = 0.0_f32;
    let mut lake_deposition = 0.0_f32;
    let deposition = &world.state.geology.deposition_rate;
    let lake_depth = &world.state.geology.lake_depth;
    let height = &world.state.geology.height;
    for i in 0..deposition.len().min(height.len()) {
        let dep = deposition[i];
        if !dep.is_finite() || dep <= 0.0 {
            continue;
        }
        total_deposition += dep;
        if lake_depth.get(i).copied().unwrap_or(0.0) > 0.0 {
            lake_deposition += dep;
        }
    }
    let open_boundary_export_fraction = if world.control.global_sediment_export > 0.0 {
        let export = world.control.global_sediment_export;
        Some(export / (export + total_deposition).max(1e-6))
    } else {
        Some(0.0)
    };
    Diagnostics {
        open_boundary_export_fraction,
        erosion_reference_coverage: Some(0.0),
        lake_deposition_share: if total_deposition > 0.0 {
            Some(lake_deposition / total_deposition)
        } else {
            None
        },
    }
}

fn slope_proxy(mesh: &world::WorldMesh, height: &[f32], index: usize) -> f32 {
    let center = height.get(index).copied().unwrap_or(0.0);
    let neighbors = mesh.cell_neighbors(index);
    if neighbors.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0_f32;
    let mut count = 0_u32;
    for &n_u32 in neighbors {
        let n = n_u32 as usize;
        if n >= height.len() {
            continue;
        }
        let diff = (center - height[n]).abs();
        if diff.is_finite() {
            sum += diff;
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f32
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

fn resolve_git_commit() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|commit| commit.trim().to_string())
}

fn score_output_path() -> PathBuf {
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let candidate = PathBuf::from(manifest_dir).join("../results/geology_main_scores.jsonl");
        if let Some(parent) = candidate.parent() {
            if parent.exists() {
                return candidate;
            }
        }
    }
    let candidates = [
        Path::new("benches/results/geology_main_scores.jsonl"),
        Path::new("results/geology_main_scores.jsonl"),
        Path::new("../benches/results/geology_main_scores.jsonl"),
        Path::new("../results/geology_main_scores.jsonl"),
        Path::new("../../benches/results/geology_main_scores.jsonl"),
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

fn format_json_number(value: Option<f32>) -> String {
    if let Some(numeric) = value {
        if numeric.is_finite() {
            return format!("{:.6}", numeric);
        }
    }
    "null".to_string()
}

fn format_option_number(value: Option<f32>) -> String {
    if let Some(numeric) = value {
        if numeric.is_finite() {
            return format!("{:.6}", numeric);
        }
    }
    "n/a".to_string()
}

#[allow(clippy::too_many_arguments)]
fn append_score_record_jsonl(
    run_metadata: &BenchRunMetadata,
    seed: &str,
    mesh_level: u32,
    cell_count: usize,
    stabilization_ticks: usize,
    sample_ticks: usize,
    geology_step_p50_ms: f32,
    geology_step_p95_ms: f32,
    phase2_metrics: &Phase2Metrics,
    diagnostics: &Diagnostics,
) -> Result<(), String> {
    let timestamp_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system time error: {}", error))?
        .as_millis();

    let line = format!(
        "{{\"schema_version\":1,\"timestamp_unix_ms\":{},\"bench\":\"geology_solo\",\"run_id\":\"{}\",\"repeat_index\":{},\"repeat_total\":{},\"git_commit\":{},\"seed\":\"{}\",\"mesh_level\":{},\"cell_count\":{},\"runtime\":{{\"geology_step_p50_ms\":{},\"geology_step_p95_ms\":{},\"stabilization_ticks\":{},\"sample_ticks\":{}}},\"phase2\":{{\"state\":\"ready\",\"metrics\":{{\"sediment_budget_ratio\":{},\"coastal_deposition_share\":{},\"low_slope_deposition_share\":{}}}}},\"diagnostics\":{{\"open_boundary_export_fraction\":{},\"erosion_reference_coverage\":{},\"lake_deposition_share\":{}}}}}\n",
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
        format_json_number(Some(geology_step_p50_ms)),
        format_json_number(Some(geology_step_p95_ms)),
        stabilization_ticks,
        sample_ticks,
        format_json_number(phase2_metrics.sediment_budget_ratio),
        format_json_number(phase2_metrics.coastal_deposition_share),
        format_json_number(phase2_metrics.low_slope_deposition_share),
        format_json_number(diagnostics.open_boundary_export_fraction),
        format_json_number(diagnostics.erosion_reference_coverage),
        format_json_number(diagnostics.lake_deposition_share),
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

use std::env;
use std::collections::VecDeque;
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

struct PlateShapeSummary {
    plate_count: usize,
    max_area_ratio: f32,
    effective_plate_count: f32,
    multi_component_plate_count: usize,
    max_component_count: usize,
    mean_detached_fragment_ratio: f32,
    max_detached_fragment_ratio: f32,
    mean_boundary_complexity: f32,
    max_boundary_complexity: f32,
    mean_elongation: f32,
    max_elongation: f32,
    mean_narrow_connection_cell_ratio: f32,
    max_narrow_connection_cell_ratio: f32,
    area_ge_1pct_plate_count: usize,
    area_ge_1pct_p95_boundary_complexity: f32,
    area_ge_1pct_p99_boundary_complexity: f32,
    area_ge_1pct_p95_elongation: f32,
    area_ge_1pct_p99_elongation: f32,
    area_ge_1pct_p95_narrow_connection_cell_ratio: f32,
    area_ge_1pct_p99_narrow_connection_cell_ratio: f32,
    top8_plate_count: usize,
    top8_p95_boundary_complexity: f32,
    top8_p99_boundary_complexity: f32,
    top8_p95_elongation: f32,
    top8_p99_elongation: f32,
    top8_p95_narrow_connection_cell_ratio: f32,
    top8_p99_narrow_connection_cell_ratio: f32,
}

#[derive(Clone)]
struct PlateMetricRow {
    area_ratio: f32,
    boundary_complexity: f32,
    elongation: f32,
    narrow_connection_cell_ratio: f32,
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
    let plate_shape_initial = compute_plate_shape_summary(&sim_world);

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
    let plate_shape = compute_plate_shape_summary(&sim_world);
    let run_metadata = BenchRunMetadata {
        run_id,
        repeat_index,
        repeat_total,
        git_commit,
    };

    println!("=== Geology Validation Solo Bench ===");
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
    println!(
        "plate_shape: plate_count={} multi_component_plate_count={} max_component_count={} max_area_ratio={:.6} effective_plate_count={:.6}",
        plate_shape.plate_count,
        plate_shape.multi_component_plate_count,
        plate_shape.max_component_count,
        plate_shape.max_area_ratio,
        plate_shape.effective_plate_count,
    );
    println!(
        "plate_shape: max_detached_fragment_ratio={:.6} max_boundary_complexity={:.6} max_elongation={:.6} max_narrow_connection_cell_ratio={:.6}",
        plate_shape.max_detached_fragment_ratio,
        plate_shape.max_boundary_complexity,
        plate_shape.max_elongation,
        plate_shape.max_narrow_connection_cell_ratio,
    );
    println!(
        "plate_shape: top8_plate_count={} top8_p99_elongation={:.6} top8_p99_narrow_connection_cell_ratio={:.6} top8_p99_boundary_complexity={:.6}",
        plate_shape.top8_plate_count,
        plate_shape.top8_p99_elongation,
        plate_shape.top8_p99_narrow_connection_cell_ratio,
        plate_shape.top8_p99_boundary_complexity,
    );
    println!(
        "plate_shape_initial: plate_count={} multi_component_plate_count={} max_component_count={} max_area_ratio={:.6} effective_plate_count={:.6}",
        plate_shape_initial.plate_count,
        plate_shape_initial.multi_component_plate_count,
        plate_shape_initial.max_component_count,
        plate_shape_initial.max_area_ratio,
        plate_shape_initial.effective_plate_count,
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
        &plate_shape_initial,
        &plate_shape,
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
    let deposition = &world.state.hydrology.deposition_rate;
    let erosion = &world.state.hydrology.erosion_rate;
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
    let deposition = &world.state.hydrology.deposition_rate;
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

fn compute_plate_shape_summary(world: &world::World) -> PlateShapeSummary {
    let plate_id = &world.state.geology.plate_id;
    let mesh = world.mesh();
    let cell_count = plate_id.len().min(mesh.positions.len());
    if cell_count == 0 || mesh.nbr_offsets.len() < cell_count + 1 {
        return PlateShapeSummary {
            plate_count: 0,
            max_area_ratio: 0.0,
            effective_plate_count: 0.0,
            multi_component_plate_count: 0,
            max_component_count: 0,
            mean_detached_fragment_ratio: 0.0,
            max_detached_fragment_ratio: 0.0,
            mean_boundary_complexity: 0.0,
            max_boundary_complexity: 0.0,
            mean_elongation: 0.0,
            max_elongation: 0.0,
            mean_narrow_connection_cell_ratio: 0.0,
            max_narrow_connection_cell_ratio: 0.0,
            area_ge_1pct_plate_count: 0,
            area_ge_1pct_p95_boundary_complexity: 0.0,
            area_ge_1pct_p99_boundary_complexity: 0.0,
            area_ge_1pct_p95_elongation: 0.0,
            area_ge_1pct_p99_elongation: 0.0,
            area_ge_1pct_p95_narrow_connection_cell_ratio: 0.0,
            area_ge_1pct_p99_narrow_connection_cell_ratio: 0.0,
            top8_plate_count: 0,
            top8_p95_boundary_complexity: 0.0,
            top8_p99_boundary_complexity: 0.0,
            top8_p95_elongation: 0.0,
            top8_p99_elongation: 0.0,
            top8_p95_narrow_connection_cell_ratio: 0.0,
            top8_p99_narrow_connection_cell_ratio: 0.0,
        };
    }

    let plate_count = plate_id
        .iter()
        .take(cell_count)
        .map(|id| id.as_usize())
        .max()
        .map(|max_id| max_id + 1)
        .unwrap_or(0);
    if plate_count == 0 {
        return PlateShapeSummary {
            plate_count: 0,
            max_area_ratio: 0.0,
            effective_plate_count: 0.0,
            multi_component_plate_count: 0,
            max_component_count: 0,
            mean_detached_fragment_ratio: 0.0,
            max_detached_fragment_ratio: 0.0,
            mean_boundary_complexity: 0.0,
            max_boundary_complexity: 0.0,
            mean_elongation: 0.0,
            max_elongation: 0.0,
            mean_narrow_connection_cell_ratio: 0.0,
            max_narrow_connection_cell_ratio: 0.0,
            area_ge_1pct_plate_count: 0,
            area_ge_1pct_p95_boundary_complexity: 0.0,
            area_ge_1pct_p99_boundary_complexity: 0.0,
            area_ge_1pct_p95_elongation: 0.0,
            area_ge_1pct_p99_elongation: 0.0,
            area_ge_1pct_p95_narrow_connection_cell_ratio: 0.0,
            area_ge_1pct_p99_narrow_connection_cell_ratio: 0.0,
            top8_plate_count: 0,
            top8_p95_boundary_complexity: 0.0,
            top8_p99_boundary_complexity: 0.0,
            top8_p95_elongation: 0.0,
            top8_p99_elongation: 0.0,
            top8_p95_narrow_connection_cell_ratio: 0.0,
            top8_p99_narrow_connection_cell_ratio: 0.0,
        };
    }

    let mut cell_counts = vec![0usize; plate_count];
    let mut boundary_contacts = vec![0usize; plate_count];
    let mut narrow_cells = vec![0usize; plate_count];
    let mut first_cell = vec![None::<usize>; plate_count];

    for i in 0..cell_count {
        let plate = plate_id[i].as_usize();
        if plate >= plate_count {
            continue;
        }
        cell_counts[plate] += 1;
        if first_cell[plate].is_none() {
            first_cell[plate] = Some(i);
        }
        let mut same_plate_neighbors = 0usize;
        for &n_u32 in mesh.cell_neighbors(i) {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            if plate_id[n] == plate_id[i] {
                same_plate_neighbors += 1;
            } else {
                boundary_contacts[plate] += 1;
            }
        }
        if same_plate_neighbors <= 2 {
            narrow_cells[plate] += 1;
        }
    }

    let mut component_counts = vec![0usize; plate_count];
    let mut largest_component_sizes = vec![0usize; plate_count];
    compute_components(
        mesh,
        plate_id,
        cell_count,
        &mut component_counts,
        &mut largest_component_sizes,
    );

    let mut area_square_sum = 0.0_f32;
    let mut max_area_ratio = 0.0_f32;
    let mut multi_component_plate_count = 0usize;
    let mut max_component_count = 0usize;
    let mut detached_sum = 0.0_f32;
    let mut max_detached = 0.0_f32;
    let mut boundary_complexity_sum = 0.0_f32;
    let mut max_boundary_complexity = 0.0_f32;
    let mut elongation_sum = 0.0_f32;
    let mut max_elongation = 0.0_f32;
    let mut narrow_ratio_sum = 0.0_f32;
    let mut max_narrow_ratio = 0.0_f32;
    let mut metric_rows = Vec::new();
    let mut populated_plate_count = 0usize;

    for plate in 0..plate_count {
        let cells = cell_counts[plate];
        if cells == 0 {
            continue;
        }
        populated_plate_count += 1;
        let area_ratio = cells as f32 / cell_count as f32;
        max_area_ratio = max_area_ratio.max(area_ratio);
        area_square_sum += area_ratio * area_ratio;

        let components = component_counts[plate];
        max_component_count = max_component_count.max(components);
        if components > 1 {
            multi_component_plate_count += 1;
        }

        let detached = 1.0 - (largest_component_sizes[plate] as f32 / cells as f32);
        detached_sum += detached;
        max_detached = max_detached.max(detached);

        let boundary_complexity = boundary_contacts[plate] as f32 / (cells as f32).sqrt().max(1.0);
        boundary_complexity_sum += boundary_complexity;
        max_boundary_complexity = max_boundary_complexity.max(boundary_complexity);

        let elongation = approximate_plate_elongation(mesh, plate_id, cell_count, plate, cells);
        elongation_sum += elongation;
        max_elongation = max_elongation.max(elongation);

        let narrow_ratio = narrow_cells[plate] as f32 / cells as f32;
        narrow_ratio_sum += narrow_ratio;
        max_narrow_ratio = max_narrow_ratio.max(narrow_ratio);
        metric_rows.push(PlateMetricRow {
            area_ratio,
            boundary_complexity,
            elongation,
            narrow_connection_cell_ratio: narrow_ratio,
        });
    }

    let denom = populated_plate_count.max(1) as f32;
    let area_ge_1pct = scoped_percentiles(
        metric_rows
            .iter()
            .filter(|row| row.area_ratio >= 0.01)
            .cloned()
            .collect(),
    );
    let mut rows_by_area = metric_rows;
    rows_by_area.sort_by(|left, right| {
        right
            .area_ratio
            .partial_cmp(&left.area_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top8 = scoped_percentiles(rows_by_area.into_iter().take(8).collect());
    PlateShapeSummary {
        plate_count: populated_plate_count,
        max_area_ratio,
        effective_plate_count: if area_square_sum > 0.0 {
            1.0 / area_square_sum
        } else {
            0.0
        },
        multi_component_plate_count,
        max_component_count,
        mean_detached_fragment_ratio: detached_sum / denom,
        max_detached_fragment_ratio: max_detached,
        mean_boundary_complexity: boundary_complexity_sum / denom,
        max_boundary_complexity,
        mean_elongation: elongation_sum / denom,
        max_elongation,
        mean_narrow_connection_cell_ratio: narrow_ratio_sum / denom,
        max_narrow_connection_cell_ratio: max_narrow_ratio,
        area_ge_1pct_plate_count: area_ge_1pct.plate_count,
        area_ge_1pct_p95_boundary_complexity: area_ge_1pct.p95_boundary_complexity,
        area_ge_1pct_p99_boundary_complexity: area_ge_1pct.p99_boundary_complexity,
        area_ge_1pct_p95_elongation: area_ge_1pct.p95_elongation,
        area_ge_1pct_p99_elongation: area_ge_1pct.p99_elongation,
        area_ge_1pct_p95_narrow_connection_cell_ratio: area_ge_1pct
            .p95_narrow_connection_cell_ratio,
        area_ge_1pct_p99_narrow_connection_cell_ratio: area_ge_1pct
            .p99_narrow_connection_cell_ratio,
        top8_plate_count: top8.plate_count,
        top8_p95_boundary_complexity: top8.p95_boundary_complexity,
        top8_p99_boundary_complexity: top8.p99_boundary_complexity,
        top8_p95_elongation: top8.p95_elongation,
        top8_p99_elongation: top8.p99_elongation,
        top8_p95_narrow_connection_cell_ratio: top8.p95_narrow_connection_cell_ratio,
        top8_p99_narrow_connection_cell_ratio: top8.p99_narrow_connection_cell_ratio,
    }
}

struct ScopedPercentiles {
    plate_count: usize,
    p95_boundary_complexity: f32,
    p99_boundary_complexity: f32,
    p95_elongation: f32,
    p99_elongation: f32,
    p95_narrow_connection_cell_ratio: f32,
    p99_narrow_connection_cell_ratio: f32,
}

fn scoped_percentiles(rows: Vec<PlateMetricRow>) -> ScopedPercentiles {
    let mut boundary = rows
        .iter()
        .map(|row| row.boundary_complexity)
        .collect::<Vec<_>>();
    let mut elongation = rows.iter().map(|row| row.elongation).collect::<Vec<_>>();
    let mut narrow = rows
        .iter()
        .map(|row| row.narrow_connection_cell_ratio)
        .collect::<Vec<_>>();
    ScopedPercentiles {
        plate_count: rows.len(),
        p95_boundary_complexity: percentile_in_place_or_zero(&mut boundary.clone(), 0.95),
        p99_boundary_complexity: percentile_in_place_or_zero(&mut boundary, 0.99),
        p95_elongation: percentile_in_place_or_zero(&mut elongation.clone(), 0.95),
        p99_elongation: percentile_in_place_or_zero(&mut elongation, 0.99),
        p95_narrow_connection_cell_ratio: percentile_in_place_or_zero(&mut narrow.clone(), 0.95),
        p99_narrow_connection_cell_ratio: percentile_in_place_or_zero(&mut narrow, 0.99),
    }
}

fn percentile_in_place_or_zero(values: &mut [f32], percentile: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    percentile_in_place(values, percentile)
}

fn compute_components(
    mesh: &world::WorldMesh,
    plate_id: &[sim::geology_types::PlateId],
    cell_count: usize,
    component_counts: &mut [usize],
    largest_component_sizes: &mut [usize],
) {
    let mut visited = vec![false; cell_count];
    let mut queue = VecDeque::<usize>::new();

    for start in 0..cell_count {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let plate = plate_id[start].as_usize();
        if plate >= component_counts.len() {
            continue;
        }

        let mut size = 0usize;
        queue.push_back(start);
        while let Some(cell) = queue.pop_front() {
            size += 1;
            for &n_u32 in mesh.cell_neighbors(cell) {
                let n = n_u32 as usize;
                if n >= cell_count || visited[n] || plate_id[n].as_usize() != plate {
                    continue;
                }
                visited[n] = true;
                queue.push_back(n);
            }
        }

        component_counts[plate] += 1;
        largest_component_sizes[plate] = largest_component_sizes[plate].max(size);
    }
}

fn approximate_plate_elongation(
    mesh: &world::WorldMesh,
    plate_id: &[sim::geology_types::PlateId],
    cell_count: usize,
    plate: usize,
    plate_cells: usize,
) -> f32 {
    let start = match plate_id
        .iter()
        .take(cell_count)
        .position(|id| id.as_usize() == plate)
    {
        Some(index) => index,
        None => return 0.0,
    };
    let far = farthest_plate_cell(mesh, plate_id, cell_count, plate, start).unwrap_or(start);
    let opposite = farthest_plate_cell(mesh, plate_id, cell_count, plate, far).unwrap_or(far);
    let diameter = angular_distance(mesh.positions[far], mesh.positions[opposite]);
    let area_proxy = (plate_cells as f32 / cell_count as f32) * (4.0 * std::f32::consts::PI);
    diameter / area_proxy.sqrt().max(1e-6)
}

fn farthest_plate_cell(
    mesh: &world::WorldMesh,
    plate_id: &[sim::geology_types::PlateId],
    cell_count: usize,
    plate: usize,
    from: usize,
) -> Option<usize> {
    let origin = *mesh.positions.get(from)?;
    let mut best = None::<usize>;
    let mut best_distance = -1.0_f32;
    for i in 0..cell_count {
        if plate_id[i].as_usize() != plate {
            continue;
        }
        let distance = angular_distance(origin, mesh.positions[i]);
        if distance > best_distance {
            best_distance = distance;
            best = Some(i);
        }
    }
    best
}

fn angular_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dot = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
    dot.acos()
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
        let candidate =
            PathBuf::from(manifest_dir).join("../results/geology_validation_main_scores.jsonl");
        if let Some(parent) = candidate.parent() {
            if parent.exists() {
                return candidate;
            }
        }
    }
    let candidates = [
        Path::new("benches/results/geology_validation_main_scores.jsonl"),
        Path::new("results/geology_validation_main_scores.jsonl"),
        Path::new("../benches/results/geology_validation_main_scores.jsonl"),
        Path::new("../results/geology_validation_main_scores.jsonl"),
        Path::new("../../benches/results/geology_validation_main_scores.jsonl"),
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
    plate_shape_initial: &PlateShapeSummary,
    plate_shape: &PlateShapeSummary,
) -> Result<(), String> {
    let timestamp_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system time error: {}", error))?
        .as_millis();

    let line = format!(
        "{{\"schema_version\":1,\"timestamp_unix_ms\":{},\"bench\":\"geology_validation_solo\",\"run_id\":\"{}\",\"repeat_index\":{},\"repeat_total\":{},\"git_commit\":{},\"seed\":\"{}\",\"mesh_level\":{},\"cell_count\":{},\"runtime\":{{\"geology_step_p50_ms\":{},\"geology_step_p95_ms\":{},\"stabilization_ticks\":{},\"sample_ticks\":{}}},\"phase2\":{{\"state\":\"ready\",\"metrics\":{{\"sediment_budget_ratio\":{},\"coastal_deposition_share\":{},\"low_slope_deposition_share\":{}}}}},\"diagnostics\":{{\"open_boundary_export_fraction\":{},\"erosion_reference_coverage\":{},\"lake_deposition_share\":{}}},\"plate_shape_initial\":{{\"plate_count\":{},\"max_area_ratio\":{},\"effective_plate_count\":{},\"multi_component_plate_count\":{},\"max_component_count\":{},\"mean_detached_fragment_ratio\":{},\"max_detached_fragment_ratio\":{},\"mean_boundary_complexity\":{},\"max_boundary_complexity\":{},\"mean_elongation\":{},\"max_elongation\":{},\"mean_narrow_connection_cell_ratio\":{},\"max_narrow_connection_cell_ratio\":{},\"area_ge_1pct_plate_count\":{},\"area_ge_1pct_p95_boundary_complexity\":{},\"area_ge_1pct_p99_boundary_complexity\":{},\"area_ge_1pct_p95_elongation\":{},\"area_ge_1pct_p99_elongation\":{},\"area_ge_1pct_p95_narrow_connection_cell_ratio\":{},\"area_ge_1pct_p99_narrow_connection_cell_ratio\":{},\"top8_plate_count\":{},\"top8_p95_boundary_complexity\":{},\"top8_p99_boundary_complexity\":{},\"top8_p95_elongation\":{},\"top8_p99_elongation\":{},\"top8_p95_narrow_connection_cell_ratio\":{},\"top8_p99_narrow_connection_cell_ratio\":{}}},\"plate_shape\":{{\"plate_count\":{},\"max_area_ratio\":{},\"effective_plate_count\":{},\"multi_component_plate_count\":{},\"max_component_count\":{},\"mean_detached_fragment_ratio\":{},\"max_detached_fragment_ratio\":{},\"mean_boundary_complexity\":{},\"max_boundary_complexity\":{},\"mean_elongation\":{},\"max_elongation\":{},\"mean_narrow_connection_cell_ratio\":{},\"max_narrow_connection_cell_ratio\":{},\"area_ge_1pct_plate_count\":{},\"area_ge_1pct_p95_boundary_complexity\":{},\"area_ge_1pct_p99_boundary_complexity\":{},\"area_ge_1pct_p95_elongation\":{},\"area_ge_1pct_p99_elongation\":{},\"area_ge_1pct_p95_narrow_connection_cell_ratio\":{},\"area_ge_1pct_p99_narrow_connection_cell_ratio\":{},\"top8_plate_count\":{},\"top8_p95_boundary_complexity\":{},\"top8_p99_boundary_complexity\":{},\"top8_p95_elongation\":{},\"top8_p99_elongation\":{},\"top8_p95_narrow_connection_cell_ratio\":{},\"top8_p99_narrow_connection_cell_ratio\":{}}}}}\n",
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
        plate_shape_initial.plate_count,
        format_json_number(Some(plate_shape_initial.max_area_ratio)),
        format_json_number(Some(plate_shape_initial.effective_plate_count)),
        plate_shape_initial.multi_component_plate_count,
        plate_shape_initial.max_component_count,
        format_json_number(Some(plate_shape_initial.mean_detached_fragment_ratio)),
        format_json_number(Some(plate_shape_initial.max_detached_fragment_ratio)),
        format_json_number(Some(plate_shape_initial.mean_boundary_complexity)),
        format_json_number(Some(plate_shape_initial.max_boundary_complexity)),
        format_json_number(Some(plate_shape_initial.mean_elongation)),
        format_json_number(Some(plate_shape_initial.max_elongation)),
        format_json_number(Some(plate_shape_initial.mean_narrow_connection_cell_ratio)),
        format_json_number(Some(plate_shape_initial.max_narrow_connection_cell_ratio)),
        plate_shape_initial.area_ge_1pct_plate_count,
        format_json_number(Some(
            plate_shape_initial.area_ge_1pct_p95_boundary_complexity,
        )),
        format_json_number(Some(
            plate_shape_initial.area_ge_1pct_p99_boundary_complexity,
        )),
        format_json_number(Some(plate_shape_initial.area_ge_1pct_p95_elongation)),
        format_json_number(Some(plate_shape_initial.area_ge_1pct_p99_elongation)),
        format_json_number(Some(
            plate_shape_initial.area_ge_1pct_p95_narrow_connection_cell_ratio,
        )),
        format_json_number(Some(
            plate_shape_initial.area_ge_1pct_p99_narrow_connection_cell_ratio,
        )),
        plate_shape_initial.top8_plate_count,
        format_json_number(Some(plate_shape_initial.top8_p95_boundary_complexity)),
        format_json_number(Some(plate_shape_initial.top8_p99_boundary_complexity)),
        format_json_number(Some(plate_shape_initial.top8_p95_elongation)),
        format_json_number(Some(plate_shape_initial.top8_p99_elongation)),
        format_json_number(Some(
            plate_shape_initial.top8_p95_narrow_connection_cell_ratio,
        )),
        format_json_number(Some(
            plate_shape_initial.top8_p99_narrow_connection_cell_ratio,
        )),
        plate_shape.plate_count,
        format_json_number(Some(plate_shape.max_area_ratio)),
        format_json_number(Some(plate_shape.effective_plate_count)),
        plate_shape.multi_component_plate_count,
        plate_shape.max_component_count,
        format_json_number(Some(plate_shape.mean_detached_fragment_ratio)),
        format_json_number(Some(plate_shape.max_detached_fragment_ratio)),
        format_json_number(Some(plate_shape.mean_boundary_complexity)),
        format_json_number(Some(plate_shape.max_boundary_complexity)),
        format_json_number(Some(plate_shape.mean_elongation)),
        format_json_number(Some(plate_shape.max_elongation)),
        format_json_number(Some(plate_shape.mean_narrow_connection_cell_ratio)),
        format_json_number(Some(plate_shape.max_narrow_connection_cell_ratio)),
        plate_shape.area_ge_1pct_plate_count,
        format_json_number(Some(plate_shape.area_ge_1pct_p95_boundary_complexity)),
        format_json_number(Some(plate_shape.area_ge_1pct_p99_boundary_complexity)),
        format_json_number(Some(plate_shape.area_ge_1pct_p95_elongation)),
        format_json_number(Some(plate_shape.area_ge_1pct_p99_elongation)),
        format_json_number(Some(
            plate_shape.area_ge_1pct_p95_narrow_connection_cell_ratio,
        )),
        format_json_number(Some(
            plate_shape.area_ge_1pct_p99_narrow_connection_cell_ratio,
        )),
        plate_shape.top8_plate_count,
        format_json_number(Some(plate_shape.top8_p95_boundary_complexity)),
        format_json_number(Some(plate_shape.top8_p99_boundary_complexity)),
        format_json_number(Some(plate_shape.top8_p95_elongation)),
        format_json_number(Some(plate_shape.top8_p99_elongation)),
        format_json_number(Some(plate_shape.top8_p95_narrow_connection_cell_ratio)),
        format_json_number(Some(plate_shape.top8_p99_narrow_connection_cell_ratio)),
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

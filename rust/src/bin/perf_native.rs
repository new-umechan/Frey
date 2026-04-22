use std::collections::BTreeMap;
use std::time::Instant;

use frey_wasm::sim;
use frey_wasm::sim::erosion::ErosionAutomatonState;
use frey_wasm::sim::world::{FeedbackQueue, World};
use frey_wasm::GeologyParams;
use serde::Serialize;

const DEFAULT_TICKS: u32 = 16;
const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_SEED: &str = "alpha";
const DEFAULT_SAMPLE_INTERVAL: u32 = 4;

#[derive(Debug)]
struct Args {
    ticks: u32,
    level: u32,
    seed: String,
    sample_interval: u32,
}

#[derive(Clone, Serialize)]
struct MetricStats {
    count: u32,
    mean: f64,
    min: f64,
    max: f64,
    p50: f64,
    p95: f64,
    p99: f64,
}

#[derive(Serialize)]
struct ModuleExecDiagnosticsSummary {
    exec_time_ms_total: f64,
    exec_time_share_of_exec_world: f64,
}

#[derive(Serialize)]
struct HydrologyDiagnosticsSummary {
    exec_time_ms_total: f64,
    exec_time_share_of_exec_world: f64,
    river_network_rebuild_count_total: u32,
    river_rebuild_rate: f64,
    river_fallback_count_total: u32,
    sink_rebuild_full_count_total: u32,
    sink_rebuild_partial_count_total: u32,
    sink_rebuild_skipped_count_total: u32,
    sink_rebuild_fallback_full_count_total: u32,
    sink_validation_fail_count_total: u32,
    sink_affected_ratio_mean: f64,
}

#[derive(Serialize)]
struct DiagnosticsModulesSummary {
    geology: ModuleExecDiagnosticsSummary,
    climate: ModuleExecDiagnosticsSummary,
    hydrology: HydrologyDiagnosticsSummary,
}

#[derive(Serialize)]
struct NormalizedDiagnosticsSummary {
    module_geology_exec_time_ms_total: f64,
    module_geology_exec_time_share_of_exec_world: f64,
    module_climate_exec_time_ms_total: f64,
    module_climate_exec_time_share_of_exec_world: f64,
    module_hydrology_exec_time_ms_total: f64,
    module_hydrology_exec_time_share_of_exec_world: f64,
    module_hydrology_river_network_rebuild_count_total: u32,
    module_hydrology_river_rebuild_rate: f64,
    module_hydrology_river_fallback_count_total: u32,
    module_hydrology_sink_rebuild_full_count_total: u32,
    module_hydrology_sink_rebuild_partial_count_total: u32,
    module_hydrology_sink_rebuild_skipped_count_total: u32,
    module_hydrology_sink_rebuild_fallback_full_count_total: u32,
    module_hydrology_sink_validation_fail_count_total: u32,
    module_hydrology_sink_affected_ratio_mean: f64,
}

#[derive(Serialize)]
struct DiagnosticsSummary {
    profile_attempt_count: u32,
    profile_success_count: u32,
    profile_fallback_count: u32,
    replay_ticks_total: u32,
    replay_time_ms_total: f64,
    exec_world_time_ms_total: f64,
    exec_world_profiled_time_ms_total: f64,
    step_geology_terrain_time_ms_total: f64,
    step_climate_time_ms_total: f64,
    step_hydrology_time_ms_total: f64,
    step_geology_river_time_ms_total: f64,
    tick_total_time_ms_total: f64,
    replay_time_share_of_wall: f64,
    replay_time_share_of_exec_world: f64,
    exec_world_share_of_tick: f64,
    river_share_of_exec_world: f64,
    river_network_rebuild_count_total: u32,
    river_rebuild_rate: f64,
    river_fallback_count_total: u32,
    geometry_update_skipped_count: u32,
    sink_rebuild_full_count_total: u32,
    sink_rebuild_partial_count_total: u32,
    sink_rebuild_skipped_count_total: u32,
    sink_rebuild_fallback_full_count_total: u32,
    sink_validation_fail_count_total: u32,
    sink_affected_ratio_mean: f64,
    modules: DiagnosticsModulesSummary,
    normalized: NormalizedDiagnosticsSummary,
}

#[derive(Serialize)]
struct PerfOutput {
    meta: BTreeMap<String, String>,
    profile: BTreeMap<String, serde_json::Value>,
    totals: BTreeMap<String, serde_json::Value>,
    metrics: BTreeMap<String, MetricStats>,
    diagnostics: DiagnosticsSummary,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut args = Args {
        ticks: DEFAULT_TICKS,
        level: DEFAULT_LEVEL,
        seed: DEFAULT_SEED.to_string(),
        sample_interval: DEFAULT_SAMPLE_INTERVAL,
    };

    let mut index = 0usize;
    while index < argv.len() {
        let token = &argv[index];
        if token == "--" {
            index += 1;
            continue;
        }
        match token.as_str() {
            "--ticks" => {
                args.ticks = next_arg(argv, index, token)?
                    .parse::<u32>()
                    .map_err(|_| "--ticks must be a finite non-negative integer".to_string())?
                    .max(1);
                index += 2;
            }
            "--level" => {
                args.level = next_arg(argv, index, token)?
                    .parse::<u32>()
                    .map_err(|_| "--level must be a finite non-negative integer".to_string())?;
                index += 2;
            }
            "--seed" => {
                args.seed = next_arg(argv, index, token)?.to_string();
                index += 2;
            }
            "--sample-interval" => {
                args.sample_interval = next_arg(argv, index, token)?
                    .parse::<u32>()
                    .map_err(|_| {
                        "--sample-interval must be a finite non-negative integer".to_string()
                    })?
                    .max(1);
                index += 2;
            }
            "--help" => {
                eprintln!("Usage: cargo run --manifest-path rust/Cargo.toml --bin perf_native -- [options]");
                eprintln!("  --ticks <n>");
                eprintln!("  --seed <seed>");
                eprintln!("  --level <n>");
                eprintln!("  --sample-interval <n>");
                std::process::exit(0);
            }
            _ => return Err(format!("Unknown argument: {token}")),
        }
    }
    Ok(args)
}

fn next_arg<'a>(argv: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    argv.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn round_ms(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    (value * 1000.0).round() / 1000.0
}

fn round_ratio(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn percentile(sorted: &[f64], ratio: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let clamped = ratio.clamp(0.0, 1.0);
    let raw = ((sorted.len() as f64) * clamped).ceil();
    let index = raw.max(1.0) as usize - 1;
    sorted[index.min(sorted.len() - 1)]
}

fn summarize(samples: &[f64]) -> MetricStats {
    if samples.is_empty() {
        return MetricStats {
            count: 0,
            mean: 0.0,
            min: 0.0,
            max: 0.0,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
        };
    }
    let mut sorted = samples
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if sorted.is_empty() {
        return MetricStats {
            count: 0,
            mean: 0.0,
            min: 0.0,
            max: 0.0,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
        };
    }
    let count = sorted.len() as u32;
    let total = sorted.iter().sum::<f64>();
    MetricStats {
        count,
        mean: round_ms(total / sorted.len() as f64),
        min: round_ms(*sorted.first().unwrap_or(&0.0)),
        max: round_ms(*sorted.last().unwrap_or(&0.0)),
        p50: round_ms(percentile(&sorted, 0.50)),
        p95: round_ms(percentile(&sorted, 0.95)),
        p99: round_ms(percentile(&sorted, 0.99)),
    }
}

fn post_step_sync_light(
    world: &mut World,
    hydrology_state: Option<&mut ErosionAutomatonState>,
    params: &GeologyParams,
) {
    let Some(state) = hydrology_state else {
        return;
    };
    frey_wasm::sim::hydrology::sync_hydrology_state_for_headless_runner(world, state, params);
}

fn run(args: &Args) -> Result<PerfOutput, String> {
    let geology_params = GeologyParams {
        level: args.level,
        ..GeologyParams::default()
    };
    let (mut world, erosion_state) = frey_wasm::sim::headless::init_world_for_headless_runner(
        args.seed.as_str(),
        args.level,
        geology_params.clone(),
    )?;
    let mut hydrology_state = Some(erosion_state);
    let mut feedback = FeedbackQueue::new(world.cell_count());

    let mut exec_world_samples = Vec::with_capacity(args.ticks as usize);
    let mut tick_total_samples = Vec::with_capacity(args.ticks as usize);
    let mut step_climate_samples = Vec::new();
    let mut step_hydrology_samples = Vec::new();
    let mut step_geology_terrain_samples = Vec::new();

    let mut profile_attempt_count = 0u32;
    let mut profile_success_count = 0u32;
    let mut river_network_rebuild_count_total = 0u32;
    let mut river_fallback_count_total = 0u32;
    let mut sink_rebuild_full_count_total = 0u32;
    let mut sink_rebuild_partial_count_total = 0u32;
    let mut sink_rebuild_skipped_count_total = 0u32;
    let mut sink_rebuild_fallback_full_count_total = 0u32;
    let mut sink_validation_fail_count_total = 0u32;
    let mut sink_affected_ratio_total = 0.0f64;

    let wall_start = Instant::now();
    let sample_interval = args.sample_interval.max(1);

    for step in 0..args.ticks {
        let tick_start = Instant::now();
        let should_profile = (step + 1) % sample_interval == 0 || (step + 1) == args.ticks;
        let exec_start = Instant::now();
        if should_profile {
            profile_attempt_count = profile_attempt_count.saturating_add(1);
            let profiled = sim::exec_world_profiled_detailed_with_feedback_and_hydrology(
                &mut world,
                &mut feedback,
                &mut hydrology_state,
            );
            profile_success_count = profile_success_count.saturating_add(1);
            step_geology_terrain_samples.push(profiled.breakdown.exec_geology_terrain_ms);
            step_climate_samples.push(profiled.breakdown.exec_climate_ms);
            step_hydrology_samples.push(profiled.breakdown.exec_hydrology_ms);
            river_network_rebuild_count_total = river_network_rebuild_count_total
                .saturating_add(profiled.river.river_network_rebuild_count);
            river_fallback_count_total =
                river_fallback_count_total.saturating_add(profiled.river.river_fallback_count);
            sink_rebuild_full_count_total = sink_rebuild_full_count_total
                .saturating_add(profiled.river.sink_rebuild_full_count);
            sink_rebuild_partial_count_total = sink_rebuild_partial_count_total
                .saturating_add(profiled.river.sink_rebuild_partial_count);
            sink_rebuild_skipped_count_total = sink_rebuild_skipped_count_total
                .saturating_add(profiled.river.sink_rebuild_skipped_count);
            sink_rebuild_fallback_full_count_total = sink_rebuild_fallback_full_count_total
                .saturating_add(profiled.river.sink_rebuild_fallback_full_count);
            sink_validation_fail_count_total = sink_validation_fail_count_total
                .saturating_add(profiled.river.sink_validation_fail_count);
            sink_affected_ratio_total += profiled.river.sink_affected_ratio.max(0.0);
        } else {
            sim::exec_world_with_feedback_and_hydrology(
                &mut world,
                &mut feedback,
                &mut hydrology_state,
            );
        }
        let exec_elapsed_ms = exec_start.elapsed().as_secs_f64() * 1000.0;
        exec_world_samples.push(exec_elapsed_ms);
        post_step_sync_light(&mut world, hydrology_state.as_mut(), &geology_params);
        let tick_elapsed_ms = tick_start.elapsed().as_secs_f64() * 1000.0;
        tick_total_samples.push(tick_elapsed_ms);
    }

    let wall_time_ms = wall_start.elapsed().as_secs_f64() * 1000.0;
    let exec_world_time_ms_total = exec_world_samples.iter().sum::<f64>();
    let tick_total_time_ms_total = tick_total_samples.iter().sum::<f64>();
    let exec_world_profiled_time_ms_total = step_geology_terrain_samples
        .iter()
        .zip(step_climate_samples.iter())
        .zip(step_hydrology_samples.iter())
        .map(|((geology, climate), hydrology)| geology + climate + hydrology)
        .sum::<f64>();
    let step_geology_terrain_time_ms_total = step_geology_terrain_samples.iter().sum::<f64>();
    let step_climate_time_ms_total = step_climate_samples.iter().sum::<f64>();
    let step_hydrology_time_ms_total = step_hydrology_samples.iter().sum::<f64>();
    let river_share_of_exec_world = if exec_world_profiled_time_ms_total > 0.0 {
        step_hydrology_time_ms_total / exec_world_profiled_time_ms_total
    } else {
        0.0
    };
    let geology_share_of_exec_world = if exec_world_profiled_time_ms_total > 0.0 {
        step_geology_terrain_time_ms_total / exec_world_profiled_time_ms_total
    } else {
        0.0
    };
    let climate_share_of_exec_world = if exec_world_profiled_time_ms_total > 0.0 {
        step_climate_time_ms_total / exec_world_profiled_time_ms_total
    } else {
        0.0
    };
    let river_rebuild_rate = if args.ticks > 0 {
        river_network_rebuild_count_total as f64 / args.ticks as f64
    } else {
        0.0
    };
    let sink_affected_ratio_mean = if profile_success_count > 0 {
        sink_affected_ratio_total / profile_success_count as f64
    } else {
        0.0
    };

    let modules = DiagnosticsModulesSummary {
        geology: ModuleExecDiagnosticsSummary {
            exec_time_ms_total: round_ms(step_geology_terrain_time_ms_total),
            exec_time_share_of_exec_world: round_ratio(geology_share_of_exec_world),
        },
        climate: ModuleExecDiagnosticsSummary {
            exec_time_ms_total: round_ms(step_climate_time_ms_total),
            exec_time_share_of_exec_world: round_ratio(climate_share_of_exec_world),
        },
        hydrology: HydrologyDiagnosticsSummary {
            exec_time_ms_total: round_ms(step_hydrology_time_ms_total),
            exec_time_share_of_exec_world: round_ratio(river_share_of_exec_world),
            river_network_rebuild_count_total,
            river_rebuild_rate: round_ratio(river_rebuild_rate),
            river_fallback_count_total,
            sink_rebuild_full_count_total,
            sink_rebuild_partial_count_total,
            sink_rebuild_skipped_count_total,
            sink_rebuild_fallback_full_count_total,
            sink_validation_fail_count_total,
            sink_affected_ratio_mean: round_ratio(sink_affected_ratio_mean),
        },
    };

    let normalized = NormalizedDiagnosticsSummary {
        module_geology_exec_time_ms_total: modules.geology.exec_time_ms_total,
        module_geology_exec_time_share_of_exec_world: modules.geology.exec_time_share_of_exec_world,
        module_climate_exec_time_ms_total: modules.climate.exec_time_ms_total,
        module_climate_exec_time_share_of_exec_world: modules.climate.exec_time_share_of_exec_world,
        module_hydrology_exec_time_ms_total: modules.hydrology.exec_time_ms_total,
        module_hydrology_exec_time_share_of_exec_world: modules
            .hydrology
            .exec_time_share_of_exec_world,
        module_hydrology_river_network_rebuild_count_total: modules
            .hydrology
            .river_network_rebuild_count_total,
        module_hydrology_river_rebuild_rate: modules.hydrology.river_rebuild_rate,
        module_hydrology_river_fallback_count_total: modules.hydrology.river_fallback_count_total,
        module_hydrology_sink_rebuild_full_count_total: modules
            .hydrology
            .sink_rebuild_full_count_total,
        module_hydrology_sink_rebuild_partial_count_total: modules
            .hydrology
            .sink_rebuild_partial_count_total,
        module_hydrology_sink_rebuild_skipped_count_total: modules
            .hydrology
            .sink_rebuild_skipped_count_total,
        module_hydrology_sink_rebuild_fallback_full_count_total: modules
            .hydrology
            .sink_rebuild_fallback_full_count_total,
        module_hydrology_sink_validation_fail_count_total: modules
            .hydrology
            .sink_validation_fail_count_total,
        module_hydrology_sink_affected_ratio_mean: modules.hydrology.sink_affected_ratio_mean,
    };

    let diagnostics = DiagnosticsSummary {
        profile_attempt_count,
        profile_success_count,
        profile_fallback_count: 0,
        replay_ticks_total: 0,
        replay_time_ms_total: 0.0,
        exec_world_time_ms_total: round_ms(exec_world_time_ms_total),
        exec_world_profiled_time_ms_total: round_ms(exec_world_profiled_time_ms_total),
        step_geology_terrain_time_ms_total: modules.geology.exec_time_ms_total,
        step_climate_time_ms_total: modules.climate.exec_time_ms_total,
        step_hydrology_time_ms_total: modules.hydrology.exec_time_ms_total,
        step_geology_river_time_ms_total: modules.hydrology.exec_time_ms_total,
        tick_total_time_ms_total: round_ms(tick_total_time_ms_total),
        replay_time_share_of_wall: 0.0,
        replay_time_share_of_exec_world: 0.0,
        exec_world_share_of_tick: round_ratio(if tick_total_time_ms_total > 0.0 {
            exec_world_time_ms_total / tick_total_time_ms_total
        } else {
            0.0
        }),
        river_share_of_exec_world: modules.hydrology.exec_time_share_of_exec_world,
        river_network_rebuild_count_total,
        river_rebuild_rate: modules.hydrology.river_rebuild_rate,
        river_fallback_count_total,
        geometry_update_skipped_count: 0,
        sink_rebuild_full_count_total,
        sink_rebuild_partial_count_total,
        sink_rebuild_skipped_count_total,
        sink_rebuild_fallback_full_count_total,
        sink_validation_fail_count_total,
        sink_affected_ratio_mean: modules.hydrology.sink_affected_ratio_mean,
        modules,
        normalized,
    };

    let mut metrics = BTreeMap::new();
    metrics.insert("exec_world".to_string(), summarize(&exec_world_samples));
    metrics.insert("tick_total".to_string(), summarize(&tick_total_samples));
    metrics.insert(
        "step_geology_terrain".to_string(),
        summarize(&step_geology_terrain_samples),
    );
    metrics.insert("step_climate".to_string(), summarize(&step_climate_samples));
    let step_hydrology_summary = summarize(&step_hydrology_samples);
    metrics.insert("step_hydrology".to_string(), step_hydrology_summary.clone());
    metrics.insert("step_geology_river".to_string(), step_hydrology_summary);

    let mut profile = BTreeMap::new();
    profile.insert(
        "label".to_string(),
        serde_json::Value::String("perf-native".to_string()),
    );
    profile.insert("tickCount".to_string(), serde_json::Value::from(args.ticks));
    profile.insert(
        "seed".to_string(),
        serde_json::Value::String(args.seed.clone()),
    );
    profile.insert("tickStart".to_string(), serde_json::Value::from(0u32));
    profile.insert("tickEnd".to_string(), serde_json::Value::from(args.ticks));

    let mut totals = BTreeMap::new();
    totals.insert(
        "wall_time_ms".to_string(),
        serde_json::Value::from(round_ms(wall_time_ms)),
    );
    totals.insert(
        "processed_ticks".to_string(),
        serde_json::Value::from(args.ticks),
    );

    let mut meta = BTreeMap::new();
    meta.insert("generated_at".to_string(), iso8601_now_utc());
    meta.insert("user_agent".to_string(), "native rust".to_string());
    meta.insert("timezone".to_string(), "UTC".to_string());

    Ok(PerfOutput {
        meta,
        profile,
        totals,
        metrics,
        diagnostics,
    })
}

fn iso8601_now_utc() -> String {
    let output = std::process::Command::new("date")
        .arg("-u")
        .arg("+%Y-%m-%dT%H:%M:%SZ")
        .output();
    match output {
        Ok(result) if result.status.success() => String::from_utf8(result.stdout)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
            .trim()
            .to_string(),
        _ => "1970-01-01T00:00:00Z".to_string(),
    }
}

fn main() {
    let argv = std::env::args().skip(1).collect::<Vec<_>>();
    let args = match parse_args(&argv) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };
    let output = match run(&args) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };
    match serde_json::to_string_pretty(&output) {
        Ok(json) => println!("{json}"),
        Err(err) => {
            eprintln!("failed to serialize native perf result: {err}");
            std::process::exit(1);
        }
    }
}

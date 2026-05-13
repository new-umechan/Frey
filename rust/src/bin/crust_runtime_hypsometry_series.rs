use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::sim::GeologyExecState;
use frey_wasm::GeologyParams;
use serde::Serialize;

const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_SEED: &str = "alpha";
const DEFAULT_TICKS: u64 = 800;
const DEFAULT_RECORD_EVERY: u64 = 1;

#[derive(Debug, Clone)]
struct BenchConfig {
    seed: String,
    level: u32,
    ticks: u64,
    record_every: u64,
    out_path: PathBuf,
}

#[derive(Debug, Serialize)]
struct BenchRecord {
    benchmark: String,
    run_id: String,
    seed: String,
    level: u32,
    ticks: u64,
    samples: Vec<TickRecord>,
}

#[derive(Debug, Serialize)]
struct TickRecord {
    tick: u64,
    land_ratio: f32,
    coastal_band_ratio: f32,
    land_freeboard_p10: f32,
    land_freeboard_p50: f32,
    land_freeboard_p90: f32,
    geology_runtime_bedrock_band_ratio: f32,
    geology_runtime_bedrock_p10: f32,
    geology_runtime_bedrock_p50: f32,
    geology_runtime_bedrock_p90: f32,
}

fn main() {
    let config = load_config();
    let run_id = default_run_id();
    let record = run_benchmark(&config, run_id.clone());
    if let Err(err) = append_jsonl(&config.out_path, &record) {
        panic!("failed to write benchmark artifact: {err}");
    }
    println!(
        "crust_runtime_hypsometry_series: PASS (samples={}, run_id={})",
        record.samples.len(),
        run_id
    );
}

fn load_config() -> BenchConfig {
    let seed = env::var("CRUST_RUNTIME_SERIES_SEED")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SEED.to_string());
    let level = env_u32("CRUST_RUNTIME_SERIES_LEVEL").unwrap_or(DEFAULT_LEVEL);
    let ticks = env_u64("CRUST_RUNTIME_SERIES_TICKS").unwrap_or(DEFAULT_TICKS);
    let record_every = env_u64("CRUST_RUNTIME_SERIES_RECORD_EVERY").unwrap_or(DEFAULT_RECORD_EVERY);
    let out_path = env::var("CRUST_RUNTIME_SERIES_BENCH_OUT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "benches/results/crust_runtime_hypsometry_series/crust_runtime_hypsometry_series.jsonl",
            )
        });
    BenchConfig {
        seed,
        level,
        ticks,
        record_every: record_every.max(1),
        out_path,
    }
}

fn run_benchmark(config: &BenchConfig, run_id: String) -> BenchRecord {
    let geology_params = GeologyParams {
        level: config.level,
        ..GeologyParams::default()
    };
    let (mut world, _) = sim::headless::init_world_for_headless_runner(
        &config.seed,
        config.level,
        geology_params,
    )
    .unwrap_or_else(|err| panic!("failed to init world: {err}"));
    let mut geology_state: GeologyExecState = None;
    let mut samples = Vec::new();

    samples.push(sample_world(&world, 0));
    for tick in 1..=config.ticks {
        let budgets = world.clock.epoch.budgets();
        sim::run_geology_step_with_state_for_bench(
            &mut world,
            &mut geology_state,
            budgets.geology,
        );
        world.clock.tick = tick;
        if tick % config.record_every == 0 {
            samples.push(sample_world(&world, tick));
        }
    }

    BenchRecord {
        benchmark: "crust_runtime_hypsometry_series".to_string(),
        run_id,
        seed: config.seed.clone(),
        level: config.level,
        ticks: config.ticks,
        samples,
    }
}

fn sample_world(world: &frey_wasm::sim::world::World, tick: u64) -> TickRecord {
    let land_ratio = world.metrics().land_ratio;
    let coastal_band_ratio = coastal_band_ratio(&world.state.geology.height, 0.02);
    let (land_freeboard_p10, land_freeboard_p50, land_freeboard_p90) =
        positive_height_percentiles(&world.state.geology.height);
    let runtime_metrics = world
        .exec_scratch
        .geology_dynamics
        .as_ref()
        .map(|state| state.cached_metrics)
        .unwrap_or_default();
    TickRecord {
        tick,
        land_ratio,
        coastal_band_ratio,
        land_freeboard_p10,
        land_freeboard_p50,
        land_freeboard_p90,
        geology_runtime_bedrock_band_ratio: runtime_metrics.bedrock_zero_level_coastal_band_ratio,
        geology_runtime_bedrock_p10: runtime_metrics.bedrock_freeboard_p10,
        geology_runtime_bedrock_p50: runtime_metrics.bedrock_freeboard_p50,
        geology_runtime_bedrock_p90: runtime_metrics.bedrock_freeboard_p90,
    }
}

fn coastal_band_ratio(heights: &[f32], band: f32) -> f32 {
    if heights.is_empty() {
        return 0.0;
    }
    let in_band = heights
        .iter()
        .filter(|&&height| height.abs() <= band)
        .count();
    in_band as f32 / heights.len() as f32
}

fn positive_height_percentiles(heights: &[f32]) -> (f32, f32, f32) {
    let mut values = heights
        .iter()
        .copied()
        .filter(|height| *height > 0.0)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    (
        percentile_sorted(&values, 0.10),
        percentile_sorted(&values, 0.50),
        percentile_sorted(&values, 0.90),
    )
}

fn percentile_sorted(values: &[f32], quantile: f32) -> f32 {
    if values.len() == 1 {
        return values[0];
    }
    let q = quantile.clamp(0.0, 1.0);
    let position = q * (values.len() - 1) as f32;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return values[lower];
    }
    let weight = position - lower as f32;
    values[lower] * (1.0 - weight) + values[upper] * weight
}

fn default_run_id() -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("crust-runtime-hypsometry-{epoch}")
}

fn append_jsonl(path: &PathBuf, record: &BenchRecord) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create directory {}: {err}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    let line = serde_json::to_string(record)
        .map_err(|err| format!("failed to serialize benchmark record: {err}"))?;
    file.write_all(line.as_bytes())
        .map_err(|err| format!("failed to write record: {err}"))?;
    file.write_all(b"\n")
        .map_err(|err| format!("failed to write newline: {err}"))?;
    Ok(())
}

fn env_u32(name: &str) -> Option<u32> {
    env::var(name).ok()?.parse::<u32>().ok()
}

fn env_u64(name: &str) -> Option<u64> {
    env::var(name).ok()?.parse::<u64>().ok()
}

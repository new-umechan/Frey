use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::GeologyParams;
use serde::Serialize;

const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_SEED: &str = "alpha";
const DEFAULT_MAX_COASTAL_BAND_RATIO: f32 = 0.12;

#[derive(Debug, Clone)]
struct BenchConfig {
    seed: String,
    level: u32,
    out_path: PathBuf,
    max_coastal_band_ratio: f32,
}

#[derive(Debug, Serialize)]
struct BenchRecord {
    benchmark: String,
    run_id: String,
    seed: String,
    level: u32,
    land_ratio: f32,
    coastal_band_ratio: f32,
    land_freeboard_p10: f32,
    land_freeboard_p50: f32,
    land_freeboard_p90: f32,
    hypsometry_bins: [u32; 8],
    violations: Vec<ViolationRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct ViolationRecord {
    kind: String,
    detail: String,
}

fn main() {
    let config = load_config();
    let run_id = default_run_id();
    let record = run_benchmark(&config, run_id.clone());
    if let Err(err) = append_jsonl(&config.out_path, &record) {
        panic!("failed to write benchmark artifact: {err}");
    }
    if !record.violations.is_empty() {
        panic!(
            "crust_hypsometry_guard failed: {} violations (run_id={})",
            record.violations.len(),
            run_id
        );
    }
    println!("crust_hypsometry_guard: PASS (run_id={})", run_id);
}

fn load_config() -> BenchConfig {
    let seed = env::var("CRUST_HYPSOMETRY_SEED")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SEED.to_string());
    let level = env_u32("CRUST_HYPSOMETRY_LEVEL").unwrap_or(DEFAULT_LEVEL);
    let out_path = env::var("CRUST_HYPSOMETRY_BENCH_OUT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("benches/results/crust_hypsometry_guard/crust_hypsometry_guard.jsonl")
        });
    let max_coastal_band_ratio = env_f32("CRUST_HYPSOMETRY_MAX_COASTAL_BAND_RATIO")
        .unwrap_or(DEFAULT_MAX_COASTAL_BAND_RATIO);
    BenchConfig {
        seed,
        level,
        out_path,
        max_coastal_band_ratio,
    }
}

fn run_benchmark(config: &BenchConfig, run_id: String) -> BenchRecord {
    let geology_params = GeologyParams {
        level: config.level,
        ..GeologyParams::default()
    };
    let (terrain, _, _, _) = sim::build_geology_with_mesh(&config.seed, geology_params);
    let land_ratio = terrain.land_ratio;
    let coastal_band_ratio = coastal_band_ratio(&terrain.height, 0.02);
    let (land_freeboard_p10, land_freeboard_p50, land_freeboard_p90) =
        positive_height_percentiles(&terrain.height);
    let hypsometry_bins = hypsometry_bins(&terrain.height);
    let mut violations = Vec::new();
    if coastal_band_ratio > config.max_coastal_band_ratio {
        violations.push(ViolationRecord {
            kind: "coastal_band_ratio".to_string(),
            detail: format!(
                "ratio={} threshold={}",
                coastal_band_ratio, config.max_coastal_band_ratio
            ),
        });
    }

    BenchRecord {
        benchmark: "crust_hypsometry_guard".to_string(),
        run_id,
        seed: config.seed.clone(),
        level: config.level,
        land_ratio,
        coastal_band_ratio,
        land_freeboard_p10,
        land_freeboard_p50,
        land_freeboard_p90,
        hypsometry_bins,
        violations,
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

fn hypsometry_bins(heights: &[f32]) -> [u32; 8] {
    let mut bins = [0_u32; 8];
    for &height in heights {
        let bucket = if height <= -0.20 {
            0
        } else if height <= -0.10 {
            1
        } else if height <= -0.02 {
            2
        } else if height <= 0.02 {
            3
        } else if height <= 0.10 {
            4
        } else if height <= 0.20 {
            5
        } else if height <= 0.40 {
            6
        } else {
            7
        };
        bins[bucket] = bins[bucket].saturating_add(1);
    }
    bins
}

fn default_run_id() -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("crust-hypsometry-{epoch}")
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

fn env_f32(name: &str) -> Option<f32> {
    env::var(name).ok()?.parse::<f32>().ok()
}

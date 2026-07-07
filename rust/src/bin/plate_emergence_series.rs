use std::env;

use frey_wasm::sim;
use frey_wasm::GeologyParams;
use serde::Serialize;

const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_SEEDS: &str = "alpha,beta,gamma,delta,epsilon";

#[derive(Serialize)]
struct SeriesRecord {
    seed: String,
    selected_valid_count: u32,
    selected_final_plate_count: u32,
    selected_max_plate_area_ratio: f32,
    selected_second_plate_area_ratio: f32,
    selected_effective_plate_count: f32,
    selected_mean_plate_boundary_complexity: f32,
    selected_max_plate_boundary_complexity: f32,
    selected_max_enclosed_plate_risk: f32,
    selected_regime_score: f32,
}

fn main() {
    let seeds_csv = env::var("PLATE_EMERGENCE_SERIES_SEEDS")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SEEDS.to_string());
    let seeds = seeds_csv
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let level = env::var("PLATE_EMERGENCE_SERIES_LEVEL")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_LEVEL);

    let mut records = Vec::with_capacity(seeds.len());
    for seed in seeds {
        let diagnostics = sim::diagnose_plate_emergence_with_override(
            &seed,
            GeologyParams {
                level,
                ..GeologyParams::default()
            },
            None,
        );
        records.push(SeriesRecord {
            seed: diagnostics.seed,
            selected_valid_count: diagnostics.selected_valid_count,
            selected_final_plate_count: diagnostics.selected_final_plate_count,
            selected_max_plate_area_ratio: diagnostics.selected_max_plate_area_ratio,
            selected_second_plate_area_ratio: diagnostics.selected_second_plate_area_ratio,
            selected_effective_plate_count: diagnostics.selected_effective_plate_count,
            selected_mean_plate_boundary_complexity: diagnostics
                .selected_mean_plate_boundary_complexity,
            selected_max_plate_boundary_complexity: diagnostics
                .selected_max_plate_boundary_complexity,
            selected_max_enclosed_plate_risk: diagnostics.selected_max_enclosed_plate_risk,
            selected_regime_score: diagnostics.selected_regime_score,
        });
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&records)
            .unwrap_or_else(|err| panic!("failed to serialize series diagnostics: {err}"))
    );
}

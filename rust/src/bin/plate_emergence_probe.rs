use std::env;

use frey_wasm::sim;
use frey_wasm::GeologyParams;

const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_SEED: &str = "alpha";

fn main() {
    let seed = env::args()
        .nth(1)
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            env::var("PLATE_EMERGENCE_SEED")
                .ok()
                .filter(|v| !v.trim().is_empty())
        })
        .unwrap_or_else(|| DEFAULT_SEED.to_string());
    let level = env::var("PLATE_EMERGENCE_LEVEL")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(DEFAULT_LEVEL);
    let mut params = GeologyParams {
        level,
        ..GeologyParams::default()
    };
    if let Some(value) = env_f32("PLATE_EMERGENCE_DAMAGE_RATE") {
        params.pre_plate_damage_rate = value;
    }
    if let Some(value) = env_f32("PLATE_EMERGENCE_HEALING_DECAY") {
        params.pre_plate_healing_decay = value;
    }
    if let Some(value) = env_u32("PLATE_EMERGENCE_STEPS") {
        params.pre_plate_steps = value;
    }
    let min_region_override = env_u32("PLATE_EMERGENCE_MIN_REGION").map(|value| value as usize);
    let diagnostics =
        sim::diagnose_plate_emergence_with_override(&seed, params, min_region_override);
    println!(
        "{}",
        serde_json::to_string_pretty(&diagnostics)
            .unwrap_or_else(|err| panic!("failed to serialize diagnostics: {err}"))
    );
}

fn env_f32(name: &str) -> Option<f32> {
    env::var(name).ok()?.parse::<f32>().ok()
}

fn env_u32(name: &str) -> Option<u32> {
    env::var(name).ok()?.parse::<u32>().ok()
}

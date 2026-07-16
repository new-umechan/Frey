use std::env;
use std::fs;

use frey_wasm::sim;
use frey_wasm::sim::GeologyExecState;
use frey_wasm::GeologyParams;
use serde::Serialize;

const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_SEED: &str = "alpha";

#[derive(Serialize)]
struct ProbeOutput {
    seed: String,
    level: u32,
    tick: u64,
    report: sim::geology::dynamics::SurfaceMaterialProbeReport,
}

fn main() {
    let seed = env::var("SURFACE_MATERIAL_PROBE_SEED")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SEED.to_string());
    let level = env::var("SURFACE_MATERIAL_PROBE_LEVEL")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_LEVEL);
    let ticks = env::var("SURFACE_MATERIAL_PROBE_TICKS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let geology_params = GeologyParams {
        level,
        ..GeologyParams::default()
    };
    let (mut world, _) =
        sim::headless::init_world_for_headless_runner(&seed, level, geology_params)
            .unwrap_or_else(|error| panic!("failed to initialize probe world: {error}"));
    let mut geology_state: GeologyExecState = None;
    for tick in 1..=ticks {
        let budget = world.clock.epoch.budgets().geology;
        sim::run_geology_step_with_state_for_bench(&mut world, &mut geology_state, budget);
        world.clock.tick = tick;
    }
    let report =
        sim::geology::dynamics::probe_surface_material_transport(&mut world, &mut geology_state)
            .unwrap_or_else(|error| panic!("surface material probe failed: {error}"));
    let output = ProbeOutput {
        seed,
        level,
        tick: ticks,
        report,
    };
    let json = serde_json::to_string_pretty(&output)
        .unwrap_or_else(|error| panic!("failed to serialize probe output: {error}"));
    if let Some(path) = env::var("SURFACE_MATERIAL_PROBE_OUTPUT")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        fs::write(&path, &json)
            .unwrap_or_else(|error| panic!("failed to write probe output to {path}: {error}"));
    }
    println!("{json}");
}

use std::collections::HashMap;
use std::env;

use frey_wasm::sim;
use frey_wasm::sim::world::ConvergentRegime;
use frey_wasm::sim::GeologyExecState;
use frey_wasm::GeologyParams;
use serde::Serialize;

const DEFAULT_SEED: &str = "alpha";
const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_TICKS: u64 = 160;
const DEFAULT_RECORD_EVERY: u64 = 1;

#[derive(Serialize)]
struct RegimeSeries {
    seed: String,
    level: u32,
    ticks: u64,
    samples: Vec<RegimeSample>,
}

#[derive(Clone, Copy, Serialize)]
struct RegimeSample {
    tick: u64,
    continental_collision_edge_count: u32,
    incipient_subduction_edge_count: u32,
    active_subduction_edge_count: u32,
    obduction_edge_count: u32,
    incipient_to_subduction_edge_count: u32,
    subduction_to_incipient_edge_count: u32,
    persistent_incipient_edge_count: u32,
}

#[derive(Default)]
struct RegimeTracker {
    previous: HashMap<(u32, u32), ConvergentRegime>,
}

impl RegimeTracker {
    fn sample(&mut self, geology_state: &GeologyExecState, tick: u64) -> RegimeSample {
        let mut counts = RegimeSample {
            tick,
            continental_collision_edge_count: 0,
            incipient_subduction_edge_count: 0,
            active_subduction_edge_count: 0,
            obduction_edge_count: 0,
            incipient_to_subduction_edge_count: 0,
            subduction_to_incipient_edge_count: 0,
            persistent_incipient_edge_count: 0,
        };
        let Some(state) = geology_state.as_ref() else {
            return counts;
        };
        let mut current = HashMap::with_capacity(state.boundary_state.edge_pairs.len());
        for (&pair, &regime) in state
            .boundary_state
            .edge_pairs
            .iter()
            .zip(state.boundary_state.edge_convergent_regimes.iter())
        {
            let key = ordered_pair(pair[0], pair[1]);
            current.insert(key, regime);
            match regime {
                ConvergentRegime::None => {}
                ConvergentRegime::ContinentalCollision => {
                    counts.continental_collision_edge_count += 1;
                }
                ConvergentRegime::IncipientSubduction => {
                    counts.incipient_subduction_edge_count += 1;
                    if self.previous.get(&key) == Some(&ConvergentRegime::IncipientSubduction) {
                        counts.persistent_incipient_edge_count += 1;
                    }
                }
                ConvergentRegime::Subduction => {
                    counts.active_subduction_edge_count += 1;
                }
                ConvergentRegime::Obduction => {
                    counts.obduction_edge_count += 1;
                }
            }
            match (self.previous.get(&key), regime) {
                (Some(ConvergentRegime::IncipientSubduction), ConvergentRegime::Subduction) => {
                    counts.incipient_to_subduction_edge_count += 1;
                }
                (Some(ConvergentRegime::Subduction), ConvergentRegime::IncipientSubduction) => {
                    counts.subduction_to_incipient_edge_count += 1;
                }
                _ => {}
            }
        }
        self.previous = current;
        counts
    }
}

fn main() {
    let seed = env_string("CONVERGENT_REGIME_SERIES_SEED", DEFAULT_SEED);
    let level = env_u32("CONVERGENT_REGIME_SERIES_LEVEL", DEFAULT_LEVEL);
    let ticks = env_u64("CONVERGENT_REGIME_SERIES_TICKS", DEFAULT_TICKS);
    let record_every = env_u64(
        "CONVERGENT_REGIME_SERIES_RECORD_EVERY",
        DEFAULT_RECORD_EVERY,
    )
    .max(1);
    let params = GeologyParams {
        level,
        ..GeologyParams::default()
    };
    let (mut world, _) = sim::headless::init_world_for_headless_runner(&seed, level, params)
        .unwrap_or_else(|error| panic!("failed to initialize world: {error}"));
    let mut geology_state: GeologyExecState = None;
    let mut tracker = RegimeTracker::default();
    let mut samples = vec![tracker.sample(&geology_state, 0)];

    for tick in 1..=ticks {
        let budget = world.clock.epoch.budgets().geology;
        sim::run_geology_step_with_state_for_bench(&mut world, &mut geology_state, budget);
        world.clock.tick = tick;
        if tick % record_every == 0 {
            samples.push(tracker.sample(&geology_state, tick));
        }
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&RegimeSeries {
            seed,
            level,
            ticks,
            samples,
        })
        .unwrap_or_else(|error| panic!("failed to serialize regime series: {error}"))
    );
}

fn ordered_pair(a: u32, b: u32) -> (u32, u32) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

fn env_string(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::sim::geology_types::PlateId;
use frey_wasm::sim::world::PlateKinematicsState;
use frey_wasm::sim::GeologyExecState;
use frey_wasm::GeologyParams;
use serde::Serialize;

const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_SEED: &str = "alpha";
const DEFAULT_TICKS: u64 = 800;
const DEFAULT_RECORD_EVERY: u64 = 1;
const EARTH_MEAN_RADIUS_KM: f32 = 6_371.0;
const YEARS_PER_MYR: f32 = 1_000_000.0;

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
    plate_emergence_regime: String,
    plate_emergence_fallback: String,
    samples: Vec<TickRecord>,
}

#[derive(Debug, Serialize)]
struct TickRecord {
    tick: u64,
    plate_count: u32,
    land_ratio: f32,
    oceanic_cell_ratio: f32,
    continental_cell_ratio: f32,
    plate_id_churn_rate: f32,
    orphan_cell_count: f32,
    single_cell_plate_count: f32,
    geology_activity: f32,
    boundary_activity: f32,
    mean_plate_speed_km_per_myr: f32,
    max_plate_speed_km_per_myr: f32,
    mean_cell_crossing_fraction_per_tick: f32,
    max_cell_crossing_fraction_per_tick: f32,
    mean_direction_persistence: f32,
    reciprocal_churn_ratio: f32,
    mean_centroid_path_straightness: f32,
}

#[derive(Debug, Default)]
struct MotionTracker {
    initial_centroids: Vec<Option<[f32; 3]>>,
    previous_centroids: Vec<Option<[f32; 3]>>,
    previous_velocity_dirs: Vec<Option<[f32; 3]>>,
    previous_plate_id: Vec<PlateId>,
    centroid_path_lengths_km: Vec<f32>,
    mean_cell_spacing_km: Option<f32>,
}

#[derive(Debug, Clone)]
struct MotionDiagnostics {
    mean_plate_speed_km_per_myr: f32,
    max_plate_speed_km_per_myr: f32,
    mean_cell_crossing_fraction_per_tick: f32,
    max_cell_crossing_fraction_per_tick: f32,
    mean_direction_persistence: f32,
    reciprocal_churn_ratio: f32,
    mean_centroid_path_straightness: f32,
}

impl Default for MotionDiagnostics {
    fn default() -> Self {
        Self {
            mean_plate_speed_km_per_myr: 0.0,
            max_plate_speed_km_per_myr: 0.0,
            mean_cell_crossing_fraction_per_tick: 0.0,
            max_cell_crossing_fraction_per_tick: 0.0,
            mean_direction_persistence: 1.0,
            reciprocal_churn_ratio: 1.0,
            mean_centroid_path_straightness: 1.0,
        }
    }
}

fn main() {
    let config = load_config();
    let run_id = default_run_id();
    let record = run_benchmark(&config, run_id.clone());
    if let Err(err) = append_jsonl(&config.out_path, &record) {
        panic!("failed to write benchmark artifact: {err}");
    }
    let final_sample = record
        .samples
        .last()
        .expect("plate series should always contain at least one sample");
    println!(
        "crust_plate_count_series: PASS (samples={}, final_plate_count={}, run_id={})",
        record.samples.len(),
        final_sample.plate_count,
        run_id
    );
}

fn load_config() -> BenchConfig {
    let seed = env::var("CRUST_PLATE_SERIES_SEED")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SEED.to_string());
    let level = env_u32("CRUST_PLATE_SERIES_LEVEL").unwrap_or(DEFAULT_LEVEL);
    let ticks = env_u64("CRUST_PLATE_SERIES_TICKS").unwrap_or(DEFAULT_TICKS);
    let record_every = env_u64("CRUST_PLATE_SERIES_RECORD_EVERY").unwrap_or(DEFAULT_RECORD_EVERY);
    let out_path = env::var("CRUST_PLATE_SERIES_BENCH_OUT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("benches/results/crust_plate_count_series/crust_plate_count_series.jsonl")
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
    let mut geology_params = GeologyParams {
        level: config.level,
        ..GeologyParams::default()
    };
    if let Some(value) = env_u32("CRUST_PLATE_SERIES_STEPS") {
        geology_params.pre_plate_steps = value;
    }
    if let Some(value) = env_f32("CRUST_PLATE_SERIES_HEALING_DECAY") {
        geology_params.pre_plate_healing_decay = value;
    }
    if let Some(value) = env_f32("CRUST_PLATE_SERIES_DAMAGE_RATE") {
        geology_params.pre_plate_damage_rate = value;
    }
    let (mut world, _) =
        sim::headless::init_world_for_headless_runner(&config.seed, config.level, geology_params)
            .unwrap_or_else(|err| panic!("failed to init world: {err}"));
    let mut geology_state: GeologyExecState = None;
    let mut motion_tracker = MotionTracker::default();
    let mut samples = Vec::new();

    samples.push(sample_world(&world, &geology_state, 0, &mut motion_tracker));
    for tick in 1..=config.ticks {
        let budgets = world.clock.epoch.budgets();
        sim::run_geology_step_with_state_for_bench(&mut world, &mut geology_state, budgets.geology);
        world.clock.tick = tick;
        if tick % config.record_every == 0 {
            samples.push(sample_world(
                &world,
                &geology_state,
                tick,
                &mut motion_tracker,
            ));
        }
    }

    BenchRecord {
        benchmark: "crust_plate_count_series".to_string(),
        run_id,
        seed: config.seed.clone(),
        level: config.level,
        ticks: config.ticks,
        plate_emergence_regime: format!("{:?}", world.state.geology.plate_emergence_regime),
        plate_emergence_fallback: format!("{:?}", world.state.geology.plate_emergence_fallback),
        samples,
    }
}

fn sample_world(
    world: &frey_wasm::sim::world::World,
    geology_state: &GeologyExecState,
    tick: u64,
    motion_tracker: &mut MotionTracker,
) -> TickRecord {
    let metrics = world.metrics();
    let runtime_metrics = geology_state
        .as_ref()
        .map(|state| state.cached_metrics)
        .unwrap_or_default();
    let motion = motion_tracker.sample(world, geology_state);
    TickRecord {
        tick,
        plate_count: unique_plate_count(&world.state.geology.plate_id),
        land_ratio: metrics.land_ratio,
        oceanic_cell_ratio: metrics.oceanic_cell_ratio,
        continental_cell_ratio: metrics.continental_cell_ratio,
        plate_id_churn_rate: runtime_metrics.plate_id_churn_rate,
        orphan_cell_count: runtime_metrics.orphan_cell_count,
        single_cell_plate_count: runtime_metrics.single_cell_plate_count,
        geology_activity: runtime_metrics.geology_activity,
        boundary_activity: runtime_metrics.boundary_activity,
        mean_plate_speed_km_per_myr: motion.mean_plate_speed_km_per_myr,
        max_plate_speed_km_per_myr: motion.max_plate_speed_km_per_myr,
        mean_cell_crossing_fraction_per_tick: motion.mean_cell_crossing_fraction_per_tick,
        max_cell_crossing_fraction_per_tick: motion.max_cell_crossing_fraction_per_tick,
        mean_direction_persistence: motion.mean_direction_persistence,
        reciprocal_churn_ratio: motion.reciprocal_churn_ratio,
        mean_centroid_path_straightness: motion.mean_centroid_path_straightness,
    }
}

impl MotionTracker {
    fn sample(
        &mut self,
        world: &frey_wasm::sim::world::World,
        geology_state: &GeologyExecState,
    ) -> MotionDiagnostics {
        let plate_id = &world.state.geology.plate_id;
        let plate_count = plate_id
            .iter()
            .copied()
            .max()
            .map(|id| id.as_usize() + 1)
            .unwrap_or(0);
        self.resize(plate_count);
        let mean_cell_spacing_km = *self
            .mean_cell_spacing_km
            .get_or_insert_with(|| mean_cell_spacing_km(world.mesh()));
        let centroids = plate_centroids(world.mesh().positions.as_slice(), plate_id, plate_count);
        let plate_states = geology_state
            .as_ref()
            .map(|state| state.plate_states.as_slice())
            .unwrap_or(&[]);

        let myr_per_tick = (world.clock.real_years_per_tick / YEARS_PER_MYR).max(1e-6);
        let mut speed_sum = 0.0_f32;
        let mut speed_count = 0_u32;
        let mut max_speed = 0.0_f32;
        let mut crossing_sum = 0.0_f32;
        let mut max_crossing = 0.0_f32;
        let mut persistence_sum = 0.0_f32;
        let mut persistence_count = 0_u32;
        let mut straightness_sum = 0.0_f32;
        let mut straightness_count = 0_u32;
        let mut velocity_dirs = vec![None; plate_count];

        for pid in 0..plate_count {
            let Some(centroid) = centroids[pid] else {
                continue;
            };
            let Some(state) = plate_states.get(pid).copied() else {
                continue;
            };
            let effective_angular_speed =
                finite_or(state.angular_speed * (0.55 + 0.45 * state.activity), 0.0).max(0.0);
            let speed_km_per_myr = effective_angular_speed * EARTH_MEAN_RADIUS_KM / myr_per_tick;
            let crossing_fraction =
                speed_km_per_myr * myr_per_tick / mean_cell_spacing_km.max(1e-6);
            speed_sum += speed_km_per_myr;
            speed_count = speed_count.saturating_add(1);
            max_speed = max_speed.max(speed_km_per_myr);
            crossing_sum += crossing_fraction;
            max_crossing = max_crossing.max(crossing_fraction);

            let dir = normalized(cross3(state.angular_axis, centroid));
            velocity_dirs[pid] = dir;
            if let (Some(prev), Some(current)) = (
                self.previous_velocity_dirs
                    .get(pid)
                    .and_then(|value| *value),
                dir,
            ) {
                persistence_sum += dot3(prev, current).clamp(-1.0, 1.0);
                persistence_count = persistence_count.saturating_add(1);
            }

            if self.initial_centroids[pid].is_none() {
                self.initial_centroids[pid] = Some(centroid);
            }
            if let Some(prev_centroid) = self.previous_centroids[pid] {
                self.centroid_path_lengths_km[pid] +=
                    great_circle_distance_km(prev_centroid, centroid);
            }
            if let Some(initial_centroid) = self.initial_centroids[pid] {
                let path = self.centroid_path_lengths_km[pid];
                if path > 1e-6 {
                    let net = great_circle_distance_km(initial_centroid, centroid);
                    straightness_sum += (net / path).clamp(0.0, 1.0);
                    straightness_count = straightness_count.saturating_add(1);
                }
            }
        }

        let reciprocal_churn_ratio =
            reciprocal_churn_ratio(self.previous_plate_id.as_slice(), plate_id.as_slice());
        self.previous_centroids = centroids;
        self.previous_velocity_dirs = velocity_dirs;
        self.previous_plate_id = plate_id.clone();

        MotionDiagnostics {
            mean_plate_speed_km_per_myr: mean_or_zero(speed_sum, speed_count),
            max_plate_speed_km_per_myr: max_speed,
            mean_cell_crossing_fraction_per_tick: mean_or_zero(crossing_sum, speed_count),
            max_cell_crossing_fraction_per_tick: max_crossing,
            mean_direction_persistence: mean_or_one(persistence_sum, persistence_count),
            reciprocal_churn_ratio,
            mean_centroid_path_straightness: mean_or_one(straightness_sum, straightness_count),
        }
    }

    fn resize(&mut self, plate_count: usize) {
        self.initial_centroids.resize(plate_count, None);
        self.previous_centroids.resize(plate_count, None);
        self.previous_velocity_dirs.resize(plate_count, None);
        self.centroid_path_lengths_km.resize(plate_count, 0.0);
    }
}

fn unique_plate_count(plate_ids: &[frey_wasm::sim::geology_types::PlateId]) -> u32 {
    plate_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .len()
        .try_into()
        .unwrap_or(u32::MAX)
}

fn plate_centroids(
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    plate_count: usize,
) -> Vec<Option<[f32; 3]>> {
    let mut sums = vec![[0.0_f32; 3]; plate_count];
    let mut counts = vec![0_u32; plate_count];
    for (index, &pid) in plate_id.iter().enumerate() {
        let plate = pid.as_usize();
        let Some(position) = positions.get(index).copied() else {
            continue;
        };
        if plate >= plate_count {
            continue;
        }
        sums[plate][0] += position[0];
        sums[plate][1] += position[1];
        sums[plate][2] += position[2];
        counts[plate] = counts[plate].saturating_add(1);
    }

    sums.into_iter()
        .zip(counts)
        .map(
            |(sum, count)| {
                if count == 0 {
                    None
                } else {
                    normalized(sum)
                }
            },
        )
        .collect()
}

fn mean_cell_spacing_km(mesh: &frey_wasm::sim::world::WorldMesh) -> f32 {
    let mut distance_sum = 0.0_f32;
    let mut edge_count = 0_u32;
    for cell in 0..mesh.positions.len() {
        let start = mesh.nbr_offsets[cell] as usize;
        let end = mesh.nbr_offsets[cell + 1] as usize;
        for &neighbor_u32 in &mesh.nbrs[start..end] {
            let neighbor = neighbor_u32 as usize;
            if neighbor <= cell || neighbor >= mesh.positions.len() {
                continue;
            }
            distance_sum +=
                great_circle_distance_km(mesh.positions[cell], mesh.positions[neighbor]);
            edge_count = edge_count.saturating_add(1);
        }
    }
    mean_or_zero(distance_sum, edge_count).max(1e-6)
}

fn reciprocal_churn_ratio(previous: &[PlateId], current: &[PlateId]) -> f32 {
    if previous.len() != current.len() || previous.is_empty() {
        return 1.0;
    }

    let mut pair_counts = BTreeMap::<(u32, u32), (u32, u32)>::new();
    for (&from, &to) in previous.iter().zip(current.iter()) {
        if from == to {
            continue;
        }
        let from = from.as_u32();
        let to = to.as_u32();
        let key = if from < to { (from, to) } else { (to, from) };
        let counts = pair_counts.entry(key).or_insert((0, 0));
        if from < to {
            counts.0 = counts.0.saturating_add(1);
        } else {
            counts.1 = counts.1.saturating_add(1);
        }
    }

    let mut net_sum = 0_u32;
    let mut total_sum = 0_u32;
    for (_, (forward, reverse)) in pair_counts {
        net_sum = net_sum.saturating_add(forward.abs_diff(reverse));
        total_sum = total_sum.saturating_add(forward.saturating_add(reverse));
    }
    if total_sum == 0 {
        1.0
    } else {
        net_sum as f32 / total_sum as f32
    }
}

fn great_circle_distance_km(a: [f32; 3], b: [f32; 3]) -> f32 {
    let chord = length3([a[0] - b[0], a[1] - b[1], a[2] - b[2]]).clamp(0.0, 2.0);
    let angle = 2.0 * (0.5 * chord).asin();
    angle * EARTH_MEAN_RADIUS_KM
}

fn normalized(value: [f32; 3]) -> Option<[f32; 3]> {
    let len = length3(value);
    if !len.is_finite() || len <= 1e-6 {
        return None;
    }
    Some([value[0] / len, value[1] / len, value[2] / len])
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length3(value: [f32; 3]) -> f32 {
    dot3(value, value).sqrt()
}

fn mean_or_zero(sum: f32, count: u32) -> f32 {
    if count == 0 {
        0.0
    } else {
        finite_or(sum / count as f32, 0.0)
    }
}

fn mean_or_one(sum: f32, count: u32) -> f32 {
    if count == 0 {
        1.0
    } else {
        finite_or(sum / count as f32, 1.0)
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn default_run_id() -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("crust-plate-count-series-{epoch}")
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

fn env_f32(name: &str) -> Option<f32> {
    env::var(name).ok()?.parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::{great_circle_distance_km, reciprocal_churn_ratio, PlateId, EARTH_MEAN_RADIUS_KM};

    #[test]
    fn reciprocal_churn_ratio_detects_one_way_motion() {
        let previous = vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)];
        let current = vec![PlateId(1), PlateId(1), PlateId(1), PlateId(1)];

        assert_eq!(reciprocal_churn_ratio(&previous, &current), 1.0);
    }

    #[test]
    fn reciprocal_churn_ratio_detects_mutual_takeover() {
        let previous = vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)];
        let current = vec![PlateId(1), PlateId(0), PlateId(0), PlateId(1)];

        assert_eq!(reciprocal_churn_ratio(&previous, &current), 0.0);
    }

    #[test]
    fn great_circle_distance_matches_quarter_circumference() {
        let distance = great_circle_distance_km([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let expected = std::f32::consts::FRAC_PI_2 * EARTH_MEAN_RADIUS_KM;

        assert!((distance - expected).abs() < 1e-3);
    }
}

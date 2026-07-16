use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::sim::geology_types::PlateId;
use frey_wasm::sim::world::{BoundaryType, PlateKinematicsState};
use frey_wasm::sim::GeologyExecState;
use frey_wasm::GeologyParams;
use serde::Serialize;

const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_SEED: &str = "alpha";
const DEFAULT_TICKS: u64 = 800;
const DEFAULT_RECORD_EVERY: u64 = 1;
const EARTH_MEAN_RADIUS_KM: f32 = 6_371.0;
const YEARS_PER_MYR: f32 = 1_000_000.0;
const CORRIDOR_CORE_DEGREE_THRESHOLDS: [usize; 2] = [3, 4];
const BOUNDARY_DISTANCE_THIN_CELLS: u32 = 2;
const CORE_EROSION_LAYERS: u32 = 2;
const BOUNDARY_COMPLEXITY_GROWTH_WINDOW_SAMPLES: usize = 4;
const PERSISTENT_BOUNDARY_COMPLEXITY_GROWTH_THRESHOLD: f32 = 1.5;
const BRANCH_PERSISTENCE_WINDOW_SAMPLES: usize = 4;
const PERSISTENT_BRANCH_AREA_RATIO_THRESHOLD: f32 = 0.15;
const ENCLOSED_PLATE_AREA_GATE: f32 = 0.10;
const WEAK_LINE_MIN_LARGEST_BLOCK_RATIO: f32 = 0.25;
const WEAK_LINE_MIN_SECONDARY_BLOCK_RATIO: f32 = 0.01;
const MULTISCALE_MAX_EROSION_CELLS: u32 = 8;
const MULTISCALE_MIN_CORE_RATIO: f32 = 0.005;
const MULTISCALE_MIN_CORE_CELLS: usize = 8;

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
    boundary_crossing_substeps: f32,
    boundary_topology_event_cell_count: f32,
    boundary_topology_constrained_segment_count: f32,
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
    net_exchange_directionality_ratio: f32,
    mutual_exchange_ratio: f32,
    temporal_reversal_ratio: f32,
    temporal_reversal_previous_velocity_aligned_ratio: f32,
    temporal_reversal_current_velocity_aligned_ratio: f32,
    mean_centroid_path_straightness: f32,
    mean_euler_rotation_residual_km: f32,
    max_euler_rotation_residual_km: f32,
    mean_euler_rotation_residual_ratio: f32,
    max_euler_rotation_residual_ratio: f32,
    boundary_transfer_evaluated_cell_count: u32,
    mean_boundary_transfer_velocity_alignment: f32,
    boundary_transfer_velocity_aligned_ratio: f32,
    boundary_transfer_velocity_unaligned_ratio: f32,
    mean_boundary_transfer_largest_component_ratio: f32,
    max_boundary_transfer_isolated_cell_ratio: f32,
    boundary_motion_expected_cell_count: f32,
    boundary_motion_actual_cell_count: u32,
    boundary_motion_response_ratio: f32,
    boundary_motion_underactive_risk: f32,
    boundary_motion_overactive_risk: f32,
    boundary_motion_runtime_raw_expected_cell_count: f32,
    boundary_motion_runtime_accumulated_expected_cell_count: f32,
    boundary_motion_runtime_component_budget_cell_count: f32,
    boundary_motion_runtime_transferable_component_budget_cell_count: f32,
    boundary_motion_runtime_plate_consistency_budget_cell_count: f32,
    boundary_motion_runtime_plate_consistency_deferred_cell_count: f32,
    boundary_motion_runtime_plate_consistency_donor_limited_cell_count: f32,
    boundary_motion_runtime_plate_consistency_outgoing_limited_cell_count: f32,
    boundary_motion_runtime_plate_consistency_incoming_limited_cell_count: f32,
    boundary_motion_runtime_plate_consistency_net_area_limited_cell_count: f32,
    boundary_motion_runtime_plate_consistency_max_projected_out_ratio: f32,
    boundary_motion_runtime_actual_transfer_cell_count: f32,
    boundary_motion_runtime_patch_rejected_component_count: f32,
    boundary_motion_runtime_patch_rejected_budget_cell_count: f32,
    boundary_motion_runtime_source_fragment_rejected_component_count: f32,
    boundary_motion_runtime_source_fragment_rejected_budget_cell_count: f32,
    boundary_motion_runtime_target_disconnected_rejected_component_count: f32,
    boundary_motion_runtime_target_disconnected_rejected_budget_cell_count: f32,
    boundary_motion_runtime_budget_utilization_ratio: f32,
    boundary_motion_runtime_plate_consistency_limited_ratio: f32,
    boundary_motion_runtime_component_limited_ratio: f32,
    material_reconstruction_hard_capacity_assigned_cell_count: f32,
    material_reconstruction_closure_assigned_cell_count: f32,
    material_reconstruction_rebalanced_cell_count: f32,
    material_reconstruction_capacity_mismatch_cell_count: f32,
    material_reconstruction_non_dominant_assignment_cell_count: f32,
    material_reconstruction_mean_assigned_confidence: f32,
    persistent_material_gap_ratio: f32,
    persistent_material_overlap_ratio: f32,
    persistent_material_unsupported_gap_ratio: f32,
    persistent_material_subduction_overlap_ratio: f32,
    persistent_material_collision_overlap_ratio: f32,
    persistent_material_unsupported_overlap_ratio: f32,
    persistent_material_element_count: f32,
    persistent_material_ownership_marker_count: f32,
    marker_empty_candidate_cell_count: f32,
    marker_single_candidate_cell_count: f32,
    marker_mixed_candidate_cell_count: f32,
    marker_changed_empty_candidate_cell_count: f32,
    marker_changed_single_candidate_cell_count: f32,
    marker_changed_mixed_candidate_cell_count: f32,
    marker_reversed_empty_candidate_cell_count: f32,
    marker_reversed_single_candidate_cell_count: f32,
    marker_reversed_mixed_candidate_cell_count: f32,
    marker_changed_divergent_cell_count: f32,
    marker_changed_subduction_cell_count: f32,
    marker_changed_collision_cell_count: f32,
    marker_changed_transform_cell_count: f32,
    mean_abs_plate_area_delta_ratio: f32,
    max_abs_plate_area_delta_ratio: f32,
    max_plate_area_growth_from_initial: f32,
    mean_slab_pull_drive: f32,
    mean_ridge_push_drive: f32,
    mean_collision_drag: f32,
    mean_force_target_speed_km_per_myr: f32,
    mean_basal_target_speed_km_per_myr: f32,
    mean_articulation_cell_ratio: f32,
    max_articulation_cell_ratio: f32,
    mean_boundary_complexity_growth: f32,
    max_boundary_complexity_growth: f32,
    mean_boundary_complexity_growth_window_mean: f32,
    max_boundary_complexity_growth_window_mean: f32,
    persistent_boundary_complexity_growth_plate_ratio: f32,
    mean_corridor_neck_risk: f32,
    max_corridor_neck_risk: f32,
    mean_boundary_thin_cell_ratio: f32,
    max_boundary_thin_cell_ratio: f32,
    mean_eroded_core_cell_ratio: f32,
    min_eroded_core_cell_ratio: f32,
    mean_plate_block_count: f32,
    max_plate_block_count: u32,
    multi_block_plate_ratio: f32,
    max_secondary_plate_block_ratio: f32,
    mean_weak_line_plate_block_count: f32,
    max_weak_line_plate_block_count: u32,
    weak_line_multi_block_plate_ratio: f32,
    max_secondary_weak_line_plate_block_ratio: f32,
    mean_enclosed_plate_risk: f32,
    max_enclosed_plate_risk: f32,
    mean_appendage_isolation_risk: f32,
    max_appendage_isolation_risk: f32,
    max_multiscale_neck_secondary_ratio: f32,
    max_branch_area_ratio: f32,
    max_branch_slenderness: f32,
    persistent_branch_plate_ratio: f32,
    nearest_generator_agreement_ratio: f32,
    cvt_energy_ratio_from_initial: f32,
    mean_generator_centroid_distance_cells: f32,
    nearest_centroid_voronoi_agreement_ratio: f32,
    centroid_voronoi_energy_ratio_from_initial: f32,
    plates: Vec<PlateMotionRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct PlateMotionRecord {
    plate_id: u32,
    cell_count: u32,
    area_ratio: f32,
    component_count: u32,
    largest_component_ratio: f32,
    detached_fragment_ratio: f32,
    secondary_component_rift_boundary_ratio: f32,
    secondary_component_ridge_boundary_ratio: f32,
    secondary_component_subduction_boundary_ratio: f32,
    secondary_component_collision_boundary_ratio: f32,
    unsupported_detached_component_ratio: f32,
    boundary_complexity: f32,
    boundary_complexity_growth: f32,
    boundary_complexity_growth_window_mean: f32,
    boundary_complexity_growth_window_min: f32,
    persistent_boundary_complexity_growth: bool,
    articulation_cell_count: u32,
    articulation_cell_ratio: f32,
    corridor_core_degree: u32,
    corridor_core_component_count: u32,
    corridor_lobe_balance: f32,
    corridor_neck_risk: f32,
    boundary_distance_p50: f32,
    boundary_distance_max: u32,
    boundary_thin_cell_ratio: f32,
    eroded_core_cell_ratio: f32,
    plate_block_core_degree: u32,
    plate_block_count: u32,
    largest_plate_block_ratio: f32,
    secondary_plate_block_ratio: f32,
    weak_line_plate_block_count: u32,
    largest_weak_line_plate_block_ratio: f32,
    secondary_weak_line_plate_block_ratio: f32,
    dominant_neighbor_plate_id: u32,
    dominant_neighbor_contact_ratio: f32,
    enclosed_plate_risk: f32,
    appendage_core_cell_ratio: f32,
    appendage_cell_ratio: f32,
    appendage_largest_component_ratio: f32,
    appendage_bridge_contact_ratio: f32,
    appendage_foreign_contact_ratio: f32,
    appendage_isolation_risk: f32,
    multiscale_neck_width_cells: u32,
    multiscale_neck_secondary_ratio: f32,
    branch_neck_width_cells: u32,
    branch_area_ratio: f32,
    branch_slenderness: f32,
    speed_km_per_myr: f32,
    cell_crossing_fraction_per_tick: f32,
    direction_persistence: f32,
    centroid_path_straightness: f32,
    euler_rotation_residual_km: f32,
    euler_rotation_residual_ratio: f32,
    boundary_transfer_acquired_cell_count: u32,
    mean_boundary_transfer_velocity_alignment: f32,
    boundary_transfer_velocity_aligned_ratio: f32,
    boundary_transfer_component_count: u32,
    boundary_transfer_largest_component_ratio: f32,
    boundary_transfer_isolated_cell_ratio: f32,
    area_delta_ratio_per_sample: f32,
    area_growth_from_initial: f32,
    slab_pull_drive: f32,
    ridge_push_drive: f32,
    collision_drag: f32,
    force_target_speed_km_per_myr: f32,
    basal_target_speed_km_per_myr: f32,
}

#[derive(Debug, Clone)]
struct PlateShapeRecord {
    cell_count: u32,
    area_ratio: f32,
    component_count: u32,
    largest_component_ratio: f32,
    detached_fragment_ratio: f32,
    secondary_component_rift_boundary_ratio: f32,
    secondary_component_ridge_boundary_ratio: f32,
    secondary_component_subduction_boundary_ratio: f32,
    secondary_component_collision_boundary_ratio: f32,
    unsupported_detached_component_ratio: f32,
    boundary_complexity: f32,
    articulation_cell_count: u32,
    articulation_cell_ratio: f32,
    corridor_core_degree: u32,
    corridor_core_component_count: u32,
    corridor_lobe_balance: f32,
    corridor_neck_risk: f32,
    boundary_distance_p50: f32,
    boundary_distance_max: u32,
    boundary_thin_cell_ratio: f32,
    eroded_core_cell_ratio: f32,
    plate_block_core_degree: u32,
    plate_block_count: u32,
    largest_plate_block_ratio: f32,
    secondary_plate_block_ratio: f32,
    weak_line_plate_block_count: u32,
    largest_weak_line_plate_block_ratio: f32,
    secondary_weak_line_plate_block_ratio: f32,
    dominant_neighbor_plate_id: u32,
    dominant_neighbor_contact_ratio: f32,
    enclosed_plate_risk: f32,
    appendage_core_cell_ratio: f32,
    appendage_cell_ratio: f32,
    appendage_largest_component_ratio: f32,
    appendage_bridge_contact_ratio: f32,
    appendage_foreign_contact_ratio: f32,
    appendage_isolation_risk: f32,
    multiscale_neck_width_cells: u32,
    multiscale_neck_secondary_ratio: f32,
    branch_neck_width_cells: u32,
    branch_area_ratio: f32,
    branch_slenderness: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct CorridorMetrics {
    core_degree: u32,
    component_count: u32,
    lobe_balance: f32,
    neck_risk: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct PlateBlockMetrics {
    core_degree: u32,
    block_count: u32,
    largest_block_ratio: f32,
    secondary_block_ratio: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct MultiscaleShapeMetrics {
    neck_width_cells: u32,
    neck_secondary_ratio: f32,
    branch_neck_width_cells: u32,
    branch_area_ratio: f32,
    branch_slenderness: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct CvtDiagnostics {
    valid: bool,
    nearest_generator_agreement_ratio: f32,
    energy: f32,
    mean_generator_centroid_distance_cells: f32,
}

#[derive(Debug, Clone, Copy)]
struct BoundaryComplexityGrowthWindow {
    mean: f32,
    min: f32,
    persistent: bool,
}

#[derive(Debug, Clone, Copy)]
struct EulerRotationResidual {
    km: f32,
    ratio: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct BoundaryTransferAlignmentSummary {
    candidate_cell_count: u32,
    evaluated_cell_count: u32,
    aligned_cell_count: u32,
    alignment_sum: f32,
    component_count: u32,
    largest_component_cells: u32,
    isolated_cell_count: u32,
}

#[derive(Debug, Default)]
struct MotionTracker {
    initial_centroids: Vec<Option<[f32; 3]>>,
    previous_centroids: Vec<Option<[f32; 3]>>,
    previous_plate_states: Vec<Option<PlateKinematicsState>>,
    previous_velocity_dirs: Vec<Option<[f32; 3]>>,
    previous_sample_tick: Option<u64>,
    previous_previous_plate_id: Vec<PlateId>,
    previous_plate_id: Vec<PlateId>,
    previous_boundary_activity: Vec<f32>,
    centroid_path_lengths_km: Vec<f32>,
    initial_plate_cell_counts: Vec<Option<u32>>,
    previous_plate_cell_counts: Vec<Option<u32>>,
    initial_boundary_complexities: Vec<Option<f32>>,
    boundary_complexity_growth_windows: Vec<VecDeque<f32>>,
    branch_area_ratio_windows: Vec<VecDeque<f32>>,
    mean_cell_spacing_km: Option<f32>,
    initial_cvt_energy: Option<f32>,
    initial_centroid_voronoi_energy: Option<f32>,
}

#[derive(Debug, Clone)]
struct MotionDiagnostics {
    mean_plate_speed_km_per_myr: f32,
    max_plate_speed_km_per_myr: f32,
    mean_cell_crossing_fraction_per_tick: f32,
    max_cell_crossing_fraction_per_tick: f32,
    mean_direction_persistence: f32,
    reciprocal_churn_ratio: f32,
    mutual_exchange_ratio: f32,
    temporal_reversal_ratio: f32,
    temporal_reversal_previous_velocity_aligned_ratio: f32,
    temporal_reversal_current_velocity_aligned_ratio: f32,
    mean_centroid_path_straightness: f32,
    mean_euler_rotation_residual_km: f32,
    max_euler_rotation_residual_km: f32,
    mean_euler_rotation_residual_ratio: f32,
    max_euler_rotation_residual_ratio: f32,
    boundary_transfer_evaluated_cell_count: u32,
    mean_boundary_transfer_velocity_alignment: f32,
    boundary_transfer_velocity_aligned_ratio: f32,
    boundary_transfer_velocity_unaligned_ratio: f32,
    mean_boundary_transfer_largest_component_ratio: f32,
    max_boundary_transfer_isolated_cell_ratio: f32,
    boundary_motion_expected_cell_count: f32,
    boundary_motion_actual_cell_count: u32,
    boundary_motion_response_ratio: f32,
    boundary_motion_underactive_risk: f32,
    boundary_motion_overactive_risk: f32,
    mean_abs_plate_area_delta_ratio: f32,
    max_abs_plate_area_delta_ratio: f32,
    max_plate_area_growth_from_initial: f32,
    mean_slab_pull_drive: f32,
    mean_ridge_push_drive: f32,
    mean_collision_drag: f32,
    mean_force_target_speed_km_per_myr: f32,
    mean_basal_target_speed_km_per_myr: f32,
    mean_articulation_cell_ratio: f32,
    max_articulation_cell_ratio: f32,
    mean_boundary_complexity_growth: f32,
    max_boundary_complexity_growth: f32,
    mean_boundary_complexity_growth_window_mean: f32,
    max_boundary_complexity_growth_window_mean: f32,
    persistent_boundary_complexity_growth_plate_ratio: f32,
    mean_corridor_neck_risk: f32,
    max_corridor_neck_risk: f32,
    mean_boundary_thin_cell_ratio: f32,
    max_boundary_thin_cell_ratio: f32,
    mean_eroded_core_cell_ratio: f32,
    min_eroded_core_cell_ratio: f32,
    mean_plate_block_count: f32,
    max_plate_block_count: u32,
    multi_block_plate_ratio: f32,
    max_secondary_plate_block_ratio: f32,
    mean_weak_line_plate_block_count: f32,
    max_weak_line_plate_block_count: u32,
    weak_line_multi_block_plate_ratio: f32,
    max_secondary_weak_line_plate_block_ratio: f32,
    mean_enclosed_plate_risk: f32,
    max_enclosed_plate_risk: f32,
    mean_appendage_isolation_risk: f32,
    max_appendage_isolation_risk: f32,
    max_multiscale_neck_secondary_ratio: f32,
    max_branch_area_ratio: f32,
    max_branch_slenderness: f32,
    persistent_branch_plate_ratio: f32,
    nearest_generator_agreement_ratio: f32,
    cvt_energy_ratio_from_initial: f32,
    mean_generator_centroid_distance_cells: f32,
    nearest_centroid_voronoi_agreement_ratio: f32,
    centroid_voronoi_energy_ratio_from_initial: f32,
    plates: Vec<PlateMotionRecord>,
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
            mutual_exchange_ratio: 0.0,
            temporal_reversal_ratio: 0.0,
            temporal_reversal_previous_velocity_aligned_ratio: 1.0,
            temporal_reversal_current_velocity_aligned_ratio: 1.0,
            mean_centroid_path_straightness: 1.0,
            mean_euler_rotation_residual_km: 0.0,
            max_euler_rotation_residual_km: 0.0,
            mean_euler_rotation_residual_ratio: 0.0,
            max_euler_rotation_residual_ratio: 0.0,
            boundary_transfer_evaluated_cell_count: 0,
            mean_boundary_transfer_velocity_alignment: 0.0,
            boundary_transfer_velocity_aligned_ratio: 1.0,
            boundary_transfer_velocity_unaligned_ratio: 0.0,
            mean_boundary_transfer_largest_component_ratio: 1.0,
            max_boundary_transfer_isolated_cell_ratio: 0.0,
            boundary_motion_expected_cell_count: 0.0,
            boundary_motion_actual_cell_count: 0,
            boundary_motion_response_ratio: 1.0,
            boundary_motion_underactive_risk: 0.0,
            boundary_motion_overactive_risk: 0.0,
            mean_abs_plate_area_delta_ratio: 0.0,
            max_abs_plate_area_delta_ratio: 0.0,
            max_plate_area_growth_from_initial: 1.0,
            mean_slab_pull_drive: 0.0,
            mean_ridge_push_drive: 0.0,
            mean_collision_drag: 0.0,
            mean_force_target_speed_km_per_myr: 0.0,
            mean_basal_target_speed_km_per_myr: 0.0,
            mean_articulation_cell_ratio: 0.0,
            max_articulation_cell_ratio: 0.0,
            mean_boundary_complexity_growth: 1.0,
            max_boundary_complexity_growth: 1.0,
            mean_boundary_complexity_growth_window_mean: 1.0,
            max_boundary_complexity_growth_window_mean: 1.0,
            persistent_boundary_complexity_growth_plate_ratio: 0.0,
            mean_corridor_neck_risk: 0.0,
            max_corridor_neck_risk: 0.0,
            mean_boundary_thin_cell_ratio: 0.0,
            max_boundary_thin_cell_ratio: 0.0,
            mean_eroded_core_cell_ratio: 0.0,
            min_eroded_core_cell_ratio: 0.0,
            mean_plate_block_count: 0.0,
            max_plate_block_count: 0,
            multi_block_plate_ratio: 0.0,
            max_secondary_plate_block_ratio: 0.0,
            mean_weak_line_plate_block_count: 0.0,
            max_weak_line_plate_block_count: 0,
            weak_line_multi_block_plate_ratio: 0.0,
            max_secondary_weak_line_plate_block_ratio: 0.0,
            mean_enclosed_plate_risk: 0.0,
            max_enclosed_plate_risk: 0.0,
            mean_appendage_isolation_risk: 0.0,
            max_appendage_isolation_risk: 0.0,
            max_multiscale_neck_secondary_ratio: 0.0,
            max_branch_area_ratio: 0.0,
            max_branch_slenderness: 0.0,
            persistent_branch_plate_ratio: 0.0,
            nearest_generator_agreement_ratio: 0.0,
            cvt_energy_ratio_from_initial: 1.0,
            mean_generator_centroid_distance_cells: 0.0,
            nearest_centroid_voronoi_agreement_ratio: 0.0,
            centroid_voronoi_energy_ratio_from_initial: 1.0,
            plates: Vec::new(),
        }
    }
}

fn main() {
    let config = load_config();
    let run_id = env::var("CRUST_PLATE_SERIES_RUN_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(default_run_id);
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
    if let Some(value) = env_u32("CRUST_PLATE_SERIES_OWNERSHIP_MODEL") {
        geology_params.plate_ownership_model = value;
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
    let motion = motion_tracker.sample(world, geology_state, tick);
    TickRecord {
        tick,
        plate_count: unique_plate_count(&world.state.geology.plate_id),
        land_ratio: metrics.land_ratio,
        oceanic_cell_ratio: metrics.oceanic_cell_ratio,
        continental_cell_ratio: metrics.continental_cell_ratio,
        plate_id_churn_rate: runtime_metrics.plate_id_churn_rate,
        boundary_crossing_substeps: runtime_metrics.boundary_crossing_substeps,
        boundary_topology_event_cell_count: runtime_metrics.boundary_topology_event_cell_count,
        boundary_topology_constrained_segment_count: runtime_metrics
            .boundary_topology_constrained_segment_count,
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
        net_exchange_directionality_ratio: motion.reciprocal_churn_ratio,
        mutual_exchange_ratio: motion.mutual_exchange_ratio,
        temporal_reversal_ratio: motion.temporal_reversal_ratio,
        temporal_reversal_previous_velocity_aligned_ratio: motion
            .temporal_reversal_previous_velocity_aligned_ratio,
        temporal_reversal_current_velocity_aligned_ratio: motion
            .temporal_reversal_current_velocity_aligned_ratio,
        mean_centroid_path_straightness: motion.mean_centroid_path_straightness,
        mean_euler_rotation_residual_km: motion.mean_euler_rotation_residual_km,
        max_euler_rotation_residual_km: motion.max_euler_rotation_residual_km,
        mean_euler_rotation_residual_ratio: motion.mean_euler_rotation_residual_ratio,
        max_euler_rotation_residual_ratio: motion.max_euler_rotation_residual_ratio,
        boundary_transfer_evaluated_cell_count: motion.boundary_transfer_evaluated_cell_count,
        mean_boundary_transfer_velocity_alignment: motion.mean_boundary_transfer_velocity_alignment,
        boundary_transfer_velocity_aligned_ratio: motion.boundary_transfer_velocity_aligned_ratio,
        boundary_transfer_velocity_unaligned_ratio: motion
            .boundary_transfer_velocity_unaligned_ratio,
        mean_boundary_transfer_largest_component_ratio: motion
            .mean_boundary_transfer_largest_component_ratio,
        max_boundary_transfer_isolated_cell_ratio: motion.max_boundary_transfer_isolated_cell_ratio,
        boundary_motion_expected_cell_count: motion.boundary_motion_expected_cell_count,
        boundary_motion_actual_cell_count: motion.boundary_motion_actual_cell_count,
        boundary_motion_response_ratio: motion.boundary_motion_response_ratio,
        boundary_motion_underactive_risk: motion.boundary_motion_underactive_risk,
        boundary_motion_overactive_risk: motion.boundary_motion_overactive_risk,
        boundary_motion_runtime_raw_expected_cell_count: runtime_metrics
            .boundary_motion_raw_expected_cell_count,
        boundary_motion_runtime_accumulated_expected_cell_count: runtime_metrics
            .boundary_motion_accumulated_expected_cell_count,
        boundary_motion_runtime_component_budget_cell_count: runtime_metrics
            .boundary_motion_component_budget_cell_count,
        boundary_motion_runtime_transferable_component_budget_cell_count: runtime_metrics
            .boundary_motion_transferable_component_budget_cell_count,
        boundary_motion_runtime_plate_consistency_budget_cell_count: runtime_metrics
            .boundary_motion_plate_consistency_budget_cell_count,
        boundary_motion_runtime_plate_consistency_deferred_cell_count: runtime_metrics
            .boundary_motion_plate_consistency_deferred_cell_count,
        boundary_motion_runtime_plate_consistency_donor_limited_cell_count: runtime_metrics
            .boundary_motion_plate_consistency_donor_limited_cell_count,
        boundary_motion_runtime_plate_consistency_outgoing_limited_cell_count: runtime_metrics
            .boundary_motion_plate_consistency_outgoing_limited_cell_count,
        boundary_motion_runtime_plate_consistency_incoming_limited_cell_count: runtime_metrics
            .boundary_motion_plate_consistency_incoming_limited_cell_count,
        boundary_motion_runtime_plate_consistency_net_area_limited_cell_count: runtime_metrics
            .boundary_motion_plate_consistency_net_area_limited_cell_count,
        boundary_motion_runtime_plate_consistency_max_projected_out_ratio: runtime_metrics
            .boundary_motion_plate_consistency_max_projected_out_ratio,
        boundary_motion_runtime_actual_transfer_cell_count: runtime_metrics
            .boundary_motion_actual_transfer_cell_count,
        boundary_motion_runtime_patch_rejected_component_count: runtime_metrics
            .boundary_motion_patch_rejected_component_count,
        boundary_motion_runtime_patch_rejected_budget_cell_count: runtime_metrics
            .boundary_motion_patch_rejected_budget_cell_count,
        boundary_motion_runtime_source_fragment_rejected_component_count: runtime_metrics
            .boundary_motion_source_fragment_rejected_component_count,
        boundary_motion_runtime_source_fragment_rejected_budget_cell_count: runtime_metrics
            .boundary_motion_source_fragment_rejected_budget_cell_count,
        boundary_motion_runtime_target_disconnected_rejected_component_count: runtime_metrics
            .boundary_motion_target_disconnected_rejected_component_count,
        boundary_motion_runtime_target_disconnected_rejected_budget_cell_count: runtime_metrics
            .boundary_motion_target_disconnected_rejected_budget_cell_count,
        boundary_motion_runtime_budget_utilization_ratio: runtime_metrics
            .boundary_motion_budget_utilization_ratio,
        boundary_motion_runtime_plate_consistency_limited_ratio: runtime_metrics
            .boundary_motion_plate_consistency_limited_ratio,
        boundary_motion_runtime_component_limited_ratio: runtime_metrics
            .boundary_motion_component_limited_ratio,
        material_reconstruction_hard_capacity_assigned_cell_count: runtime_metrics
            .material_reconstruction_hard_capacity_assigned_cell_count,
        material_reconstruction_closure_assigned_cell_count: runtime_metrics
            .material_reconstruction_closure_assigned_cell_count,
        material_reconstruction_rebalanced_cell_count: runtime_metrics
            .material_reconstruction_rebalanced_cell_count,
        material_reconstruction_capacity_mismatch_cell_count: runtime_metrics
            .material_reconstruction_capacity_mismatch_cell_count,
        material_reconstruction_non_dominant_assignment_cell_count: runtime_metrics
            .material_reconstruction_non_dominant_assignment_cell_count,
        material_reconstruction_mean_assigned_confidence: runtime_metrics
            .material_reconstruction_mean_assigned_confidence,
        persistent_material_gap_ratio: runtime_metrics.persistent_material_gap_ratio,
        persistent_material_overlap_ratio: runtime_metrics.persistent_material_overlap_ratio,
        persistent_material_unsupported_gap_ratio: runtime_metrics
            .persistent_material_unsupported_gap_ratio,
        persistent_material_subduction_overlap_ratio: runtime_metrics
            .persistent_material_subduction_overlap_ratio,
        persistent_material_collision_overlap_ratio: runtime_metrics
            .persistent_material_collision_overlap_ratio,
        persistent_material_unsupported_overlap_ratio: runtime_metrics
            .persistent_material_unsupported_overlap_ratio,
        persistent_material_element_count: runtime_metrics.persistent_material_element_count,
        persistent_material_ownership_marker_count: runtime_metrics
            .persistent_material_ownership_marker_count,
        marker_empty_candidate_cell_count: runtime_metrics.marker_empty_candidate_cell_count,
        marker_single_candidate_cell_count: runtime_metrics.marker_single_candidate_cell_count,
        marker_mixed_candidate_cell_count: runtime_metrics.marker_mixed_candidate_cell_count,
        marker_changed_empty_candidate_cell_count: runtime_metrics
            .marker_changed_empty_candidate_cell_count,
        marker_changed_single_candidate_cell_count: runtime_metrics
            .marker_changed_single_candidate_cell_count,
        marker_changed_mixed_candidate_cell_count: runtime_metrics
            .marker_changed_mixed_candidate_cell_count,
        marker_reversed_empty_candidate_cell_count: runtime_metrics
            .marker_reversed_empty_candidate_cell_count,
        marker_reversed_single_candidate_cell_count: runtime_metrics
            .marker_reversed_single_candidate_cell_count,
        marker_reversed_mixed_candidate_cell_count: runtime_metrics
            .marker_reversed_mixed_candidate_cell_count,
        marker_changed_divergent_cell_count: runtime_metrics.marker_changed_divergent_cell_count,
        marker_changed_subduction_cell_count: runtime_metrics.marker_changed_subduction_cell_count,
        marker_changed_collision_cell_count: runtime_metrics.marker_changed_collision_cell_count,
        marker_changed_transform_cell_count: runtime_metrics.marker_changed_transform_cell_count,
        mean_abs_plate_area_delta_ratio: motion.mean_abs_plate_area_delta_ratio,
        max_abs_plate_area_delta_ratio: motion.max_abs_plate_area_delta_ratio,
        max_plate_area_growth_from_initial: motion.max_plate_area_growth_from_initial,
        mean_slab_pull_drive: motion.mean_slab_pull_drive,
        mean_ridge_push_drive: motion.mean_ridge_push_drive,
        mean_collision_drag: motion.mean_collision_drag,
        mean_force_target_speed_km_per_myr: motion.mean_force_target_speed_km_per_myr,
        mean_basal_target_speed_km_per_myr: motion.mean_basal_target_speed_km_per_myr,
        mean_articulation_cell_ratio: motion.mean_articulation_cell_ratio,
        max_articulation_cell_ratio: motion.max_articulation_cell_ratio,
        mean_boundary_complexity_growth: motion.mean_boundary_complexity_growth,
        max_boundary_complexity_growth: motion.max_boundary_complexity_growth,
        mean_boundary_complexity_growth_window_mean: motion
            .mean_boundary_complexity_growth_window_mean,
        max_boundary_complexity_growth_window_mean: motion
            .max_boundary_complexity_growth_window_mean,
        persistent_boundary_complexity_growth_plate_ratio: motion
            .persistent_boundary_complexity_growth_plate_ratio,
        mean_corridor_neck_risk: motion.mean_corridor_neck_risk,
        max_corridor_neck_risk: motion.max_corridor_neck_risk,
        mean_boundary_thin_cell_ratio: motion.mean_boundary_thin_cell_ratio,
        max_boundary_thin_cell_ratio: motion.max_boundary_thin_cell_ratio,
        mean_eroded_core_cell_ratio: motion.mean_eroded_core_cell_ratio,
        min_eroded_core_cell_ratio: motion.min_eroded_core_cell_ratio,
        mean_plate_block_count: motion.mean_plate_block_count,
        max_plate_block_count: motion.max_plate_block_count,
        multi_block_plate_ratio: motion.multi_block_plate_ratio,
        max_secondary_plate_block_ratio: motion.max_secondary_plate_block_ratio,
        mean_weak_line_plate_block_count: motion.mean_weak_line_plate_block_count,
        max_weak_line_plate_block_count: motion.max_weak_line_plate_block_count,
        weak_line_multi_block_plate_ratio: motion.weak_line_multi_block_plate_ratio,
        max_secondary_weak_line_plate_block_ratio: motion.max_secondary_weak_line_plate_block_ratio,
        mean_enclosed_plate_risk: motion.mean_enclosed_plate_risk,
        max_enclosed_plate_risk: motion.max_enclosed_plate_risk,
        mean_appendage_isolation_risk: motion.mean_appendage_isolation_risk,
        max_appendage_isolation_risk: motion.max_appendage_isolation_risk,
        max_multiscale_neck_secondary_ratio: motion.max_multiscale_neck_secondary_ratio,
        max_branch_area_ratio: motion.max_branch_area_ratio,
        max_branch_slenderness: motion.max_branch_slenderness,
        persistent_branch_plate_ratio: motion.persistent_branch_plate_ratio,
        nearest_generator_agreement_ratio: motion.nearest_generator_agreement_ratio,
        cvt_energy_ratio_from_initial: motion.cvt_energy_ratio_from_initial,
        mean_generator_centroid_distance_cells: motion.mean_generator_centroid_distance_cells,
        nearest_centroid_voronoi_agreement_ratio: motion.nearest_centroid_voronoi_agreement_ratio,
        centroid_voronoi_energy_ratio_from_initial: motion
            .centroid_voronoi_energy_ratio_from_initial,
        plates: motion.plates,
    }
}

impl MotionTracker {
    fn sample(
        &mut self,
        world: &frey_wasm::sim::world::World,
        geology_state: &GeologyExecState,
        tick: u64,
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
        let influence_centers = geology_state
            .as_ref()
            .map(|state| state.plate_influence_centers.as_slice())
            .unwrap_or(&[]);
        let cvt = cvt_diagnostics(
            world.mesh().positions.as_slice(),
            plate_id,
            influence_centers,
            &centroids,
            mean_cell_spacing_km / EARTH_MEAN_RADIUS_KM,
        );
        let cvt_energy_ratio_from_initial = if cvt.valid {
            let initial = self.initial_cvt_energy.get_or_insert(cvt.energy.max(1e-8));
            cvt.energy / (*initial).max(1e-8)
        } else {
            1.0
        };
        let centroid_voronoi =
            centroid_voronoi_diagnostics(world.mesh().positions.as_slice(), plate_id, &centroids);
        let centroid_voronoi_energy_ratio_from_initial = if centroid_voronoi.valid {
            let initial = self
                .initial_centroid_voronoi_energy
                .get_or_insert(centroid_voronoi.energy.max(1e-8));
            centroid_voronoi.energy / (*initial).max(1e-8)
        } else {
            1.0
        };
        let plate_states = geology_state
            .as_ref()
            .map(|state| state.plate_states.as_slice())
            .unwrap_or(&[]);
        let boundary_activity = geology_state
            .as_ref()
            .map(|state| state.boundary_state.activity.as_slice())
            .unwrap_or(&[]);
        let boundary_types = geology_state
            .as_ref()
            .map(|state| state.boundary_state.dominant_type.as_slice())
            .unwrap_or(&[]);
        let weakness_proxy = plate_weakness_proxy(world, geology_state);
        let shape_records = plate_shape_records(
            &world.mesh().nbr_offsets,
            &world.mesh().nbrs,
            plate_id,
            weakness_proxy.as_slice(),
            boundary_types,
            plate_count,
        );
        let boundary_transfer_alignment = self.boundary_transfer_alignment(
            world.mesh().positions.as_slice(),
            &world.mesh().nbr_offsets,
            &world.mesh().nbrs,
            plate_id,
            plate_count,
        );
        let boundary_transfer_total =
            summarize_boundary_transfer_alignment(boundary_transfer_alignment.as_slice());
        let sample_ticks = self
            .previous_sample_tick
            .map(|previous_tick| tick.saturating_sub(previous_tick))
            .unwrap_or(0);
        let boundary_motion_expected_cell_count = boundary_motion_expected_cell_count(
            world.mesh().positions.as_slice(),
            &world.mesh().nbr_offsets,
            &world.mesh().nbrs,
            self.previous_plate_id.as_slice(),
            self.previous_plate_states.as_slice(),
            self.previous_boundary_activity.as_slice(),
            sample_ticks,
        );
        let boundary_motion_actual_cell_count = boundary_transfer_total.evaluated_cell_count;
        let boundary_motion_response_ratio = boundary_motion_response_ratio(
            boundary_motion_actual_cell_count,
            boundary_motion_expected_cell_count,
        );
        let boundary_motion_underactive_risk = boundary_motion_underactive_risk(
            boundary_motion_expected_cell_count,
            boundary_motion_response_ratio,
        );
        let boundary_motion_overactive_risk = boundary_motion_overactive_risk(
            boundary_motion_expected_cell_count,
            boundary_motion_response_ratio,
        );

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
        let mut euler_residual_km_sum = 0.0_f32;
        let mut max_euler_residual_km = 0.0_f32;
        let mut euler_residual_ratio_sum = 0.0_f32;
        let mut max_euler_residual_ratio = 0.0_f32;
        let mut euler_residual_count = 0_u32;
        let mut abs_area_delta_ratio_sum = 0.0_f32;
        let mut max_abs_area_delta_ratio = 0.0_f32;
        let mut max_area_growth_from_initial = 1.0_f32;
        let mut slab_pull_drive_sum = 0.0_f32;
        let mut ridge_push_drive_sum = 0.0_f32;
        let mut collision_drag_sum = 0.0_f32;
        let mut force_target_speed_sum = 0.0_f32;
        let mut basal_target_speed_sum = 0.0_f32;
        let mut drive_count = 0_u32;
        let mut articulation_ratio_sum = 0.0_f32;
        let mut max_articulation_ratio = 0.0_f32;
        let mut boundary_complexity_growth_sum = 0.0_f32;
        let mut max_boundary_complexity_growth = 1.0_f32;
        let mut boundary_complexity_growth_window_sum = 0.0_f32;
        let mut max_boundary_complexity_growth_window = 1.0_f32;
        let mut persistent_boundary_complexity_growth_count = 0_u32;
        let mut corridor_neck_risk_sum = 0.0_f32;
        let mut max_corridor_neck_risk = 0.0_f32;
        let mut boundary_thin_ratio_sum = 0.0_f32;
        let mut max_boundary_thin_ratio = 0.0_f32;
        let mut eroded_core_ratio_sum = 0.0_f32;
        let mut min_eroded_core_ratio = 1.0_f32;
        let mut plate_block_count_sum = 0.0_f32;
        let mut max_plate_block_count = 0_u32;
        let mut multi_block_plate_count = 0_u32;
        let mut max_secondary_plate_block_ratio = 0.0_f32;
        let mut weak_line_block_count_sum = 0.0_f32;
        let mut max_weak_line_block_count = 0_u32;
        let mut weak_line_multi_block_plate_count = 0_u32;
        let mut max_secondary_weak_line_block_ratio = 0.0_f32;
        let mut enclosed_plate_risk_sum = 0.0_f32;
        let mut max_enclosed_plate_risk = 0.0_f32;
        let mut appendage_isolation_risk_sum = 0.0_f32;
        let mut max_appendage_isolation_risk = 0.0_f32;
        let mut max_multiscale_neck_secondary_ratio = 0.0_f32;
        let mut max_branch_area_ratio = 0.0_f32;
        let mut max_branch_slenderness = 0.0_f32;
        let mut persistent_branch_plate_count = 0_u32;
        let mut velocity_dirs = vec![None; plate_count];
        let mut plate_records = Vec::<PlateMotionRecord>::new();

        for pid in 0..plate_count {
            let Some(centroid) = centroids[pid] else {
                continue;
            };
            let Some(state) = plate_states.get(pid).copied() else {
                continue;
            };
            let effective_angular_speed = finite_or(state.angular_speed, 0.0).max(0.0);
            let speed_km_per_myr = effective_angular_speed * EARTH_MEAN_RADIUS_KM / myr_per_tick;
            let crossing_fraction =
                speed_km_per_myr * myr_per_tick / mean_cell_spacing_km.max(1e-6);
            speed_sum += speed_km_per_myr;
            speed_count = speed_count.saturating_add(1);
            max_speed = max_speed.max(speed_km_per_myr);
            crossing_sum += crossing_fraction;
            max_crossing = max_crossing.max(crossing_fraction);
            slab_pull_drive_sum += finite_or(state.slab_pull_drive, 0.0).max(0.0);
            ridge_push_drive_sum += finite_or(state.ridge_push_drive, 0.0).max(0.0);
            collision_drag_sum += finite_or(state.collision_drag, 0.0).max(0.0);
            force_target_speed_sum += finite_or(state.force_target_speed_km_per_myr, 0.0).max(0.0);
            basal_target_speed_sum += finite_or(state.basal_target_speed_km_per_myr, 0.0).max(0.0);
            let boundary_complexity_growth =
                self.boundary_complexity_growth(pid, shape_records[pid].boundary_complexity);
            let area_growth_from_initial =
                self.plate_area_growth_from_initial(pid, shape_records[pid].cell_count);
            let area_delta_ratio_per_sample =
                self.plate_area_delta_ratio(pid, shape_records[pid].cell_count);
            abs_area_delta_ratio_sum += area_delta_ratio_per_sample.abs();
            max_abs_area_delta_ratio =
                max_abs_area_delta_ratio.max(area_delta_ratio_per_sample.abs());
            max_area_growth_from_initial =
                max_area_growth_from_initial.max(area_growth_from_initial);
            let boundary_complexity_window =
                self.record_boundary_complexity_growth(pid, boundary_complexity_growth);
            boundary_complexity_growth_sum += boundary_complexity_growth;
            max_boundary_complexity_growth =
                max_boundary_complexity_growth.max(boundary_complexity_growth);
            boundary_complexity_growth_window_sum += boundary_complexity_window.mean;
            max_boundary_complexity_growth_window =
                max_boundary_complexity_growth_window.max(boundary_complexity_window.mean);
            if boundary_complexity_window.persistent {
                persistent_boundary_complexity_growth_count =
                    persistent_boundary_complexity_growth_count.saturating_add(1);
            }
            articulation_ratio_sum += shape_records[pid].articulation_cell_ratio;
            max_articulation_ratio =
                max_articulation_ratio.max(shape_records[pid].articulation_cell_ratio);
            corridor_neck_risk_sum += shape_records[pid].corridor_neck_risk;
            max_corridor_neck_risk =
                max_corridor_neck_risk.max(shape_records[pid].corridor_neck_risk);
            boundary_thin_ratio_sum += shape_records[pid].boundary_thin_cell_ratio;
            max_boundary_thin_ratio =
                max_boundary_thin_ratio.max(shape_records[pid].boundary_thin_cell_ratio);
            eroded_core_ratio_sum += shape_records[pid].eroded_core_cell_ratio;
            min_eroded_core_ratio =
                min_eroded_core_ratio.min(shape_records[pid].eroded_core_cell_ratio);
            plate_block_count_sum += shape_records[pid].plate_block_count as f32;
            max_plate_block_count = max_plate_block_count.max(shape_records[pid].plate_block_count);
            if shape_records[pid].plate_block_count > 1 {
                multi_block_plate_count = multi_block_plate_count.saturating_add(1);
            }
            max_secondary_plate_block_ratio =
                max_secondary_plate_block_ratio.max(shape_records[pid].secondary_plate_block_ratio);
            weak_line_block_count_sum += shape_records[pid].weak_line_plate_block_count as f32;
            max_weak_line_block_count =
                max_weak_line_block_count.max(shape_records[pid].weak_line_plate_block_count);
            if shape_records[pid].weak_line_plate_block_count > 1 {
                weak_line_multi_block_plate_count =
                    weak_line_multi_block_plate_count.saturating_add(1);
            }
            max_secondary_weak_line_block_ratio = max_secondary_weak_line_block_ratio
                .max(shape_records[pid].secondary_weak_line_plate_block_ratio);
            enclosed_plate_risk_sum += shape_records[pid].enclosed_plate_risk;
            max_enclosed_plate_risk =
                max_enclosed_plate_risk.max(shape_records[pid].enclosed_plate_risk);
            appendage_isolation_risk_sum += shape_records[pid].appendage_isolation_risk;
            max_appendage_isolation_risk =
                max_appendage_isolation_risk.max(shape_records[pid].appendage_isolation_risk);
            max_multiscale_neck_secondary_ratio = max_multiscale_neck_secondary_ratio
                .max(shape_records[pid].multiscale_neck_secondary_ratio);
            max_branch_area_ratio = max_branch_area_ratio.max(shape_records[pid].branch_area_ratio);
            max_branch_slenderness =
                max_branch_slenderness.max(shape_records[pid].branch_slenderness);
            if self.record_branch_area_ratio(pid, shape_records[pid].branch_area_ratio) {
                persistent_branch_plate_count = persistent_branch_plate_count.saturating_add(1);
            }
            drive_count = drive_count.saturating_add(1);

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
            let direction_persistence = if let (Some(prev), Some(current)) = (
                self.previous_velocity_dirs
                    .get(pid)
                    .and_then(|value| *value),
                dir,
            ) {
                dot3(prev, current).clamp(-1.0, 1.0)
            } else {
                1.0
            };

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
            let centroid_path_straightness =
                if let Some(initial_centroid) = self.initial_centroids[pid] {
                    let path = self.centroid_path_lengths_km[pid];
                    if path > 1e-6 {
                        let net = great_circle_distance_km(initial_centroid, centroid);
                        (net / path).clamp(0.0, 1.0)
                    } else {
                        1.0
                    }
                } else {
                    1.0
                };
            let euler_residual =
                self.euler_rotation_residual(pid, centroid, tick, mean_cell_spacing_km, state);
            if let Some(residual) = euler_residual {
                euler_residual_km_sum += residual.km;
                max_euler_residual_km = max_euler_residual_km.max(residual.km);
                euler_residual_ratio_sum += residual.ratio;
                max_euler_residual_ratio = max_euler_residual_ratio.max(residual.ratio);
                euler_residual_count = euler_residual_count.saturating_add(1);
            }
            let euler_rotation_residual_km = euler_residual.map(|value| value.km).unwrap_or(0.0);
            let euler_rotation_residual_ratio =
                euler_residual.map(|value| value.ratio).unwrap_or(0.0);
            plate_records.push(PlateMotionRecord {
                plate_id: pid as u32,
                cell_count: shape_records[pid].cell_count,
                area_ratio: shape_records[pid].area_ratio,
                component_count: shape_records[pid].component_count,
                largest_component_ratio: shape_records[pid].largest_component_ratio,
                detached_fragment_ratio: shape_records[pid].detached_fragment_ratio,
                secondary_component_rift_boundary_ratio: shape_records[pid]
                    .secondary_component_rift_boundary_ratio,
                secondary_component_ridge_boundary_ratio: shape_records[pid]
                    .secondary_component_ridge_boundary_ratio,
                secondary_component_subduction_boundary_ratio: shape_records[pid]
                    .secondary_component_subduction_boundary_ratio,
                secondary_component_collision_boundary_ratio: shape_records[pid]
                    .secondary_component_collision_boundary_ratio,
                unsupported_detached_component_ratio: shape_records[pid]
                    .unsupported_detached_component_ratio,
                boundary_complexity: shape_records[pid].boundary_complexity,
                boundary_complexity_growth,
                boundary_complexity_growth_window_mean: boundary_complexity_window.mean,
                boundary_complexity_growth_window_min: boundary_complexity_window.min,
                persistent_boundary_complexity_growth: boundary_complexity_window.persistent,
                articulation_cell_count: shape_records[pid].articulation_cell_count,
                articulation_cell_ratio: shape_records[pid].articulation_cell_ratio,
                corridor_core_degree: shape_records[pid].corridor_core_degree,
                corridor_core_component_count: shape_records[pid].corridor_core_component_count,
                corridor_lobe_balance: shape_records[pid].corridor_lobe_balance,
                corridor_neck_risk: shape_records[pid].corridor_neck_risk,
                boundary_distance_p50: shape_records[pid].boundary_distance_p50,
                boundary_distance_max: shape_records[pid].boundary_distance_max,
                boundary_thin_cell_ratio: shape_records[pid].boundary_thin_cell_ratio,
                eroded_core_cell_ratio: shape_records[pid].eroded_core_cell_ratio,
                plate_block_core_degree: shape_records[pid].plate_block_core_degree,
                plate_block_count: shape_records[pid].plate_block_count,
                largest_plate_block_ratio: shape_records[pid].largest_plate_block_ratio,
                secondary_plate_block_ratio: shape_records[pid].secondary_plate_block_ratio,
                weak_line_plate_block_count: shape_records[pid].weak_line_plate_block_count,
                largest_weak_line_plate_block_ratio: shape_records[pid]
                    .largest_weak_line_plate_block_ratio,
                secondary_weak_line_plate_block_ratio: shape_records[pid]
                    .secondary_weak_line_plate_block_ratio,
                dominant_neighbor_plate_id: shape_records[pid].dominant_neighbor_plate_id,
                dominant_neighbor_contact_ratio: shape_records[pid].dominant_neighbor_contact_ratio,
                enclosed_plate_risk: shape_records[pid].enclosed_plate_risk,
                appendage_core_cell_ratio: shape_records[pid].appendage_core_cell_ratio,
                appendage_cell_ratio: shape_records[pid].appendage_cell_ratio,
                appendage_largest_component_ratio: shape_records[pid]
                    .appendage_largest_component_ratio,
                appendage_bridge_contact_ratio: shape_records[pid].appendage_bridge_contact_ratio,
                appendage_foreign_contact_ratio: shape_records[pid].appendage_foreign_contact_ratio,
                appendage_isolation_risk: shape_records[pid].appendage_isolation_risk,
                multiscale_neck_width_cells: shape_records[pid].multiscale_neck_width_cells,
                multiscale_neck_secondary_ratio: shape_records[pid].multiscale_neck_secondary_ratio,
                branch_neck_width_cells: shape_records[pid].branch_neck_width_cells,
                branch_area_ratio: shape_records[pid].branch_area_ratio,
                branch_slenderness: shape_records[pid].branch_slenderness,
                speed_km_per_myr,
                cell_crossing_fraction_per_tick: crossing_fraction,
                direction_persistence,
                centroid_path_straightness,
                euler_rotation_residual_km,
                euler_rotation_residual_ratio,
                boundary_transfer_acquired_cell_count: boundary_transfer_alignment[pid]
                    .evaluated_cell_count,
                mean_boundary_transfer_velocity_alignment: boundary_transfer_alignment[pid].mean(),
                boundary_transfer_velocity_aligned_ratio: boundary_transfer_alignment[pid]
                    .aligned_ratio(),
                boundary_transfer_component_count: boundary_transfer_alignment[pid].component_count,
                boundary_transfer_largest_component_ratio: boundary_transfer_alignment[pid]
                    .largest_component_ratio(),
                boundary_transfer_isolated_cell_ratio: boundary_transfer_alignment[pid]
                    .isolated_cell_ratio(),
                area_delta_ratio_per_sample,
                area_growth_from_initial,
                slab_pull_drive: finite_or(state.slab_pull_drive, 0.0).max(0.0),
                ridge_push_drive: finite_or(state.ridge_push_drive, 0.0).max(0.0),
                collision_drag: finite_or(state.collision_drag, 0.0).max(0.0),
                force_target_speed_km_per_myr: finite_or(state.force_target_speed_km_per_myr, 0.0)
                    .max(0.0),
                basal_target_speed_km_per_myr: finite_or(state.basal_target_speed_km_per_myr, 0.0)
                    .max(0.0),
            });
        }

        let reciprocal_churn_ratio =
            reciprocal_churn_ratio(self.previous_plate_id.as_slice(), plate_id.as_slice());
        let mutual_exchange_ratio =
            mutual_exchange_ratio(self.previous_plate_id.as_slice(), plate_id.as_slice());
        let temporal_reversal_ratio = temporal_reversal_ratio(
            self.previous_previous_plate_id.as_slice(),
            self.previous_plate_id.as_slice(),
            plate_id.as_slice(),
        );
        let (
            temporal_reversal_previous_velocity_aligned_ratio,
            temporal_reversal_current_velocity_aligned_ratio,
        ) = temporal_reversal_velocity_alignment_ratios(
            world.mesh().positions.as_slice(),
            &world.mesh().nbr_offsets,
            &world.mesh().nbrs,
            self.previous_previous_plate_id.as_slice(),
            self.previous_plate_id.as_slice(),
            plate_id.as_slice(),
            self.previous_plate_states.as_slice(),
        );
        self.previous_centroids = centroids;
        self.previous_plate_states = plate_states
            .iter()
            .copied()
            .map(Some)
            .chain(std::iter::repeat(None))
            .take(plate_count)
            .collect();
        self.previous_velocity_dirs = velocity_dirs;
        self.previous_sample_tick = Some(tick);
        self.previous_plate_cell_counts = shape_records
            .iter()
            .map(|record| {
                if record.cell_count == 0 {
                    None
                } else {
                    Some(record.cell_count)
                }
            })
            .collect();
        self.previous_previous_plate_id =
            std::mem::replace(&mut self.previous_plate_id, plate_id.clone());
        self.previous_boundary_activity = boundary_activity.to_vec();

        MotionDiagnostics {
            mean_plate_speed_km_per_myr: mean_or_zero(speed_sum, speed_count),
            max_plate_speed_km_per_myr: max_speed,
            mean_cell_crossing_fraction_per_tick: mean_or_zero(crossing_sum, speed_count),
            max_cell_crossing_fraction_per_tick: max_crossing,
            mean_direction_persistence: mean_or_one(persistence_sum, persistence_count),
            reciprocal_churn_ratio,
            mutual_exchange_ratio,
            temporal_reversal_ratio,
            temporal_reversal_previous_velocity_aligned_ratio,
            temporal_reversal_current_velocity_aligned_ratio,
            mean_centroid_path_straightness: mean_or_one(straightness_sum, straightness_count),
            mean_euler_rotation_residual_km: mean_or_zero(
                euler_residual_km_sum,
                euler_residual_count,
            ),
            max_euler_rotation_residual_km: max_euler_residual_km,
            mean_euler_rotation_residual_ratio: mean_or_zero(
                euler_residual_ratio_sum,
                euler_residual_count,
            ),
            max_euler_rotation_residual_ratio: max_euler_residual_ratio,
            boundary_transfer_evaluated_cell_count: boundary_transfer_total.evaluated_cell_count,
            mean_boundary_transfer_velocity_alignment: boundary_transfer_total.mean(),
            boundary_transfer_velocity_aligned_ratio: boundary_transfer_total.aligned_ratio(),
            boundary_transfer_velocity_unaligned_ratio: boundary_transfer_total.unaligned_ratio(),
            mean_boundary_transfer_largest_component_ratio:
                mean_boundary_transfer_largest_component_ratio(
                    boundary_transfer_alignment.as_slice(),
                ),
            max_boundary_transfer_isolated_cell_ratio: max_boundary_transfer_isolated_cell_ratio(
                boundary_transfer_alignment.as_slice(),
            ),
            boundary_motion_expected_cell_count,
            boundary_motion_actual_cell_count,
            boundary_motion_response_ratio,
            boundary_motion_underactive_risk,
            boundary_motion_overactive_risk,
            mean_abs_plate_area_delta_ratio: mean_or_zero(abs_area_delta_ratio_sum, drive_count),
            max_abs_plate_area_delta_ratio: max_abs_area_delta_ratio,
            max_plate_area_growth_from_initial: max_area_growth_from_initial,
            mean_slab_pull_drive: mean_or_zero(slab_pull_drive_sum, drive_count),
            mean_ridge_push_drive: mean_or_zero(ridge_push_drive_sum, drive_count),
            mean_collision_drag: mean_or_zero(collision_drag_sum, drive_count),
            mean_force_target_speed_km_per_myr: mean_or_zero(force_target_speed_sum, drive_count),
            mean_basal_target_speed_km_per_myr: mean_or_zero(basal_target_speed_sum, drive_count),
            mean_articulation_cell_ratio: mean_or_zero(articulation_ratio_sum, drive_count),
            max_articulation_cell_ratio: max_articulation_ratio,
            mean_boundary_complexity_growth: mean_or_one(
                boundary_complexity_growth_sum,
                drive_count,
            ),
            max_boundary_complexity_growth,
            mean_boundary_complexity_growth_window_mean: mean_or_one(
                boundary_complexity_growth_window_sum,
                drive_count,
            ),
            max_boundary_complexity_growth_window_mean: max_boundary_complexity_growth_window,
            persistent_boundary_complexity_growth_plate_ratio: mean_or_zero(
                persistent_boundary_complexity_growth_count as f32,
                drive_count,
            ),
            mean_corridor_neck_risk: mean_or_zero(corridor_neck_risk_sum, drive_count),
            max_corridor_neck_risk,
            mean_boundary_thin_cell_ratio: mean_or_zero(boundary_thin_ratio_sum, drive_count),
            max_boundary_thin_cell_ratio: max_boundary_thin_ratio,
            mean_eroded_core_cell_ratio: mean_or_zero(eroded_core_ratio_sum, drive_count),
            min_eroded_core_cell_ratio: if drive_count == 0 {
                0.0
            } else {
                min_eroded_core_ratio
            },
            mean_plate_block_count: mean_or_zero(plate_block_count_sum, drive_count),
            max_plate_block_count,
            multi_block_plate_ratio: mean_or_zero(multi_block_plate_count as f32, drive_count),
            max_secondary_plate_block_ratio,
            mean_weak_line_plate_block_count: mean_or_zero(weak_line_block_count_sum, drive_count),
            max_weak_line_plate_block_count: max_weak_line_block_count,
            weak_line_multi_block_plate_ratio: mean_or_zero(
                weak_line_multi_block_plate_count as f32,
                drive_count,
            ),
            max_secondary_weak_line_plate_block_ratio: max_secondary_weak_line_block_ratio,
            mean_enclosed_plate_risk: mean_or_zero(enclosed_plate_risk_sum, drive_count),
            max_enclosed_plate_risk,
            mean_appendage_isolation_risk: mean_or_zero(appendage_isolation_risk_sum, drive_count),
            max_appendage_isolation_risk,
            max_multiscale_neck_secondary_ratio,
            max_branch_area_ratio,
            max_branch_slenderness,
            persistent_branch_plate_ratio: mean_or_zero(
                persistent_branch_plate_count as f32,
                drive_count,
            ),
            nearest_generator_agreement_ratio: cvt.nearest_generator_agreement_ratio,
            cvt_energy_ratio_from_initial,
            mean_generator_centroid_distance_cells: cvt.mean_generator_centroid_distance_cells,
            nearest_centroid_voronoi_agreement_ratio: centroid_voronoi
                .nearest_generator_agreement_ratio,
            centroid_voronoi_energy_ratio_from_initial,
            plates: plate_records,
        }
    }

    fn resize(&mut self, plate_count: usize) {
        self.initial_centroids.resize(plate_count, None);
        self.previous_centroids.resize(plate_count, None);
        self.previous_plate_states.resize(plate_count, None);
        self.previous_velocity_dirs.resize(plate_count, None);
        self.centroid_path_lengths_km.resize(plate_count, 0.0);
        self.initial_plate_cell_counts.resize(plate_count, None);
        self.previous_plate_cell_counts.resize(plate_count, None);
        self.initial_boundary_complexities.resize(plate_count, None);
        self.boundary_complexity_growth_windows
            .resize_with(plate_count, VecDeque::new);
        self.branch_area_ratio_windows
            .resize_with(plate_count, VecDeque::new);
    }

    fn boundary_complexity_growth(&mut self, plate: usize, current: f32) -> f32 {
        let current = finite_or(current, 0.0).max(0.0);
        let initial = self.initial_boundary_complexities[plate].get_or_insert(current);
        if *initial <= 1e-6 {
            if current <= 1e-6 {
                1.0
            } else {
                current
            }
        } else {
            current / *initial
        }
    }

    fn record_branch_area_ratio(&mut self, plate: usize, ratio: f32) -> bool {
        let window = &mut self.branch_area_ratio_windows[plate];
        window.push_back(finite_or(ratio, 0.0).max(0.0));
        while window.len() > BRANCH_PERSISTENCE_WINDOW_SAMPLES {
            window.pop_front();
        }
        window.len() >= BRANCH_PERSISTENCE_WINDOW_SAMPLES
            && window
                .iter()
                .all(|value| *value >= PERSISTENT_BRANCH_AREA_RATIO_THRESHOLD)
    }

    fn plate_area_growth_from_initial(&mut self, plate: usize, current: u32) -> f32 {
        let current = current.max(1);
        let initial = self.initial_plate_cell_counts[plate].get_or_insert(current);
        current as f32 / (*initial).max(1) as f32
    }

    fn plate_area_delta_ratio(&self, plate: usize, current: u32) -> f32 {
        let Some(previous) = self.previous_plate_cell_counts[plate] else {
            return 0.0;
        };
        let previous = previous.max(1);
        (current as f32 - previous as f32) / previous as f32
    }

    fn record_boundary_complexity_growth(
        &mut self,
        plate: usize,
        growth: f32,
    ) -> BoundaryComplexityGrowthWindow {
        let growth = finite_or(growth, 1.0).max(0.0);
        let window = &mut self.boundary_complexity_growth_windows[plate];
        window.push_back(growth);
        while window.len() > BOUNDARY_COMPLEXITY_GROWTH_WINDOW_SAMPLES {
            window.pop_front();
        }
        let mut sum = 0.0_f32;
        let mut min = f32::MAX;
        for &value in window.iter() {
            sum += value;
            min = min.min(value);
        }
        let mean = if window.is_empty() {
            1.0
        } else {
            finite_or(sum / window.len() as f32, 1.0)
        };
        let min = if window.is_empty() { 1.0 } else { min };
        let persistent = window.len() >= BOUNDARY_COMPLEXITY_GROWTH_WINDOW_SAMPLES
            && min >= PERSISTENT_BOUNDARY_COMPLEXITY_GROWTH_THRESHOLD;

        BoundaryComplexityGrowthWindow {
            mean,
            min,
            persistent,
        }
    }

    fn euler_rotation_residual(
        &self,
        plate: usize,
        current_centroid: [f32; 3],
        current_tick: u64,
        mean_cell_spacing_km: f32,
        current_state: PlateKinematicsState,
    ) -> Option<EulerRotationResidual> {
        let previous_centroid = self
            .previous_centroids
            .get(plate)
            .and_then(|value| *value)?;
        let previous_tick = self.previous_sample_tick?;
        let sample_ticks = current_tick.saturating_sub(previous_tick);
        if sample_ticks == 0 {
            return None;
        }
        let previous_state = self
            .previous_plate_states
            .get(plate)
            .and_then(|value| *value)
            .unwrap_or(current_state);
        let angular_speed = finite_or(previous_state.angular_speed, 0.0).max(0.0);
        let angle = angular_speed * sample_ticks as f32;
        let predicted = rotate_unit_vector(previous_centroid, previous_state.angular_axis, angle);
        let residual_km = great_circle_distance_km(predicted, current_centroid);
        let expected_displacement_km = angle.abs() * EARTH_MEAN_RADIUS_KM;
        let denominator = expected_displacement_km.max(mean_cell_spacing_km).max(1e-6);

        Some(EulerRotationResidual {
            km: residual_km,
            ratio: finite_or(residual_km / denominator, 0.0),
        })
    }

    fn boundary_transfer_alignment(
        &self,
        positions: &[[f32; 3]],
        nbr_offsets: &[u32],
        nbrs: &[u32],
        current_plate_id: &[PlateId],
        plate_count: usize,
    ) -> Vec<BoundaryTransferAlignmentSummary> {
        let mut summaries = vec![BoundaryTransferAlignmentSummary::default(); plate_count];
        if self.previous_plate_id.len() != current_plate_id.len()
            || self.previous_plate_states.is_empty()
        {
            return summaries;
        }

        for cell in 0..current_plate_id.len() {
            let from = self.previous_plate_id[cell];
            let to = current_plate_id[cell];
            if from == to {
                continue;
            }
            let to_plate = to.as_usize();
            if to_plate >= summaries.len() || cell >= positions.len() {
                continue;
            }
            summaries[to_plate].candidate_cell_count =
                summaries[to_plate].candidate_cell_count.saturating_add(1);
            let Some(alignment) = boundary_transfer_cell_alignment(
                positions,
                nbr_offsets,
                nbrs,
                &self.previous_plate_id,
                &self.previous_plate_states,
                cell,
                from,
                to,
            ) else {
                continue;
            };
            summaries[to_plate].record(alignment);
        }
        annotate_boundary_transfer_components(
            nbr_offsets,
            nbrs,
            &self.previous_plate_id,
            current_plate_id,
            &mut summaries,
        );

        summaries
    }
}

impl BoundaryTransferAlignmentSummary {
    fn record(&mut self, alignment: f32) {
        let alignment = finite_or(alignment, 0.0).clamp(-1.0, 1.0);
        self.evaluated_cell_count = self.evaluated_cell_count.saturating_add(1);
        if alignment > 0.0 {
            self.aligned_cell_count = self.aligned_cell_count.saturating_add(1);
        }
        self.alignment_sum += alignment;
    }

    fn merge(&mut self, other: Self) {
        self.candidate_cell_count = self
            .candidate_cell_count
            .saturating_add(other.candidate_cell_count);
        self.evaluated_cell_count = self
            .evaluated_cell_count
            .saturating_add(other.evaluated_cell_count);
        self.aligned_cell_count = self
            .aligned_cell_count
            .saturating_add(other.aligned_cell_count);
        self.alignment_sum += other.alignment_sum;
        self.component_count = self.component_count.saturating_add(other.component_count);
        self.largest_component_cells = self
            .largest_component_cells
            .saturating_add(other.largest_component_cells);
        self.isolated_cell_count = self
            .isolated_cell_count
            .saturating_add(other.isolated_cell_count);
    }

    fn mean(&self) -> f32 {
        mean_or_zero(self.alignment_sum, self.evaluated_cell_count)
    }

    fn aligned_ratio(&self) -> f32 {
        if self.evaluated_cell_count == 0 {
            1.0
        } else {
            self.aligned_cell_count as f32 / self.evaluated_cell_count as f32
        }
    }

    fn unaligned_ratio(&self) -> f32 {
        1.0 - self.aligned_ratio()
    }

    fn largest_component_ratio(&self) -> f32 {
        if self.candidate_cell_count == 0 {
            1.0
        } else {
            self.largest_component_cells as f32 / self.candidate_cell_count as f32
        }
    }

    fn isolated_cell_ratio(&self) -> f32 {
        if self.candidate_cell_count == 0 {
            0.0
        } else {
            self.isolated_cell_count as f32 / self.candidate_cell_count as f32
        }
    }
}

fn summarize_boundary_transfer_alignment(
    summaries: &[BoundaryTransferAlignmentSummary],
) -> BoundaryTransferAlignmentSummary {
    let mut total = BoundaryTransferAlignmentSummary::default();
    for &summary in summaries {
        total.merge(summary);
    }
    total
}

fn plate_weakness_proxy(
    world: &frey_wasm::sim::world::World,
    geology_state: &GeologyExecState,
) -> Vec<f32> {
    let cell_count = world.state.geology.plate_id.len();
    let mut weakness = vec![0.0_f32; cell_count];
    let vertex_states = geology_state
        .as_ref()
        .map(|state| state.vertex_states.as_slice())
        .unwrap_or(&[]);
    for (cell, value) in weakness.iter_mut().enumerate() {
        let boundary = world
            .state
            .geology
            .boundary_condition
            .get(cell)
            .copied()
            .unwrap_or(0.0)
            .max(0.0);
        let runtime_stress = vertex_states
            .get(cell)
            .map(|state| finite_or(state.stress.abs(), 0.0))
            .unwrap_or(0.0);
        let internal_stress = world
            .state
            .geology
            .geology_internal
            .get(cell)
            .map(|state| {
                let tensor = state.stress;
                finite_or(
                    ((tensor.xx * tensor.xx + tensor.yy * tensor.yy).sqrt()
                        + tensor.xy.abs() * 0.5)
                        .abs(),
                    0.0,
                )
            })
            .unwrap_or(0.0);
        *value = boundary
            .max(runtime_stress)
            .max(internal_stress)
            .clamp(0.0, 1.0);
    }
    weakness
}

fn mean_boundary_transfer_largest_component_ratio(
    summaries: &[BoundaryTransferAlignmentSummary],
) -> f32 {
    let mut sum = 0.0_f32;
    let mut count = 0_u32;
    for summary in summaries {
        if summary.candidate_cell_count == 0 {
            continue;
        }
        sum += summary.largest_component_ratio();
        count = count.saturating_add(1);
    }
    mean_or_one(sum, count)
}

fn max_boundary_transfer_isolated_cell_ratio(
    summaries: &[BoundaryTransferAlignmentSummary],
) -> f32 {
    summaries
        .iter()
        .filter(|summary| summary.candidate_cell_count > 0)
        .map(BoundaryTransferAlignmentSummary::isolated_cell_ratio)
        .fold(0.0_f32, f32::max)
}

fn annotate_boundary_transfer_components(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    previous_plate_id: &[PlateId],
    current_plate_id: &[PlateId],
    summaries: &mut [BoundaryTransferAlignmentSummary],
) {
    let mut acquired_by_plate = vec![Vec::<usize>::new(); summaries.len()];
    for cell in 0..current_plate_id.len() {
        let previous = previous_plate_id.get(cell).copied();
        let current = current_plate_id[cell];
        if previous == Some(current) {
            continue;
        }
        let plate = current.as_usize();
        if plate < acquired_by_plate.len() {
            acquired_by_plate[plate].push(cell);
        }
    }

    let mut acquired = vec![false; current_plate_id.len()];
    let mut visited = vec![false; current_plate_id.len()];
    let mut stack = Vec::<usize>::new();
    for (plate, cells) in acquired_by_plate.iter().enumerate() {
        if cells.is_empty() {
            continue;
        }
        for &cell in cells {
            if cell < acquired.len() {
                acquired[cell] = true;
            }
        }

        let mut component_count = 0_u32;
        let mut largest_component = 0_u32;
        let mut isolated_count = 0_u32;
        for &start_cell in cells {
            if start_cell >= visited.len() || visited[start_cell] {
                continue;
            }
            visited[start_cell] = true;
            stack.push(start_cell);
            let mut component_cells = 0_u32;
            while let Some(cell) = stack.pop() {
                component_cells = component_cells.saturating_add(1);
                let start = nbr_offsets[cell] as usize;
                let end = nbr_offsets[cell + 1] as usize;
                for &neighbor_u32 in &nbrs[start..end] {
                    let neighbor = neighbor_u32 as usize;
                    if neighbor >= acquired.len() || visited[neighbor] || !acquired[neighbor] {
                        continue;
                    }
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
            component_count = component_count.saturating_add(1);
            if component_cells == 1 {
                isolated_count = isolated_count.saturating_add(1);
            }
            largest_component = largest_component.max(component_cells);
        }

        summaries[plate].component_count = component_count;
        summaries[plate].largest_component_cells = largest_component;
        summaries[plate].isolated_cell_count = isolated_count;
        for &cell in cells {
            if cell < acquired.len() {
                acquired[cell] = false;
                visited[cell] = false;
            }
        }
    }
}

fn boundary_transfer_cell_alignment(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    previous_plate_id: &[PlateId],
    previous_plate_states: &[Option<PlateKinematicsState>],
    cell: usize,
    from: PlateId,
    to: PlateId,
) -> Option<f32> {
    let from_state = previous_plate_states
        .get(from.as_usize())
        .and_then(|value| value.as_ref());
    let to_state = previous_plate_states
        .get(to.as_usize())
        .and_then(|value| value.as_ref());
    let cell_position = *positions.get(cell)?;
    let from_velocity = plate_velocity_from_optional_state(from_state, from, cell_position);
    let start = *nbr_offsets.get(cell)? as usize;
    let end = *nbr_offsets.get(cell + 1)? as usize;
    let mut best_alignment: Option<f32> = None;

    for &neighbor_u32 in nbrs.get(start..end)? {
        let neighbor = neighbor_u32 as usize;
        if previous_plate_id.get(neighbor).copied() != Some(to) {
            continue;
        }
        let Some(&neighbor_position) = positions.get(neighbor) else {
            continue;
        };
        let Some(dir) = normalized([
            cell_position[0] - neighbor_position[0],
            cell_position[1] - neighbor_position[1],
            cell_position[2] - neighbor_position[2],
        ]) else {
            continue;
        };
        let to_velocity = plate_velocity_from_optional_state(to_state, to, neighbor_position);
        let neighbor_inflow = dot3(to_velocity, dir);
        let current_motion = dot3(from_velocity, dir);
        let relative_inflow = neighbor_inflow - current_motion;
        let denom = neighbor_inflow.abs() + current_motion.abs() + 1e-6;
        let alignment = finite_or(relative_inflow / denom, 0.0).clamp(-1.0, 1.0);
        best_alignment = Some(best_alignment.map_or(alignment, |best| best.max(alignment)));
    }

    best_alignment
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

fn cvt_diagnostics(
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    generators: &[[f32; 3]],
    centroids: &[Option<[f32; 3]>],
    mean_cell_spacing_radians: f32,
) -> CvtDiagnostics {
    if positions.len() != plate_id.len()
        || generators.is_empty()
        || plate_id
            .iter()
            .any(|plate| plate.as_usize() >= generators.len())
    {
        return CvtDiagnostics::default();
    }
    let mut agreement_count = 0_u32;
    let mut energy_sum = 0.0_f32;
    for (&position, &plate) in positions.iter().zip(plate_id) {
        let nearest = generators
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| dot3(position, **a).total_cmp(&dot3(position, **b)))
            .map(|(index, _)| index)
            .unwrap_or(plate.as_usize());
        if nearest == plate.as_usize() {
            agreement_count = agreement_count.saturating_add(1);
        }
        let cosine = dot3(position, generators[plate.as_usize()]).clamp(-1.0, 1.0);
        let distance = cosine.acos();
        energy_sum += distance * distance;
    }
    let mut centroid_distance_sum = 0.0_f32;
    let mut centroid_count = 0_u32;
    for (generator, centroid) in generators.iter().zip(centroids) {
        let Some(centroid) = centroid else {
            continue;
        };
        centroid_distance_sum += dot3(*generator, *centroid).clamp(-1.0, 1.0).acos()
            / mean_cell_spacing_radians.max(1e-6);
        centroid_count = centroid_count.saturating_add(1);
    }
    CvtDiagnostics {
        valid: true,
        nearest_generator_agreement_ratio: agreement_count as f32 / positions.len().max(1) as f32,
        energy: energy_sum / positions.len().max(1) as f32,
        mean_generator_centroid_distance_cells: mean_or_zero(centroid_distance_sum, centroid_count),
    }
}

fn centroid_voronoi_diagnostics(
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    centroids: &[Option<[f32; 3]>],
) -> CvtDiagnostics {
    if positions.len() != plate_id.len() || centroids.iter().all(Option::is_none) {
        return CvtDiagnostics::default();
    }
    let mut agreement_count = 0_u32;
    let mut energy_sum = 0.0_f32;
    let mut evaluated_count = 0_u32;
    for (&position, &plate) in positions.iter().zip(plate_id) {
        let Some(own_centroid) = centroids.get(plate.as_usize()).and_then(|value| *value) else {
            continue;
        };
        let nearest = centroids
            .iter()
            .enumerate()
            .filter_map(|(index, centroid)| centroid.map(|value| (index, value)))
            .max_by(|(_, a), (_, b)| dot3(position, *a).total_cmp(&dot3(position, *b)))
            .map(|(index, _)| index);
        if nearest == Some(plate.as_usize()) {
            agreement_count = agreement_count.saturating_add(1);
        }
        let distance = dot3(position, own_centroid).clamp(-1.0, 1.0).acos();
        energy_sum += distance * distance;
        evaluated_count = evaluated_count.saturating_add(1);
    }
    CvtDiagnostics {
        valid: evaluated_count > 0,
        nearest_generator_agreement_ratio: mean_or_zero(agreement_count as f32, evaluated_count),
        energy: mean_or_zero(energy_sum, evaluated_count),
        mean_generator_centroid_distance_cells: 0.0,
    }
}

fn plate_shape_records(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    weakness: &[f32],
    boundary_types: &[BoundaryType],
    plate_count: usize,
) -> Vec<PlateShapeRecord> {
    let total_cells = plate_id.len().max(1) as f32;
    let mut cell_counts = vec![0_u32; plate_count];
    let mut boundary_contacts = vec![0_u32; plate_count];
    for (cell, &pid) in plate_id.iter().enumerate() {
        let plate = pid.as_usize();
        if plate >= plate_count {
            continue;
        }
        cell_counts[plate] = cell_counts[plate].saturating_add(1);
        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        for &neighbor_u32 in &nbrs[start..end] {
            let neighbor = neighbor_u32 as usize;
            if neighbor >= plate_id.len() || plate_id[neighbor] == pid {
                continue;
            }
            boundary_contacts[plate] = boundary_contacts[plate].saturating_add(1);
        }
    }

    let mut records = vec![
        PlateShapeRecord {
            cell_count: 0,
            area_ratio: 0.0,
            component_count: 0,
            largest_component_ratio: 0.0,
            detached_fragment_ratio: 0.0,
            secondary_component_rift_boundary_ratio: 0.0,
            secondary_component_ridge_boundary_ratio: 0.0,
            secondary_component_subduction_boundary_ratio: 0.0,
            secondary_component_collision_boundary_ratio: 0.0,
            unsupported_detached_component_ratio: 0.0,
            boundary_complexity: 0.0,
            articulation_cell_count: 0,
            articulation_cell_ratio: 0.0,
            corridor_core_degree: 0,
            corridor_core_component_count: 0,
            corridor_lobe_balance: 0.0,
            corridor_neck_risk: 0.0,
            boundary_distance_p50: 0.0,
            boundary_distance_max: 0,
            boundary_thin_cell_ratio: 0.0,
            eroded_core_cell_ratio: 0.0,
            plate_block_core_degree: 0,
            plate_block_count: 0,
            largest_plate_block_ratio: 0.0,
            secondary_plate_block_ratio: 0.0,
            weak_line_plate_block_count: 0,
            largest_weak_line_plate_block_ratio: 0.0,
            secondary_weak_line_plate_block_ratio: 0.0,
            dominant_neighbor_plate_id: u32::MAX,
            dominant_neighbor_contact_ratio: 0.0,
            enclosed_plate_risk: 0.0,
            appendage_core_cell_ratio: 0.0,
            appendage_cell_ratio: 0.0,
            appendage_largest_component_ratio: 0.0,
            appendage_bridge_contact_ratio: 0.0,
            appendage_foreign_contact_ratio: 0.0,
            appendage_isolation_risk: 0.0,
            multiscale_neck_width_cells: 0,
            multiscale_neck_secondary_ratio: 0.0,
            branch_neck_width_cells: 0,
            branch_area_ratio: 0.0,
            branch_slenderness: 0.0,
        };
        plate_count
    ];
    let mut visited = vec![false; plate_id.len()];
    let mut stack = Vec::<usize>::new();

    for plate in 0..plate_count {
        let cells = cell_counts[plate];
        if cells == 0 {
            continue;
        }
        let mut component_count = 0_u32;
        let mut largest_component_cells = 0_u32;
        let mut second_largest_component_cells = 0_u32;
        let mut components = Vec::<Vec<usize>>::new();
        for start_cell in 0..plate_id.len() {
            if visited[start_cell] || plate_id[start_cell].as_usize() != plate {
                continue;
            }
            visited[start_cell] = true;
            stack.push(start_cell);
            let mut component_cells = 0_u32;
            let mut component = Vec::new();
            while let Some(cell) = stack.pop() {
                component_cells = component_cells.saturating_add(1);
                component.push(cell);
                let start = nbr_offsets[cell] as usize;
                let end = nbr_offsets[cell + 1] as usize;
                for &neighbor_u32 in &nbrs[start..end] {
                    let neighbor = neighbor_u32 as usize;
                    if neighbor >= plate_id.len()
                        || visited[neighbor]
                        || plate_id[neighbor].as_usize() != plate
                    {
                        continue;
                    }
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
            component_count = component_count.saturating_add(1);
            if component_cells > largest_component_cells {
                second_largest_component_cells = largest_component_cells;
                largest_component_cells = component_cells;
            } else if component_cells > second_largest_component_cells {
                second_largest_component_cells = component_cells;
            }
            components.push(component);
        }
        components.sort_by_key(|component| std::cmp::Reverse(component.len()));
        let secondary_regimes = components
            .get(1)
            .map(|component| {
                component_boundary_regimes(component, nbr_offsets, nbrs, plate_id, boundary_types)
            })
            .unwrap_or_default();
        let largest_component_ratio = largest_component_cells as f32 / cells.max(1) as f32;
        let articulation_cell_count =
            articulation_cell_count(nbr_offsets, nbrs, plate_id, plate) as u32;
        let corridor = corridor_metrics(nbr_offsets, nbrs, plate_id, plate, cells as usize);
        let block = plate_block_metrics(
            nbr_offsets,
            nbrs,
            plate_id,
            plate,
            cells as usize,
            component_count,
            largest_component_cells as usize,
            second_largest_component_cells as usize,
        );
        let weak_line_block = weak_line_plate_block_metrics(
            nbr_offsets,
            nbrs,
            plate_id,
            weakness,
            plate,
            cells as usize,
        );
        let boundary_profile =
            boundary_distance_profile(nbr_offsets, nbrs, plate_id, plate, cells as usize);
        let enclosure = enclosure_metrics(nbr_offsets, nbrs, plate_id, plate, cells as usize);
        let appendage =
            appendage_isolation_metrics(nbr_offsets, nbrs, plate_id, plate, cells as usize, 3);
        let multiscale = multiscale_shape_metrics(
            nbr_offsets,
            nbrs,
            plate_id,
            plate,
            cells as usize,
            MULTISCALE_MAX_EROSION_CELLS,
        );
        records[plate] = PlateShapeRecord {
            cell_count: cells,
            area_ratio: cells as f32 / total_cells,
            component_count,
            largest_component_ratio,
            detached_fragment_ratio: 1.0 - largest_component_ratio,
            secondary_component_rift_boundary_ratio: secondary_regimes.rift,
            secondary_component_ridge_boundary_ratio: secondary_regimes.ridge,
            secondary_component_subduction_boundary_ratio: secondary_regimes.subduction,
            secondary_component_collision_boundary_ratio: secondary_regimes.collision,
            unsupported_detached_component_ratio: if secondary_regimes.causal_support() >= 0.5 {
                0.0
            } else {
                1.0 - largest_component_ratio
            },
            boundary_complexity: boundary_contacts[plate] as f32 / (cells as f32).sqrt().max(1.0),
            articulation_cell_count,
            articulation_cell_ratio: articulation_cell_count as f32 / cells.max(1) as f32,
            corridor_core_degree: corridor.core_degree,
            corridor_core_component_count: corridor.component_count,
            corridor_lobe_balance: corridor.lobe_balance,
            corridor_neck_risk: corridor.neck_risk,
            boundary_distance_p50: boundary_profile.p50,
            boundary_distance_max: boundary_profile.max,
            boundary_thin_cell_ratio: boundary_profile.thin_ratio,
            eroded_core_cell_ratio: boundary_profile.eroded_core_ratio,
            plate_block_core_degree: block.core_degree,
            plate_block_count: block.block_count,
            largest_plate_block_ratio: block.largest_block_ratio,
            secondary_plate_block_ratio: block.secondary_block_ratio,
            weak_line_plate_block_count: weak_line_block.block_count,
            largest_weak_line_plate_block_ratio: weak_line_block.largest_block_ratio,
            secondary_weak_line_plate_block_ratio: weak_line_block.secondary_block_ratio,
            dominant_neighbor_plate_id: enclosure.dominant_neighbor_plate_id,
            dominant_neighbor_contact_ratio: enclosure.dominant_neighbor_contact_ratio,
            enclosed_plate_risk: enclosure.enclosed_plate_risk,
            appendage_core_cell_ratio: appendage.core_cell_ratio,
            appendage_cell_ratio: appendage.appendage_cell_ratio,
            appendage_largest_component_ratio: appendage.largest_component_ratio,
            appendage_bridge_contact_ratio: appendage.bridge_contact_ratio,
            appendage_foreign_contact_ratio: appendage.foreign_contact_ratio,
            appendage_isolation_risk: appendage.isolation_risk,
            multiscale_neck_width_cells: multiscale.neck_width_cells,
            multiscale_neck_secondary_ratio: multiscale.neck_secondary_ratio,
            branch_neck_width_cells: multiscale.branch_neck_width_cells,
            branch_area_ratio: multiscale.branch_area_ratio,
            branch_slenderness: multiscale.branch_slenderness,
        };
    }

    records
}

#[derive(Debug, Clone, Copy, Default)]
struct ComponentBoundaryRegimes {
    rift: f32,
    ridge: f32,
    subduction: f32,
    collision: f32,
}

impl ComponentBoundaryRegimes {
    fn causal_support(self) -> f32 {
        (self.rift + self.ridge + self.subduction + self.collision).clamp(0.0, 1.0)
    }
}

fn component_boundary_regimes(
    component: &[usize],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    boundary_types: &[BoundaryType],
) -> ComponentBoundaryRegimes {
    let Some(&first) = component.first() else {
        return Default::default();
    };
    let plate = plate_id[first];
    let mut total = 0_u32;
    let mut rift = 0_u32;
    let mut ridge = 0_u32;
    let mut subduction = 0_u32;
    let mut collision = 0_u32;
    for &cell in component {
        let is_boundary = nbrs[nbr_offsets[cell] as usize..nbr_offsets[cell + 1] as usize]
            .iter()
            .any(|&neighbor| plate_id.get(neighbor as usize) != Some(&plate));
        if !is_boundary {
            continue;
        }
        total = total.saturating_add(1);
        match boundary_types.get(cell).copied() {
            Some(BoundaryType::Rift) => rift = rift.saturating_add(1),
            Some(BoundaryType::Ridge) => ridge = ridge.saturating_add(1),
            Some(BoundaryType::Subduction) => subduction = subduction.saturating_add(1),
            Some(BoundaryType::Collision) => collision = collision.saturating_add(1),
            _ => {}
        }
    }
    let denominator = total.max(1) as f32;
    ComponentBoundaryRegimes {
        rift: rift as f32 / denominator,
        ridge: ridge as f32 / denominator,
        subduction: subduction as f32 / denominator,
        collision: collision as f32 / denominator,
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct BoundaryDistanceProfile {
    p50: f32,
    max: u32,
    thin_ratio: f32,
    eroded_core_ratio: f32,
}

#[derive(Debug, Clone, Copy)]
struct EnclosureMetrics {
    dominant_neighbor_plate_id: u32,
    dominant_neighbor_contact_ratio: f32,
    enclosed_plate_risk: f32,
}

impl Default for EnclosureMetrics {
    fn default() -> Self {
        Self {
            dominant_neighbor_plate_id: u32::MAX,
            dominant_neighbor_contact_ratio: 0.0,
            enclosed_plate_risk: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct AppendageIsolationMetrics {
    core_cell_ratio: f32,
    appendage_cell_ratio: f32,
    largest_component_ratio: f32,
    bridge_contact_ratio: f32,
    foreign_contact_ratio: f32,
    isolation_risk: f32,
}

fn enclosure_metrics(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
    plate_cells: usize,
) -> EnclosureMetrics {
    if plate_cells == 0 {
        return EnclosureMetrics::default();
    }
    let mut contacts = BTreeMap::<u32, u32>::new();
    let mut total_contacts = 0_u32;
    for cell in 0..plate_id.len() {
        if plate_id[cell].as_usize() != plate {
            continue;
        }
        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        for &neighbor_u32 in &nbrs[start..end] {
            let neighbor = neighbor_u32 as usize;
            if neighbor >= plate_id.len() || plate_id[neighbor].as_usize() == plate {
                continue;
            }
            total_contacts = total_contacts.saturating_add(1);
            *contacts.entry(plate_id[neighbor].as_u32()).or_insert(0) += 1;
        }
    }
    let Some((dominant_plate, dominant_contacts)) =
        contacts.into_iter().max_by_key(|(_, count)| *count)
    else {
        return EnclosureMetrics::default();
    };
    let dominant_ratio = dominant_contacts as f32 / total_contacts.max(1) as f32;
    let area_ratio = plate_cells as f32 / plate_id.len().max(1) as f32;
    let small_plate_gate =
        ((ENCLOSED_PLATE_AREA_GATE - area_ratio) / ENCLOSED_PLATE_AREA_GATE).clamp(0.0, 1.0);
    EnclosureMetrics {
        dominant_neighbor_plate_id: dominant_plate,
        dominant_neighbor_contact_ratio: dominant_ratio,
        enclosed_plate_risk: dominant_ratio * small_plate_gate,
    }
}

fn appendage_isolation_metrics(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
    plate_cells: usize,
    core_min_degree: usize,
) -> AppendageIsolationMetrics {
    if plate_cells == 0 {
        return AppendageIsolationMetrics::default();
    }
    let retained = plate_k_core(nbr_offsets, nbrs, plate_id, plate, core_min_degree);
    let core_cells = retained.iter().filter(|&&value| value).count();
    if core_cells == 0 || core_cells == plate_cells {
        return AppendageIsolationMetrics {
            core_cell_ratio: core_cells as f32 / plate_cells as f32,
            appendage_cell_ratio: (plate_cells.saturating_sub(core_cells)) as f32
                / plate_cells as f32,
            ..Default::default()
        };
    }

    let mut visited = vec![false; plate_id.len()];
    let mut stack = Vec::<usize>::new();
    let mut best_component_cells = 0_usize;
    let mut best_bridge_contacts = 0_u32;
    let mut best_foreign_contacts = 0_u32;
    let mut best_isolation_risk = 0.0_f32;

    for start_cell in 0..plate_id.len() {
        if visited[start_cell] || retained[start_cell] || plate_id[start_cell].as_usize() != plate {
            continue;
        }
        visited[start_cell] = true;
        stack.push(start_cell);
        let mut component_cells = 0_usize;
        let mut bridge_contacts = 0_u32;
        let mut foreign_contacts = 0_u32;
        while let Some(cell) = stack.pop() {
            component_cells = component_cells.saturating_add(1);
            let start = nbr_offsets[cell] as usize;
            let end = nbr_offsets[cell + 1] as usize;
            for &neighbor_u32 in &nbrs[start..end] {
                let neighbor = neighbor_u32 as usize;
                if neighbor >= plate_id.len() {
                    foreign_contacts = foreign_contacts.saturating_add(1);
                    continue;
                }
                if plate_id[neighbor].as_usize() != plate {
                    foreign_contacts = foreign_contacts.saturating_add(1);
                    continue;
                }
                if retained[neighbor] {
                    bridge_contacts = bridge_contacts.saturating_add(1);
                    continue;
                }
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        let component_ratio = component_cells as f32 / plate_cells as f32;
        let bridge_ratio = bridge_contacts as f32 / component_cells.max(1) as f32;
        let foreign_ratio =
            foreign_contacts as f32 / (foreign_contacts + bridge_contacts).max(1) as f32;
        let narrow_bridge_gate = (1.0 / (1.0 + bridge_ratio)).clamp(0.0, 1.0);
        let isolation_risk = component_ratio * foreign_ratio * narrow_bridge_gate;
        if isolation_risk > best_isolation_risk {
            best_component_cells = component_cells;
            best_bridge_contacts = bridge_contacts;
            best_foreign_contacts = foreign_contacts;
            best_isolation_risk = isolation_risk;
        }
    }

    let appendage_cells = plate_cells.saturating_sub(core_cells);
    AppendageIsolationMetrics {
        core_cell_ratio: core_cells as f32 / plate_cells as f32,
        appendage_cell_ratio: appendage_cells as f32 / plate_cells as f32,
        largest_component_ratio: best_component_cells as f32 / plate_cells as f32,
        bridge_contact_ratio: best_bridge_contacts as f32 / best_component_cells.max(1) as f32,
        foreign_contact_ratio: best_foreign_contacts as f32
            / (best_foreign_contacts + best_bridge_contacts).max(1) as f32,
        isolation_risk: best_isolation_risk,
    }
}

fn boundary_distance_profile(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
    plate_cells: usize,
) -> BoundaryDistanceProfile {
    if plate_cells == 0 {
        return BoundaryDistanceProfile::default();
    }
    let mut distance = vec![u32::MAX; plate_id.len()];
    let mut queue = VecDeque::<usize>::new();
    for cell in 0..plate_id.len() {
        if plate_id[cell].as_usize() != plate {
            continue;
        }
        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        let is_boundary = nbrs[start..end].iter().any(|&neighbor_u32| {
            let neighbor = neighbor_u32 as usize;
            neighbor >= plate_id.len() || plate_id[neighbor].as_usize() != plate
        });
        if is_boundary {
            distance[cell] = 0;
            queue.push_back(cell);
        }
    }

    while let Some(cell) = queue.pop_front() {
        let next_distance = distance[cell].saturating_add(1);
        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        for &neighbor_u32 in &nbrs[start..end] {
            let neighbor = neighbor_u32 as usize;
            if neighbor >= plate_id.len()
                || plate_id[neighbor].as_usize() != plate
                || distance[neighbor] <= next_distance
            {
                continue;
            }
            distance[neighbor] = next_distance;
            queue.push_back(neighbor);
        }
    }

    let mut values = Vec::<u32>::with_capacity(plate_cells);
    for cell in 0..plate_id.len() {
        if plate_id[cell].as_usize() == plate {
            values.push(distance[cell]);
        }
    }
    values.sort_unstable();
    let max = values.last().copied().unwrap_or(0);
    let p50 = percentile_u32(&values, 0.5);
    let thin_count = values
        .iter()
        .filter(|&&value| value <= BOUNDARY_DISTANCE_THIN_CELLS)
        .count();
    let core_count = values
        .iter()
        .filter(|&&value| value > CORE_EROSION_LAYERS)
        .count();

    BoundaryDistanceProfile {
        p50,
        max,
        thin_ratio: thin_count as f32 / plate_cells as f32,
        eroded_core_ratio: core_count as f32 / plate_cells as f32,
    }
}

fn multiscale_shape_metrics(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
    plate_cells: usize,
    max_erosion_cells: u32,
) -> MultiscaleShapeMetrics {
    if plate_cells == 0 {
        return MultiscaleShapeMetrics::default();
    }
    let distances = plate_boundary_distances(nbr_offsets, nbrs, plate_id, plate);
    let min_core_cells = MULTISCALE_MIN_CORE_CELLS
        .max((plate_cells as f32 * MULTISCALE_MIN_CORE_RATIO).ceil() as usize);
    let mut best = MultiscaleShapeMetrics::default();

    for radius in 1..=max_erosion_cells {
        let retained = distances
            .iter()
            .map(|&distance| distance != u32::MAX && distance >= radius)
            .collect::<Vec<_>>();
        let mut cores = masked_components(nbr_offsets, nbrs, plate_id, plate, &retained);
        cores.retain(|component| component.len() >= min_core_cells);
        cores.sort_by_key(|component| std::cmp::Reverse(component.len()));

        if cores.len() >= 2 {
            let secondary_ratio = cores[1].len() as f32 / plate_cells as f32;
            if secondary_ratio > best.neck_secondary_ratio {
                best.neck_width_cells = radius.saturating_mul(2);
                best.neck_secondary_ratio = secondary_ratio;
            }
        }
        if cores.is_empty() {
            continue;
        }

        let opened =
            dilate_mask_within_plate(nbr_offsets, nbrs, plate_id, plate, &retained, radius);
        let residual = opened
            .iter()
            .enumerate()
            .map(|(cell, &is_opened)| plate_id[cell].as_usize() == plate && !is_opened)
            .collect::<Vec<_>>();
        for branch in masked_components(nbr_offsets, nbrs, plate_id, plate, &residual) {
            if branch.len() < 4 {
                continue;
            }
            let bridge_width = branch_bridge_width(&branch, nbr_offsets, nbrs, &opened);
            if bridge_width == 0 {
                continue;
            }
            let diameter = component_graph_diameter(&branch, nbr_offsets, nbrs);
            let half_width = branch
                .iter()
                .map(|&cell| distances[cell])
                .filter(|distance| *distance != u32::MAX)
                .max()
                .unwrap_or(0);
            let local_width = half_width.saturating_mul(2).saturating_add(1).max(1);
            let slenderness = (diameter.saturating_add(1)) as f32 / local_width as f32;
            let area_ratio = branch.len() as f32 / plate_cells as f32;
            if slenderness > best.branch_slenderness
                || slenderness == best.branch_slenderness && area_ratio > best.branch_area_ratio
            {
                best.branch_neck_width_cells = bridge_width;
                best.branch_area_ratio = area_ratio;
                best.branch_slenderness = slenderness;
            }
        }
    }
    best
}

fn plate_boundary_distances(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
) -> Vec<u32> {
    let mut distances = vec![u32::MAX; plate_id.len()];
    let mut queue = VecDeque::new();
    for cell in 0..plate_id.len() {
        if plate_id[cell].as_usize() != plate {
            continue;
        }
        if cell_neighbors(cell, nbr_offsets, nbrs)
            .iter()
            .any(|&neighbor| plate_id[neighbor as usize].as_usize() != plate)
        {
            distances[cell] = 0;
            queue.push_back(cell);
        }
    }
    while let Some(cell) = queue.pop_front() {
        let next = distances[cell].saturating_add(1);
        for &neighbor in cell_neighbors(cell, nbr_offsets, nbrs) {
            let neighbor = neighbor as usize;
            if plate_id[neighbor].as_usize() == plate && distances[neighbor] > next {
                distances[neighbor] = next;
                queue.push_back(neighbor);
            }
        }
    }
    distances
}

fn masked_components(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
    mask: &[bool],
) -> Vec<Vec<usize>> {
    let mut visited = vec![false; plate_id.len()];
    let mut queue = VecDeque::new();
    let mut components = Vec::new();
    for start in 0..plate_id.len() {
        if visited[start] || !mask.get(start).copied().unwrap_or(false) {
            continue;
        }
        visited[start] = true;
        queue.push_back(start);
        let mut component = Vec::new();
        while let Some(cell) = queue.pop_front() {
            component.push(cell);
            for &neighbor in cell_neighbors(cell, nbr_offsets, nbrs) {
                let neighbor = neighbor as usize;
                if !visited[neighbor]
                    && mask.get(neighbor).copied().unwrap_or(false)
                    && plate_id[neighbor].as_usize() == plate
                {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        components.push(component);
    }
    components
}

fn dilate_mask_within_plate(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
    seed: &[bool],
    radius: u32,
) -> Vec<bool> {
    let mut opened = seed.to_vec();
    let mut distance = vec![u32::MAX; plate_id.len()];
    let mut queue = VecDeque::new();
    for (cell, &is_seed) in seed.iter().enumerate() {
        if is_seed {
            distance[cell] = 0;
            queue.push_back(cell);
        }
    }
    while let Some(cell) = queue.pop_front() {
        if distance[cell] >= radius {
            continue;
        }
        let next = distance[cell].saturating_add(1);
        for &neighbor in cell_neighbors(cell, nbr_offsets, nbrs) {
            let neighbor = neighbor as usize;
            if plate_id[neighbor].as_usize() == plate && distance[neighbor] > next {
                distance[neighbor] = next;
                opened[neighbor] = true;
                queue.push_back(neighbor);
            }
        }
    }
    opened
}

fn branch_bridge_width(
    branch: &[usize],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    opened: &[bool],
) -> u32 {
    let mut contacts = BTreeSet::new();
    for &cell in branch {
        for &neighbor in cell_neighbors(cell, nbr_offsets, nbrs) {
            let neighbor = neighbor as usize;
            if opened.get(neighbor).copied().unwrap_or(false) {
                contacts.insert(neighbor);
            }
        }
    }
    contacts.len() as u32
}

fn component_graph_diameter(component: &[usize], nbr_offsets: &[u32], nbrs: &[u32]) -> u32 {
    let Some(&start) = component.first() else {
        return 0;
    };
    let membership = component.iter().copied().collect::<BTreeSet<_>>();
    let (furthest, _) = furthest_component_cell(start, &membership, nbr_offsets, nbrs);
    furthest_component_cell(furthest, &membership, nbr_offsets, nbrs).1
}

fn furthest_component_cell(
    start: usize,
    membership: &BTreeSet<usize>,
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> (usize, u32) {
    let mut distance = BTreeMap::new();
    let mut queue = VecDeque::new();
    distance.insert(start, 0_u32);
    queue.push_back(start);
    let mut furthest = (start, 0_u32);
    while let Some(cell) = queue.pop_front() {
        let next = distance[&cell].saturating_add(1);
        for &neighbor in cell_neighbors(cell, nbr_offsets, nbrs) {
            let neighbor = neighbor as usize;
            if membership.contains(&neighbor) && !distance.contains_key(&neighbor) {
                distance.insert(neighbor, next);
                queue.push_back(neighbor);
                if next > furthest.1 {
                    furthest = (neighbor, next);
                }
            }
        }
    }
    furthest
}

fn cell_neighbors<'a>(cell: usize, nbr_offsets: &[u32], nbrs: &'a [u32]) -> &'a [u32] {
    let Some(&start) = nbr_offsets.get(cell) else {
        return &[];
    };
    let Some(&end) = nbr_offsets.get(cell + 1) else {
        return &[];
    };
    nbrs.get(start as usize..end as usize).unwrap_or(&[])
}

fn percentile_u32(values: &[u32], q: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let rank = (values.len().saturating_sub(1) as f32) * q.clamp(0.0, 1.0);
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper || upper >= values.len() {
        return values[lower.min(values.len() - 1)] as f32;
    }
    let t = rank - lower as f32;
    values[lower] as f32 + (values[upper] as f32 - values[lower] as f32) * t
}

fn corridor_metrics(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
    plate_cells: usize,
) -> CorridorMetrics {
    let mut best = CorridorMetrics::default();
    for min_degree in CORRIDOR_CORE_DEGREE_THRESHOLDS {
        let retained = plate_k_core(nbr_offsets, nbrs, plate_id, plate, min_degree);
        let (component_count, largest, second_largest) =
            retained_component_summary(nbr_offsets, nbrs, plate_id, plate, &retained);
        if component_count < 2 || second_largest == 0 {
            continue;
        }
        let neck_risk = second_largest as f32 / plate_cells.max(1) as f32;
        let lobe_balance = second_largest as f32 / largest.max(1) as f32;
        if neck_risk > best.neck_risk {
            best = CorridorMetrics {
                core_degree: min_degree as u32,
                component_count,
                lobe_balance,
                neck_risk,
            };
        }
    }
    best
}

fn plate_block_metrics(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
    plate_cells: usize,
    connected_component_count: u32,
    largest_connected_component_cells: usize,
    second_largest_connected_component_cells: usize,
) -> PlateBlockMetrics {
    if plate_cells == 0 {
        return PlateBlockMetrics::default();
    }

    let mut best = PlateBlockMetrics {
        core_degree: 0,
        block_count: connected_component_count.max(1),
        largest_block_ratio: largest_connected_component_cells as f32 / plate_cells as f32,
        secondary_block_ratio: second_largest_connected_component_cells as f32 / plate_cells as f32,
    };

    for min_degree in CORRIDOR_CORE_DEGREE_THRESHOLDS {
        let retained = plate_k_core(nbr_offsets, nbrs, plate_id, plate, min_degree);
        let retained_cells = retained.iter().filter(|&&value| value).count();
        if retained_cells == 0 {
            continue;
        }
        let (component_count, largest, second_largest) =
            retained_component_summary(nbr_offsets, nbrs, plate_id, plate, &retained);
        if component_count == 0 || largest == 0 {
            continue;
        }
        let metrics = PlateBlockMetrics {
            core_degree: min_degree as u32,
            block_count: component_count,
            largest_block_ratio: largest as f32 / plate_cells as f32,
            secondary_block_ratio: second_largest as f32 / plate_cells as f32,
        };
        if metrics.secondary_block_ratio > best.secondary_block_ratio
            || (metrics.secondary_block_ratio == best.secondary_block_ratio
                && metrics.block_count > best.block_count)
        {
            best = metrics;
        }
    }

    best
}

fn weak_line_plate_block_metrics(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    weakness: &[f32],
    plate: usize,
    plate_cells: usize,
) -> PlateBlockMetrics {
    if plate_cells == 0 || weakness.len() != plate_id.len() {
        return PlateBlockMetrics {
            core_degree: 0,
            block_count: if plate_cells == 0 { 0 } else { 1 },
            largest_block_ratio: if plate_cells == 0 { 0.0 } else { 1.0 },
            secondary_block_ratio: 0.0,
        };
    }

    let threshold = weak_line_threshold(plate_id, weakness, plate);
    let mut retained = vec![false; plate_id.len()];
    let mut retained_count = 0_usize;
    for cell in 0..plate_id.len() {
        if plate_id[cell].as_usize() == plate
            && finite_or(weakness[cell], 0.0).clamp(0.0, 1.0) < threshold
        {
            retained[cell] = true;
            retained_count = retained_count.saturating_add(1);
        }
    }
    if retained_count == 0 {
        return PlateBlockMetrics {
            core_degree: 0,
            block_count: 1,
            largest_block_ratio: 0.0,
            secondary_block_ratio: 0.0,
        };
    }

    let (component_count, largest, second_largest) =
        retained_component_summary(nbr_offsets, nbrs, plate_id, plate, &retained);
    let largest_block_ratio = largest as f32 / plate_cells as f32;
    let secondary_block_ratio = second_largest as f32 / plate_cells as f32;
    if component_count < 2
        || largest_block_ratio < WEAK_LINE_MIN_LARGEST_BLOCK_RATIO
        || secondary_block_ratio < WEAK_LINE_MIN_SECONDARY_BLOCK_RATIO
    {
        return PlateBlockMetrics {
            core_degree: 0,
            block_count: 1,
            largest_block_ratio,
            secondary_block_ratio: 0.0,
        };
    }
    PlateBlockMetrics {
        core_degree: 0,
        block_count: component_count.max(1),
        largest_block_ratio,
        secondary_block_ratio,
    }
}

fn weak_line_threshold(plate_id: &[PlateId], weakness: &[f32], plate: usize) -> f32 {
    let mut values = Vec::<f32>::new();
    for (cell, &pid) in plate_id.iter().enumerate() {
        if pid.as_usize() != plate {
            continue;
        }
        values.push(finite_or(weakness.get(cell).copied().unwrap_or(0.0), 0.0).clamp(0.0, 1.0));
    }
    if values.is_empty() {
        return 1.0;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let min = values.first().copied().unwrap_or(0.0);
    let max = values.last().copied().unwrap_or(0.0);
    if max - min < 0.05 {
        return 1.0;
    }
    percentile_f32(&values, 0.85).clamp(min + 1e-4, 0.99)
}

fn percentile_f32(values: &[f32], q: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let rank = (values.len().saturating_sub(1) as f32) * q.clamp(0.0, 1.0);
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper || upper >= values.len() {
        return values[lower.min(values.len() - 1)];
    }
    let t = rank - lower as f32;
    values[lower] + (values[upper] - values[lower]) * t
}

fn plate_k_core(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
    min_degree: usize,
) -> Vec<bool> {
    let mut retained = plate_id
        .iter()
        .map(|pid| pid.as_usize() == plate)
        .collect::<Vec<_>>();
    loop {
        let mut changed = false;
        let mut remove = Vec::<usize>::new();
        for cell in 0..plate_id.len() {
            if !retained[cell] {
                continue;
            }
            let mut degree = 0usize;
            let start = nbr_offsets[cell] as usize;
            let end = nbr_offsets[cell + 1] as usize;
            for &neighbor_u32 in &nbrs[start..end] {
                let neighbor = neighbor_u32 as usize;
                if neighbor < retained.len() && retained[neighbor] {
                    degree = degree.saturating_add(1);
                }
            }
            if degree < min_degree {
                remove.push(cell);
            }
        }
        for cell in remove {
            retained[cell] = false;
            changed = true;
        }
        if !changed {
            break;
        }
    }
    retained
}

fn retained_component_summary(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
    retained: &[bool],
) -> (u32, usize, usize) {
    let mut visited = vec![false; plate_id.len()];
    let mut stack = Vec::<usize>::new();
    let mut component_count = 0u32;
    let mut largest = 0usize;
    let mut second_largest = 0usize;

    for start_cell in 0..plate_id.len() {
        if visited[start_cell] || !retained[start_cell] || plate_id[start_cell].as_usize() != plate
        {
            continue;
        }
        visited[start_cell] = true;
        stack.push(start_cell);
        let mut component_cells = 0usize;
        while let Some(cell) = stack.pop() {
            component_cells = component_cells.saturating_add(1);
            let start = nbr_offsets[cell] as usize;
            let end = nbr_offsets[cell + 1] as usize;
            for &neighbor_u32 in &nbrs[start..end] {
                let neighbor = neighbor_u32 as usize;
                if neighbor >= plate_id.len()
                    || visited[neighbor]
                    || !retained[neighbor]
                    || plate_id[neighbor].as_usize() != plate
                {
                    continue;
                }
                visited[neighbor] = true;
                stack.push(neighbor);
            }
        }
        component_count = component_count.saturating_add(1);
        if component_cells > largest {
            second_largest = largest;
            largest = component_cells;
        } else if component_cells > second_largest {
            second_largest = component_cells;
        }
    }

    (component_count, largest, second_largest)
}

fn articulation_cell_count(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
) -> usize {
    #[derive(Clone, Copy)]
    struct Frame {
        cell: usize,
        next_neighbor: usize,
        end_neighbor: usize,
        child_count: usize,
    }

    let mut visited = vec![false; plate_id.len()];
    let mut discovery = vec![0usize; plate_id.len()];
    let mut low = vec![0usize; plate_id.len()];
    let mut parent = vec![usize::MAX; plate_id.len()];
    let mut articulation = vec![false; plate_id.len()];
    let mut time = 0usize;
    let mut stack = Vec::<Frame>::new();

    for root in 0..plate_id.len() {
        if visited[root] || plate_id[root].as_usize() != plate {
            continue;
        }
        visited[root] = true;
        time = time.saturating_add(1);
        discovery[root] = time;
        low[root] = time;
        stack.push(Frame {
            cell: root,
            next_neighbor: nbr_offsets[root] as usize,
            end_neighbor: nbr_offsets[root + 1] as usize,
            child_count: 0,
        });

        while let Some(frame) = stack.last_mut() {
            if frame.next_neighbor < frame.end_neighbor {
                let neighbor = nbrs[frame.next_neighbor] as usize;
                frame.next_neighbor += 1;
                if neighbor >= plate_id.len() || plate_id[neighbor].as_usize() != plate {
                    continue;
                }
                let cell = frame.cell;
                if !visited[neighbor] {
                    frame.child_count = frame.child_count.saturating_add(1);
                    parent[neighbor] = cell;
                    visited[neighbor] = true;
                    time = time.saturating_add(1);
                    discovery[neighbor] = time;
                    low[neighbor] = time;
                    stack.push(Frame {
                        cell: neighbor,
                        next_neighbor: nbr_offsets[neighbor] as usize,
                        end_neighbor: nbr_offsets[neighbor + 1] as usize,
                        child_count: 0,
                    });
                } else if neighbor != parent[cell] {
                    low[cell] = low[cell].min(discovery[neighbor]);
                }
                continue;
            }

            let completed = stack.pop().expect("frame exists");
            let cell = completed.cell;
            if parent[cell] == usize::MAX {
                if completed.child_count > 1 {
                    articulation[cell] = true;
                }
                continue;
            }
            let parent_cell = parent[cell];
            low[parent_cell] = low[parent_cell].min(low[cell]);
            if parent[parent_cell] != usize::MAX && low[cell] >= discovery[parent_cell] {
                articulation[parent_cell] = true;
            }
        }
    }

    articulation.into_iter().filter(|value| *value).count()
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

fn mutual_exchange_ratio(previous: &[PlateId], current: &[PlateId]) -> f32 {
    if previous.len() != current.len() || previous.is_empty() {
        return 0.0;
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

    let mut mutual_sum = 0_u32;
    let mut total_sum = 0_u32;
    for (_, (forward, reverse)) in pair_counts {
        mutual_sum = mutual_sum.saturating_add(forward.min(reverse).saturating_mul(2));
        total_sum = total_sum.saturating_add(forward.saturating_add(reverse));
    }
    if total_sum == 0 {
        0.0
    } else {
        mutual_sum as f32 / total_sum as f32
    }
}

fn temporal_reversal_ratio(
    previous_previous: &[PlateId],
    previous: &[PlateId],
    current: &[PlateId],
) -> f32 {
    if previous_previous.len() != previous.len()
        || previous.len() != current.len()
        || previous.is_empty()
    {
        return 0.0;
    }
    let mut previous_transfer_count = 0_u32;
    let mut reversed_count = 0_u32;
    for ((&before, &middle), &after) in previous_previous.iter().zip(previous).zip(current) {
        if before == middle {
            continue;
        }
        previous_transfer_count = previous_transfer_count.saturating_add(1);
        if after == before {
            reversed_count = reversed_count.saturating_add(1);
        }
    }
    if previous_transfer_count == 0 {
        0.0
    } else {
        reversed_count as f32 / previous_transfer_count as f32
    }
}

fn temporal_reversal_velocity_alignment_ratios(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    previous_previous: &[PlateId],
    previous: &[PlateId],
    current: &[PlateId],
    plate_states: &[Option<PlateKinematicsState>],
) -> (f32, f32) {
    if previous_previous.len() != previous.len()
        || previous.len() != current.len()
        || previous.len() != positions.len()
    {
        return (1.0, 1.0);
    }

    let mut previous_aligned = 0_u32;
    let mut previous_evaluated = 0_u32;
    let mut current_aligned = 0_u32;
    let mut current_evaluated = 0_u32;
    for cell in 0..current.len() {
        let before = previous_previous[cell];
        let middle = previous[cell];
        if before == middle || current[cell] != before {
            continue;
        }
        if let Some(alignment) = boundary_transfer_cell_alignment(
            positions,
            nbr_offsets,
            nbrs,
            previous_previous,
            plate_states,
            cell,
            before,
            middle,
        ) {
            previous_evaluated = previous_evaluated.saturating_add(1);
            previous_aligned = previous_aligned.saturating_add(u32::from(alignment > 0.0));
        }
        if let Some(alignment) = boundary_transfer_cell_alignment(
            positions,
            nbr_offsets,
            nbrs,
            previous,
            plate_states,
            cell,
            middle,
            before,
        ) {
            current_evaluated = current_evaluated.saturating_add(1);
            current_aligned = current_aligned.saturating_add(u32::from(alignment > 0.0));
        }
    }

    (
        if previous_evaluated == 0 {
            1.0
        } else {
            previous_aligned as f32 / previous_evaluated as f32
        },
        if current_evaluated == 0 {
            1.0
        } else {
            current_aligned as f32 / current_evaluated as f32
        },
    )
}

fn great_circle_distance_km(a: [f32; 3], b: [f32; 3]) -> f32 {
    let chord = length3([a[0] - b[0], a[1] - b[1], a[2] - b[2]]).clamp(0.0, 2.0);
    let angle = 2.0 * (0.5 * chord).asin();
    angle * EARTH_MEAN_RADIUS_KM
}

fn rotate_unit_vector(value: [f32; 3], axis: [f32; 3], angle: f32) -> [f32; 3] {
    let Some(axis) = normalized(axis) else {
        return value;
    };
    let (sin_angle, cos_angle) = angle.sin_cos();
    let cross = cross3(axis, value);
    let axis_dot_value = dot3(axis, value);
    let rotated = [
        value[0] * cos_angle + cross[0] * sin_angle + axis[0] * axis_dot_value * (1.0 - cos_angle),
        value[1] * cos_angle + cross[1] * sin_angle + axis[1] * axis_dot_value * (1.0 - cos_angle),
        value[2] * cos_angle + cross[2] * sin_angle + axis[2] * axis_dot_value * (1.0 - cos_angle),
    ];
    normalized(rotated).unwrap_or(value)
}

fn plate_velocity_from_optional_state(
    state: Option<&PlateKinematicsState>,
    _plate_id: PlateId,
    pos: [f32; 3],
) -> [f32; 3] {
    let Some(state) = state else {
        return [0.0, 0.0, 0.0];
    };
    let omega = [
        finite_or(state.angular_axis[0], 0.0) * finite_or(state.angular_speed, 0.0),
        finite_or(state.angular_axis[1], 0.0) * finite_or(state.angular_speed, 0.0),
        finite_or(state.angular_axis[2], 0.0) * finite_or(state.angular_speed, 0.0),
    ];
    cross3(omega, pos)
}

fn boundary_motion_expected_cell_count(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate_states: &[Option<PlateKinematicsState>],
    boundary_activity: &[f32],
    sample_ticks: u64,
) -> f32 {
    if sample_ticks == 0 || plate_id.is_empty() {
        return 0.0;
    }
    let mut expected = 0.0_f32;
    let sample_ticks = sample_ticks as f32;
    for cell in 0..plate_id.len() {
        if boundary_activity.get(cell).copied().unwrap_or(0.0) <= 0.0 {
            continue;
        }
        let source_plate = plate_id[cell];
        let source_velocity = plate_velocity_from_optional_state(
            plate_states
                .get(source_plate.as_usize())
                .and_then(|state| state.as_ref()),
            source_plate,
            positions[cell],
        );
        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        let mut best_cell_fraction = 0.0_f32;
        for &neighbor_u32 in &nbrs[start..end] {
            let neighbor = neighbor_u32 as usize;
            if neighbor >= plate_id.len() || plate_id[neighbor] == source_plate {
                continue;
            }
            let target_plate = plate_id[neighbor];
            let target_velocity = plate_velocity_from_optional_state(
                plate_states
                    .get(target_plate.as_usize())
                    .and_then(|state| state.as_ref()),
                target_plate,
                positions[neighbor],
            );
            let edge = [
                positions[cell][0] - positions[neighbor][0],
                positions[cell][1] - positions[neighbor][1],
                positions[cell][2] - positions[neighbor][2],
            ];
            let edge_spacing = length3(edge).max(1e-5);
            let normal = [
                edge[0] / edge_spacing,
                edge[1] / edge_spacing,
                edge[2] / edge_spacing,
            ];
            let relative_velocity = [
                target_velocity[0] - source_velocity[0],
                target_velocity[1] - source_velocity[1],
                target_velocity[2] - source_velocity[2],
            ];
            let cell_fraction = dot3(relative_velocity, normal).max(0.0) / edge_spacing;
            best_cell_fraction = best_cell_fraction.max(finite_or(cell_fraction, 0.0));
        }
        expected += (best_cell_fraction * sample_ticks).clamp(0.0, 1.0);
    }
    expected
}

fn boundary_motion_response_ratio(actual_cell_count: u32, expected_cell_count: f32) -> f32 {
    if expected_cell_count < 1.0 {
        return 1.0;
    }
    finite_or(actual_cell_count as f32 / expected_cell_count, 1.0).max(0.0)
}

fn boundary_motion_underactive_risk(expected_cell_count: f32, response_ratio: f32) -> f32 {
    if expected_cell_count < 4.0 {
        return 0.0;
    }
    finite_or((0.5 - response_ratio) / 0.5, 0.0).clamp(0.0, 1.0)
}

fn boundary_motion_overactive_risk(expected_cell_count: f32, response_ratio: f32) -> f32 {
    if expected_cell_count < 1.0 {
        return 0.0;
    }
    finite_or((response_ratio - 2.0) / 2.0, 0.0).clamp(0.0, 1.0)
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
    use super::{
        annotate_boundary_transfer_components, boundary_motion_expected_cell_count,
        boundary_motion_overactive_risk, boundary_motion_response_ratio,
        boundary_motion_underactive_risk, boundary_transfer_cell_alignment,
        centroid_voronoi_diagnostics, great_circle_distance_km, multiscale_shape_metrics,
        mutual_exchange_ratio, plate_shape_records, reciprocal_churn_ratio, rotate_unit_vector,
        temporal_reversal_ratio, BoundaryTransferAlignmentSummary, MotionTracker, PlateId,
        PlateKinematicsState, EARTH_MEAN_RADIUS_KM,
    };

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
    fn mutual_exchange_ratio_detects_one_way_motion() {
        let previous = vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)];
        let current = vec![PlateId(1), PlateId(1), PlateId(1), PlateId(1)];

        assert_eq!(mutual_exchange_ratio(&previous, &current), 0.0);
    }

    #[test]
    fn mutual_exchange_ratio_detects_mutual_takeover() {
        let previous = vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)];
        let current = vec![PlateId(1), PlateId(0), PlateId(0), PlateId(1)];

        assert_eq!(mutual_exchange_ratio(&previous, &current), 1.0);
    }

    #[test]
    fn temporal_reversal_ratio_detects_same_cell_return() {
        let before = vec![PlateId(0), PlateId(0), PlateId(1)];
        let middle = vec![PlateId(1), PlateId(1), PlateId(1)];
        let after = vec![PlateId(0), PlateId(1), PlateId(0)];

        assert_eq!(temporal_reversal_ratio(&before, &middle, &after), 0.5);
    }

    #[test]
    fn branch_persistence_requires_four_consecutive_samples() {
        let mut tracker = MotionTracker::default();
        tracker.resize(1);

        assert!(!tracker.record_branch_area_ratio(0, 0.2));
        assert!(!tracker.record_branch_area_ratio(0, 0.2));
        assert!(!tracker.record_branch_area_ratio(0, 0.2));
        assert!(tracker.record_branch_area_ratio(0, 0.2));
        assert!(!tracker.record_branch_area_ratio(0, 0.1));
    }

    #[test]
    fn centroid_voronoi_agreement_is_model_independent() {
        let positions = vec![[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]];
        let centroids = vec![Some([1.0, 0.0, 0.0]), Some([-1.0, 0.0, 0.0])];

        let diagnostics =
            centroid_voronoi_diagnostics(&positions, &[PlateId(0), PlateId(1)], &centroids);

        assert_eq!(diagnostics.nearest_generator_agreement_ratio, 1.0);
    }

    #[test]
    fn detached_component_regime_detects_subduction_support() {
        let regimes = super::component_boundary_regimes(
            &[0],
            &[0, 1, 2],
            &[1, 0],
            &[PlateId(0), PlateId(1)],
            &[
                super::BoundaryType::Subduction,
                super::BoundaryType::PassiveMargin,
            ],
        );

        assert_eq!(regimes.subduction, 1.0);
        assert_eq!(regimes.causal_support(), 1.0);
    }

    #[test]
    fn great_circle_distance_matches_quarter_circumference() {
        let distance = great_circle_distance_km([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let expected = std::f32::consts::FRAC_PI_2 * EARTH_MEAN_RADIUS_KM;

        assert!((distance - expected).abs() < 1e-3);
    }

    #[test]
    fn boundary_motion_expected_count_tracks_relative_inflow() {
        let positions = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let nbr_offsets = vec![0, 1, 2];
        let nbrs = vec![1, 0];
        let plate_id = vec![PlateId(0), PlateId(1)];
        let states = vec![
            Some(test_plate_state([0.0, 0.0, 1.0], 0.0)),
            Some(test_plate_state([0.0, 0.0, -1.0], 1.0)),
        ];
        let boundary_activity = vec![1.0, 0.0];

        let expected = boundary_motion_expected_cell_count(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_id,
            &states,
            &boundary_activity,
            1,
        );

        assert!(expected > 0.0);
        assert!(expected <= 1.0);
    }

    #[test]
    fn boundary_motion_risk_flags_underactive_and_overactive_updates() {
        let under_response = boundary_motion_response_ratio(1, 10.0);
        let over_response = boundary_motion_response_ratio(30, 10.0);

        assert!(boundary_motion_underactive_risk(10.0, under_response) > 0.0);
        assert_eq!(boundary_motion_overactive_risk(10.0, under_response), 0.0);
        assert!(boundary_motion_overactive_risk(10.0, over_response) > 0.0);
        assert_eq!(boundary_motion_underactive_risk(10.0, over_response), 0.0);
    }

    #[test]
    fn rotate_unit_vector_uses_right_hand_rule() {
        let rotated = rotate_unit_vector(
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f32::consts::FRAC_PI_2,
        );

        assert!(great_circle_distance_km(rotated, [0.0, 1.0, 0.0]) < 1e-3);
    }

    #[test]
    fn euler_rotation_residual_is_zero_for_predicted_centroid() {
        let mut tracker = MotionTracker::default();
        tracker.resize(1);
        tracker.previous_sample_tick = Some(10);
        tracker.previous_centroids[0] = Some([1.0, 0.0, 0.0]);
        tracker.previous_plate_states[0] = Some(PlateKinematicsState {
            angular_axis: [0.0, 0.0, 1.0],
            angular_speed: std::f32::consts::FRAC_PI_2 / 5.0,
            reference_angular_speed: 0.0,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: 0.0,
            activity: 1.0,
        });

        let residual = tracker
            .euler_rotation_residual(
                0,
                [0.0, 1.0, 0.0],
                15,
                1.0,
                tracker.previous_plate_states[0].expect("state exists"),
            )
            .expect("residual exists");

        assert!(residual.km < 1e-3);
        assert!(residual.ratio < 1e-6);
    }

    #[test]
    fn plate_area_growth_tracks_initial_and_previous_sample_counts() {
        let mut tracker = MotionTracker::default();
        tracker.resize(1);

        assert_eq!(tracker.plate_area_growth_from_initial(0, 100), 1.0);
        assert_eq!(tracker.plate_area_delta_ratio(0, 100), 0.0);

        tracker.previous_plate_cell_counts[0] = Some(100);

        assert_eq!(tracker.plate_area_growth_from_initial(0, 150), 1.5);
        assert!((tracker.plate_area_delta_ratio(0, 150) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn boundary_transfer_alignment_is_positive_for_target_inflow() {
        let positions = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let nbr_offsets = vec![0, 1, 2];
        let nbrs = vec![1, 0];
        let previous_plate_id = vec![PlateId(0), PlateId(1)];
        let previous_plate_states = vec![
            Some(test_plate_state([0.0, 0.0, 1.0], 0.0)),
            Some(test_plate_state([0.0, 0.0, -1.0], 1.0)),
        ];

        let alignment = boundary_transfer_cell_alignment(
            &positions,
            &nbr_offsets,
            &nbrs,
            &previous_plate_id,
            &previous_plate_states,
            0,
            PlateId(0),
            PlateId(1),
        )
        .expect("alignment exists");

        assert!(alignment > 0.0);
    }

    #[test]
    fn boundary_transfer_alignment_is_negative_for_target_outflow() {
        let positions = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let nbr_offsets = vec![0, 1, 2];
        let nbrs = vec![1, 0];
        let previous_plate_id = vec![PlateId(0), PlateId(1)];
        let previous_plate_states = vec![
            Some(test_plate_state([0.0, 0.0, 1.0], 0.0)),
            Some(test_plate_state([0.0, 0.0, 1.0], 1.0)),
        ];

        let alignment = boundary_transfer_cell_alignment(
            &positions,
            &nbr_offsets,
            &nbrs,
            &previous_plate_id,
            &previous_plate_states,
            0,
            PlateId(0),
            PlateId(1),
        )
        .expect("alignment exists");

        assert!(alignment < 0.0);
    }

    #[test]
    fn boundary_transfer_components_detect_isolated_acquisitions() {
        let adjacency = vec![vec![1], vec![0, 2], vec![1, 3], vec![2]];
        let (nbr_offsets, nbrs) = adjacency_to_csr(&adjacency);
        let previous_plate_id = vec![PlateId(0), PlateId(0), PlateId(0), PlateId(0)];
        let current_plate_id = vec![PlateId(1), PlateId(1), PlateId(0), PlateId(1)];
        let mut summaries = vec![BoundaryTransferAlignmentSummary::default(); 2];
        summaries[1].candidate_cell_count = 3;

        annotate_boundary_transfer_components(
            &nbr_offsets,
            &nbrs,
            &previous_plate_id,
            &current_plate_id,
            &mut summaries,
        );

        assert_eq!(summaries[1].component_count, 2);
        assert_eq!(summaries[1].largest_component_cells, 2);
        assert_eq!(summaries[1].isolated_cell_count, 1);
        assert!((summaries[1].largest_component_ratio() - 2.0 / 3.0).abs() < 1e-6);
        assert!((summaries[1].isolated_cell_ratio() - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn plate_shape_records_detect_detached_fragments() {
        let nbr_offsets = vec![0, 1, 3, 5, 6];
        let nbrs = vec![1, 0, 2, 1, 3, 2];
        let plate_id = vec![PlateId(0), PlateId(1), PlateId(1), PlateId(0)];

        let records = plate_shape_records(&nbr_offsets, &nbrs, &plate_id, &[], &[], 2);

        assert_eq!(records[0].cell_count, 2);
        assert_eq!(records[0].component_count, 2);
        assert_eq!(records[0].largest_component_ratio, 0.5);
        assert_eq!(records[0].detached_fragment_ratio, 0.5);
        assert_eq!(records[1].component_count, 1);
        assert_eq!(records[1].detached_fragment_ratio, 0.0);
    }

    #[test]
    fn plate_shape_records_detect_enclosed_small_plate() {
        let adjacency = vec![
            vec![1, 2, 3, 4, 5, 6],
            vec![0, 2, 7],
            vec![0, 1, 3, 8],
            vec![0, 2, 4, 9],
            vec![0, 3, 5, 10],
            vec![0, 4, 6, 7],
            vec![0, 5, 8, 10],
            vec![1, 5, 9],
            vec![2, 6, 10],
            vec![3, 7, 10],
            vec![4, 6, 8, 9],
        ];
        let (nbr_offsets, nbrs) = adjacency_to_csr(&adjacency);
        let plate_id = vec![
            PlateId(1),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
        ];

        let records = plate_shape_records(&nbr_offsets, &nbrs, &plate_id, &[], &[], 2);

        assert_eq!(records[1].dominant_neighbor_plate_id, 0);
        assert_eq!(records[1].dominant_neighbor_contact_ratio, 1.0);
        assert!(records[1].enclosed_plate_risk > 0.0);
    }

    #[test]
    fn plate_shape_records_detect_appendage_isolation_risk() {
        let adjacency = vec![
            vec![1, 2, 3, 4, 5],
            vec![0, 2, 3],
            vec![0, 1, 3],
            vec![0, 1, 2],
            vec![0, 5, 6],
            vec![0, 4, 6],
            vec![4, 5, 7],
            vec![6, 8, 9],
            vec![7, 9],
            vec![7, 8],
        ];
        let (nbr_offsets, nbrs) = adjacency_to_csr(&adjacency);
        let plate_id = vec![
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(1),
        ];

        let records = plate_shape_records(&nbr_offsets, &nbrs, &plate_id, &[], &[], 2);

        assert!(records[0].appendage_core_cell_ratio > 0.0);
        assert!(records[0].appendage_largest_component_ratio > 0.0);
        assert!(records[0].appendage_isolation_risk > 0.0);
    }

    #[test]
    fn plate_shape_records_detect_single_cell_neck_without_detachment() {
        // Two lobes connected through cell 2. The plate is still one component,
        // but removing cell 2 splits it, so this is a topological neck.
        let nbr_offsets = vec![0, 2, 4, 8, 10, 12];
        let nbrs = vec![
            1, 2, // 0
            0, 2, // 1
            0, 1, 3, 4, // 2
            2, 4, // 3
            2, 3, // 4
        ];
        let plate_id = vec![PlateId(0); 5];

        let records = plate_shape_records(&nbr_offsets, &nbrs, &plate_id, &[], &[], 1);

        assert_eq!(records[0].component_count, 1);
        assert_eq!(records[0].detached_fragment_ratio, 0.0);
        assert_eq!(records[0].articulation_cell_count, 1);
        assert!((records[0].articulation_cell_ratio - 0.2).abs() < 1e-6);
    }

    #[test]
    fn plate_shape_records_do_not_flag_compact_cycle_as_neck() {
        let nbr_offsets = vec![0, 2, 4, 6];
        let nbrs = vec![
            1, 2, // 0
            0, 2, // 1
            0, 1, // 2
        ];
        let plate_id = vec![PlateId(0); 3];

        let records = plate_shape_records(&nbr_offsets, &nbrs, &plate_id, &[], &[], 1);

        assert_eq!(records[0].component_count, 1);
        assert_eq!(records[0].articulation_cell_count, 0);
        assert_eq!(records[0].articulation_cell_ratio, 0.0);
    }

    #[test]
    fn boundary_complexity_growth_tracks_relative_shape_degradation() {
        let mut tracker = MotionTracker::default();
        tracker.resize(1);

        assert_eq!(tracker.boundary_complexity_growth(0, 8.0), 1.0);
        assert_eq!(tracker.boundary_complexity_growth(0, 12.0), 1.5);
    }

    #[test]
    fn boundary_complexity_growth_window_requires_persistence() {
        let mut tracker = MotionTracker::default();
        tracker.resize(1);

        assert!(!tracker.record_boundary_complexity_growth(0, 1.0).persistent);
        assert!(!tracker.record_boundary_complexity_growth(0, 2.0).persistent);
        assert!(!tracker.record_boundary_complexity_growth(0, 2.0).persistent);
        assert!(!tracker.record_boundary_complexity_growth(0, 2.0).persistent);

        assert!(tracker.record_boundary_complexity_growth(0, 1.5).persistent);
    }

    #[test]
    fn plate_shape_records_detect_low_degree_corridor_between_lobes() {
        let adjacency = vec![
            vec![1, 2, 3, 4],
            vec![0, 2, 3],
            vec![0, 1, 3],
            vec![0, 1, 2],
            vec![0, 5],
            vec![4, 6],
            vec![5, 7, 8, 9],
            vec![6, 8, 9],
            vec![6, 7, 9],
            vec![6, 7, 8],
        ];
        let (nbr_offsets, nbrs) = adjacency_to_csr(&adjacency);
        let plate_id = vec![PlateId(0); adjacency.len()];

        let records = plate_shape_records(&nbr_offsets, &nbrs, &plate_id, &[], &[], 1);

        assert_eq!(records[0].component_count, 1);
        assert_eq!(records[0].corridor_core_degree, 3);
        assert_eq!(records[0].corridor_core_component_count, 2);
        assert!((records[0].corridor_lobe_balance - 1.0).abs() < 1e-6);
        assert!((records[0].corridor_neck_risk - 0.4).abs() < 1e-6);
        assert_eq!(records[0].plate_block_core_degree, 3);
        assert_eq!(records[0].plate_block_count, 2);
        assert!((records[0].secondary_plate_block_ratio - 0.4).abs() < 1e-6);
    }

    #[test]
    fn plate_shape_records_do_not_flag_compact_core_as_corridor() {
        let adjacency = vec![vec![1, 2, 3], vec![0, 2, 3], vec![0, 1, 3], vec![0, 1, 2]];
        let (nbr_offsets, nbrs) = adjacency_to_csr(&adjacency);
        let plate_id = vec![PlateId(0); adjacency.len()];

        let records = plate_shape_records(&nbr_offsets, &nbrs, &plate_id, &[], &[], 1);

        assert_eq!(records[0].component_count, 1);
        assert_eq!(records[0].corridor_core_degree, 0);
        assert_eq!(records[0].corridor_core_component_count, 0);
        assert_eq!(records[0].corridor_neck_risk, 0.0);
        assert_eq!(records[0].plate_block_count, 1);
        assert_eq!(records[0].secondary_plate_block_ratio, 0.0);
    }

    #[test]
    fn plate_shape_records_detect_weak_line_between_blocks() {
        let adjacency = vec![
            vec![1, 2, 3, 4],
            vec![0, 2, 3],
            vec![0, 1, 3],
            vec![0, 1, 2],
            vec![0, 5],
            vec![4, 6],
            vec![5, 7, 8, 9],
            vec![6, 8, 9],
            vec![6, 7, 9],
            vec![6, 7, 8],
        ];
        let (nbr_offsets, nbrs) = adjacency_to_csr(&adjacency);
        let plate_id = vec![PlateId(0); adjacency.len()];
        let weakness = vec![0.0, 0.0, 0.0, 0.0, 0.9, 0.9, 0.0, 0.0, 0.0, 0.0];

        let records = plate_shape_records(&nbr_offsets, &nbrs, &plate_id, &weakness, &[], 1);

        assert_eq!(records[0].weak_line_plate_block_count, 2);
        assert!((records[0].secondary_weak_line_plate_block_ratio - 0.4).abs() < 1e-6);
    }

    #[test]
    fn plate_shape_records_mark_chain_as_boundary_dominated() {
        let adjacency = vec![
            vec![1, 5],
            vec![0, 2],
            vec![1, 3],
            vec![2, 4],
            vec![3, 6],
            vec![0],
            vec![4],
        ];
        let (nbr_offsets, nbrs) = adjacency_to_csr(&adjacency);
        let plate_id = vec![
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(1),
            PlateId(1),
        ];

        let records = plate_shape_records(&nbr_offsets, &nbrs, &plate_id, &[], &[], 2);

        assert_eq!(records[0].boundary_distance_max, 2);
        assert_eq!(records[0].boundary_thin_cell_ratio, 1.0);
        assert_eq!(records[0].eroded_core_cell_ratio, 0.0);
    }

    #[test]
    fn plate_shape_records_keep_core_for_deep_chain() {
        let adjacency = vec![
            vec![1, 7],
            vec![0, 2],
            vec![1, 3],
            vec![2, 4],
            vec![3, 5],
            vec![4, 6],
            vec![5, 8],
            vec![0],
            vec![6],
        ];
        let (nbr_offsets, nbrs) = adjacency_to_csr(&adjacency);
        let plate_id = vec![
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(0),
            PlateId(1),
            PlateId(1),
        ];

        let records = plate_shape_records(&nbr_offsets, &nbrs, &plate_id, &[], &[], 2);

        assert_eq!(records[0].boundary_distance_max, 3);
        assert!((records[0].boundary_thin_cell_ratio - 6.0 / 7.0).abs() < 1e-6);
        assert!((records[0].eroded_core_cell_ratio - 1.0 / 7.0).abs() < 1e-6);
    }

    #[test]
    fn multiscale_shape_detects_lobes_joined_by_thin_neck() {
        let width = 15;
        let height = 7;
        let adjacency = grid_adjacency(width, height);
        let (nbr_offsets, nbrs) = adjacency_to_csr(&adjacency);
        let mut plate_id = vec![PlateId(1); width * height];
        for y in 1..=5 {
            for x in 1..=5 {
                plate_id[y * width + x] = PlateId(0);
            }
            for x in 9..=13 {
                plate_id[y * width + x] = PlateId(0);
            }
        }
        for x in 6..=8 {
            plate_id[3 * width + x] = PlateId(0);
        }
        let plate_cells = plate_id
            .iter()
            .filter(|&&plate| plate == PlateId(0))
            .count();

        let metrics = multiscale_shape_metrics(&nbr_offsets, &nbrs, &plate_id, 0, plate_cells, 4);

        assert!(metrics.neck_width_cells > 0);
        assert!(metrics.neck_secondary_ratio > 0.1);
    }

    #[test]
    fn multiscale_shape_detects_long_terminal_branch() {
        let width = 15;
        let height = 9;
        let adjacency = grid_adjacency(width, height);
        let (nbr_offsets, nbrs) = adjacency_to_csr(&adjacency);
        let mut plate_id = vec![PlateId(1); width * height];
        for y in 1..=7 {
            for x in 1..=6 {
                plate_id[y * width + x] = PlateId(0);
            }
        }
        for x in 7..=13 {
            plate_id[4 * width + x] = PlateId(0);
        }
        let plate_cells = plate_id
            .iter()
            .filter(|&&plate| plate == PlateId(0))
            .count();

        let metrics = multiscale_shape_metrics(&nbr_offsets, &nbrs, &plate_id, 0, plate_cells, 4);

        assert!(metrics.branch_area_ratio > 0.05);
        assert!(metrics.branch_slenderness >= 3.0);
    }

    #[test]
    fn multiscale_shape_keeps_compact_region_unflagged() {
        let width = 9;
        let height = 9;
        let adjacency = grid_adjacency(width, height);
        let (nbr_offsets, nbrs) = adjacency_to_csr(&adjacency);
        let mut plate_id = vec![PlateId(1); width * height];
        for y in 1..=7 {
            for x in 1..=7 {
                plate_id[y * width + x] = PlateId(0);
            }
        }
        let plate_cells = 49;

        let metrics = multiscale_shape_metrics(&nbr_offsets, &nbrs, &plate_id, 0, plate_cells, 4);

        assert_eq!(metrics.neck_secondary_ratio, 0.0);
        assert!(
            metrics.branch_area_ratio < 0.05,
            "compact metrics: {metrics:?}"
        );
    }

    fn grid_adjacency(width: usize, height: usize) -> Vec<Vec<usize>> {
        let mut adjacency = vec![Vec::new(); width * height];
        for y in 0..height {
            for x in 0..width {
                let cell = y * width + x;
                if x > 0 {
                    adjacency[cell].push(cell - 1);
                }
                if x + 1 < width {
                    adjacency[cell].push(cell + 1);
                }
                if y > 0 {
                    adjacency[cell].push(cell - width);
                }
                if y + 1 < height {
                    adjacency[cell].push(cell + width);
                }
            }
        }
        adjacency
    }

    fn adjacency_to_csr(adjacency: &[Vec<usize>]) -> (Vec<u32>, Vec<u32>) {
        let mut offsets = Vec::with_capacity(adjacency.len() + 1);
        let mut neighbors = Vec::new();
        offsets.push(0);
        for row in adjacency {
            neighbors.extend(row.iter().map(|neighbor| *neighbor as u32));
            offsets.push(neighbors.len() as u32);
        }
        (offsets, neighbors)
    }

    fn test_plate_state(axis: [f32; 3], speed: f32) -> PlateKinematicsState {
        PlateKinematicsState {
            angular_axis: axis,
            angular_speed: speed,
            reference_angular_speed: speed,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: 0.0,
            activity: 1.0,
        }
    }
}

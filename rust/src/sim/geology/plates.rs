use super::*;
use crate::sim::geology_types::{
    InitialPlateKinematics, PlateEmergenceFallbackKind, PlateId, TectonicRegime,
};

pub(super) fn pick_plate_seeds(positions: &[[f32; 3]], plate_count: usize) -> Vec<usize> {
    let target_len = plate_count.min(positions.len());
    let mut seeds = Vec::<usize>::with_capacity(target_len);
    if target_len == 0 {
        return seeds;
    }

    seeds.push(seed_nearest_direction(positions, [1.0, 0.0, 0.0]));
    while seeds.len() < target_len {
        let next = farthest_point_seed(positions, &seeds);
        if seeds.contains(&next) {
            break;
        }
        seeds.push(next);
    }
    seeds
}

pub(super) fn seed_nearest_direction(positions: &[[f32; 3]], direction: [f32; 3]) -> usize {
    let mut best_idx = 0usize;
    let mut best_dot = f32::NEG_INFINITY;
    for (i, &position) in positions.iter().enumerate() {
        let score = dot3(position, direction);
        if score > best_dot {
            best_dot = score;
            best_idx = i;
        }
    }
    best_idx
}

pub(super) fn farthest_point_seed(positions: &[[f32; 3]], seeds: &[usize]) -> usize {
    let mut best_idx = 0usize;
    let mut best_dist = f32::NEG_INFINITY;

    for (i, &position) in positions.iter().enumerate() {
        if seeds.contains(&i) {
            continue;
        }

        let mut min_dist = f32::INFINITY;
        for &seed in seeds {
            min_dist = min_dist.min(spherical_distance(position, positions[seed]));
        }

        if min_dist > best_dist {
            best_dist = min_dist;
            best_idx = i;
        }
    }

    best_idx
}

pub(super) fn generate_plate_power_weights(
    plate_count: usize,
    rng: &mut DeterministicRng,
) -> Vec<f32> {
    if plate_count == 0 {
        return Vec::new();
    }

    let target_angle = (4.0 * std::f32::consts::PI / plate_count as f32).sqrt();
    let scale = 0.20 * target_angle * target_angle;
    let half_range = 3.0_f32.sqrt() * scale;
    let mut weights = Vec::with_capacity(plate_count);
    for _ in 0..plate_count {
        weights.push(rng.gen_range_f32(-half_range, half_range));
    }

    let mean = weights.iter().sum::<f32>() / plate_count as f32;
    for weight in &mut weights {
        *weight -= mean;
    }
    weights
}

pub(super) fn random_unit_vector3(rng: &mut DeterministicRng) -> [f32; 3] {
    let v = [
        rng.standard_normal(),
        rng.standard_normal(),
        rng.standard_normal(),
    ];
    let n = normalize3(v);
    if length3(n) <= 1e-6 {
        [1.0, 0.0, 0.0]
    } else {
        n
    }
}

pub(super) fn local_plate_velocity(attr: &PlateAttr, plate: usize, position: [f32; 3]) -> [f32; 3] {
    let base = project_to_tangent(attr.velocity, position);
    let base_mag = length3(base);

    let blend = 0.5 + 0.5 * clamp(dot3(position, attr.drift_mix_axis), -1.0, 1.0);
    let mixed_axis = normalize3(add3(
        mul3(attr.drift_axis_primary, 1.0 - blend),
        mul3(attr.drift_axis_secondary, blend),
    ));
    let drift_axis = project_to_tangent(mixed_axis, position);
    let drift_mag = length3(drift_axis);

    let seed = plate as u32;
    let local_hash = 2.0 * trig_hash01(position, seed ^ 0x9e37_79b9) - 1.0;
    let local_scale = attr.drift_variability * local_hash;

    if drift_mag <= 1e-6 {
        return base;
    }
    let drift_dir = mul3(drift_axis, 1.0 / drift_mag);
    let mixed = add3(base, mul3(drift_dir, base_mag * local_scale));
    let tangent = project_to_tangent(mixed, position);
    if length3(tangent) <= 1e-6 {
        base
    } else {
        tangent
    }
}

pub(super) struct PlatePartitionInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub weights: &'a [f32],
}

pub(super) fn partition_plates(input: PlatePartitionInput<'_>, seeds: &[usize]) -> Vec<PlateId> {
    let positions = input.positions;
    let weights = input.weights;
    let mut plate_id = Vec::with_capacity(positions.len());

    for &position in positions {
        let mut best_plate = 0usize;
        let mut best_score = f32::INFINITY;
        for (plate, &seed) in seeds.iter().enumerate() {
            let distance = spherical_distance(position, positions[seed]);
            let score = distance * distance - weights.get(plate).copied().unwrap_or(0.0);
            if score < best_score {
                best_score = score;
                best_plate = plate;
            }
        }
        plate_id.push(PlateId(best_plate as u32));
    }

    plate_id
}

pub(super) struct EmergentPlateField {
    pub(super) plate_id: Vec<PlateId>,
    pub(super) plate_count: usize,
    pub(super) regime: TectonicRegime,
    pub(super) fallback: PlateEmergenceFallbackKind,
    pub(super) initial_kinematics: Vec<InitialPlateKinematics>,
    pub(super) plume: Vec<f32>,
    pub(super) downwelling: Vec<f32>,
    pub(super) craton_resistance: Vec<f32>,
}

pub(super) fn build_damage_first_plate_field(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    phi: &[f32],
    params: &GeologyParams,
    rng: &mut DeterministicRng,
) -> EmergentPlateField {
    let count = positions.len();
    if count == 0 {
        return proto_lid_plate_field(
            positions,
            params,
            TectonicRegime::StagnantLid,
            PlateEmergenceFallbackKind::StagnantLidProtoPlates,
            rng,
        );
    }

    let mut plume = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 2, 12, rng);
    let mut downwelling = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 4, 16, rng);
    let mut shear = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 1, 5, rng);
    normalize_zscore_if_var(&mut plume);
    normalize_zscore_if_var(&mut downwelling);
    normalize_zscore_if_var(&mut shear);

    let mut material_contrast = vec![0.0_f32; count];
    for v in 0..count {
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        if start == end {
            continue;
        }
        let mut sum = 0.0;
        let mut n_count = 0.0;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            sum += (phi[v] - phi[n]).abs();
            n_count += 1.0;
        }
        material_contrast[v] = sum / n_count;
    }
    normalize_zscore_if_var(&mut material_contrast);

    let mut craton_resistance = vec![0.0_f32; count];
    let mut inherited_weakness = vec![0.0_f32; count];
    let mut active_damage = vec![0.0_f32; count];

    for v in 0..count {
        let phi_high = clamp(0.5 + 0.22 * phi[v], 0.0, 1.0);
        let cool = clamp(0.5 + 0.22 * downwelling[v] - 0.18 * plume[v], 0.0, 1.0);
        craton_resistance[v] = clamp(phi_high * cool, 0.0, 1.0);
        inherited_weakness[v] = clamp(
            0.09 + 0.13 * clamp(0.5 + 0.25 * material_contrast[v], 0.0, 1.0)
                + 0.08 * clamp(0.5 + 0.25 * plume[v], 0.0, 1.0),
            0.0,
            0.32,
        );
        active_damage[v] = 0.35 * inherited_weakness[v];
    }

    let evolution = evolve_damage_until_plate_field_settles(
        nbr_offsets,
        nbrs,
        phi,
        &plume,
        &downwelling,
        &shear,
        &material_contrast,
        &craton_resistance,
        &inherited_weakness,
        active_damage,
        params,
    );
    let active_damage = evolution.active_damage;
    let extraction = evolution.selected;
    let plate_id = promote_damage_components(
        nbr_offsets,
        nbrs,
        &active_damage,
        &extraction.boundary_mask,
        extraction.min_region,
    );

    let plate_count = count_unique_plates(&plate_id);
    if validate_plate_partition(nbr_offsets, nbrs, &plate_id, plate_count).is_err()
        || extraction.regime != TectonicRegime::MobileLid
    {
        return proto_lid_plate_field(
            positions,
            params,
            extraction.regime,
            fallback_kind_for_regime(extraction.regime),
            rng,
        );
    }

    for v in &mut plume {
        *v = clamp(0.5 + 0.24 * *v, 0.0, 1.0);
    }
    for v in &mut downwelling {
        *v = clamp(0.5 + 0.24 * *v, 0.0, 1.0);
    }

    let initial_kinematics = build_initial_plate_kinematics(
        positions,
        &plate_id,
        plate_count,
        &plume,
        &downwelling,
        &craton_resistance,
        rng,
    );

    EmergentPlateField {
        plate_id,
        plate_count,
        regime: extraction.regime,
        fallback: PlateEmergenceFallbackKind::None,
        initial_kinematics,
        plume,
        downwelling,
        craton_resistance,
    }
}

#[derive(Clone)]
struct BoundaryExtraction {
    boundary_ratio: f32,
    boundary_mask: Vec<bool>,
    min_region: usize,
    regime: TectonicRegime,
    regime_score: f32,
    stats: ComponentStats,
    shape_stats: PlateShapeStats,
}

const MOBILE_LID_SELECTION_SCORE_MARGIN: f32 = 0.10;
const DAMAGE_EVOLUTION_EVAL_INTERVAL: u32 = 8;
const DAMAGE_EVOLUTION_MAX_STEP_MULTIPLIER: u32 = 2;
const DAMAGE_EVOLUTION_STAGNATION_CHECKPOINTS: u32 = 2;
const PRE_PLATE_BOUNDARY_POTENTIAL_SMOOTH_PASSES: u32 = 1;
const PRE_PLATE_BOUNDARY_SPUR_PRUNE_ROUNDS: u32 = 2;
const PRE_PLATE_BOUNDARY_SPUR_MIN_NEIGHBORS: usize = 2;
const PRE_PLATE_BOUNDARY_RATIO_MIN: f32 = 0.08;
const PRE_PLATE_BOUNDARY_RATIO_MAX: f32 = 0.72;
const PRE_PLATE_BOUNDARY_RATIO_STEPS: usize = 32;
const PRE_PLATE_ASSIGNMENT_SMOOTH_ROUNDS: u32 = 1;
const PRE_PLATE_ASSIGNMENT_DOMINANCE_MARGIN: usize = 2;
const PRE_PLATE_ASSIGNMENT_MIN_MAJORITY_NEIGHBORS: usize = 3;
const PRE_PLATE_ABSORB_SIZE_PENALTY_GAIN: f32 = 0.22;
const PRE_PLATE_ABSORB_SIZE_PENALTY_START: f32 = 0.24;
const PRE_PLATE_SMOOTH_SIZE_MARGIN_GAIN: f32 = 3.5;
const PRE_PLATE_SMOOTH_SIZE_MARGIN_START: f32 = 0.24;

#[derive(Clone)]
struct DamageEvolutionCheckpoint {
    step: u32,
    mean_abs_damage_delta: f32,
    max_damage_delta: f32,
    selected: BoundaryExtraction,
}

struct DamageEvolutionResult {
    active_damage: Vec<f32>,
    boundary_potential: Vec<f32>,
    selected: BoundaryExtraction,
    checkpoints: Vec<DamageEvolutionCheckpoint>,
    base_step_budget: u32,
    max_step_budget: u32,
    settled_steps: u32,
}

#[derive(Clone, Copy, Default)]
struct PlateShapeStats {
    single_cell_plate_count: usize,
    min_plate_cells: usize,
    final_plate_count: usize,
    multi_component_plate_count: usize,
    max_plate_component_count: usize,
    mean_detached_fragment_ratio: f32,
    max_plate_area_ratio: f32,
    second_plate_area_ratio: f32,
    effective_plate_count: f32,
    mean_boundary_complexity: f32,
    max_boundary_complexity: f32,
    max_enclosed_plate_risk: f32,
}

fn proto_lid_plate_field(
    positions: &[[f32; 3]],
    params: &GeologyParams,
    regime: TectonicRegime,
    fallback: PlateEmergenceFallbackKind,
    rng: &mut DeterministicRng,
) -> EmergentPlateField {
    let proto_count = proto_plate_count_for_regime(params, regime, positions.len());
    let seeds = pick_plate_seeds(positions, proto_count);
    let plate_weights = generate_plate_power_weights(proto_count, rng);
    let plate_id = partition_plates(
        PlatePartitionInput {
            positions,
            weights: &plate_weights,
        },
        &seeds,
    );
    let initial_kinematics = build_initial_plate_kinematics(
        positions,
        &plate_id,
        proto_count,
        &vec![0.5; positions.len()],
        &vec![0.5; positions.len()],
        &vec![0.0; positions.len()],
        rng,
    );
    EmergentPlateField {
        plate_id,
        plate_count: proto_count,
        regime,
        fallback,
        initial_kinematics,
        plume: vec![0.0; positions.len()],
        downwelling: vec![0.0; positions.len()],
        craton_resistance: vec![0.0; positions.len()],
    }
}

fn proto_plate_count_for_regime(
    params: &GeologyParams,
    regime: TectonicRegime,
    cell_count: usize,
) -> usize {
    let min_count = params.plate_count_min.max(1) as usize;
    let max_count = params.plate_count_max.max(params.plate_count_min.max(1)) as usize;
    let target = match regime {
        TectonicRegime::StagnantLid => min_count,
        TectonicRegime::MobileLid => (min_count + max_count) / 2,
        TectonicRegime::ShatteredLid => max_count,
    };
    target.clamp(1, cell_count.max(1))
}

fn fallback_kind_for_regime(regime: TectonicRegime) -> PlateEmergenceFallbackKind {
    match regime {
        TectonicRegime::StagnantLid => PlateEmergenceFallbackKind::StagnantLidProtoPlates,
        TectonicRegime::MobileLid => PlateEmergenceFallbackKind::None,
        TectonicRegime::ShatteredLid => PlateEmergenceFallbackKind::ShatteredLidProtoBlocks,
    }
}

fn compute_boundary_potential(
    active_damage: &[f32],
    damage_memory: &[f32],
    inherited_weakness: &[f32],
    material_contrast: &[f32],
    shear: &[f32],
) -> Vec<f32> {
    let mut out = vec![0.0; active_damage.len()];
    for i in 0..out.len() {
        let active = active_damage[i];
        let memory = damage_memory.get(i).copied().unwrap_or(0.0);
        let inherited = inherited_weakness.get(i).copied().unwrap_or(0.0);
        let contrast = clamp(
            0.5 + 0.25 * material_contrast.get(i).copied().unwrap_or(0.0),
            0.0,
            1.0,
        );
        let shear = clamp(0.5 + 0.22 * shear.get(i).copied().unwrap_or(0.0), 0.0, 1.0);
        out[i] = clamp(
            active * 0.62 + inherited * 0.22 + memory * 0.16 + contrast * 0.18 + shear * 0.12,
            0.0,
            1.0,
        );
    }
    out
}

#[derive(Clone, Copy)]
struct DamageEvolutionDelta {
    mean_abs_damage_delta: f32,
    max_damage_delta: f32,
}

struct DamageEvolutionInputs<'a> {
    nbr_offsets: &'a [u32],
    nbrs: &'a [u32],
    phi: &'a [f32],
    plume: &'a [f32],
    downwelling: &'a [f32],
    shear: &'a [f32],
    material_contrast: &'a [f32],
    craton_resistance: &'a [f32],
    inherited_weakness: &'a [f32],
}

fn evolve_active_damage_chunk(
    inputs: &DamageEvolutionInputs<'_>,
    active_damage: &mut [f32],
    steps: u32,
    damage_rate: f32,
    healing_decay: f32,
) -> DamageEvolutionDelta {
    if steps == 0 || active_damage.is_empty() {
        return DamageEvolutionDelta {
            mean_abs_damage_delta: 0.0,
            max_damage_delta: 0.0,
        };
    }

    let mut abs_delta_sum = 0.0_f32;
    let mut max_damage_delta = 0.0_f32;
    let cell_count = active_damage.len() as f32;

    for step in 0..steps {
        for v in 0..active_damage.len() {
            let hot = clamp(
                0.5 + 0.24 * inputs.plume[v] + 0.08 * inputs.phi[v],
                0.0,
                1.0,
            );
            let thickness = clamp(
                0.48 + 0.26 * inputs.downwelling[v] - 0.16 * inputs.plume[v]
                    + 0.18 * inputs.craton_resistance[v],
                0.0,
                1.0,
            );
            let contrast = clamp(0.5 + 0.25 * inputs.material_contrast[v], 0.0, 1.0);
            let mantle_shear = clamp(0.5 + 0.22 * inputs.shear[v], 0.0, 1.0);
            let old_damage = active_damage[v];
            let strength = clamp(
                0.50 + 0.34 * inputs.craton_resistance[v] + 0.24 * thickness
                    - 0.24 * hot
                    - 0.22 * old_damage,
                0.04,
                1.20,
            );
            let stress = clamp(
                0.30 + 0.30 * clamp(0.5 + 0.24 * inputs.plume[v], 0.0, 1.0)
                    + 0.20 * clamp(0.5 + 0.22 * inputs.downwelling[v], 0.0, 1.0)
                    + 0.28 * contrast
                    + 0.18 * mantle_shear,
                0.0,
                1.40,
            );
            let excess = (stress - strength).max(0.0);
            let new_damage = (old_damage * healing_decay + excess * damage_rate)
                .max(inputs.inherited_weakness[v] * 0.55)
                .min(1.0);
            let delta = (new_damage - old_damage).abs();
            abs_delta_sum += delta;
            max_damage_delta = max_damage_delta.max(delta);
            active_damage[v] = new_damage;
        }
        if step % 8 == 7 {
            smooth_scalar_field(inputs.nbr_offsets, inputs.nbrs, active_damage, 1);
        }
    }
    smooth_scalar_field(inputs.nbr_offsets, inputs.nbrs, active_damage, 1);

    DamageEvolutionDelta {
        mean_abs_damage_delta: abs_delta_sum / (cell_count * steps as f32),
        max_damage_delta,
    }
}

fn evolve_damage_until_plate_field_settles(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    phi: &[f32],
    plume: &[f32],
    downwelling: &[f32],
    shear: &[f32],
    material_contrast: &[f32],
    craton_resistance: &[f32],
    inherited_weakness: &[f32],
    mut active_damage: Vec<f32>,
    params: &GeologyParams,
) -> DamageEvolutionResult {
    let base_step_budget = params.pre_plate_steps.clamp(1, 160);
    let max_step_budget = base_step_budget
        .saturating_mul(DAMAGE_EVOLUTION_MAX_STEP_MULTIPLIER)
        .clamp(base_step_budget, 160);
    let inputs = DamageEvolutionInputs {
        nbr_offsets,
        nbrs,
        phi,
        plume,
        downwelling,
        shear,
        material_contrast,
        craton_resistance,
        inherited_weakness,
    };
    let damage_rate = params.pre_plate_damage_rate.max(0.0);
    let healing_decay = params.pre_plate_healing_decay.clamp(0.90, 0.9999);
    let mut checkpoints = Vec::<DamageEvolutionCheckpoint>::new();
    let mut total_steps = 0u32;
    let mut settled_steps = base_step_budget;
    let mut stagnation_count = 0u32;
    let mut best_active_damage = active_damage.clone();
    let mut best_boundary_potential = build_smoothed_boundary_potential(
        nbr_offsets,
        nbrs,
        &active_damage,
        inherited_weakness,
        material_contrast,
        shear,
    );
    let mut best_selected =
        choose_boundary_extraction(nbr_offsets, nbrs, &active_damage, &best_boundary_potential);

    while total_steps < max_step_budget {
        let remaining = max_step_budget - total_steps;
        let until_base = base_step_budget.saturating_sub(total_steps);
        let chunk_steps = if until_base > 0 && until_base < DAMAGE_EVOLUTION_EVAL_INTERVAL {
            until_base
        } else {
            DAMAGE_EVOLUTION_EVAL_INTERVAL.min(remaining)
        };
        let delta = evolve_active_damage_chunk(
            &inputs,
            &mut active_damage,
            chunk_steps,
            damage_rate,
            healing_decay,
        );
        total_steps += chunk_steps;

        let boundary_potential = build_smoothed_boundary_potential(
            nbr_offsets,
            nbrs,
            &active_damage,
            inherited_weakness,
            material_contrast,
            shear,
        );
        let selected =
            choose_boundary_extraction(nbr_offsets, nbrs, &active_damage, &boundary_potential);
        checkpoints.push(DamageEvolutionCheckpoint {
            step: total_steps,
            mean_abs_damage_delta: delta.mean_abs_damage_delta,
            max_damage_delta: delta.max_damage_delta,
            selected: selected.clone(),
        });

        if boundary_extraction_improves(&selected, &best_selected) {
            best_active_damage = active_damage.clone();
            best_boundary_potential = boundary_potential;
            best_selected = selected.clone();
            if total_steps >= base_step_budget {
                stagnation_count = 0;
            }
        }

        if total_steps < base_step_budget {
            continue;
        }

        settled_steps = total_steps;
        if !boundary_extraction_improves(&selected, &best_selected) {
            stagnation_count += 1;
            if stagnation_count >= DAMAGE_EVOLUTION_STAGNATION_CHECKPOINTS {
                break;
            }
        }
    }

    DamageEvolutionResult {
        active_damage: best_active_damage,
        boundary_potential: best_boundary_potential,
        selected: best_selected,
        checkpoints,
        base_step_budget,
        max_step_budget,
        settled_steps,
    }
}

fn build_smoothed_boundary_potential(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    active_damage: &[f32],
    inherited_weakness: &[f32],
    material_contrast: &[f32],
    shear: &[f32],
) -> Vec<f32> {
    let damage_memory = vec![0.0_f32; active_damage.len()];
    let mut boundary_potential = compute_boundary_potential(
        active_damage,
        &damage_memory,
        inherited_weakness,
        material_contrast,
        shear,
    );
    smooth_scalar_field(
        nbr_offsets,
        nbrs,
        &mut boundary_potential,
        PRE_PLATE_BOUNDARY_POTENTIAL_SMOOTH_PASSES,
    );
    boundary_potential
}

fn boundary_extraction_improves(
    candidate: &BoundaryExtraction,
    incumbent: &BoundaryExtraction,
) -> bool {
    if candidate.regime != incumbent.regime {
        return candidate.regime_score + 1e-6 < incumbent.regime_score;
    }
    if candidate.regime == TectonicRegime::MobileLid {
        if candidate.stats.valid_count != incumbent.stats.valid_count {
            return candidate.stats.valid_count > incumbent.stats.valid_count;
        }
        if candidate.stats.largest_ratio + 0.005 < incumbent.stats.largest_ratio {
            return true;
        }
    }
    candidate.regime_score + 1e-6 < incumbent.regime_score
}

fn choose_boundary_extraction(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    damage: &[f32],
    boundary_potential: &[f32],
) -> BoundaryExtraction {
    let min_region = min_promoted_region_size(boundary_potential.len());
    let mut candidates = Vec::<BoundaryExtraction>::new();

    for ratio in boundary_ratio_candidates() {
        let mut mask = build_damage_boundary_mask(boundary_potential, ratio);
        remove_tiny_boundary_islands(nbr_offsets, nbrs, &mut mask, 4);
        prune_boundary_spurs(
            nbr_offsets,
            nbrs,
            &mut mask,
            PRE_PLATE_BOUNDARY_SPUR_PRUNE_ROUNDS,
            PRE_PLATE_BOUNDARY_SPUR_MIN_NEIGHBORS,
        );
        let stats = component_stats(nbr_offsets, nbrs, &mask, min_region);
        let plate_id = promote_damage_components(nbr_offsets, nbrs, damage, &mask, min_region);
        let shape_stats = plate_shape_stats(nbr_offsets, nbrs, &plate_id);
        let regime = classify_tectonic_regime(&stats);
        let score = regime_score(regime, &stats, &shape_stats);
        candidates.push(BoundaryExtraction {
            boundary_ratio: ratio,
            boundary_mask: mask,
            min_region,
            regime,
            regime_score: score,
            stats,
            shape_stats,
        });
    }

    let Some((best_index, best_score)) = candidates
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            a.regime_score
                .partial_cmp(&b.regime_score)
                .unwrap_or(Ordering::Equal)
        })
        .map(|(index, candidate)| (index, candidate.regime_score))
    else {
        return BoundaryExtraction {
            boundary_ratio: 0.0,
            boundary_mask: vec![false; boundary_potential.len()],
            min_region,
            regime: TectonicRegime::StagnantLid,
            regime_score: f32::INFINITY,
            stats: ComponentStats::default(),
            shape_stats: PlateShapeStats::default(),
        };
    };

    let mut mobile_candidates = candidates
        .iter()
        .filter(|candidate| {
            candidate.regime == TectonicRegime::MobileLid
                && candidate.regime_score <= best_score + MOBILE_LID_SELECTION_SCORE_MARGIN
        })
        .cloned()
        .collect::<Vec<_>>();
    if !mobile_candidates.is_empty() {
        mobile_candidates.sort_by(|a, b| {
            b.stats
                .valid_count
                .cmp(&a.stats.valid_count)
                .then_with(|| {
                    a.shape_stats
                        .multi_component_plate_count
                        .cmp(&b.shape_stats.multi_component_plate_count)
                })
                .then_with(|| {
                    a.shape_stats
                        .mean_detached_fragment_ratio
                        .partial_cmp(&b.shape_stats.mean_detached_fragment_ratio)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| {
                    a.shape_stats
                        .mean_boundary_complexity
                        .partial_cmp(&b.shape_stats.mean_boundary_complexity)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| {
                    a.shape_stats
                        .max_boundary_complexity
                        .partial_cmp(&b.shape_stats.max_boundary_complexity)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| {
                    a.shape_stats
                        .max_enclosed_plate_risk
                        .partial_cmp(&b.shape_stats.max_enclosed_plate_risk)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| {
                    a.stats
                        .largest_ratio
                        .partial_cmp(&b.stats.largest_ratio)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| {
                    a.regime_score
                        .partial_cmp(&b.regime_score)
                        .unwrap_or(Ordering::Equal)
                })
        });
        return mobile_candidates.remove(0);
    }

    candidates.swap_remove(best_index)
}

fn build_damage_boundary_mask(damage: &[f32], boundary_ratio: f32) -> Vec<bool> {
    if damage.is_empty() {
        return Vec::new();
    }
    let mut sorted = damage.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let keep = (1.0 - boundary_ratio.clamp(0.01, 0.75)) * (sorted.len() - 1) as f32;
    let threshold = sorted[keep.round() as usize];
    damage.iter().map(|&v| v >= threshold).collect()
}

fn boundary_ratio_candidates() -> Vec<f32> {
    (0..=PRE_PLATE_BOUNDARY_RATIO_STEPS)
        .map(|i| {
            let t = i as f32 / PRE_PLATE_BOUNDARY_RATIO_STEPS as f32;
            lerp(
                PRE_PLATE_BOUNDARY_RATIO_MIN,
                PRE_PLATE_BOUNDARY_RATIO_MAX,
                t,
            )
        })
        .collect()
}

#[derive(Clone, Copy, Default)]
struct ComponentStats {
    valid_count: usize,
    largest_ratio: f32,
    tiny_fragment_ratio: f32,
}

pub(super) fn diagnose_plate_emergence_with_mesh(
    seed: &str,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    phi: &[f32],
    params: &GeologyParams,
    min_region_override: Option<usize>,
    rng: &mut DeterministicRng,
) -> crate::sim::geology_types::PlateEmergenceDiagnostics {
    let count = positions.len();
    if count == 0 {
        return crate::sim::geology_types::PlateEmergenceDiagnostics {
            seed: seed.to_string(),
            level: params.level,
            min_region: 0,
            base_step_budget: params.pre_plate_steps.clamp(1, 160),
            max_step_budget: params.pre_plate_steps.clamp(1, 160),
            settled_steps: 0,
            selected_boundary_ratio: 0.0,
            selected_valid_count: 0,
            selected_largest_ratio: 0.0,
            selected_tiny_fragment_ratio: 0.0,
            selected_single_cell_plate_count: 0,
            selected_min_plate_cells: 0,
            selected_final_plate_count: 0,
            selected_multi_component_plate_count: 0,
            selected_max_plate_component_count: 0,
            selected_mean_detached_fragment_ratio: 0.0,
            selected_max_plate_area_ratio: 0.0,
            selected_second_plate_area_ratio: 0.0,
            selected_effective_plate_count: 0.0,
            selected_mean_plate_boundary_complexity: 0.0,
            selected_max_plate_boundary_complexity: 0.0,
            selected_max_enclosed_plate_risk: 0.0,
            selected_regime: TectonicRegime::StagnantLid,
            selected_regime_score: f32::INFINITY,
            evolution_iterations: Vec::new(),
            threshold_candidates: Vec::new(),
        };
    }

    let mut plume = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 2, 12, rng);
    let mut downwelling = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 4, 16, rng);
    let mut shear = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 1, 5, rng);
    normalize_zscore_if_var(&mut plume);
    normalize_zscore_if_var(&mut downwelling);
    normalize_zscore_if_var(&mut shear);

    let mut material_contrast = vec![0.0_f32; count];
    for v in 0..count {
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        if start == end {
            continue;
        }
        let mut sum = 0.0;
        let mut n_count = 0.0;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            sum += (phi[v] - phi[n]).abs();
            n_count += 1.0;
        }
        material_contrast[v] = sum / n_count;
    }
    normalize_zscore_if_var(&mut material_contrast);

    let mut craton_resistance = vec![0.0_f32; count];
    let mut inherited_weakness = vec![0.0_f32; count];
    let mut active_damage = vec![0.0_f32; count];

    for v in 0..count {
        let phi_high = clamp(0.5 + 0.22 * phi[v], 0.0, 1.0);
        let cool = clamp(0.5 + 0.22 * downwelling[v] - 0.18 * plume[v], 0.0, 1.0);
        craton_resistance[v] = clamp(phi_high * cool, 0.0, 1.0);
        inherited_weakness[v] = clamp(
            0.09 + 0.13 * clamp(0.5 + 0.25 * material_contrast[v], 0.0, 1.0)
                + 0.08 * clamp(0.5 + 0.25 * plume[v], 0.0, 1.0),
            0.0,
            0.32,
        );
        active_damage[v] = 0.35 * inherited_weakness[v];
    }

    let evolution = evolve_damage_until_plate_field_settles(
        nbr_offsets,
        nbrs,
        phi,
        &plume,
        &downwelling,
        &shear,
        &material_contrast,
        &craton_resistance,
        &inherited_weakness,
        active_damage,
        params,
    );
    let boundary_potential = evolution.boundary_potential;
    let selected = evolution.selected;

    let min_region =
        min_region_override.unwrap_or_else(|| min_promoted_region_size(boundary_potential.len()));
    let threshold_candidates = boundary_ratio_candidates()
        .into_iter()
        .map(|ratio| {
            let mut mask = build_damage_boundary_mask(&boundary_potential, ratio);
            remove_tiny_boundary_islands(nbr_offsets, nbrs, &mut mask, 4);
            let stats = component_stats(nbr_offsets, nbrs, &mask, min_region);
            let plate_id = promote_damage_components(
                nbr_offsets,
                nbrs,
                &evolution.active_damage,
                &mask,
                min_region,
            );
            let shape_stats = plate_shape_stats(nbr_offsets, nbrs, &plate_id);
            let regime = classify_tectonic_regime(&stats);
            crate::sim::geology_types::PlateEmergenceThresholdDiagnostic {
                boundary_ratio: ratio,
                valid_count: stats.valid_count as u32,
                largest_ratio: stats.largest_ratio,
                tiny_fragment_ratio: stats.tiny_fragment_ratio,
                single_cell_plate_count: shape_stats.single_cell_plate_count as u32,
                min_plate_cells: shape_stats.min_plate_cells as u32,
                final_plate_count: shape_stats.final_plate_count as u32,
                multi_component_plate_count: shape_stats.multi_component_plate_count as u32,
                max_plate_component_count: shape_stats.max_plate_component_count as u32,
                mean_detached_fragment_ratio: shape_stats.mean_detached_fragment_ratio,
                max_plate_area_ratio: shape_stats.max_plate_area_ratio,
                second_plate_area_ratio: shape_stats.second_plate_area_ratio,
                effective_plate_count: shape_stats.effective_plate_count,
                mean_plate_boundary_complexity: shape_stats.mean_boundary_complexity,
                max_plate_boundary_complexity: shape_stats.max_boundary_complexity,
                max_enclosed_plate_risk: shape_stats.max_enclosed_plate_risk,
                regime,
                regime_score: regime_score(regime, &stats, &shape_stats),
            }
        })
        .collect::<Vec<_>>();
    crate::sim::geology_types::PlateEmergenceDiagnostics {
        seed: seed.to_string(),
        level: params.level,
        min_region: min_region as u32,
        base_step_budget: evolution.base_step_budget,
        max_step_budget: evolution.max_step_budget,
        settled_steps: evolution.settled_steps,
        selected_boundary_ratio: selected.boundary_ratio,
        selected_valid_count: selected.stats.valid_count as u32,
        selected_largest_ratio: selected.stats.largest_ratio,
        selected_tiny_fragment_ratio: selected.stats.tiny_fragment_ratio,
        selected_single_cell_plate_count: selected.shape_stats.single_cell_plate_count as u32,
        selected_min_plate_cells: selected.shape_stats.min_plate_cells as u32,
        selected_final_plate_count: selected.shape_stats.final_plate_count as u32,
        selected_multi_component_plate_count: selected.shape_stats.multi_component_plate_count
            as u32,
        selected_max_plate_component_count: selected.shape_stats.max_plate_component_count as u32,
        selected_mean_detached_fragment_ratio: selected.shape_stats.mean_detached_fragment_ratio,
        selected_max_plate_area_ratio: selected.shape_stats.max_plate_area_ratio,
        selected_second_plate_area_ratio: selected.shape_stats.second_plate_area_ratio,
        selected_effective_plate_count: selected.shape_stats.effective_plate_count,
        selected_mean_plate_boundary_complexity: selected.shape_stats.mean_boundary_complexity,
        selected_max_plate_boundary_complexity: selected.shape_stats.max_boundary_complexity,
        selected_max_enclosed_plate_risk: selected.shape_stats.max_enclosed_plate_risk,
        selected_regime: selected.regime,
        selected_regime_score: selected.regime_score,
        evolution_iterations: evolution
            .checkpoints
            .iter()
            .map(
                |checkpoint| crate::sim::geology_types::PlateEmergenceIterationDiagnostic {
                    step: checkpoint.step,
                    mean_abs_damage_delta: checkpoint.mean_abs_damage_delta,
                    max_damage_delta: checkpoint.max_damage_delta,
                    selected_boundary_ratio: checkpoint.selected.boundary_ratio,
                    selected_valid_count: checkpoint.selected.stats.valid_count as u32,
                    selected_largest_ratio: checkpoint.selected.stats.largest_ratio,
                    selected_tiny_fragment_ratio: checkpoint.selected.stats.tiny_fragment_ratio,
                    selected_single_cell_plate_count: checkpoint
                        .selected
                        .shape_stats
                        .single_cell_plate_count
                        as u32,
                    selected_min_plate_cells: checkpoint.selected.shape_stats.min_plate_cells
                        as u32,
                    selected_final_plate_count: checkpoint.selected.shape_stats.final_plate_count
                        as u32,
                    selected_multi_component_plate_count: checkpoint
                        .selected
                        .shape_stats
                        .multi_component_plate_count
                        as u32,
                    selected_max_plate_component_count: checkpoint
                        .selected
                        .shape_stats
                        .max_plate_component_count
                        as u32,
                    selected_mean_detached_fragment_ratio: checkpoint
                        .selected
                        .shape_stats
                        .mean_detached_fragment_ratio,
                    selected_max_plate_area_ratio: checkpoint
                        .selected
                        .shape_stats
                        .max_plate_area_ratio,
                    selected_second_plate_area_ratio: checkpoint
                        .selected
                        .shape_stats
                        .second_plate_area_ratio,
                    selected_effective_plate_count: checkpoint
                        .selected
                        .shape_stats
                        .effective_plate_count,
                    selected_mean_plate_boundary_complexity: checkpoint
                        .selected
                        .shape_stats
                        .mean_boundary_complexity,
                    selected_max_plate_boundary_complexity: checkpoint
                        .selected
                        .shape_stats
                        .max_boundary_complexity,
                    selected_max_enclosed_plate_risk: checkpoint
                        .selected
                        .shape_stats
                        .max_enclosed_plate_risk,
                    selected_regime: checkpoint.selected.regime,
                    selected_regime_score: checkpoint.selected.regime_score,
                },
            )
            .collect(),
        threshold_candidates,
    }
}

fn component_stats(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    boundary_mask: &[bool],
    min_region: usize,
) -> ComponentStats {
    let mut visited = vec![false; boundary_mask.len()];
    let mut stack = Vec::<usize>::new();
    let mut valid_count = 0usize;
    let mut largest = 0usize;
    let mut tiny_cells = 0usize;

    for start_v in 0..boundary_mask.len() {
        if visited[start_v] || boundary_mask[start_v] {
            continue;
        }
        visited[start_v] = true;
        stack.push(start_v);
        let mut size = 0usize;
        while let Some(v) = stack.pop() {
            size += 1;
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            for &n_u32 in &nbrs[start..end] {
                let n = n_u32 as usize;
                if visited[n] || boundary_mask[n] {
                    continue;
                }
                visited[n] = true;
                stack.push(n);
            }
        }
        largest = largest.max(size);
        if size >= min_region {
            valid_count += 1;
        } else {
            tiny_cells += size;
        }
    }

    let denom = boundary_mask.len().max(1) as f32;
    ComponentStats {
        valid_count,
        largest_ratio: largest as f32 / denom,
        tiny_fragment_ratio: tiny_cells as f32 / denom,
    }
}

fn classify_tectonic_regime(stats: &ComponentStats) -> TectonicRegime {
    if stats.valid_count < 4 || stats.largest_ratio > 0.75 {
        TectonicRegime::StagnantLid
    } else if stats.tiny_fragment_ratio > 0.28 {
        TectonicRegime::ShatteredLid
    } else {
        TectonicRegime::MobileLid
    }
}

fn regime_score(
    regime: TectonicRegime,
    stats: &ComponentStats,
    shape_stats: &PlateShapeStats,
) -> f32 {
    match regime {
        TectonicRegime::MobileLid => {
            let expected_valid_count = expected_mobile_lid_component_count(stats);
            let singleton_penalty = 0.24 * shape_stats.single_cell_plate_count as f32;
            let disconnected_plate_penalty = 0.28 * shape_stats.multi_component_plate_count as f32;
            let detached_fragment_penalty = 1.10 * shape_stats.mean_detached_fragment_ratio;
            let dominant_plate_penalty = 1.20 * (shape_stats.max_plate_area_ratio - 0.34).max(0.0);
            let weak_second_plate_penalty =
                0.60 * (0.16 - shape_stats.second_plate_area_ratio).max(0.0);
            let low_effective_plate_penalty =
                0.20 * (4.8 - shape_stats.effective_plate_count).max(0.0);
            let over_fragment_penalty =
                0.06 * (shape_stats.final_plate_count.saturating_sub(10)) as f32;
            let under_split_penalty =
                0.14 * (6usize.saturating_sub(shape_stats.final_plate_count)) as f32;
            let mean_complexity_penalty =
                0.05 * (shape_stats.mean_boundary_complexity - 4.8).max(0.0);
            let max_complexity_penalty =
                0.08 * (shape_stats.max_boundary_complexity - 6.2).max(0.0);
            let enclosed_plate_penalty = 1.80 * shape_stats.max_enclosed_plate_risk;
            (stats.largest_ratio - 0.45).abs()
                + stats.tiny_fragment_ratio
                + 0.04 * (stats.valid_count as f32 - expected_valid_count).abs()
                + singleton_penalty
                + disconnected_plate_penalty
                + detached_fragment_penalty
                + dominant_plate_penalty
                + weak_second_plate_penalty
                + low_effective_plate_penalty
                + over_fragment_penalty
                + under_split_penalty
                + mean_complexity_penalty
                + max_complexity_penalty
                + enclosed_plate_penalty
        }
        TectonicRegime::StagnantLid => {
            10.0 + stats.largest_ratio + (4 - stats.valid_count.min(4)) as f32
        }
        TectonicRegime::ShatteredLid => 10.0 + stats.tiny_fragment_ratio,
    }
}

fn expected_mobile_lid_component_count(stats: &ComponentStats) -> f32 {
    let largest_share_penalty = ((stats.largest_ratio - 0.33).max(0.0) * 6.0).clamp(0.0, 1.5);
    let fragment_penalty = (stats.tiny_fragment_ratio * 6.0).clamp(0.0, 1.0);
    (5.5 - largest_share_penalty - fragment_penalty).clamp(4.0, 6.0)
}

fn remove_tiny_boundary_islands(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    boundary_mask: &mut [bool],
    min_size: usize,
) {
    let mut visited = vec![false; boundary_mask.len()];
    let mut stack = Vec::<usize>::new();
    for start_v in 0..boundary_mask.len() {
        if visited[start_v] || !boundary_mask[start_v] {
            continue;
        }
        visited[start_v] = true;
        stack.push(start_v);
        let mut component = Vec::<usize>::new();
        while let Some(v) = stack.pop() {
            component.push(v);
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            for &n_u32 in &nbrs[start..end] {
                let n = n_u32 as usize;
                if visited[n] || !boundary_mask[n] {
                    continue;
                }
                visited[n] = true;
                stack.push(n);
            }
        }
        if component.len() < min_size {
            for v in component {
                boundary_mask[v] = false;
            }
        }
    }
}

fn prune_boundary_spurs(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    boundary_mask: &mut [bool],
    rounds: u32,
    min_boundary_neighbors: usize,
) {
    if rounds == 0 || boundary_mask.is_empty() {
        return;
    }

    for _ in 0..rounds {
        let mut to_clear = Vec::<usize>::new();
        for v in 0..boundary_mask.len() {
            if !boundary_mask[v] {
                continue;
            }
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            let mut boundary_neighbors = 0usize;
            for &n_u32 in &nbrs[start..end] {
                if boundary_mask[n_u32 as usize] {
                    boundary_neighbors += 1;
                }
            }
            if boundary_neighbors < min_boundary_neighbors {
                to_clear.push(v);
            }
        }
        if to_clear.is_empty() {
            break;
        }
        for v in to_clear {
            boundary_mask[v] = false;
        }
    }
}

fn promote_damage_components(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    damage: &[f32],
    boundary_mask: &[bool],
    min_region: usize,
) -> Vec<PlateId> {
    let count = damage.len();
    let mut component_id = vec![usize::MAX; count];
    let mut component_sizes = Vec::<usize>::new();
    let mut stack = Vec::<usize>::new();

    for start_v in 0..count {
        if boundary_mask[start_v] || component_id[start_v] != usize::MAX {
            continue;
        }
        let cid = component_sizes.len();
        component_sizes.push(0);
        component_id[start_v] = cid;
        stack.push(start_v);
        while let Some(v) = stack.pop() {
            component_sizes[cid] += 1;
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            for &n_u32 in &nbrs[start..end] {
                let n = n_u32 as usize;
                if boundary_mask[n] || component_id[n] != usize::MAX {
                    continue;
                }
                component_id[n] = cid;
                stack.push(n);
            }
        }
    }

    let mut valid_components = component_sizes
        .iter()
        .enumerate()
        .filter(|(_, size)| **size >= min_region)
        .map(|(cid, size)| (cid, *size))
        .collect::<Vec<_>>();
    valid_components.sort_by(|a, b| b.1.cmp(&a.1));
    let mut component_to_plate = vec![usize::MAX; component_sizes.len()];
    for (plate, (cid, _)) in valid_components.iter().enumerate() {
        component_to_plate[*cid] = plate;
    }

    let mut plate_id = vec![PlateId(u32::MAX); count];
    for v in 0..count {
        let cid = component_id[v];
        if cid < component_to_plate.len() && component_to_plate[cid] != usize::MAX {
            plate_id[v] = PlateId(component_to_plate[cid] as u32);
        }
    }

    absorb_unassigned_cells(nbr_offsets, nbrs, damage, &mut plate_id);
    smooth_plate_assignments(
        nbr_offsets,
        nbrs,
        &mut plate_id,
        PRE_PLATE_ASSIGNMENT_SMOOTH_ROUNDS,
        PRE_PLATE_ASSIGNMENT_DOMINANCE_MARGIN,
        PRE_PLATE_ASSIGNMENT_MIN_MAJORITY_NEIGHBORS,
    );
    compact_existing_plate_ids(plate_id)
}

fn min_promoted_region_size(cell_count: usize) -> usize {
    (cell_count / 240).clamp(4, 96)
}

fn absorb_unassigned_cells(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    damage: &[f32],
    plate_id: &mut [PlateId],
) {
    for _ in 0..plate_id.len().max(1) {
        let mut changed = false;
        let plate_sizes = plate_cell_counts_local(plate_id);
        let total_cells = plate_id.len().max(1) as f32;
        for v in 0..plate_id.len() {
            if plate_id[v].as_u32() != u32::MAX {
                continue;
            }
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            let mut best = None::<PlateId>;
            let mut best_score = f32::NEG_INFINITY;
            for &n_u32 in &nbrs[start..end] {
                let n = n_u32 as usize;
                if plate_id[n].as_u32() == u32::MAX {
                    continue;
                }
                let neighbor_plate = plate_id[n].as_usize();
                let plate_area_ratio =
                    plate_sizes.get(neighbor_plate).copied().unwrap_or(0) as f32 / total_cells;
                let size_penalty = PRE_PLATE_ABSORB_SIZE_PENALTY_GAIN
                    * (plate_area_ratio - PRE_PLATE_ABSORB_SIZE_PENALTY_START).max(0.0);
                let score = 1.0 - damage[n] - size_penalty;
                if score > best_score {
                    best_score = score;
                    best = Some(plate_id[n]);
                }
            }
            if let Some(pid) = best {
                plate_id[v] = pid;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for pid in plate_id {
        if pid.as_u32() == u32::MAX {
            *pid = PlateId(0);
        }
    }
}

fn compact_existing_plate_ids(mut plate_id: Vec<PlateId>) -> Vec<PlateId> {
    let mut ids = plate_id
        .iter()
        .map(|id| id.as_u32())
        .filter(|id| *id != u32::MAX)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    for pid in &mut plate_id {
        let old = pid.as_u32();
        let new_id = ids.iter().position(|id| *id == old).unwrap_or(0);
        *pid = PlateId(new_id as u32);
    }
    plate_id
}

fn count_unique_plates(plate_id: &[PlateId]) -> usize {
    let mut ids = plate_id.iter().map(|id| id.as_u32()).collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids.len()
}

fn smooth_plate_assignments(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &mut [PlateId],
    rounds: u32,
    dominance_margin: usize,
    min_majority_neighbors: usize,
) {
    if rounds == 0 || plate_id.is_empty() {
        return;
    }

    for _ in 0..rounds {
        let current = plate_id.to_vec();
        let plate_sizes = plate_cell_counts_local(&current);
        let plate_count = plate_sizes.len();
        if plate_count == 0 {
            break;
        }

        let mut changed = false;
        let total_cells = current.len().max(1) as f32;
        for v in 0..current.len() {
            let own_plate = current[v].as_usize();
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            if start == end {
                continue;
            }

            let mut counts = vec![0usize; plate_count];
            for &n_u32 in &nbrs[start..end] {
                let neighbor_plate = current[n_u32 as usize].as_usize();
                if neighbor_plate < plate_count {
                    counts[neighbor_plate] += 1;
                }
            }

            let own_count = counts.get(own_plate).copied().unwrap_or(0);
            let mut best_plate = own_plate;
            let mut best_count = own_count;
            for (plate, &count) in counts.iter().enumerate() {
                if count > best_count {
                    best_plate = plate;
                    best_count = count;
                }
            }

            let target_area_ratio =
                plate_sizes.get(best_plate).copied().unwrap_or(0) as f32 / total_cells;
            let dynamic_margin = dominance_margin
                + ((target_area_ratio - PRE_PLATE_SMOOTH_SIZE_MARGIN_START).max(0.0)
                    * PRE_PLATE_SMOOTH_SIZE_MARGIN_GAIN)
                    .ceil() as usize;
            if best_plate != own_plate
                && best_count >= min_majority_neighbors
                && best_count >= own_count + dynamic_margin
            {
                plate_id[v] = PlateId(best_plate as u32);
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }
}

fn plate_cell_counts_local(plate_id: &[PlateId]) -> Vec<usize> {
    let plate_count = assigned_plate_count(plate_id);
    let mut counts = vec![0usize; plate_count];
    for &pid in plate_id {
        let plate = pid.as_usize();
        if plate < plate_count {
            counts[plate] += 1;
        }
    }
    counts
}

fn plate_shape_stats(nbr_offsets: &[u32], nbrs: &[u32], plate_id: &[PlateId]) -> PlateShapeStats {
    if plate_id.is_empty() {
        return PlateShapeStats::default();
    }

    let plate_count = assigned_plate_count(plate_id);
    if plate_count == 0 {
        return PlateShapeStats::default();
    }

    let mut cell_counts = vec![0usize; plate_count];
    let mut boundary_contacts = vec![0usize; plate_count];

    for (v, &pid) in plate_id.iter().enumerate() {
        let plate = pid.as_usize();
        if plate >= plate_count {
            continue;
        }
        cell_counts[plate] += 1;

        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let neighbor_plate = plate_id[n_u32 as usize].as_usize();
            if neighbor_plate < plate_count && neighbor_plate != plate {
                boundary_contacts[plate] += 1;
            }
        }
    }

    let mut single_cell_plate_count = 0usize;
    let mut min_plate_cells = usize::MAX;
    let mut multi_component_plate_count = 0usize;
    let mut max_plate_component_count = 0usize;
    let mut detached_fragment_ratio_sum = 0.0_f32;
    let mut complexity_sum = 0.0_f32;
    let mut complexity_count = 0usize;
    let mut max_boundary_complexity = 0.0_f32;
    let mut max_enclosed_plate_risk = 0.0_f32;
    let mut area_ratios = Vec::<f32>::with_capacity(plate_count);
    let mut hhi = 0.0_f32;
    let mut visited = vec![false; plate_id.len()];
    let mut stack = Vec::<usize>::new();
    let total_cells = plate_id.len().max(1) as f32;

    for plate in 0..plate_count {
        let cells = cell_counts[plate];
        if cells == 0 {
            continue;
        }
        if cells == 1 {
            single_cell_plate_count += 1;
        }
        min_plate_cells = min_plate_cells.min(cells);
        let area_ratio = cells as f32 / total_cells;
        area_ratios.push(area_ratio);
        hhi += area_ratio * area_ratio;
        let complexity = boundary_contacts[plate] as f32 / (cells as f32).sqrt();
        complexity_sum += complexity;
        complexity_count += 1;
        max_boundary_complexity = max_boundary_complexity.max(complexity);
        max_enclosed_plate_risk = max_enclosed_plate_risk.max(enclosed_plate_risk(
            nbr_offsets,
            nbrs,
            plate_id,
            plate,
            cells,
        ));

        let mut component_count = 0usize;
        let mut largest_component_cells = 0usize;
        for start_v in 0..plate_id.len() {
            if visited[start_v] || plate_id[start_v].as_usize() != plate {
                continue;
            }
            visited[start_v] = true;
            stack.push(start_v);
            let mut component_cells = 0usize;
            while let Some(v) = stack.pop() {
                component_cells += 1;
                let start = nbr_offsets[v] as usize;
                let end = nbr_offsets[v + 1] as usize;
                for &n_u32 in &nbrs[start..end] {
                    let n = n_u32 as usize;
                    if visited[n] || plate_id[n].as_usize() != plate {
                        continue;
                    }
                    visited[n] = true;
                    stack.push(n);
                }
            }
            component_count += 1;
            largest_component_cells = largest_component_cells.max(component_cells);
        }
        if component_count > 1 {
            multi_component_plate_count += 1;
        }
        max_plate_component_count = max_plate_component_count.max(component_count);
        detached_fragment_ratio_sum += 1.0 - largest_component_cells as f32 / cells.max(1) as f32;
    }

    area_ratios.sort_by(|a, b| b.partial_cmp(a).unwrap_or(Ordering::Equal));

    PlateShapeStats {
        single_cell_plate_count,
        min_plate_cells: if min_plate_cells == usize::MAX {
            0
        } else {
            min_plate_cells
        },
        final_plate_count: area_ratios.len(),
        multi_component_plate_count,
        max_plate_component_count,
        mean_detached_fragment_ratio: if complexity_count == 0 {
            0.0
        } else {
            detached_fragment_ratio_sum / complexity_count as f32
        },
        max_plate_area_ratio: area_ratios.first().copied().unwrap_or(0.0),
        second_plate_area_ratio: area_ratios.get(1).copied().unwrap_or(0.0),
        effective_plate_count: if hhi <= 1e-6 { 0.0 } else { 1.0 / hhi },
        mean_boundary_complexity: if complexity_count == 0 {
            0.0
        } else {
            complexity_sum / complexity_count as f32
        },
        max_boundary_complexity,
        max_enclosed_plate_risk,
    }
}

fn enclosed_plate_risk(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate: usize,
    plate_cells: usize,
) -> f32 {
    if plate_cells == 0 || plate_id.is_empty() {
        return 0.0;
    }
    let mut contacts = std::collections::BTreeMap::<u32, u32>::new();
    let mut total_contacts = 0_u32;
    for v in 0..plate_id.len() {
        if plate_id[v].as_usize() != plate {
            continue;
        }
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if n >= plate_id.len() || plate_id[n].as_usize() == plate {
                continue;
            }
            total_contacts = total_contacts.saturating_add(1);
            *contacts.entry(plate_id[n].as_u32()).or_insert(0) += 1;
        }
    }
    let Some(dominant_contacts) = contacts.values().copied().max() else {
        return 0.0;
    };
    let dominant_ratio = dominant_contacts as f32 / total_contacts.max(1) as f32;
    let area_ratio = plate_cells as f32 / plate_id.len().max(1) as f32;
    let small_plate_gate = ((0.10 - area_ratio) / 0.10).clamp(0.0, 1.0);
    dominant_ratio * small_plate_gate
}

fn assigned_plate_count(plate_id: &[PlateId]) -> usize {
    plate_id
        .iter()
        .filter_map(|pid| {
            if pid.as_u32() == u32::MAX {
                None
            } else {
                Some(pid.as_usize())
            }
        })
        .max()
        .map_or(0, |max_id| max_id.saturating_add(1))
}

fn build_initial_plate_kinematics(
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    plate_count: usize,
    plume: &[f32],
    downwelling: &[f32],
    craton_resistance: &[f32],
    rng: &mut DeterministicRng,
) -> Vec<InitialPlateKinematics> {
    let mut centroid = vec![[0.0_f32; 3]; plate_count];
    let mut plume_vec = vec![[0.0_f32; 3]; plate_count];
    let mut down_vec = vec![[0.0_f32; 3]; plate_count];
    let mut craton_sum = vec![0.0_f32; plate_count];
    let mut counts = vec![0.0_f32; plate_count];

    for (v, &pid) in plate_id.iter().enumerate() {
        let p = pid.as_usize();
        if p >= plate_count {
            continue;
        }
        centroid[p] = add3(centroid[p], positions[v]);
        plume_vec[p] = add3(plume_vec[p], mul3(positions[v], plume[v]));
        down_vec[p] = add3(down_vec[p], mul3(positions[v], downwelling[v]));
        craton_sum[p] += craton_resistance[v];
        counts[p] += 1.0;
    }

    let mut out = Vec::with_capacity(plate_count);
    for pid in 0..plate_count {
        let center = if counts[pid] > 0.0 {
            normalize3(centroid[pid])
        } else {
            random_unit_vector3(rng)
        };
        let plume_bias = project_to_tangent(plume_vec[pid], center);
        let down_bias = project_to_tangent(down_vec[pid], center);
        let random_axis = random_unit_vector3(rng);
        let mixed_axis = normalize3(add3(
            mul3(random_axis, 0.58),
            add3(mul3(plume_bias, 0.24), mul3(down_bias, 0.18)),
        ));
        let angular_axis = if length3(mixed_axis) <= 1e-6 {
            random_axis
        } else {
            mixed_axis
        };
        let craton_mean = if counts[pid] > 0.0 {
            craton_sum[pid] / counts[pid]
        } else {
            0.0
        };
        let subduction_tendency = clamp(
            length3(down_bias) * 0.35 + (1.0 - craton_mean) * 0.18,
            0.0,
            1.0,
        );
        let angular_speed = clamp(
            rng.gen_range_f32(0.055, 0.16) * (1.0 - 0.35 * craton_mean)
                + 0.02 * subduction_tendency,
            0.025,
            0.22,
        );
        out.push(InitialPlateKinematics {
            angular_axis,
            angular_speed,
            activity: clamp(0.45 + 0.35 * subduction_tendency, 0.0, 1.0),
            plume_divergence_bias: plume_bias,
            downwelling_convergence_bias: down_bias,
            subduction_tendency,
            craton_resistance: craton_mean,
        });
    }
    out
}

pub(super) fn bias_plate_motion_from_pre_plate_fields(
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    attributes: &mut [PlateAttr],
    plume: &[f32],
    downwelling: &[f32],
    craton_resistance: &[f32],
) {
    let plate_count = attributes.len();
    if plate_count == 0 {
        return;
    }
    let mut centroid = vec![[0.0_f32; 3]; plate_count];
    let mut plume_vec = vec![[0.0_f32; 3]; plate_count];
    let mut down_vec = vec![[0.0_f32; 3]; plate_count];
    let mut craton_sum = vec![0.0_f32; plate_count];
    let mut counts = vec![0.0_f32; plate_count];

    for (v, &pid) in plate_id.iter().enumerate() {
        let p = pid.as_usize();
        if p >= plate_count {
            continue;
        }
        centroid[p] = add3(centroid[p], positions[v]);
        plume_vec[p] = add3(plume_vec[p], mul3(positions[v], plume[v]));
        down_vec[p] = add3(down_vec[p], mul3(positions[v], downwelling[v]));
        craton_sum[p] += craton_resistance[v];
        counts[p] += 1.0;
    }

    for pid in 0..plate_count {
        if counts[pid] <= 0.0 {
            continue;
        }
        let center = normalize3(centroid[pid]);
        let plume_bias = project_to_tangent(plume_vec[pid], center);
        let down_bias = project_to_tangent(down_vec[pid], center);
        let craton_mean = craton_sum[pid] / counts[pid];
        let inherited = attributes[pid].velocity;
        let speed_scale = clamp(1.0 - 0.35 * craton_mean, 0.45, 1.0);
        let mixed = add3(
            mul3(inherited, 0.72),
            add3(mul3(plume_bias, 0.22), mul3(down_bias, 0.18)),
        );
        let tangent = project_to_tangent(mixed, center);
        if length3(tangent) > 1e-6 {
            attributes[pid].velocity = mul3(tangent, speed_scale);
            attributes[pid].drift_axis_primary = normalize3(add3(
                mul3(attributes[pid].drift_axis_primary, 0.65),
                mul3(plume_bias, 0.35),
            ));
            attributes[pid].drift_axis_secondary = normalize3(add3(
                mul3(attributes[pid].drift_axis_secondary, 0.70),
                mul3(down_bias, 0.30),
            ));
        }
    }
}

pub(super) fn spherical_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    clamp(dot3(a, b), -1.0, 1.0).acos()
}

pub(super) fn validate_plate_partition(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate_count: usize,
) -> Result<(), String> {
    if plate_count == 0 {
        return Err("plate_count is zero".to_string());
    }

    let mut counts = vec![0usize; plate_count];
    for (cell, &pid) in plate_id.iter().enumerate() {
        let pid = pid.as_usize();
        if pid >= plate_count {
            return Err(format!(
                "cell {cell} has invalid plate_id {pid}; plate_count={plate_count}"
            ));
        }
        counts[pid] += 1;
    }

    if let Some((pid, _)) = counts.iter().enumerate().find(|(_, count)| **count == 0) {
        return Err(format!("plate {pid} is empty"));
    }

    let components = component_counts_by_plate(nbr_offsets, nbrs, plate_id, plate_count);
    if let Some((pid, count)) = components
        .iter()
        .enumerate()
        .find(|(_, component_count)| **component_count > 1)
    {
        return Err(format!("plate {pid} has {count} disconnected components"));
    }

    Ok(())
}

pub(super) fn component_counts_by_plate(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate_count: usize,
) -> Vec<usize> {
    let mut components = vec![0usize; plate_count];
    let mut visited = vec![false; plate_id.len()];
    let mut stack = Vec::<usize>::new();

    for start_v in 0..plate_id.len() {
        if visited[start_v] {
            continue;
        }
        visited[start_v] = true;
        let plate = plate_id[start_v].as_usize();
        if plate >= plate_count {
            continue;
        }
        components[plate] += 1;

        stack.push(start_v);
        while let Some(v) = stack.pop() {
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            for &n in &nbrs[start..end] {
                let n = n as usize;
                if visited[n] || plate_id[n].as_usize() != plate {
                    continue;
                }
                visited[n] = true;
                stack.push(n);
            }
        }
    }

    components
}

pub(super) fn assign_plate_attributes(
    plate_id: &[PlateId],
    plate_count: usize,
    phi: &[f32],
    rng: &mut DeterministicRng,
    ocean_plate_ratio: f32,
) -> Vec<PlateAttr> {
    let mut plate_counts = vec![0usize; plate_count];
    let mut plate_phi_sum = vec![0.0f32; plate_count];
    for (v, &pid) in plate_id.iter().enumerate() {
        let pid_idx = pid.as_usize();
        if pid_idx >= plate_count {
            continue;
        }
        plate_counts[pid_idx] += 1;
        plate_phi_sum[pid_idx] += phi[v];
    }

    let mut plate_scores = Vec::with_capacity(plate_count);
    for pid in 0..plate_count {
        let mean_phi = if plate_counts[pid] > 0 {
            plate_phi_sum[pid] / plate_counts[pid] as f32
        } else {
            0.0
        };
        let jitter = rng.gen_range_f32(-0.12, 0.12);
        plate_scores.push((pid, mean_phi + jitter, mean_phi));
    }
    plate_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

    let mut ocean_target = ((plate_count as f32) * ocean_plate_ratio).round() as usize;
    if plate_count >= 2 {
        ocean_target = ocean_target.clamp(1, plate_count - 1);
    } else {
        ocean_target = ocean_target.min(plate_count);
    }
    let continent_target = plate_count.saturating_sub(ocean_target);
    let mut is_ocean_plate = vec![true; plate_count];
    for (rank, (pid, _, _)) in plate_scores.iter().enumerate() {
        is_ocean_plate[*pid] = rank >= continent_target;
    }

    let mut attrs = Vec::with_capacity(plate_count);

    for pid in 0..plate_count {
        let is_ocean = is_ocean_plate[pid];
        let dir = rng.gen_range_f32(0.0, 2.0 * std::f32::consts::PI);
        let speed = rng.gen_range_f32(0.3, 1.0);
        let velocity = [speed * dir.cos(), speed * dir.sin(), 0.0];
        let drift_axis_primary = random_unit_vector3(rng);
        let drift_axis_secondary = random_unit_vector3(rng);
        let drift_mix_axis = random_unit_vector3(rng);
        let drift_variability = rng.gen_range_f32(0.06, 0.32);
        let mean_phi = if plate_counts[pid] > 0 {
            plate_phi_sum[pid] / plate_counts[pid] as f32
        } else {
            0.0
        };

        let base_height = if is_ocean {
            clamp(
                -0.09 + 0.02 * mean_phi + rng.gen_range_f32(-0.03, 0.02),
                -0.20,
                -0.02,
            )
        } else {
            clamp(
                0.12 + 0.05 * mean_phi + rng.gen_range_f32(-0.05, 0.06),
                0.03,
                0.30,
            )
        };
        let base_weight = if is_ocean {
            0.62 + rng.gen_range_f32(-0.06, 0.08)
        } else {
            0.22 + rng.gen_range_f32(-0.04, 0.04)
        };

        attrs.push(PlateAttr {
            is_ocean,
            velocity,
            drift_axis_primary,
            drift_axis_secondary,
            drift_mix_axis,
            drift_variability,
            base_height,
            base_weight,
        });
    }

    attrs
}

pub(super) fn compute_vertex_lithosphere(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    attributes: &[PlateAttr],
    boundary_edges: &[BoundaryEdge],
    params: &GeologyParams,
) -> Vec<VertexLithosphere> {
    const AGE_SPEED_REF: f32 = 0.65;
    const AGE_DIRECTIONAL_INFLUENCE: f32 = 0.35;

    let v_count = positions.len();
    let mut crust_age_dist = vec![f32::INFINITY; v_count];
    let mut lith = vec![
        VertexLithosphere {
            age_norm: 0.0,
            weight: 0.0,
            buoyancy: 0.0,
            competence: 0.5,
        };
        v_count
    ];
    let mut heap = BinaryHeap::new();
    let plate_count = attributes.len();
    let mut plate_age_distance_weight = vec![1.0_f32; plate_count];

    // プレートの速度に応じて重み付け
    for (pid, attr) in attributes.iter().enumerate() {
        let speed = length3(attr.velocity).max(1e-4);
        plate_age_distance_weight[pid] = AGE_SPEED_REF / speed;
    }

    let mut has_divergent_source = vec![false; plate_count];
    let mut has_boundary_seed = vec![false; plate_count];

    for i in 0..v_count {
        let pid = plate_id[i].as_usize();
        lith[i].weight = attributes[pid].base_weight;
        lith[i].buoyancy = attributes[pid].base_height;
        lith[i].competence = 0.5;
    }

    let mut continental_competence_raw = vec![0.0_f32; v_count];
    for v in 0..v_count {
        let pid = plate_id[v].as_usize();
        if attributes[pid].is_ocean {
            continue;
        }
        continental_competence_raw[v] = sample_continental_competence_noise(
            positions[v],
            pid as u32,
            params.continent_competence_large_scale,
            params.continent_competence_mid_scale,
        );
    }
    smooth_continental_field_by_plate(
        nbr_offsets,
        nbrs,
        plate_id,
        attributes,
        &mut continental_competence_raw,
        3,
    );

    for edge in boundary_edges {
        let is_divergent = matches!(edge.boundary_type, EdgeReliefType::Divergent);
        for &v in &[edge.a, edge.b] {
            let pv = plate_id[v].as_usize();
            if !attributes[pv].is_ocean {
                continue;
            }
            has_boundary_seed[pv] = true;
            if is_divergent {
                has_divergent_source[pv] = true;
                if crust_age_dist[v] > 0.0 {
                    crust_age_dist[v] = 0.0;
                    heap.push(BoundaryDistState {
                        cost: 0.0,
                        vertex: v,
                        source_edge: v,
                    });
                }
            }
        }
    }

    for i in 0..v_count {
        let pid = plate_id[i].as_usize();
        if !attributes[pid].is_ocean {
            continue;
        }
        if has_divergent_source[pid] {
            continue;
        }
        if has_boundary_seed[pid] && crust_age_dist[i].is_infinite() {
            let start = nbr_offsets[i] as usize;
            let end = nbr_offsets[i + 1] as usize;
            let is_boundary = nbrs[start..end]
                .iter()
                .any(|&n| plate_id[n as usize] != plate_id[i]);
            if is_boundary {
                crust_age_dist[i] = 0.0;
                heap.push(BoundaryDistState {
                    cost: 0.0,
                    vertex: i,
                    source_edge: i,
                });
            }
        }
    }

    while let Some(state) = heap.pop() {
        if state.cost > crust_age_dist[state.vertex] + 1e-6 {
            continue;
        }
        let pid = plate_id[state.vertex].as_usize();
        if !attributes[pid].is_ocean {
            continue;
        }

        let start = nbr_offsets[state.vertex] as usize;
        let end = nbr_offsets[state.vertex + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if plate_id[n] != plate_id[state.vertex] {
                continue;
            }
            let npid = plate_id[n].as_usize();
            if !attributes[npid].is_ocean {
                continue;
            }
            let step = chord_distance(positions[state.vertex], positions[n]).max(1e-4);
            let base_weight = plate_age_distance_weight[pid];

            let edge_vec = sub3(positions[n], positions[state.vertex]);
            let edge_tangent = project_to_tangent(edge_vec, positions[state.vertex]);
            let edge_dir = normalize3(edge_tangent);

            let plate_vel_tangent =
                local_plate_velocity(&attributes[pid], pid, positions[state.vertex]);
            let plate_vel_dir = normalize3(plate_vel_tangent);
            let dir_alignment = dot3(edge_dir, plate_vel_dir);
            let dir_weight = if length3(plate_vel_tangent) <= 1e-6 {
                1.0
            } else {
                clamp(1.0 - AGE_DIRECTIONAL_INFLUENCE * dir_alignment, 0.55, 1.45)
            };

            let weighted_step = step * base_weight * dir_weight;
            let next_cost = state.cost + weighted_step;
            if next_cost + 1e-6 < crust_age_dist[n] {
                crust_age_dist[n] = next_cost;
                heap.push(BoundaryDistState {
                    cost: next_cost,
                    vertex: n,
                    source_edge: state.source_edge,
                });
            }
        }
    }

    let mut ocean_plate_max_age = vec![0.0_f32; plate_count];
    for v in 0..v_count {
        let pid = plate_id[v].as_usize();
        if !attributes[pid].is_ocean {
            continue;
        }
        if crust_age_dist[v].is_finite() {
            ocean_plate_max_age[pid] = ocean_plate_max_age[pid].max(crust_age_dist[v]);
        }
    }

    for v in 0..v_count {
        let pid = plate_id[v].as_usize();
        if !attributes[pid].is_ocean {
            lith[v].age_norm = 0.0;
            let competence = clamp(
                0.5 + params.continent_competence_noise_gain * continental_competence_raw[v],
                0.0,
                1.0,
            );
            let weight = attributes[pid].base_weight
                + params.continent_competence_weight_gain * (competence - 0.5);
            lith[v].weight = weight;
            lith[v].buoyancy = attributes[pid].base_height;
            lith[v].competence = competence;
            continue;
        }
        let max_age = ocean_plate_max_age[pid].max(1e-4);
        let age = if crust_age_dist[v].is_finite() {
            clamp(crust_age_dist[v] / max_age, 0.0, 1.0)
        } else {
            0.0
        };
        let weight = attributes[pid].base_weight + 0.42 * age;
        // 海洋の標高は「海嶺で軽く高い → 老化で重く低い」を浮力で一元表現する。
        let buoyancy = (-0.08 + 0.06 * (1.0 - age)) - 0.26 * (weight - 0.62);
        lith[v] = VertexLithosphere {
            age_norm: age,
            weight,
            buoyancy,
            competence: 0.5,
        };
    }

    lith
}

pub(super) fn sample_continental_competence_noise(
    pos: [f32; 3],
    plate_seed: u32,
    large_scale: f32,
    mid_scale: f32,
) -> f32 {
    let axis_a = seeded_unit_vec(plate_seed ^ 0x85eb_ca6b);
    let axis_b = seeded_unit_vec(plate_seed ^ 0xc2b2_ae35);
    let axis_c = seeded_unit_vec(plate_seed ^ 0x27d4_eb2f);
    let phase_a = std::f32::consts::TAU * hash01_u32(plate_seed ^ 0x517c_c1b7);
    let phase_b = std::f32::consts::TAU * hash01_u32(plate_seed ^ 0x9e37_79b9);
    let phase_c = std::f32::consts::TAU * hash01_u32(plate_seed ^ 0x94d0_49bb);

    let large = (dot3(pos, axis_a) * large_scale + phase_a).sin();
    let mid_primary = (dot3(pos, axis_b) * mid_scale + phase_b).sin();
    let mid_secondary = (dot3(pos, axis_c) * (mid_scale * 1.37) + phase_c).sin();
    let mixed = 0.70 * large + 0.20 * mid_primary + 0.10 * mid_secondary;
    clamp(mixed, -1.0, 1.0)
}

pub(super) fn smooth_continental_field_by_plate(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    attributes: &[PlateAttr],
    field: &mut [f32],
    iter: u32,
) {
    if iter == 0 || field.is_empty() {
        return;
    }
    let mut buf = field.to_vec();
    for _ in 0..iter {
        for v in 0..field.len() {
            let pid = plate_id[v].as_usize();
            if attributes[pid].is_ocean {
                buf[v] = field[v];
                continue;
            }
            let mut sum = field[v];
            let mut wsum = 1.0_f32;
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            for &n_u32 in &nbrs[start..end] {
                let n = n_u32 as usize;
                if plate_id[n] != plate_id[v] {
                    continue;
                }
                sum += field[n];
                wsum += 1.0;
            }
            buf[v] = sum / wsum;
        }
        field.copy_from_slice(&buf);
    }
}

pub(super) fn hash01_u32(seed: u32) -> f32 {
    let s = ((seed as f32) * 12.9898 + 78.233).sin();
    fract01(s * 43_758.547)
}

pub(super) fn seeded_unit_vec(seed: u32) -> [f32; 3] {
    let z = 2.0 * hash01_u32(seed ^ 0x68bc_21eb) - 1.0;
    let phi = std::f32::consts::TAU * hash01_u32(seed ^ 0x02e5_be93);
    let r = (1.0 - z * z).max(0.0).sqrt();
    [r * phi.cos(), z, r * phi.sin()]
}

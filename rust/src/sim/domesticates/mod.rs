pub mod types;

#[allow(unused_imports)]
pub use crate::sim::domesticates::types::*;

use crate::sim::world::{Biome, EraKind, World, N_CROPS, N_LIVESTOCK};

const ORIGIN_COUNT_LIMIT: usize = 3;
const ORIGIN_SEED_STRENGTH_CROP: f32 = 0.26;
const ORIGIN_SEED_STRENGTH_LIVESTOCK: f32 = 0.22;
const FEEDBACK_RETENTION: f32 = 0.35;
const POP_PRESSURE_RETENTION: f32 = 0.65;

#[derive(Clone, Copy)]
struct SpeciesParams {
    temp_optimum: f32,
    temp_sigma: f32,
    moisture_optimum: f32,
    moisture_sigma: f32,
    height_limit: f32,
    tree_pref: f32,
    ground_pref: f32,
    river_bonus_weight: f32,
    threshold: f32,
    growth_rate: f32,
    decay_rate: f32,
}

const CROP_PARAMS: [SpeciesParams; N_CROPS] = [
    SpeciesParams {
        temp_optimum: 15.0,
        temp_sigma: 10.0,
        moisture_optimum: 0.58,
        moisture_sigma: 0.24,
        height_limit: 1.30,
        tree_pref: 0.30,
        ground_pref: 0.65,
        river_bonus_weight: 0.12,
        threshold: 0.15,
        growth_rate: 0.12,
        decay_rate: 0.035,
    }, // Wheat
    SpeciesParams {
        temp_optimum: 25.0,
        temp_sigma: 7.0,
        moisture_optimum: 0.82,
        moisture_sigma: 0.15,
        height_limit: 0.80,
        tree_pref: 0.22,
        ground_pref: 0.68,
        river_bonus_weight: 0.40,
        threshold: 0.14,
        growth_rate: 0.14,
        decay_rate: 0.040,
    }, // Rice
    SpeciesParams {
        temp_optimum: 22.0,
        temp_sigma: 9.0,
        moisture_optimum: 0.58,
        moisture_sigma: 0.24,
        height_limit: 1.40,
        tree_pref: 0.28,
        ground_pref: 0.62,
        river_bonus_weight: 0.08,
        threshold: 0.14,
        growth_rate: 0.12,
        decay_rate: 0.036,
    }, // Maize
    SpeciesParams {
        temp_optimum: 13.0,
        temp_sigma: 10.0,
        moisture_optimum: 0.36,
        moisture_sigma: 0.20,
        height_limit: 1.60,
        tree_pref: 0.20,
        ground_pref: 0.72,
        river_bonus_weight: 0.06,
        threshold: 0.13,
        growth_rate: 0.11,
        decay_rate: 0.030,
    }, // Millet
    SpeciesParams {
        temp_optimum: 18.0,
        temp_sigma: 11.0,
        moisture_optimum: 0.66,
        moisture_sigma: 0.23,
        height_limit: 1.50,
        tree_pref: 0.35,
        ground_pref: 0.55,
        river_bonus_weight: 0.26,
        threshold: 0.12,
        growth_rate: 0.11,
        decay_rate: 0.032,
    }, // Tuber
    SpeciesParams {
        temp_optimum: 17.0,
        temp_sigma: 11.0,
        moisture_optimum: 0.54,
        moisture_sigma: 0.28,
        height_limit: 1.55,
        tree_pref: 0.32,
        ground_pref: 0.56,
        river_bonus_weight: 0.10,
        threshold: 0.12,
        growth_rate: 0.11,
        decay_rate: 0.030,
    }, // Legume
    SpeciesParams {
        temp_optimum: 10.0,
        temp_sigma: 11.0,
        moisture_optimum: 0.34,
        moisture_sigma: 0.19,
        height_limit: 1.80,
        tree_pref: 0.22,
        ground_pref: 0.70,
        river_bonus_weight: 0.05,
        threshold: 0.13,
        growth_rate: 0.10,
        decay_rate: 0.028,
    }, // Barley
];

const LIVESTOCK_PARAMS: [SpeciesParams; N_LIVESTOCK] = [
    SpeciesParams {
        temp_optimum: 18.0,
        temp_sigma: 12.0,
        moisture_optimum: 0.46,
        moisture_sigma: 0.25,
        height_limit: 1.65,
        tree_pref: 0.24,
        ground_pref: 0.74,
        river_bonus_weight: 0.04,
        threshold: 0.12,
        growth_rate: 0.10,
        decay_rate: 0.028,
    }, // Cattle
    SpeciesParams {
        temp_optimum: 12.0,
        temp_sigma: 12.0,
        moisture_optimum: 0.36,
        moisture_sigma: 0.18,
        height_limit: 1.90,
        tree_pref: 0.18,
        ground_pref: 0.82,
        river_bonus_weight: 0.02,
        threshold: 0.11,
        growth_rate: 0.10,
        decay_rate: 0.026,
    }, // Horse
    SpeciesParams {
        temp_optimum: 11.0,
        temp_sigma: 12.0,
        moisture_optimum: 0.30,
        moisture_sigma: 0.18,
        height_limit: 2.20,
        tree_pref: 0.16,
        ground_pref: 0.86,
        river_bonus_weight: 0.01,
        threshold: 0.10,
        growth_rate: 0.10,
        decay_rate: 0.024,
    }, // Sheep
    SpeciesParams {
        temp_optimum: 20.0,
        temp_sigma: 10.0,
        moisture_optimum: 0.62,
        moisture_sigma: 0.22,
        height_limit: 1.40,
        tree_pref: 0.46,
        ground_pref: 0.52,
        river_bonus_weight: 0.16,
        threshold: 0.12,
        growth_rate: 0.11,
        decay_rate: 0.031,
    }, // Pig
    SpeciesParams {
        temp_optimum: 28.0,
        temp_sigma: 8.0,
        moisture_optimum: 0.18,
        moisture_sigma: 0.12,
        height_limit: 1.60,
        tree_pref: 0.08,
        ground_pref: 0.54,
        river_bonus_weight: -0.08,
        threshold: 0.11,
        growth_rate: 0.11,
        decay_rate: 0.028,
    }, // Camel
];

pub(crate) fn update_domesticates(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    let n = world.state.geology.height.len();
    ensure_domesticates_state_shape(world, n);

    if !domesticates_enabled(world.clock.epoch) {
        disable_domesticates(world);
        return;
    }

    let prev_crop_adoption = world.state.domesticates.crop_adoption.clone();
    let prev_livestock_adoption = world.state.domesticates.livestock_adoption.clone();
    let river_max = world
        .state
        .hydrology
        .river_flow
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-6);
    let dt = (budget.max(1) as f32).min(4.0);

    let mut crop_niche = vec![[0.0; N_CROPS]; n];
    let mut livestock_niche = vec![[0.0; N_LIVESTOCK]; n];

    for i in 0..n {
        if world.state.geology.height[i] <= world.control.sea_level_offset {
            world.state.domesticates.crop_available[i] = 0;
            world.state.domesticates.livestock_available[i] = 0;
            continue;
        }
        let context = CellContext::from_world(world, i, river_max);
        let mut crop_bits = 0u8;
        for (kind_idx, params) in CROP_PARAMS.iter().enumerate() {
            let niche = crop_niche_score(kind_idx, &context, *params);
            crop_niche[i][kind_idx] = niche;
            if niche >= params.threshold && !crop_hard_exclusion(kind_idx, &context) {
                crop_bits |= 1u8 << kind_idx;
            }
        }
        let mut livestock_bits = 0u8;
        for (kind_idx, params) in LIVESTOCK_PARAMS.iter().enumerate() {
            let niche = livestock_niche_score(kind_idx, &context, *params);
            livestock_niche[i][kind_idx] = niche;
            if niche >= params.threshold && !livestock_hard_exclusion(kind_idx, &context) {
                livestock_bits |= 1u8 << kind_idx;
            }
        }
        world.state.domesticates.crop_available[i] = crop_bits;
        world.state.domesticates.livestock_available[i] = livestock_bits;
    }

    if !world
        .state
        .domesticates
        .domesticates_internal
        .iter()
        .any(|internal| internal.origin_initialized)
    {
        seed_origins(world, &crop_niche, &livestock_niche, river_max);
    }

    for i in 0..n {
        let is_land = world.state.geology.height[i] > world.control.sea_level_offset;
        if !is_land {
            world.state.domesticates.crop_adoption[i] = [0.0; N_CROPS];
            world.state.domesticates.livestock_adoption[i] = [0.0; N_LIVESTOCK];
            let internal = &mut world.state.domesticates.domesticates_internal[i];
            internal.spread_pressure_crop = [0.0; N_CROPS];
            internal.spread_pressure_livestock = [0.0; N_LIVESTOCK];
            internal.routed_feedback_crop = [0.0; N_CROPS];
            internal.routed_feedback_livestock = [0.0; N_LIVESTOCK];
            internal.population_pressure_bonus = 0.0;
            continue;
        }

        let context = CellContext::from_world(world, i, river_max);
        let local_conductance = terrain_conductance(&context);
        let population_bonus = world.state.domesticates.domesticates_internal[i]
            .population_pressure_bonus
            .clamp(0.0, 0.9);
        let intensification_factor = 1.0 + population_bonus;

        let mut next_crop = [0.0; N_CROPS];
        for kind_idx in 0..N_CROPS {
            let local_neighbor = local_neighbor_adoption(&prev_crop_adoption, world, i, kind_idx);
            let routed = world.state.domesticates.domesticates_internal[i].routed_feedback_crop
                [kind_idx]
                .max(0.0);
            let spread = (local_neighbor * local_conductance + routed).clamp(0.0, 1.0);
            world.state.domesticates.domesticates_internal[i].spread_pressure_crop[kind_idx] =
                spread;

            let available = (world.state.domesticates.crop_available[i] & (1u8 << kind_idx)) != 0;
            let target = if available {
                let origin =
                    world.state.domesticates.domesticates_internal[i].origin_seed_crop[kind_idx];
                clamp01(origin.max(spread) * intensification_factor)
            } else {
                0.0
            };
            let current = prev_crop_adoption[i][kind_idx];
            let unsuitability = if available {
                0.0
            } else {
                (1.0 - crop_niche[i][kind_idx]).clamp(0.2, 1.0)
            };
            let params = CROP_PARAMS[kind_idx];
            let next = current + params.growth_rate * (target - current) * dt
                - params.decay_rate * unsuitability * dt;
            next_crop[kind_idx] = clamp01(next);
        }

        let mut next_livestock = [0.0; N_LIVESTOCK];
        for kind_idx in 0..N_LIVESTOCK {
            let local_neighbor =
                local_neighbor_adoption(&prev_livestock_adoption, world, i, kind_idx);
            let routed = world.state.domesticates.domesticates_internal[i]
                .routed_feedback_livestock[kind_idx]
                .max(0.0);
            let spread = (local_neighbor * local_conductance + routed).clamp(0.0, 1.0);
            world.state.domesticates.domesticates_internal[i].spread_pressure_livestock[kind_idx] =
                spread;

            let available =
                (world.state.domesticates.livestock_available[i] & (1u8 << kind_idx)) != 0;
            let target = if available {
                let origin = world.state.domesticates.domesticates_internal[i]
                    .origin_seed_livestock[kind_idx];
                clamp01(origin.max(spread) * intensification_factor)
            } else {
                0.0
            };
            let current = prev_livestock_adoption[i][kind_idx];
            let unsuitability = if available {
                0.0
            } else {
                (1.0 - livestock_niche[i][kind_idx]).clamp(0.2, 1.0)
            };
            let params = LIVESTOCK_PARAMS[kind_idx];
            let next = current + params.growth_rate * (target - current) * dt
                - params.decay_rate * unsuitability * dt;
            next_livestock[kind_idx] = clamp01(next);
        }

        world.state.domesticates.crop_adoption[i] = next_crop;
        world.state.domesticates.livestock_adoption[i] = next_livestock;

        let internal = &mut world.state.domesticates.domesticates_internal[i];
        for value in internal.routed_feedback_crop.iter_mut() {
            *value = (*value * FEEDBACK_RETENTION).max(0.0);
        }
        for value in internal.routed_feedback_livestock.iter_mut() {
            *value = (*value * FEEDBACK_RETENTION).max(0.0);
        }
        internal.population_pressure_bonus =
            (internal.population_pressure_bonus * POP_PRESSURE_RETENTION).max(0.0);
        internal.diffusion_memory =
            (internal.diffusion_memory * 0.85 + local_conductance * 0.15).clamp(0.0, 1.0);
    }
}

fn seed_origins(
    world: &mut World,
    crop_niche: &[[f32; N_CROPS]],
    livestock_niche: &[[f32; N_LIVESTOCK]],
    river_max: f32,
) {
    let n = world.cell_count();
    for kind_idx in 0..N_CROPS {
        let chosen = choose_origin_cells_for_crop(world, crop_niche, river_max, kind_idx);
        for index in chosen {
            world.state.domesticates.domesticates_internal[index].origin_seed_crop[kind_idx] =
                ORIGIN_SEED_STRENGTH_CROP;
        }
    }
    for kind_idx in 0..N_LIVESTOCK {
        let chosen = choose_origin_cells_for_livestock(world, livestock_niche, river_max, kind_idx);
        for index in chosen {
            world.state.domesticates.domesticates_internal[index].origin_seed_livestock[kind_idx] =
                ORIGIN_SEED_STRENGTH_LIVESTOCK;
        }
    }
    for i in 0..n {
        world.state.domesticates.domesticates_internal[i].origin_initialized = true;
    }
}

fn choose_origin_cells_for_crop(
    world: &World,
    crop_niche: &[[f32; N_CROPS]],
    river_max: f32,
    kind_idx: usize,
) -> Vec<usize> {
    choose_origin_cells(
        world,
        river_max,
        |index| crop_niche[index][kind_idx],
        |index| (world.state.domesticates.crop_available[index] & (1u8 << kind_idx)) != 0,
    )
}

fn choose_origin_cells_for_livestock(
    world: &World,
    livestock_niche: &[[f32; N_LIVESTOCK]],
    river_max: f32,
    kind_idx: usize,
) -> Vec<usize> {
    choose_origin_cells(
        world,
        river_max,
        |index| livestock_niche[index][kind_idx],
        |index| (world.state.domesticates.livestock_available[index] & (1u8 << kind_idx)) != 0,
    )
}

fn choose_origin_cells(
    world: &World,
    river_max: f32,
    niche_at: impl Fn(usize) -> f32,
    is_available: impl Fn(usize) -> bool,
) -> Vec<usize> {
    let mut candidates = Vec::new();
    for i in 0..world.cell_count() {
        if world.state.geology.height[i] <= world.control.sea_level_offset || !is_available(i) {
            continue;
        }
        let context = CellContext::from_world(world, i, river_max);
        let corridor = corridor_score(&context);
        let human = human_management_score(&context);
        let potential = niche_at(i) * corridor * human;
        if potential > 0.0 {
            candidates.push((i, potential));
        }
    }
    candidates.sort_by(|lhs, rhs| {
        rhs.1
            .partial_cmp(&lhs.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(lhs.0.cmp(&rhs.0))
    });
    if candidates.is_empty() {
        return Vec::new();
    }
    let top = candidates[0].1.max(1e-6);
    let cutoff = top * 0.7;
    let mut chosen = Vec::new();
    'outer: for (idx, potential) in candidates {
        if potential < cutoff && !chosen.is_empty() {
            continue;
        }
        for &existing in &chosen {
            if world
                .cell_neighbors(existing)
                .iter()
                .any(|&n| n as usize == idx)
            {
                continue 'outer;
            }
        }
        chosen.push(idx);
        if chosen.len() >= ORIGIN_COUNT_LIMIT {
            break;
        }
    }
    if chosen.is_empty() {
        chosen.push(candidates_fallback(world, is_available));
    }
    chosen
}

fn candidates_fallback(world: &World, is_available: impl Fn(usize) -> bool) -> usize {
    for i in 0..world.cell_count() {
        if world.state.geology.height[i] > world.control.sea_level_offset && is_available(i) {
            return i;
        }
    }
    0
}

fn domesticates_enabled(epoch: EraKind) -> bool {
    matches!(epoch, EraKind::Civilization | EraKind::History)
}

fn disable_domesticates(world: &mut World) {
    for i in 0..world.cell_count() {
        world.state.domesticates.crop_available[i] = 0;
        world.state.domesticates.livestock_available[i] = 0;
        world.state.domesticates.crop_adoption[i] = [0.0; N_CROPS];
        world.state.domesticates.livestock_adoption[i] = [0.0; N_LIVESTOCK];
        world.state.domesticates.domesticates_internal[i] = Default::default();
    }
}

fn ensure_domesticates_state_shape(world: &mut World, n: usize) {
    if world.state.domesticates.crop_available.len() != n {
        world.state.domesticates.crop_available.resize(n, 0);
    }
    if world.state.domesticates.livestock_available.len() != n {
        world.state.domesticates.livestock_available.resize(n, 0);
    }
    if world.state.domesticates.crop_adoption.len() != n {
        world
            .state
            .domesticates
            .crop_adoption
            .resize(n, [0.0; N_CROPS]);
    }
    if world.state.domesticates.livestock_adoption.len() != n {
        world
            .state
            .domesticates
            .livestock_adoption
            .resize(n, [0.0; N_LIVESTOCK]);
    }
    if world.state.domesticates.domesticates_internal.len() != n {
        world
            .state
            .domesticates
            .domesticates_internal
            .resize(n, Default::default());
    }
}

#[inline]
fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

#[inline]
fn gaussian_score(x: f32, mean: f32, sigma: f32) -> f32 {
    let z = (x - mean) / sigma.max(1e-6);
    (-0.5 * z * z).exp()
}

#[derive(Clone, Copy)]
struct CellContext {
    temperature: f32,
    moisture_index: f32,
    aridity: f32,
    height: f32,
    tree_cover: f32,
    ground_cover: f32,
    soil_fertility: f32,
    river_norm: f32,
    biome: Biome,
}

impl CellContext {
    fn from_world(world: &World, index: usize, river_max: f32) -> Self {
        let precipitation = world.state.climate.precipitation[index].max(0.0);
        let aridity = world.state.climate.aridity[index].max(0.01);
        let moisture_index = clamp01((precipitation / 1500.0) * (1.2 / (1.0 + aridity)));
        Self {
            temperature: world.state.climate.temperature[index],
            moisture_index,
            aridity,
            height: world.state.geology.height[index],
            tree_cover: world.state.ecology.tree_cover[index].clamp(0.0, 1.0),
            ground_cover: world.state.ecology.ground_cover[index].clamp(0.0, 1.0),
            soil_fertility: world.state.ecology.soil_fertility[index].clamp(0.0, 1.0),
            river_norm: clamp01(world.state.hydrology.river_flow[index].max(0.0) / river_max),
            biome: world.state.ecology.biome[index],
        }
    }
}

fn crop_niche_score(kind_idx: usize, context: &CellContext, params: SpeciesParams) -> f32 {
    let temperature_score =
        gaussian_score(context.temperature, params.temp_optimum, params.temp_sigma);
    let moisture_score = gaussian_score(
        context.moisture_index,
        params.moisture_optimum,
        params.moisture_sigma,
    );
    let terrain_score = clamp01(1.0 - (context.height.max(0.0) / params.height_limit.max(0.1)));
    let cover_score = (0.6 * gaussian_score(context.tree_cover, params.tree_pref, 0.25)
        + 0.4 * gaussian_score(context.ground_cover, params.ground_pref, 0.25))
    .clamp(0.0, 1.0);
    let fertility_score =
        (context.soil_fertility * 0.75 + context.ground_cover * 0.25).clamp(0.0, 1.0);
    let river_bonus = clamp01(1.0 + params.river_bonus_weight * context.river_norm);
    let mut niche = temperature_score
        * moisture_score
        * terrain_score
        * cover_score
        * fertility_score
        * river_bonus;
    if kind_idx == CropKind::Rice as usize || kind_idx == CropKind::Tuber as usize {
        niche *= 1.0 + 0.25 * context.river_norm;
    }
    clamp01(niche)
}

fn livestock_niche_score(kind_idx: usize, context: &CellContext, params: SpeciesParams) -> f32 {
    let temperature_score =
        gaussian_score(context.temperature, params.temp_optimum, params.temp_sigma);
    let moisture_score = gaussian_score(
        context.moisture_index,
        params.moisture_optimum,
        params.moisture_sigma,
    );
    let terrain_score = clamp01(1.0 - (context.height.max(0.0) / params.height_limit.max(0.1)));
    let cover_score = (0.55 * gaussian_score(context.tree_cover, params.tree_pref, 0.28)
        + 0.45 * gaussian_score(context.ground_cover, params.ground_pref, 0.24))
    .clamp(0.0, 1.0);
    let pasture_score = clamp01(context.ground_cover * 0.7 + (1.0 - context.tree_cover) * 0.3);
    let mut niche =
        temperature_score * moisture_score * terrain_score * cover_score * pasture_score;
    if kind_idx == LivestockKind::Pig as usize {
        niche *= 1.0 + 0.15 * context.river_norm;
    }
    if kind_idx == LivestockKind::Camel as usize {
        niche *= 1.0 - 0.12 * context.river_norm;
    }
    clamp01(niche)
}

fn crop_hard_exclusion(kind_idx: usize, context: &CellContext) -> bool {
    if kind_idx == CropKind::Rice as usize {
        return context.moisture_index < 0.24 || context.aridity > 4.8;
    }
    if kind_idx == CropKind::Maize as usize {
        return context.temperature < 6.0;
    }
    false
}

fn livestock_hard_exclusion(kind_idx: usize, context: &CellContext) -> bool {
    if kind_idx == LivestockKind::Camel as usize {
        return context.moisture_index > 0.70 && context.tree_cover > 0.55;
    }
    if kind_idx == LivestockKind::Horse as usize {
        return context.tree_cover > 0.72 || context.height > 1.8;
    }
    if kind_idx == LivestockKind::Pig as usize {
        return context.moisture_index < 0.20 && context.ground_cover > 0.8;
    }
    false
}

fn local_neighbor_adoption<const N: usize>(
    adoption: &[[f32; N]],
    world: &World,
    index: usize,
    kind_idx: usize,
) -> f32 {
    let neighbors = world.cell_neighbors(index);
    if neighbors.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut count = 0.0;
    for &nbr in neighbors {
        let j = nbr as usize;
        if j >= adoption.len() {
            continue;
        }
        sum += adoption[j][kind_idx];
        count += 1.0;
    }
    if count <= 0.0 {
        0.0
    } else {
        (sum / count).clamp(0.0, 1.0)
    }
}

fn terrain_conductance(context: &CellContext) -> f32 {
    let lowland = clamp01(1.0 - context.height.max(0.0) / 1.5);
    let biome_factor = match context.biome {
        Biome::TropicalForest => 0.42,
        Biome::Savanna => 0.80,
        Biome::Desert => 0.28,
        Biome::Grassland => 0.86,
        Biome::TemperateForest => 0.68,
        Biome::BorealForest => 0.56,
        Biome::Tundra => 0.50,
        Biome::Wetland => 0.62,
        Biome::Alpine => 0.24,
    };
    clamp01(0.45 * context.river_norm + 0.35 * lowland + 0.20 * biome_factor)
}

fn corridor_score(context: &CellContext) -> f32 {
    let lowland = clamp01(1.0 - context.height.max(0.0) / 1.3);
    let coastal_bonus = match context.biome {
        Biome::Wetland => 1.0,
        Biome::Savanna | Biome::Grassland => 0.7,
        Biome::TemperateForest => 0.6,
        _ => 0.45,
    };
    clamp01(0.55 * context.river_norm + 0.30 * lowland + 0.15 * coastal_bonus)
}

fn human_management_score(context: &CellContext) -> f32 {
    let env_moderation = (gaussian_score(context.temperature, 14.0, 16.0)
        * gaussian_score(context.moisture_index, 0.55, 0.30))
    .clamp(0.0, 1.0);
    let cover_balance = clamp01(1.0 - ((context.tree_cover + context.ground_cover) - 0.9).abs());
    let lowland = clamp01(1.0 - context.height.max(0.0) / 1.6);
    clamp01(0.45 * corridor_score(context) + 0.35 * env_moderation + 0.20 * cover_balance * lowland)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::{GeologyState, WorldMesh};
    use crate::PlateId;

    fn build_test_world() -> World {
        let mesh = WorldMesh {
            positions: vec![
                [0.0, 0.8, 0.6],
                [0.7, 0.2, 0.6],
                [0.4, -0.7, 0.6],
                [-0.6, -0.1, 0.8],
            ],
            nbr_offsets: vec![0, 3, 6, 9, 12],
            nbrs: vec![1, 2, 3, 0, 2, 3, 0, 1, 3, 0, 1, 2],
        };
        let geology = GeologyState {
            height: vec![0.3, 0.25, 0.35, -0.2],
            lake_depth: vec![0.0; 4],
            plate_id: vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)],
            erosion_rate: vec![0.0; 4],
            deposition_rate: vec![0.0; 4],
            volcanism: vec![0.0; 4],
            vertex_buoyancy: vec![0.0; 4],
            geology_internal: vec![crate::sim::geology_types::GeologyInternal::default(); 4],
            boundary_condition: vec![0.0; 4],
        };
        World::new(mesh, geology)
    }

    #[test]
    fn domesticates_disabled_before_civilization() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Life;
        update_domesticates(&mut world, 2);
        assert_eq!(world.state.domesticates.crop_available[0], 0);
        assert_eq!(world.state.domesticates.livestock_available[0], 0);
    }

    #[test]
    fn rice_prefers_warm_wet_lowland() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Civilization;
        world.state.climate.temperature = vec![27.0, 8.0, 16.0, 20.0];
        world.state.climate.precipitation = vec![1_400.0, 320.0, 700.0, 1_000.0];
        world.state.climate.aridity = vec![0.9, 2.8, 1.4, 1.0];
        world.state.hydrology.river_flow = vec![180.0, 20.0, 60.0, 0.0];
        world.state.ecology.tree_cover = vec![0.18, 0.35, 0.32, 0.1];
        world.state.ecology.ground_cover = vec![0.7, 0.5, 0.58, 0.2];
        world.state.ecology.soil_fertility = vec![0.75, 0.48, 0.58, 0.2];
        update_domesticates(&mut world, 2);
        let rice_bit = 1u8 << CropKind::Rice as u8;
        assert_ne!(world.state.domesticates.crop_available[0] & rice_bit, 0);
        assert_eq!(world.state.domesticates.crop_available[1] & rice_bit, 0);
    }

    #[test]
    fn unsuitable_cells_decay_instead_of_instant_zero() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Civilization;
        world.state.domesticates.crop_adoption[0][CropKind::Rice as usize] = 0.8;
        world.state.climate.temperature = vec![8.0, 8.0, 8.0, 8.0];
        world.state.climate.precipitation = vec![220.0, 220.0, 220.0, 220.0];
        world.state.climate.aridity = vec![4.0, 4.0, 4.0, 4.0];
        world.state.hydrology.river_flow = vec![0.0; 4];
        world.state.ecology.tree_cover = vec![0.85, 0.85, 0.85, 0.85];
        world.state.ecology.ground_cover = vec![0.10, 0.10, 0.10, 0.10];
        world.state.ecology.soil_fertility = vec![0.10, 0.10, 0.10, 0.10];
        update_domesticates(&mut world, 1);
        let value = world.state.domesticates.crop_adoption[0][CropKind::Rice as usize];
        assert!(value > 0.0);
        assert!(value < 0.8);
    }

    #[test]
    fn population_feedback_boosts_adoption_growth() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Civilization;
        world.state.climate.temperature = vec![18.0, 18.0, 18.0, 18.0];
        world.state.climate.precipitation = vec![700.0, 700.0, 700.0, 700.0];
        world.state.climate.aridity = vec![1.2, 1.2, 1.2, 1.2];
        world.state.hydrology.river_flow = vec![80.0, 80.0, 80.0, 0.0];
        world.state.ecology.tree_cover = vec![0.30, 0.30, 0.30, 0.30];
        world.state.ecology.ground_cover = vec![0.65, 0.65, 0.65, 0.65];
        world.state.ecology.soil_fertility = vec![0.60, 0.60, 0.60, 0.60];

        let crop_idx = CropKind::Wheat as usize;
        world.state.domesticates.domesticates_internal[0].origin_seed_crop[crop_idx] = 0.5;
        world.state.domesticates.domesticates_internal[0].origin_initialized = true;
        world.state.domesticates.domesticates_internal[0].population_pressure_bonus = 0.0;
        update_domesticates(&mut world, 1);
        let base = world.state.domesticates.crop_adoption[0][crop_idx];

        world.state.domesticates.crop_adoption[0][crop_idx] = 0.0;
        world.state.domesticates.domesticates_internal[0].population_pressure_bonus = 0.6;
        update_domesticates(&mut world, 1);
        let boosted = world.state.domesticates.crop_adoption[0][crop_idx];
        assert!(boosted > base);
    }
}

pub mod types;

#[allow(unused_imports)]
pub use crate::sim::domesticates::types::*;

use crate::sim::world::{Biome, EraKind, World, N_CROPS, N_LIVESTOCK};

const ORIGIN_COUNT_LIMIT: usize = 3;
const ORIGIN_MIN_REGION_CELLS: usize = 2;
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
    conductance_river_w: f32,
    conductance_height_w: f32,
    conductance_biome_w: f32,
}

const CROP_PARAMS: [SpeciesParams; N_CROPS] = [
    SpeciesParams {
        temp_optimum: 15.0,
        temp_sigma: 9.5,
        moisture_optimum: 0.56,
        moisture_sigma: 0.22,
        height_limit: 1.35,
        tree_pref: 0.30,
        ground_pref: 0.68,
        river_bonus_weight: 0.10,
        threshold: 0.12,
        growth_rate: 0.12,
        decay_rate: 0.035,
        conductance_river_w: 0.28,
        conductance_height_w: 0.42,
        conductance_biome_w: 0.30,
    }, // Wheat
    SpeciesParams {
        temp_optimum: 26.0,
        temp_sigma: 6.8,
        moisture_optimum: 0.84,
        moisture_sigma: 0.14,
        height_limit: 0.82,
        tree_pref: 0.25,
        ground_pref: 0.70,
        river_bonus_weight: 0.42,
        threshold: 0.11,
        growth_rate: 0.14,
        decay_rate: 0.041,
        conductance_river_w: 0.56,
        conductance_height_w: 0.26,
        conductance_biome_w: 0.18,
    }, // Rice
    SpeciesParams {
        temp_optimum: 23.0,
        temp_sigma: 8.0,
        moisture_optimum: 0.60,
        moisture_sigma: 0.22,
        height_limit: 1.32,
        tree_pref: 0.32,
        ground_pref: 0.63,
        river_bonus_weight: 0.09,
        threshold: 0.12,
        growth_rate: 0.12,
        decay_rate: 0.036,
        conductance_river_w: 0.26,
        conductance_height_w: 0.34,
        conductance_biome_w: 0.40,
    }, // Maize
    SpeciesParams {
        temp_optimum: 18.0,
        temp_sigma: 9.5,
        moisture_optimum: 0.36,
        moisture_sigma: 0.19,
        height_limit: 1.72,
        tree_pref: 0.20,
        ground_pref: 0.75,
        river_bonus_weight: 0.04,
        threshold: 0.11,
        growth_rate: 0.11,
        decay_rate: 0.031,
        conductance_river_w: 0.18,
        conductance_height_w: 0.38,
        conductance_biome_w: 0.44,
    }, // Millet
    SpeciesParams {
        temp_optimum: 11.0,
        temp_sigma: 8.8,
        moisture_optimum: 0.50,
        moisture_sigma: 0.24,
        height_limit: 2.30,
        tree_pref: 0.25,
        ground_pref: 0.60,
        river_bonus_weight: 0.03,
        threshold: 0.11,
        growth_rate: 0.11,
        decay_rate: 0.031,
        conductance_river_w: 0.14,
        conductance_height_w: 0.52,
        conductance_biome_w: 0.34,
    }, // Potato
    SpeciesParams {
        temp_optimum: 27.0,
        temp_sigma: 7.0,
        moisture_optimum: 0.42,
        moisture_sigma: 0.21,
        height_limit: 1.22,
        tree_pref: 0.35,
        ground_pref: 0.55,
        river_bonus_weight: 0.05,
        threshold: 0.11,
        growth_rate: 0.11,
        decay_rate: 0.031,
        conductance_river_w: 0.20,
        conductance_height_w: 0.34,
        conductance_biome_w: 0.46,
    }, // Cassava
    SpeciesParams {
        temp_optimum: 28.0,
        temp_sigma: 7.2,
        moisture_optimum: 0.26,
        moisture_sigma: 0.14,
        height_limit: 1.36,
        tree_pref: 0.16,
        ground_pref: 0.82,
        river_bonus_weight: 0.03,
        threshold: 0.11,
        growth_rate: 0.11,
        decay_rate: 0.030,
        conductance_river_w: 0.16,
        conductance_height_w: 0.30,
        conductance_biome_w: 0.54,
    }, // Sorghum
    SpeciesParams {
        temp_optimum: 27.0,
        temp_sigma: 7.2,
        moisture_optimum: 0.74,
        moisture_sigma: 0.16,
        height_limit: 1.16,
        tree_pref: 0.56,
        ground_pref: 0.48,
        river_bonus_weight: 0.18,
        threshold: 0.11,
        growth_rate: 0.11,
        decay_rate: 0.032,
        conductance_river_w: 0.30,
        conductance_height_w: 0.24,
        conductance_biome_w: 0.46,
    }, // Yam
];

const LIVESTOCK_PARAMS: [SpeciesParams; N_LIVESTOCK] = [
    SpeciesParams {
        temp_optimum: 19.0,
        temp_sigma: 11.5,
        moisture_optimum: 0.48,
        moisture_sigma: 0.22,
        height_limit: 1.60,
        tree_pref: 0.22,
        ground_pref: 0.78,
        river_bonus_weight: 0.04,
        threshold: 0.11,
        growth_rate: 0.10,
        decay_rate: 0.028,
        conductance_river_w: 0.20,
        conductance_height_w: 0.42,
        conductance_biome_w: 0.38,
    }, // Cattle
    SpeciesParams {
        temp_optimum: 13.0,
        temp_sigma: 11.0,
        moisture_optimum: 0.36,
        moisture_sigma: 0.18,
        height_limit: 1.85,
        tree_pref: 0.14,
        ground_pref: 0.86,
        river_bonus_weight: 0.03,
        threshold: 0.10,
        growth_rate: 0.10,
        decay_rate: 0.026,
        conductance_river_w: 0.22,
        conductance_height_w: 0.30,
        conductance_biome_w: 0.48,
    }, // Horse
    SpeciesParams {
        temp_optimum: 12.0,
        temp_sigma: 11.6,
        moisture_optimum: 0.28,
        moisture_sigma: 0.18,
        height_limit: 2.05,
        tree_pref: 0.12,
        ground_pref: 0.83,
        river_bonus_weight: 0.02,
        threshold: 0.10,
        growth_rate: 0.10,
        decay_rate: 0.025,
        conductance_river_w: 0.16,
        conductance_height_w: 0.36,
        conductance_biome_w: 0.48,
    }, // Sheep
    SpeciesParams {
        temp_optimum: 22.0,
        temp_sigma: 9.6,
        moisture_optimum: 0.67,
        moisture_sigma: 0.18,
        height_limit: 1.34,
        tree_pref: 0.56,
        ground_pref: 0.52,
        river_bonus_weight: 0.16,
        threshold: 0.11,
        growth_rate: 0.11,
        decay_rate: 0.031,
        conductance_river_w: 0.28,
        conductance_height_w: 0.32,
        conductance_biome_w: 0.40,
    }, // Pig
    SpeciesParams {
        temp_optimum: 30.0,
        temp_sigma: 7.0,
        moisture_optimum: 0.18,
        moisture_sigma: 0.11,
        height_limit: 1.56,
        tree_pref: 0.08,
        ground_pref: 0.50,
        river_bonus_weight: -0.10,
        threshold: 0.10,
        growth_rate: 0.11,
        decay_rate: 0.028,
        conductance_river_w: 0.12,
        conductance_height_w: 0.38,
        conductance_biome_w: 0.50,
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
            let conductance = terrain_conductance_crop(kind_idx, &context);
            let spread = (local_neighbor * conductance + routed).clamp(0.0, 1.0);
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
            let next = converge_toward(current, target, params.growth_rate, dt)
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
            let conductance = terrain_conductance_livestock(kind_idx, &context);
            let spread = (local_neighbor * conductance + routed).clamp(0.0, 1.0);
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
            let next = converge_toward(current, target, params.growth_rate, dt)
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
        internal.diffusion_memory = (internal.diffusion_memory * 0.85
            + terrain_conductance_generic(&context, 0.35, 0.35, 0.30) * 0.15)
            .clamp(0.0, 1.0);
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
    is_available: impl Copy + Fn(usize) -> bool,
) -> Vec<usize> {
    let mut potential = vec![0.0_f32; world.cell_count()];
    let mut top = 0.0_f32;

    for (i, slot) in potential.iter_mut().enumerate().take(world.cell_count()) {
        if world.state.geology.height[i] <= world.control.sea_level_offset || !is_available(i) {
            continue;
        }
        let context = CellContext::from_world(world, i, river_max);
        let corridor = corridor_score(&context);
        let human = human_management_score(&context);
        let value = niche_at(i) * corridor * human;
        *slot = value;
        top = top.max(value);
    }

    if top <= 0.0 {
        return vec![candidates_fallback(world, is_available)];
    }

    let cutoff = top * 0.72;
    let mut candidate = vec![false; world.cell_count()];
    for (i, flag) in candidate.iter_mut().enumerate().take(world.cell_count()) {
        if potential[i] >= cutoff {
            *flag = true;
        }
    }

    let components = collect_origin_components(world, &candidate, &potential);
    if components.is_empty() {
        return vec![candidates_fallback(world, is_available)];
    }

    let mut ranked = components
        .into_iter()
        .filter(|component| {
            component.len() >= ORIGIN_MIN_REGION_CELLS
                || component.iter().any(|&index| potential[index] >= top * 0.9)
        })
        .map(|component| {
            let best = component
                .iter()
                .copied()
                .max_by(|&lhs, &rhs| {
                    potential[lhs]
                        .partial_cmp(&potential[rhs])
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(lhs.cmp(&rhs).reverse())
                })
                .unwrap_or(0);
            let score = potential[best];
            (best, score)
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|lhs, rhs| {
        rhs.1
            .partial_cmp(&lhs.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(lhs.0.cmp(&rhs.0))
    });

    let mut chosen = ranked
        .into_iter()
        .take(ORIGIN_COUNT_LIMIT)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();

    if chosen.is_empty() {
        chosen.push(candidates_fallback(world, is_available));
    }

    chosen
}

fn collect_origin_components(
    world: &World,
    candidate: &[bool],
    potential: &[f32],
) -> Vec<Vec<usize>> {
    let mut visited = vec![false; candidate.len()];
    let mut components = Vec::new();

    for start in 0..candidate.len() {
        if !candidate[start] || visited[start] || potential[start] <= 0.0 {
            continue;
        }
        let mut stack = vec![start];
        let mut comp = Vec::new();
        visited[start] = true;

        while let Some(node) = stack.pop() {
            comp.push(node);
            for &nbr in world.cell_neighbors(node) {
                let next = nbr as usize;
                if next >= candidate.len()
                    || visited[next]
                    || !candidate[next]
                    || potential[next] <= 0.0
                {
                    continue;
                }
                visited[next] = true;
                stack.push(next);
            }
        }

        if !comp.is_empty() {
            components.push(comp);
        }
    }

    components
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
    matches!(
        epoch,
        EraKind::Life | EraKind::Civilization | EraKind::History
    )
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
fn converge_toward(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    current + (target - current) * rate * dt
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
    let lowland_score = clamp01(1.0 - context.height.max(0.0) / params.height_limit.max(0.1));
    let highland_score = clamp01(context.height.max(0.0) / params.height_limit.max(0.1));
    let terrain_score = if kind_idx == CropKind::Potato as usize {
        (0.45 * highland_score + 0.55 * clamp01(1.0 - (context.height - 1.2).abs() / 1.2))
            .clamp(0.0, 1.0)
    } else {
        lowland_score
    };
    let cover_score = (0.6 * gaussian_score(context.tree_cover, params.tree_pref, 0.24)
        + 0.4 * gaussian_score(context.ground_cover, params.ground_pref, 0.24))
    .clamp(0.0, 1.0);
    let fertility_score = if kind_idx == CropKind::Millet as usize
        || kind_idx == CropKind::Cassava as usize
        || kind_idx == CropKind::Sorghum as usize
    {
        clamp01(context.soil_fertility * 0.45 + context.ground_cover * 0.55)
    } else {
        clamp01(context.soil_fertility * 0.75 + context.ground_cover * 0.25)
    };

    let river_bonus = clamp01(1.0 + params.river_bonus_weight * context.river_norm);
    let mut niche = temperature_score
        * moisture_score
        * terrain_score
        * cover_score
        * fertility_score
        * river_bonus;

    match kind_idx {
        x if x == CropKind::Rice as usize => {
            niche *= 1.0 + 0.28 * context.river_norm;
            niche *= clamp01(1.0 - context.aridity * 0.12);
        }
        x if x == CropKind::Yam as usize => {
            let forest_edge = clamp01(1.0 - (context.tree_cover - 0.62).abs() / 0.52);
            niche *= 0.7 + 0.3 * forest_edge;
        }
        x if x == CropKind::Sorghum as usize => {
            let dry_gain = gaussian_score(context.moisture_index, 0.22, 0.13);
            niche *= 0.72 + 0.28 * dry_gain;
        }
        x if x == CropKind::Cassava as usize => {
            let warm = gaussian_score(context.temperature, 27.0, 8.0);
            niche *= 0.72 + 0.28 * warm;
        }
        _ => {}
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
    let pasture_score = if kind_idx == LivestockKind::Pig as usize {
        clamp01(
            (context.river_norm * 0.35)
                + context.soil_fertility * 0.25
                + context.tree_cover * 0.25
                + context.ground_cover * 0.15,
        )
    } else if kind_idx == LivestockKind::Camel as usize {
        clamp01(
            (1.0 - context.tree_cover) * 0.45
                + context.ground_cover * 0.20
                + (1.0 - context.moisture_index) * 0.35,
        )
    } else {
        clamp01(context.ground_cover * 0.7 + (1.0 - context.tree_cover) * 0.3)
    };

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
    if kind_idx == CropKind::Potato as usize {
        return context.temperature > 28.0 && context.height < 0.8;
    }
    if kind_idx == CropKind::Cassava as usize
        || kind_idx == CropKind::Sorghum as usize
        || kind_idx == CropKind::Yam as usize
    {
        return context.temperature < 8.0;
    }
    if kind_idx == CropKind::Yam as usize {
        return context.moisture_index < 0.30;
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

fn terrain_conductance_crop(kind_idx: usize, context: &CellContext) -> f32 {
    let params = CROP_PARAMS[kind_idx];
    let biome = crop_biome_conductance(kind_idx, context.biome);
    terrain_conductance_generic(
        context,
        params.conductance_river_w,
        params.conductance_height_w,
        params.conductance_biome_w * biome,
    )
}

fn terrain_conductance_livestock(kind_idx: usize, context: &CellContext) -> f32 {
    let params = LIVESTOCK_PARAMS[kind_idx];
    let biome = livestock_biome_conductance(kind_idx, context.biome);
    terrain_conductance_generic(
        context,
        params.conductance_river_w,
        params.conductance_height_w,
        params.conductance_biome_w * biome,
    )
}

fn terrain_conductance_generic(
    context: &CellContext,
    river_w: f32,
    height_w: f32,
    biome_weighted: f32,
) -> f32 {
    let lowland = clamp01(1.0 - context.height.max(0.0) / 1.6);
    let biome_base = biome_common_passability(context.biome);
    clamp01(river_w * context.river_norm + height_w * lowland + biome_weighted * biome_base)
}

fn biome_common_passability(biome: Biome) -> f32 {
    match biome {
        Biome::TropicalForest => 0.42,
        Biome::Savanna => 0.80,
        Biome::Desert => 0.28,
        Biome::Grassland => 0.86,
        Biome::TemperateForest => 0.68,
        Biome::BorealForest => 0.56,
        Biome::Tundra => 0.50,
        Biome::Wetland => 0.62,
        Biome::Alpine => 0.24,
    }
}

fn crop_biome_conductance(kind_idx: usize, biome: Biome) -> f32 {
    match kind_idx {
        x if x == CropKind::Rice as usize => match biome {
            Biome::Wetland => 1.0,
            Biome::Savanna | Biome::TemperateForest => 0.75,
            Biome::Grassland => 0.62,
            Biome::TropicalForest => 0.58,
            Biome::Alpine | Biome::Desert => 0.25,
            _ => 0.45,
        },
        x if x == CropKind::Millet as usize || x == CropKind::Sorghum as usize => match biome {
            Biome::Savanna | Biome::Grassland => 1.0,
            Biome::Desert => 0.72,
            Biome::Wetland | Biome::TropicalForest => 0.28,
            _ => 0.54,
        },
        x if x == CropKind::Yam as usize => match biome {
            Biome::TropicalForest | Biome::Wetland => 1.0,
            Biome::Savanna | Biome::TemperateForest => 0.70,
            Biome::Desert | Biome::Alpine => 0.20,
            _ => 0.45,
        },
        _ => 0.70,
    }
}

fn livestock_biome_conductance(kind_idx: usize, biome: Biome) -> f32 {
    match kind_idx {
        x if x == LivestockKind::Horse as usize => match biome {
            Biome::Grassland | Biome::Savanna => 1.0,
            Biome::Desert => 0.75,
            Biome::TropicalForest | Biome::TemperateForest | Biome::BorealForest => 0.24,
            Biome::Alpine => 0.20,
            _ => 0.45,
        },
        x if x == LivestockKind::Camel as usize => match biome {
            Biome::Desert => 1.0,
            Biome::Savanna | Biome::Grassland => 0.72,
            Biome::TropicalForest | Biome::Wetland => 0.12,
            _ => 0.40,
        },
        x if x == LivestockKind::Pig as usize => match biome {
            Biome::TropicalForest | Biome::TemperateForest | Biome::Wetland => 1.0,
            Biome::Savanna => 0.64,
            Biome::Desert => 0.18,
            _ => 0.44,
        },
        _ => 0.68,
    }
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
    fn domesticates_enabled_in_life() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Life;
        world.state.climate.temperature = vec![16.0; 4];
        world.state.climate.precipitation = vec![800.0; 4];
        world.state.climate.aridity = vec![1.2; 4];
        world.state.hydrology.river_flow = vec![20.0; 4];
        world.state.ecology.tree_cover = vec![0.3; 4];
        world.state.ecology.ground_cover = vec![0.6; 4];
        world.state.ecology.soil_fertility = vec![0.5; 4];

        update_domesticates(&mut world, 2);

        assert!(world.state.domesticates.crop_available[0] != 0);
    }

    #[test]
    fn rice_prefers_warm_wet_lowland() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Civilization;
        world.state.climate.temperature = vec![27.0, 8.0, 16.0, 20.0];
        world.state.climate.precipitation = vec![1_400.0, 320.0, 700.0, 1_000.0];
        world.state.climate.aridity = vec![0.9, 2.8, 1.4, 1.0];
        world.state.geology.height = vec![0.15, 1.4, 0.8, -0.2];
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
    fn semi_arid_cool_favors_millet_and_sorghum_over_rice() {
        let context = CellContext {
            temperature: 17.0,
            moisture_index: 0.30,
            aridity: 2.4,
            height: 0.8,
            tree_cover: 0.15,
            ground_cover: 0.75,
            soil_fertility: 0.4,
            river_norm: 0.2,
            biome: Biome::Savanna,
        };
        let rice = crop_niche_score(
            CropKind::Rice as usize,
            &context,
            CROP_PARAMS[CropKind::Rice as usize],
        );
        let millet = crop_niche_score(
            CropKind::Millet as usize,
            &context,
            CROP_PARAMS[CropKind::Millet as usize],
        );
        let sorghum = crop_niche_score(
            CropKind::Sorghum as usize,
            &context,
            CROP_PARAMS[CropKind::Sorghum as usize],
        );
        assert!(millet > rice);
        assert!(sorghum > rice);
    }

    #[test]
    fn cool_highland_potato_advantage_over_warm_crops() {
        let context = CellContext {
            temperature: 11.0,
            moisture_index: 0.46,
            aridity: 1.5,
            height: 1.8,
            tree_cover: 0.2,
            ground_cover: 0.6,
            soil_fertility: 0.4,
            river_norm: 0.15,
            biome: Biome::Alpine,
        };
        let potato = crop_niche_score(
            CropKind::Potato as usize,
            &context,
            CROP_PARAMS[CropKind::Potato as usize],
        );
        let maize = crop_niche_score(
            CropKind::Maize as usize,
            &context,
            CROP_PARAMS[CropKind::Maize as usize],
        );
        let cassava = crop_niche_score(
            CropKind::Cassava as usize,
            &context,
            CROP_PARAMS[CropKind::Cassava as usize],
        );
        let yam = crop_niche_score(
            CropKind::Yam as usize,
            &context,
            CROP_PARAMS[CropKind::Yam as usize],
        );
        assert!(potato > maize);
        assert!(potato > cassava);
        assert!(potato > yam);
    }

    #[test]
    fn hot_dry_open_sorghum_beats_cassava_and_maize() {
        let context = CellContext {
            temperature: 31.0,
            moisture_index: 0.20,
            aridity: 3.4,
            height: 0.7,
            tree_cover: 0.06,
            ground_cover: 0.82,
            soil_fertility: 0.28,
            river_norm: 0.05,
            biome: Biome::Savanna,
        };
        let sorghum = crop_niche_score(
            CropKind::Sorghum as usize,
            &context,
            CROP_PARAMS[CropKind::Sorghum as usize],
        );
        let cassava = crop_niche_score(
            CropKind::Cassava as usize,
            &context,
            CROP_PARAMS[CropKind::Cassava as usize],
        );
        let maize = crop_niche_score(
            CropKind::Maize as usize,
            &context,
            CROP_PARAMS[CropKind::Maize as usize],
        );
        assert!(sorghum > cassava);
        assert!(sorghum > maize * 1.2);
    }

    #[test]
    fn tropical_wet_forest_edge_yam_beats_sorghum() {
        let context = CellContext {
            temperature: 28.0,
            moisture_index: 0.78,
            aridity: 0.9,
            height: 0.5,
            tree_cover: 0.62,
            ground_cover: 0.45,
            soil_fertility: 0.62,
            river_norm: 0.45,
            biome: Biome::TropicalForest,
        };
        let yam = crop_niche_score(
            CropKind::Yam as usize,
            &context,
            CROP_PARAMS[CropKind::Yam as usize],
        );
        let sorghum = crop_niche_score(
            CropKind::Sorghum as usize,
            &context,
            CROP_PARAMS[CropKind::Sorghum as usize],
        );
        assert!(yam > sorghum);
    }

    #[test]
    fn dry_open_land_camel_and_sheep_high_pig_low() {
        let context = CellContext {
            temperature: 30.0,
            moisture_index: 0.18,
            aridity: 3.2,
            height: 0.7,
            tree_cover: 0.08,
            ground_cover: 0.78,
            soil_fertility: 0.3,
            river_norm: 0.06,
            biome: Biome::Desert,
        };
        let camel = livestock_niche_score(
            LivestockKind::Camel as usize,
            &context,
            LIVESTOCK_PARAMS[LivestockKind::Camel as usize],
        );
        let sheep = livestock_niche_score(
            LivestockKind::Sheep as usize,
            &context,
            LIVESTOCK_PARAMS[LivestockKind::Sheep as usize],
        );
        let pig = livestock_niche_score(
            LivestockKind::Pig as usize,
            &context,
            LIVESTOCK_PARAMS[LivestockKind::Pig as usize],
        );
        assert!(camel > pig);
        assert!(sheep > pig);
    }

    #[test]
    fn origin_seed_initializes_every_category() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Life;
        world.state.climate.temperature = vec![25.0, 19.0, 13.0, 8.0];
        world.state.climate.precipitation = vec![1200.0, 900.0, 600.0, 500.0];
        world.state.climate.aridity = vec![0.9, 1.2, 1.6, 1.8];
        world.state.hydrology.river_flow = vec![120.0, 75.0, 30.0, 0.0];
        world.state.ecology.tree_cover = vec![0.35, 0.25, 0.18, 0.08];
        world.state.ecology.ground_cover = vec![0.62, 0.68, 0.74, 0.62];
        world.state.ecology.soil_fertility = vec![0.7, 0.62, 0.55, 0.42];

        update_domesticates(&mut world, 2);

        for kind_idx in 0..N_CROPS {
            assert!(
                world
                    .state
                    .domesticates
                    .domesticates_internal
                    .iter()
                    .any(|internal| internal.origin_seed_crop[kind_idx] > 0.0),
                "crop kind {kind_idx} has no origin seed"
            );
        }
        for kind_idx in 0..N_LIVESTOCK {
            assert!(
                world
                    .state
                    .domesticates
                    .domesticates_internal
                    .iter()
                    .any(|internal| internal.origin_seed_livestock[kind_idx] > 0.0),
                "livestock kind {kind_idx} has no origin seed"
            );
        }
    }

    #[test]
    fn suitable_but_isolated_cell_does_not_jump_to_one_tick_full_adoption() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Civilization;
        world.state.climate.temperature = vec![17.0, 17.0, 17.0, 17.0];
        world.state.climate.precipitation = vec![750.0, 750.0, 750.0, 750.0];
        world.state.climate.aridity = vec![1.2, 1.2, 1.2, 1.2];
        world.state.hydrology.river_flow = vec![30.0, 30.0, 30.0, 0.0];
        world.state.ecology.tree_cover = vec![0.28, 0.28, 0.28, 0.28];
        world.state.ecology.ground_cover = vec![0.70, 0.70, 0.70, 0.70];
        world.state.ecology.soil_fertility = vec![0.62, 0.62, 0.62, 0.62];

        for internal in &mut world.state.domesticates.domesticates_internal {
            internal.origin_initialized = true;
            internal.origin_seed_crop = [0.0; N_CROPS];
            internal.routed_feedback_crop = [0.0; N_CROPS];
        }

        let kind = CropKind::Wheat as usize;
        update_domesticates(&mut world, 1);

        assert!(world.state.domesticates.crop_adoption[0][kind] < 0.2);
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

    #[test]
    fn rice_conductance_is_higher_on_river_lowland_than_mountain() {
        let river_lowland = CellContext {
            temperature: 25.0,
            moisture_index: 0.75,
            aridity: 1.0,
            height: 0.3,
            tree_cover: 0.3,
            ground_cover: 0.7,
            soil_fertility: 0.7,
            river_norm: 0.9,
            biome: Biome::Wetland,
        };
        let mountain = CellContext {
            temperature: 16.0,
            moisture_index: 0.6,
            aridity: 1.4,
            height: 1.9,
            tree_cover: 0.4,
            ground_cover: 0.5,
            soil_fertility: 0.5,
            river_norm: 0.1,
            biome: Biome::Alpine,
        };
        let rice = terrain_conductance_crop(CropKind::Rice as usize, &river_lowland);
        let rice_mountain = terrain_conductance_crop(CropKind::Rice as usize, &mountain);
        let millet_mountain = terrain_conductance_crop(CropKind::Millet as usize, &mountain);

        assert!(rice > rice_mountain);
        assert!(rice_mountain < millet_mountain);
    }

    #[test]
    fn horse_conductance_drops_in_forest_but_high_on_steppe_corridor() {
        let steppe = CellContext {
            temperature: 14.0,
            moisture_index: 0.35,
            aridity: 1.8,
            height: 0.5,
            tree_cover: 0.1,
            ground_cover: 0.8,
            soil_fertility: 0.4,
            river_norm: 0.4,
            biome: Biome::Grassland,
        };
        let forest = CellContext {
            temperature: 14.0,
            moisture_index: 0.55,
            aridity: 1.0,
            height: 0.5,
            tree_cover: 0.75,
            ground_cover: 0.4,
            soil_fertility: 0.5,
            river_norm: 0.4,
            biome: Biome::TemperateForest,
        };
        let horse_steppe = terrain_conductance_livestock(LivestockKind::Horse as usize, &steppe);
        let horse_forest = terrain_conductance_livestock(LivestockKind::Horse as usize, &forest);
        assert!(horse_steppe > horse_forest);
    }
}

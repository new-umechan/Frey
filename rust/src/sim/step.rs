use std::cmp::Ordering;

use crate::domains;

use super::world::{
    era_for_tick, CellLayer, CivilizationLayer, ClimateLayer, EcologyLayer, EraKind, LayerKind,
    SubsystemBudgets, World,
};

pub fn step_world(world: &mut World) {
    world.budgets = compute_budgets(world.era);
    ensure_required_layers(world);
    run_terrain_step(world, world.budgets.terrain);
    run_river_step(world, world.budgets.river);
    run_climate_step(world, world.budgets.climate);
    run_ecology_step(world, world.budgets.ecology);
    run_civilization_step(world, world.budgets.civilization);
    update_era_transition(world);
    world.tick = world.tick.saturating_add(1);
}

fn compute_budgets(era: EraKind) -> SubsystemBudgets {
    match era {
        EraKind::Crust => SubsystemBudgets {
            terrain: 4,
            river: 1,
            climate: 0,
            ecology: 0,
            civilization: 0,
        },
        EraKind::Environment => SubsystemBudgets {
            terrain: 2,
            river: 4,
            climate: 3,
            ecology: 1,
            civilization: 0,
        },
        EraKind::Life => SubsystemBudgets {
            terrain: 1,
            river: 2,
            climate: 3,
            ecology: 4,
            civilization: 1,
        },
        EraKind::Civilization => SubsystemBudgets {
            terrain: 1,
            river: 1,
            climate: 2,
            ecology: 2,
            civilization: 4,
        },
        EraKind::History => SubsystemBudgets {
            terrain: 0,
            river: 1,
            climate: 1,
            ecology: 1,
            civilization: 4,
        },
    }
}

fn ensure_required_layers(world: &mut World) {
    let cell_count = world.core.height.len();
    if world.era == EraKind::Environment
        || world.era == EraKind::Life
        || world.era == EraKind::Civilization
        || world.era == EraKind::History
    {
        let layer = world.layers.entry(LayerKind::Climate).or_insert_with(|| {
            CellLayer::Climate(ClimateLayer {
                temp: vec![0.5; cell_count],
                rain: vec![0.5; cell_count],
            })
        });
        if let CellLayer::Climate(climate) = layer {
            resize_with_fill(&mut climate.temp, cell_count, 0.5);
            resize_with_fill(&mut climate.rain, cell_count, 0.5);
        }
    }
    if world.era == EraKind::Life || world.era == EraKind::Civilization || world.era == EraKind::History {
        let layer = world.layers.entry(LayerKind::Ecology).or_insert_with(|| {
            CellLayer::Ecology(EcologyLayer {
                habitability: vec![0.0; cell_count],
                productivity: vec![0.0; cell_count],
            })
        });
        if let CellLayer::Ecology(ecology) = layer {
            resize_with_fill(&mut ecology.habitability, cell_count, 0.0);
            resize_with_fill(&mut ecology.productivity, cell_count, 0.0);
        }
    }
    if world.era == EraKind::Civilization || world.era == EraKind::History {
        let layer = world.layers.entry(LayerKind::Civilization).or_insert_with(|| {
            CellLayer::Civilization(CivilizationLayer {
                population: vec![0.0; cell_count],
                state_id: vec![0; cell_count],
            })
        });
        if let CellLayer::Civilization(civilization) = layer {
            resize_with_fill(&mut civilization.population, cell_count, 0.0);
            resize_with_fill(&mut civilization.state_id, cell_count, 0_u32);
        }
    }
}

fn run_terrain_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    if world.mesh.nbr_offsets.len() != world.core.height.len() + 1 {
        return;
    }

    let mut next = world.core.height.clone();
    let boundary_mix = 0.4_f32;
    for _ in 0..budget {
        for i in 0..world.core.height.len() {
            let start = world.mesh.nbr_offsets[i] as usize;
            let end = world.mesh.nbr_offsets[i + 1] as usize;
            if end <= start {
                continue;
            }

            let mut nbr_sum = 0.0_f32;
            let mut nbr_count = 0usize;
            let mut same_plate_count = 0usize;
            for &n_u32 in &world.mesh.nbrs[start..end] {
                let n = n_u32 as usize;
                if n >= world.core.height.len() {
                    continue;
                }
                nbr_sum += world.core.height[n];
                nbr_count += 1;
                if world.core.plate_id.get(i) == world.core.plate_id.get(n) {
                    same_plate_count += 1;
                }
            }
            if nbr_count == 0 {
                continue;
            }

            let avg_nbr = nbr_sum / nbr_count as f32;
            let plate_similarity = same_plate_count as f32 / nbr_count as f32;
            let relax = 0.05 + 0.10 * (plate_similarity * (1.0 - boundary_mix) + boundary_mix);
            next[i] = (world.core.height[i] + (avg_nbr - world.core.height[i]) * relax).clamp(-1.0, 1.0);
        }
        world.core.height.clone_from(&next);
    }

    if let Some(state) = world.river_erosion_state.as_mut() {
        if state.height.len() == world.core.height.len() {
            state.height.clone_from(&world.core.height);
        }
    }
}

fn run_river_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    if let Some(state) = world.river_erosion_state.as_mut() {
        if state.height.len() == world.core.height.len()
            && state.river_flux.len() == world.core.river_flux.len()
            && state.river_next.len() == world.core.river_next.len()
        {
            let cell_count = world.core.height.len() as u32;
            let budget_cells = (cell_count.saturating_mul(budget).max(1) / 12).max(32);
            domains::step_erosion_automaton(state, budget_cells);
            world.core.height.clone_from(&state.height);
            world.core.river_flux.clone_from(&state.river_flux);
            world.core.river_next.clone_from(&state.river_next);
            return;
        }
    }

    run_river_fallback(world);
}

fn run_river_fallback(world: &mut World) {
    let cell_count = world.core.height.len();
    if cell_count == 0 || world.mesh.nbr_offsets.len() != cell_count + 1 {
        return;
    }

    let mut river_next = vec![-1_i32; cell_count];
    for (i, river_next_i) in river_next.iter_mut().enumerate() {
        let start = world.mesh.nbr_offsets[i] as usize;
        let end = world.mesh.nbr_offsets[i + 1] as usize;
        let mut best_downstream = None::<(usize, f32)>;
        for &n_u32 in &world.mesh.nbrs[start..end] {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            let drop = world.core.height[i] - world.core.height[n];
            if drop <= 1e-5 {
                continue;
            }
            match best_downstream {
                Some((_, best_drop)) if drop <= best_drop => {}
                _ => best_downstream = Some((n, drop)),
            }
        }
        if let Some((n, _)) = best_downstream {
            *river_next_i = n as i32;
        }
    }

    let rain = build_rain_for_fallback(world);
    let mut flux = rain;
    let mut order = (0..cell_count).collect::<Vec<_>>();
    order.sort_by(|&a, &b| {
        world.core.height[b]
            .partial_cmp(&world.core.height[a])
            .unwrap_or(Ordering::Equal)
    });
    for i in order {
        let next = river_next[i];
        if next < 0 {
            continue;
        }
        let n = next as usize;
        if n < cell_count {
            flux[n] += flux[i];
        }
    }

    world.core.river_next = river_next;
    world.core.river_flux = flux;
}

fn build_rain_for_fallback(world: &World) -> Vec<f32> {
    if let Some(CellLayer::Climate(climate)) = world.layers.get(&LayerKind::Climate) {
        if climate.rain.len() == world.core.height.len() {
            return climate.rain.clone();
        }
    }
    world
        .core
        .height
        .iter()
        .map(|&h| if h > 0.0 { 0.65 } else { 0.35 })
        .collect()
}

fn run_climate_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let Some(CellLayer::Climate(climate)) = world.layers.get_mut(&LayerKind::Climate) else {
        return;
    };
    let alpha = blend_alpha(budget, 0.10);
    let max_flux = world
        .core
        .river_flux
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    for i in 0..world.core.height.len() {
        let pos = world.mesh.positions.get(i).copied().unwrap_or([0.0, 0.0, 1.0]);
        let latitude = pos[1].abs().clamp(0.0, 1.0);
        let altitude = world.core.height[i].max(0.0);
        let base_temp = 0.15 + (1.0 - latitude) * 0.85;
        let target_temp = (base_temp - altitude * 0.35).clamp(0.0, 1.0);

        let river_norm = (world.core.river_flux[i] / max_flux).clamp(0.0, 1.0);
        let orographic = (altitude * 0.50).clamp(0.0, 0.35);
        let target_rain = (0.20 + river_norm * 0.45 + (1.0 - latitude) * 0.25 + orographic).clamp(0.0, 1.0);

        climate.temp[i] = lerp(climate.temp[i], target_temp, alpha);
        climate.rain[i] = lerp(climate.rain[i], target_rain, alpha);
    }
}

fn run_ecology_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let (climate_temp, climate_rain) = match world.layers.get(&LayerKind::Climate) {
        Some(CellLayer::Climate(climate))
            if climate.temp.len() == world.core.height.len()
                && climate.rain.len() == world.core.height.len() =>
        {
            (climate.temp.clone(), climate.rain.clone())
        }
        _ => return,
    };
    let Some(CellLayer::Ecology(ecology)) = world.layers.get_mut(&LayerKind::Ecology) else {
        return;
    };
    if ecology.habitability.len() != world.core.height.len()
        || ecology.productivity.len() != world.core.height.len()
    {
        return;
    }
    let alpha = blend_alpha(budget, 0.16);
    let max_flux = world
        .core
        .river_flux
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    for i in 0..world.core.height.len() {
        let temp = climate_temp[i];
        let rain = climate_rain[i];
        let land = if world.core.height[i] > 0.0 { 1.0 } else { 0.15 };
        let river_bonus = (world.core.river_flux[i] / max_flux).clamp(0.0, 1.0) * 0.20;
        let temp_suit = 1.0 - ((temp - 0.55).abs() / 0.55).clamp(0.0, 1.0);
        let rain_suit = 1.0 - ((rain - 0.60).abs() / 0.60).clamp(0.0, 1.0);
        let target_habitability = ((temp_suit * 0.55 + rain_suit * 0.45) * land + river_bonus).clamp(0.0, 1.0);
        let target_productivity = (target_habitability * (0.45 + rain * 0.40 + river_bonus)).clamp(0.0, 1.0);

        ecology.habitability[i] = lerp(ecology.habitability[i], target_habitability, alpha);
        ecology.productivity[i] = lerp(ecology.productivity[i], target_productivity, alpha);
    }
}

fn run_civilization_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let (eco_habitability, eco_productivity) = match world.layers.get(&LayerKind::Ecology) {
        Some(CellLayer::Ecology(ecology))
            if ecology.habitability.len() == world.core.height.len()
                && ecology.productivity.len() == world.core.height.len() =>
        {
            (ecology.habitability.clone(), ecology.productivity.clone())
        }
        _ => return,
    };
    let Some(CellLayer::Civilization(civilization)) = world.layers.get_mut(&LayerKind::Civilization) else {
        return;
    };
    if civilization.population.len() != world.core.height.len()
        || civilization.state_id.len() != world.core.height.len()
    {
        return;
    }
    let alpha = blend_alpha(budget, 0.12);
    let max_flux = world
        .core
        .river_flux
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    for i in 0..world.core.height.len() {
        if world.core.height[i] <= 0.0 {
            civilization.population[i] *= 0.98;
            civilization.state_id[i] = 0;
            continue;
        }

        let river_support = (world.core.river_flux[i] / max_flux).clamp(0.0, 1.0);
        let carrying = 1.0 + eco_productivity[i] * 130.0 + eco_habitability[i] * 70.0 + river_support * 40.0;
        let current = civilization.population[i].max(0.0);
        let seeded = if current < 1.0 && eco_habitability[i] > 0.55 {
            1.0
        } else {
            current
        };
        let growth = 0.18 * eco_habitability[i].max(0.05) * seeded * (1.0 - seeded / carrying).max(-0.5);
        let next_population = (seeded + growth * alpha * 4.0).max(0.0);
        civilization.population[i] = next_population;
        civilization.state_id[i] = if next_population >= 10.0 { (i + 1) as u32 } else { 0 };
    }
}

fn update_era_transition(world: &mut World) {
    let next_tick = world.tick.saturating_add(1);
    world.era = era_for_tick(next_tick);
}

fn resize_with_fill<T: Clone>(values: &mut Vec<T>, size: usize, fill: T) {
    if values.len() != size {
        values.resize(size, fill);
    }
}

fn blend_alpha(budget: u32, base: f32) -> f32 {
    let b = budget.max(1) as f32;
    (1.0 - (1.0 - base).powf(b)).clamp(0.0, 1.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use crate::{ErosionAutomatonState, TerrainParams};

    use super::*;
    use crate::sim::world::{CoreCells, World, WorldMesh};

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
        let core = CoreCells {
            height: vec![0.45, 0.15, -0.25, 0.05],
            plate_id: vec![0, 0, 1, 1],
            river_flux: vec![0.1, 0.2, 0.3, 0.1],
            river_next: vec![1, 2, -1, 2],
        };
        World::new(mesh, core)
    }

    #[test]
    fn step_world_advances_tick_and_sets_budgets() {
        let mut world = build_test_world();
        step_world(&mut world);
        assert_eq!(world.tick, 1);
        assert_eq!(world.budgets.terrain, 4);
        assert_eq!(world.budgets.river, 1);
    }

    #[test]
    fn step_world_generates_required_layers_by_era() {
        let mut world = build_test_world();
        world.era = EraKind::Life;
        step_world(&mut world);

        assert!(matches!(
            world.layers.get(&LayerKind::Climate),
            Some(CellLayer::Climate(_))
        ));
        assert!(matches!(
            world.layers.get(&LayerKind::Ecology),
            Some(CellLayer::Ecology(_))
        ));
        assert!(world.layers.get(&LayerKind::Civilization).is_none());
    }

    #[test]
    fn river_fallback_updates_flux_and_next() {
        let mut world = build_test_world();
        world.river_erosion_state = None;
        step_world(&mut world);

        assert!(world.core.river_flux.iter().all(|v| v.is_finite() && *v >= 0.0));
        assert!(world.core.river_next.iter().any(|&n| n >= 0));
    }

    #[test]
    fn river_erosion_state_is_used_when_present() {
        let mut world = build_test_world();
        let erosion_state = ErosionAutomatonState {
            positions: world.mesh.positions.clone(),
            nbr_offsets: world.mesh.nbr_offsets.clone(),
            nbrs: world.mesh.nbrs.clone(),
            height: world.core.height.clone(),
            water: vec![0.0; world.core.height.len()],
            sediment: vec![0.0; world.core.height.len()],
            armor: vec![0.0; world.core.height.len()],
            rain: vec![0.5; world.core.height.len()],
            river_flux: world.core.river_flux.clone(),
            river_next: world.core.river_next.clone(),
            active_queue: (0..world.core.height.len() as u32).collect(),
            active_head: 0,
            in_queue: vec![1; world.core.height.len()],
            rain_cursor: 0,
            tick: 0,
            recent_changed: Vec::new(),
            params: TerrainParams::default(),
        };
        world.attach_river_erosion_state(erosion_state).unwrap();

        step_world(&mut world);

        let tick = world.river_erosion_state.as_ref().map(|s| s.tick).unwrap_or(0);
        assert_eq!(tick, 1);
    }
}

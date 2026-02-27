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
    if world.era == EraKind::Life
        || world.era == EraKind::Civilization
        || world.era == EraKind::History
    {
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
        let layer = world
            .layers
            .entry(LayerKind::Civilization)
            .or_insert_with(|| {
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
    if world.core.plate_id.len() != world.core.height.len() {
        return;
    }

    let mut next = world.core.height.clone();
    let target_sea_ratio = world.target_sea_ratio.clamp(0.08, 0.92);
    let tectonic_phase = world.tick as f32 * 0.042;
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
            let mut convergent_strength = 0.0_f32;
            let mut divergent_strength = 0.0_f32;
            let mut shear_strength = 0.0_f32;
            let mut max_drop = 0.0_f32;
            let pos_i = world.mesh.positions[i];
            let plate_i = world.core.plate_id[i];
            let vel_i = pseudo_plate_velocity(pos_i, plate_i, tectonic_phase);
            for &n_u32 in &world.mesh.nbrs[start..end] {
                let n = n_u32 as usize;
                if n >= world.core.height.len() {
                    continue;
                }
                nbr_sum += world.core.height[n];
                nbr_count += 1;
                let h_drop = world.core.height[i] - world.core.height[n];
                if h_drop > max_drop {
                    max_drop = h_drop;
                }

                let same_plate = world.core.plate_id.get(i) == world.core.plate_id.get(n);
                if same_plate {
                    same_plate_count += 1;
                }

                let pos_n = world.mesh.positions[n];
                let edge_vec = [
                    pos_n[0] - pos_i[0],
                    pos_n[1] - pos_i[1],
                    pos_n[2] - pos_i[2],
                ];
                let edge_len = length3(edge_vec).max(1e-5);
                let edge_dir = [
                    edge_vec[0] / edge_len,
                    edge_vec[1] / edge_len,
                    edge_vec[2] / edge_len,
                ];
                let vel_n = pseudo_plate_velocity(pos_n, world.core.plate_id[n], tectonic_phase);
                let rel_v = [
                    vel_n[0] - vel_i[0],
                    vel_n[1] - vel_i[1],
                    vel_n[2] - vel_i[2],
                ];
                let rel_n = dot(rel_v, edge_dir);
                let rel_mag = length3(rel_v);

                if same_plate {
                    shear_strength += (rel_mag * 0.55).min(0.8);
                } else if rel_n > 0.0 {
                    convergent_strength += rel_n;
                    shear_strength += (rel_mag * 0.30).min(0.4);
                } else {
                    divergent_strength += -rel_n;
                }
            }
            if nbr_count == 0 {
                continue;
            }

            let avg_nbr = nbr_sum / nbr_count as f32;
            let plate_similarity = same_plate_count as f32 / nbr_count as f32;
            let boundary_strength = (1.0 - plate_similarity).clamp(0.0, 1.0);
            let inland_strength = plate_similarity.clamp(0.0, 1.0);
            let height_i = world.core.height[i];
            let land = if height_i > 0.0 { 1.0 } else { 0.0 };
            let ridge_anchor = (height_i.max(0.0) / 0.55).clamp(0.0, 1.0);
            let slope = (max_drop / 0.18).clamp(0.0, 1.0);

            let mut relax = 0.030 + 0.090 * (0.45 + 0.55 * boundary_strength);
            relax *= 1.0 - 0.55 * ridge_anchor * inland_strength;
            relax *= 1.0 - 0.35 * land * (1.0 - slope) * inland_strength;

            let conv = convergent_strength / nbr_count as f32;
            let div = divergent_strength / nbr_count as f32;
            let shear = shear_strength / nbr_count as f32;
            let uplift = 0.080 * conv * (0.45 + 0.55 * land);
            let subsidence = 0.048 * div * (0.65 + 0.35 * (1.0 - land));
            let intraplate_fold = 0.030 * shear * inland_strength * (0.25 + 0.75 * ridge_anchor);

            let erosion = (avg_nbr - height_i) * relax;
            next[i] = (height_i + erosion + uplift + intraplate_fold - subsidence).clamp(-1.0, 1.0);
        }
        preserve_target_sea_ratio(&mut next, target_sea_ratio, 0.78);
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
        let pos = world
            .mesh
            .positions
            .get(i)
            .copied()
            .unwrap_or([0.0, 0.0, 1.0]);
        let latitude = pos[1].abs().clamp(0.0, 1.0);
        let altitude = world.core.height[i].max(0.0);
        let base_temp = 0.15 + (1.0 - latitude) * 0.85;
        let target_temp = (base_temp - altitude * 0.35).clamp(0.0, 1.0);

        let river_norm = (world.core.river_flux[i] / max_flux).clamp(0.0, 1.0);
        let orographic = (altitude * 0.50).clamp(0.0, 0.35);
        let target_rain =
            (0.20 + river_norm * 0.45 + (1.0 - latitude) * 0.25 + orographic).clamp(0.0, 1.0);

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
        let land = if world.core.height[i] > 0.0 {
            1.0
        } else {
            0.15
        };
        let river_bonus = (world.core.river_flux[i] / max_flux).clamp(0.0, 1.0) * 0.20;
        let temp_suit = 1.0 - ((temp - 0.55).abs() / 0.55).clamp(0.0, 1.0);
        let rain_suit = 1.0 - ((rain - 0.60).abs() / 0.60).clamp(0.0, 1.0);
        let target_habitability =
            ((temp_suit * 0.55 + rain_suit * 0.45) * land + river_bonus).clamp(0.0, 1.0);
        let target_productivity =
            (target_habitability * (0.45 + rain * 0.40 + river_bonus)).clamp(0.0, 1.0);

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
    let Some(CellLayer::Civilization(civilization)) =
        world.layers.get_mut(&LayerKind::Civilization)
    else {
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
        let carrying =
            1.0 + eco_productivity[i] * 130.0 + eco_habitability[i] * 70.0 + river_support * 40.0;
        let current = civilization.population[i].max(0.0);
        let seeded = if current < 1.0 && eco_habitability[i] > 0.55 {
            1.0
        } else {
            current
        };
        let growth =
            0.18 * eco_habitability[i].max(0.05) * seeded * (1.0 - seeded / carrying).max(-0.5);
        let next_population = (seeded + growth * alpha * 4.0).max(0.0);
        civilization.population[i] = next_population;
        civilization.state_id[i] = if next_population >= 10.0 {
            (i + 1) as u32
        } else {
            0
        };
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

fn preserve_target_sea_ratio(height: &mut [f32], target_sea_ratio: f32, strength: f32) {
    if height.is_empty() {
        return;
    }

    let mut sorted = height.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let sea_idx = ((sorted.len() as f32) * target_sea_ratio.clamp(0.02, 0.98)) as usize;
    let sea_idx = sea_idx.min(sorted.len().saturating_sub(1));
    let sea_level = sorted[sea_idx];
    let shift = sea_level * strength.clamp(0.0, 1.0);

    for h in height.iter_mut() {
        *h = (*h - shift).clamp(-1.0, 1.0);
    }
}

fn pseudo_plate_velocity(pos: [f32; 3], plate_id: u16, phase: f32) -> [f32; 3] {
    let seed = plate_id as u32;
    let base_azimuth = std::f32::consts::TAU * hash01(seed ^ 0x85eb_ca6b);
    let speed = 0.10 + 0.42 * hash01(seed ^ 0xc2b2_ae35);
    let base = [speed * base_azimuth.cos(), speed * base_azimuth.sin(), 0.0];

    let axis_a = seeded_axis(seed ^ 0x27d4_eb2f);
    let axis_b = seeded_axis(seed ^ 0x1656_67b1);
    let twist = (dot(pos, axis_a) * 6.0 + phase + 7.0 * hash01(seed ^ 0x9e37_79b9)).sin();
    let bend = (dot(pos, axis_b) * 9.0 - 0.7 * phase + 5.0 * hash01(seed ^ 0x94d0_49bb)).sin();
    let variation = 0.08 + 0.18 * hash01(seed ^ 0x68bc_21eb);

    [
        base[0] + variation * (0.65 * twist + 0.35 * bend),
        base[1] + variation * (0.65 * bend - 0.30 * twist),
        variation * 0.22 * (twist - bend),
    ]
}

fn seeded_axis(seed: u32) -> [f32; 3] {
    let z = 2.0 * hash01(seed ^ 0x7feb_352d) - 1.0;
    let phi = std::f32::consts::TAU * hash01(seed ^ 0x846c_a68b);
    let r = (1.0 - z * z).max(0.0).sqrt();
    [r * phi.cos(), z, r * phi.sin()]
}

fn hash01(seed: u32) -> f32 {
    let s = ((seed as f32) * 12.9898 + 78.233).sin();
    fract01(s * 43_758.547)
}

fn fract01(v: f32) -> f32 {
    v - v.floor()
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
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
    fn terrain_step_preserves_initial_sea_ratio() {
        let mut world = build_test_world();
        let base_ratio = world.core.height.iter().filter(|&&h| h <= 0.0).count() as f32
            / world.core.height.len() as f32;
        world.target_sea_ratio = base_ratio;

        for _ in 0..24 {
            run_terrain_step(&mut world, 4);
        }

        let ratio = world.core.height.iter().filter(|&&h| h <= 0.0).count() as f32
            / world.core.height.len() as f32;
        assert!((ratio - base_ratio).abs() <= 0.26);
    }

    #[test]
    fn pseudo_plate_velocity_varies_inside_same_plate() {
        let a = pseudo_plate_velocity([0.0, 0.8, 0.6], 3, 0.0);
        let b = pseudo_plate_velocity([0.7, 0.2, 0.6], 3, 0.0);
        let diff = (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs();
        assert!(diff > 1e-3);
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

        assert!(world
            .core
            .river_flux
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0));
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

        let tick = world
            .river_erosion_state
            .as_ref()
            .map(|s| s.tick)
            .unwrap_or(0);
        assert_eq!(tick, 1);
    }
}

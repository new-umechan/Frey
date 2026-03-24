use std::collections::HashMap;

pub mod types;

#[allow(unused_imports)]
pub use crate::sim::polity::types::*;

use crate::sim::world::{PolityComponent, World};

pub(crate) fn update_polity(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let n = world.state.geology.height.len();
    let mut polity_cells: HashMap<u32, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let pop = world.state.population.population[i];
        let polity_id = if pop >= 10.0 {
            Some((i + 1) as u32)
        } else {
            None
        };
        world.state.polity.polity_id[i] = polity_id;
        world.state.polity.territory_status[i] = if polity_id.is_none() {
            0
        } else {
            1
        };
        world.state.polity.language_group[i] = if polity_id.is_none() {
            0
        } else {
            (i % 8) as u16 + 1
        };
        world.state.polity.polity_stability[i] =
            (1.0 - world.state.population.migration_pressure[i]).clamp(0.0, 1.0);
        if let Some(id) = polity_id {
            polity_cells.entry(id).or_default().push(i);
        }
    }

    let mut polity_components = Vec::new();
    for (polity_id, cells) in &polity_cells {
        let mut capital_cell = cells[0] as u32;
        let mut max_population = -1.0_f32;
        let mut stability_sum = 0.0_f32;
        for &cell in cells {
            let pop = world.state.population.population[cell];
            if pop > max_population {
                max_population = pop;
                capital_cell = cell as u32;
            }
            stability_sum += world.state.polity.polity_stability[cell];
        }
        let stability = if cells.is_empty() {
            0.0
        } else {
            stability_sum / cells.len() as f32
        };
        polity_components.push(PolityComponent {
            polity_id: *polity_id,
            capital_cell,
            legitimacy: stability,
            centralization: (0.35 + stability * 0.50).clamp(0.0, 1.0),
            military_tech: (0.20 + max_population / 120.0).clamp(0.0, 1.0),
        });
    }
    polity_components.sort_by_key(|component| component.polity_id);
    world.entities.replace_polities(polity_components.clone());

    let active_ids = polity_components
        .iter()
        .map(|component| component.polity_id)
        .collect::<Vec<_>>();
    world
        .polity_relations
        .retain(|(from, to), _| active_ids.contains(from) && active_ids.contains(to) && from != to);
    for from in &active_ids {
        for to in &active_ids {
            if from == to {
                continue;
            }
            let relation = world
                .polity_relations
                .entry((*from, *to))
                .or_insert_with(PolityRelation::default);
            relation.trade = relation.trade.clamp(0.0, 1.0).max(0.2);
            relation.alliance = relation.alliance.clamp(-1.0, 1.0);
            relation.at_war = false;
            relation.suzerain = None;
        }
    }
}

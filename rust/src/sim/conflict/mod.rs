pub mod types;

#[allow(unused_imports)]
pub use crate::sim::conflict::types::*;

use std::collections::{HashMap, HashSet};

use crate::sim::world::{CellId, PolityId, RegionComponent, RegionId, World};

pub(crate) fn update_conflict(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    let n = world.state.geology.height.len();
    world.state.conflict.conflict_intensity.fill(0.0);
    world.state.conflict.occupier_id.fill(None);

    let mut polity_cells: HashMap<PolityId, Vec<CellId>> = HashMap::new();
    let mut frontline_cells = Vec::new();
    let mut hostile_pairs = HashSet::new();
    let mut relation_pairs = world.polity_relations.keys().copied().collect::<Vec<_>>();
    relation_pairs.sort_unstable();
    for pair in relation_pairs {
        if let Some(relation) = world.polity_relations.get_mut(&pair) {
            relation.at_war = false;
        }
    }

    for i in 0..n {
        let Some(polity_id) = world.state.polity.polity_id[i] else {
            continue;
        };
        if polity_id.as_u32() > 0 {
            polity_cells.entry(polity_id).or_default().push(CellId(i as u32));
        }
        let start = world.mesh.nbr_offsets.get(i).copied().unwrap_or(0) as usize;
        let end = world
            .mesh
            .nbr_offsets
            .get(i + 1)
            .copied()
            .unwrap_or(start as u32) as usize;
        for &nbr in world.mesh.nbrs.get(start..end).unwrap_or(&[]) {
            let j = nbr as usize;
            if j >= n {
                continue;
            }
            let Some(other_polity) = world.state.polity.polity_id[j] else {
                continue;
            };
            if other_polity == polity_id {
                continue;
            }
            world.state.conflict.conflict_intensity[i] = 1.0;
            frontline_cells.push(CellId(i as u32));
            let pair = if polity_id < other_polity {
                (polity_id, other_polity)
            } else {
                (other_polity, polity_id)
            };
            hostile_pairs.insert(pair);
        }
    }

    for (a, b) in hostile_pairs {
        if let Some(rel) = world.polity_relations.get_mut(&(a, b)) {
            rel.at_war = true;
            rel.alliance = rel.alliance.min(-0.6);
        }
        if let Some(rel) = world.polity_relations.get_mut(&(b, a)) {
            rel.at_war = true;
            rel.alliance = rel.alliance.min(-0.6);
        }
    }

    let mut regions = Vec::new();
    let mut region_id = 1_u32;
    for cells in polity_cells.values() {
        regions.push(RegionComponent {
            region_id: RegionId(region_id),
            cells: cells.clone(),
        });
        region_id = region_id.saturating_add(1);
    }
    frontline_cells.sort_unstable();
    frontline_cells.dedup();
    if !frontline_cells.is_empty() {
        regions.push(RegionComponent {
            region_id: RegionId(region_id),
            cells: frontline_cells,
        });
    }
    world.entities.replace_regions(regions);
}

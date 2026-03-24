use crate::sim::world::{
    CellFieldId, ComponentPatch, EntityBundle, FeedbackPayload, FieldValue, TargetRef, World,
};

pub(super) fn apply_feedback_queue(world: &mut World) {
    apply_payload_entries(world);
}

fn apply_payload_entries(world: &mut World) {
    let cell_count = world.cell_count();
    let entries = std::mem::take(&mut world.feedback.entries);
    let mut remaining = Vec::new();
    for entry in entries {
        if entry.enqueued_tick >= world.clock.tick {
            remaining.push(entry);
            continue;
        }
        match entry.payload {
            FeedbackPayload::DeltaF32 { field, cell, delta } => {
                apply_feedback_f32_delta(
                    world,
                    field,
                    cell as usize,
                    delta,
                    cell_count,
                    &entry.target_ref,
                );
            }
            FeedbackPayload::SetValue {
                field,
                cell,
                value: FieldValue::F32(value),
            } => {
                apply_feedback_f32_set(
                    world,
                    field,
                    cell as usize,
                    value,
                    cell_count,
                    &entry.target_ref,
                );
            }
            FeedbackPayload::SpawnEntity { bundle } => {
                apply_spawn_entity(world, bundle);
            }
            FeedbackPayload::DestroyEntity { id } => {
                apply_destroy_entity(world, &entry.target_ref, id);
            }
            FeedbackPayload::MutateEntity { id, patch } => {
                apply_mutate_entity(world, &entry.target_ref, id, patch);
            }
            FeedbackPayload::TriggerEpochTransition { to } => {
                world.clock.epoch = to;
            }
            _ => {}
        }
    }
    world.entities.sync_world_from_components();
    world.feedback.entries = remaining;
}

fn apply_feedback_f32_delta(
    world: &mut World,
    field: CellFieldId,
    cell: usize,
    value: f32,
    cell_count: usize,
    _target_ref: &TargetRef,
) {
    if cell >= cell_count {
        return;
    }
    match field {
        CellFieldId::CropAdoption(crop_id) => {
            let idx = crop_id as usize;
            if idx < world.state.domesticates.crop_adoption[cell].len() {
                world.state.domesticates.crop_adoption[cell][idx] += value;
            }
        }
        CellFieldId::LivestockAdoption(livestock_id) => {
            let idx = livestock_id as usize;
            if idx < world.state.domesticates.livestock_adoption[cell].len() {
                world.state.domesticates.livestock_adoption[cell][idx] += value;
            }
        }
    }
}

fn apply_feedback_f32_set(
    world: &mut World,
    field: CellFieldId,
    cell: usize,
    value: f32,
    cell_count: usize,
    _target_ref: &TargetRef,
) {
    if cell >= cell_count {
        return;
    }
    match field {
        CellFieldId::CropAdoption(crop_id) => {
            let idx = crop_id as usize;
            if idx < world.state.domesticates.crop_adoption[cell].len() {
                world.state.domesticates.crop_adoption[cell][idx] = value;
            }
        }
        CellFieldId::LivestockAdoption(livestock_id) => {
            let idx = livestock_id as usize;
            if idx < world.state.domesticates.livestock_adoption[cell].len() {
                world.state.domesticates.livestock_adoption[cell][idx] = value;
            }
        }
    }
}

fn apply_spawn_entity(world: &mut World, bundle: EntityBundle) {
    match bundle {
        EntityBundle::Polity(component) => world.entities.polity_components.push(component),
        EntityBundle::Settlement(component) => world.entities.settlement_components.push(component),
        EntityBundle::Region(component) => world.entities.region_components.push(component),
    }
}

fn apply_destroy_entity(world: &mut World, target_ref: &TargetRef, id: u32) {
    match target_ref {
        TargetRef::Polity(_) => {
            world
                .entities
                .polity_components
                .retain(|component| component.polity_id != id);
        }
        TargetRef::Settlement(_) => {
            world
                .entities
                .settlement_components
                .retain(|component| component.settlement_id != id);
        }
        TargetRef::Region(_) => {
            world
                .entities
                .region_components
                .retain(|component| component.region_id != id);
        }
        _ => {}
    }
}

fn apply_mutate_entity(world: &mut World, target_ref: &TargetRef, id: u32, patch: ComponentPatch) {
    match (target_ref, patch) {
        (
            TargetRef::Polity(_),
            ComponentPatch::Polity {
                capital_cell,
                stability,
            },
        ) => {
            if let Some(component) = world
                .entities
                .polity_components
                .iter_mut()
                .find(|component| component.polity_id == id)
            {
                if let Some(value) = capital_cell {
                    component.capital_cell = value;
                }
                if let Some(value) = stability {
                    component.legitimacy = value;
                }
            }
        }
        (TargetRef::Settlement(_), ComponentPatch::Settlement { cell }) => {
            if let Some(component) = world
                .entities
                .settlement_components
                .iter_mut()
                .find(|component| component.settlement_id == id)
            {
                if let Some(value) = cell {
                    component.cell = value;
                }
            }
        }
        (TargetRef::Region(_), ComponentPatch::Region { cells }) => {
            if let Some(component) = world
                .entities
                .region_components
                .iter_mut()
                .find(|component| component.region_id == id)
            {
                if let Some(value) = cells {
                    component.cells = value;
                }
            }
        }
        _ => {}
    }
}

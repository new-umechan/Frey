use crate::sim::world::{
    CellFieldId, ComponentPatch, EntityBundle, FeedbackPayload, FeedbackQueue, FieldValue,
    TargetRef, World,
};

pub(super) fn apply_feedback_queue(world: &mut World, feedback: &mut FeedbackQueue) {
    apply_payload_entries(world, feedback);
}

fn apply_payload_entries(world: &mut World, feedback: &mut FeedbackQueue) {
    let cell_count = world.cell_count();
    let entries = std::mem::take(&mut feedback.entries);
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
                    cell.as_usize(),
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
                    cell.as_usize(),
                    value,
                    cell_count,
                    &entry.target_ref,
                );
            }
            FeedbackPayload::SpawnEntity { bundle } => {
                apply_spawn_entity(world, bundle);
            }
            FeedbackPayload::DestroyEntity { entity } => {
                apply_destroy_entity(world, &entity);
            }
            FeedbackPayload::MutateEntity { entity, patch } => {
                apply_mutate_entity(world, &entry.target_ref, &entity, patch);
            }
            FeedbackPayload::TriggerEpochTransition { .. } => {}
            _ => {}
        }
    }
    feedback.entries = remaining;
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
    if let Err(error) = world.entities.apply_entity_bundle(bundle) {
        debug_assert!(false, "failed to apply entity bundle: {error}");
    }
}

fn apply_destroy_entity(world: &mut World, entity: &crate::sim::world::EntityRef) {
    world.entities.destroy_entity(entity);
}

fn apply_mutate_entity(
    world: &mut World,
    target_ref: &TargetRef,
    entity: &crate::sim::world::EntityRef,
    patch: ComponentPatch,
) {
    world.entities.mutate_entity(target_ref, entity, patch);
}

use crate::sim::world::{FeedbackFields, World};

pub(super) fn apply_feedback_queue(world: &mut World) {
    let active = world.exec.feedback_queue.pending.clone();
    world.exec.feedback_queue.active = active;
    world.exec.feedback_queue.pending.clear();

    let cell_count = world.cell_count();
    for i in 0..cell_count {
        let withdrawal = channel_value(
            &world.exec.feedback_queue.active,
            FeedbackFields::WATER_WITHDRAWAL_KEY,
            i,
        );
        let dam_pressure = channel_value(
            &world.exec.feedback_queue.active,
            FeedbackFields::DAM_PRESSURE_KEY,
            i,
        );
        let pollution = channel_value(
            &world.exec.feedback_queue.active,
            FeedbackFields::POLLUTION_KEY,
            i,
        );

        world.state.subsistence.water_withdrawal[i] = withdrawal;
        world.state.subsistence.dam_pressure[i] = dam_pressure;
        world.state.subsistence.pollution[i] = pollution;
    }
}

fn channel_value(fields: &FeedbackFields, key: &str, index: usize) -> f32 {
    fields
        .channel(key)
        .and_then(|values| values.get(index).copied())
        .unwrap_or(0.0)
}

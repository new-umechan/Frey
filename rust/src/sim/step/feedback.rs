use super::World;

pub(super) fn apply_feedback_queue(world: &mut World) {
    let active = world.exec.feedback_queue.pending.clone();
    world.exec.feedback_queue.active = active;
    world.exec.feedback_queue.pending.clear();

    let cell_count = world.cell_count();
    for i in 0..cell_count {
        let withdrawal = world.exec.feedback_queue.active.water_withdrawal[i];
        let dam_pressure = world.exec.feedback_queue.active.dam_pressure[i];
        let pollution = world.exec.feedback_queue.active.pollution[i];

        world.state.civilization.water_withdrawal[i] = withdrawal;
        world.state.civilization.dam_level[i] = dam_pressure;
        world.state.civilization.pollution[i] = pollution;
    }
}

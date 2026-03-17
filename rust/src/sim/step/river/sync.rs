pub(super) fn erosion_state_matches_world(
    state: &crate::ErosionAutomatonState,
    expected_height: usize,
    expected_flux: usize,
    expected_next: usize,
) -> bool {
    state.height.len() == expected_height
        && state.river_flux.len() == expected_flux
        && state.river_next.len() == expected_next
}

pub(super) fn sync_erosion_rain(state: &mut crate::ErosionAutomatonState, runoff: &[f32]) {
    if state.rain.len() != runoff.len() {
        return;
    }
    for (dst, src) in state.rain.iter_mut().zip(runoff.iter().copied()) {
        *dst = src.max(0.0);
    }
}

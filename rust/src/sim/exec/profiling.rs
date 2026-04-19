#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};

#[derive(Clone, Copy, Debug, Default)]
pub struct ExecWorldBreakdown {
    pub exec_feedback_ms: f64,
    pub exec_geology_terrain_ms: f64,
    pub exec_climate_ms: f64,
    pub exec_glaciology_ms: f64,
    pub exec_hydrology_ms: f64,
    pub exec_ecology_ms: f64,
    pub exec_society_ms: f64,
    pub exec_transition_ms: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExecWorldRiverBreakdown {
    pub step_geology_river_prepare_ms: f64,
    pub step_geology_river_automaton_ms: f64,
    pub step_geology_river_automaton_sink_ms: f64,
    pub step_geology_river_automaton_cell_ms: f64,
    pub step_geology_river_automaton_queue_ms: f64,
    pub step_geology_river_network_ms: f64,
    pub step_geology_river_sync_ms: f64,
    pub step_geology_river_fallback_ms: f64,
    pub river_network_rebuild_count: u32,
    pub river_fallback_count: u32,
    pub sink_rebuild_full_count: u32,
    pub sink_rebuild_partial_count: u32,
    pub sink_rebuild_skipped_count: u32,
    pub sink_rebuild_fallback_full_count: u32,
    pub step_geology_river_sink_incremental_rebuild_ms: f64,
    pub step_geology_river_sink_full_rebuild_ms: f64,
    pub sink_affected_ratio: f64,
    pub sink_validation_fail_count: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExecWorldBreakdownDetailed {
    pub breakdown: ExecWorldBreakdown,
    pub river: ExecWorldRiverBreakdown,
}

#[cfg(target_arch = "wasm32")]
pub(super) type ProfileClock = f64;
#[cfg(not(target_arch = "wasm32"))]
pub(super) type ProfileClock = std::time::Instant;

#[cfg(target_arch = "wasm32")]
pub(super) fn profile_now() -> ProfileClock {
    let global = js_sys::global();
    let performance = js_sys::Reflect::get(&global, &JsValue::from_str("performance")).ok();
    if let Some(perf) = performance {
        if !perf.is_null() && !perf.is_undefined() {
            let now_fn = js_sys::Reflect::get(&perf, &JsValue::from_str("now")).ok();
            if let Some(now_fn) = now_fn {
                if let Some(now_fn) = now_fn.dyn_ref::<js_sys::Function>() {
                    if let Ok(value) = now_fn.call0(&perf) {
                        if let Some(ms) = value.as_f64() {
                            return ms;
                        }
                    }
                }
            }
        }
    }
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn profile_now() -> ProfileClock {
    std::time::Instant::now()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    profile_now() - start
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

impl ExecWorldBreakdown {
    pub(super) fn capture_elapsed(elapsed_from: ProfileClock) -> f64 {
        profile_elapsed_ms(elapsed_from)
    }

    pub fn accumulate(&mut self, other: &Self) {
        self.exec_feedback_ms += other.exec_feedback_ms;
        self.exec_geology_terrain_ms += other.exec_geology_terrain_ms;
        self.exec_climate_ms += other.exec_climate_ms;
        self.exec_glaciology_ms += other.exec_glaciology_ms;
        self.exec_hydrology_ms += other.exec_hydrology_ms;
        self.exec_ecology_ms += other.exec_ecology_ms;
        self.exec_society_ms += other.exec_society_ms;
        self.exec_transition_ms += other.exec_transition_ms;
    }
}

impl ExecWorldRiverBreakdown {
    pub fn accumulate(&mut self, other: &Self) {
        self.step_geology_river_prepare_ms += other.step_geology_river_prepare_ms;
        self.step_geology_river_automaton_ms += other.step_geology_river_automaton_ms;
        self.step_geology_river_automaton_sink_ms += other.step_geology_river_automaton_sink_ms;
        self.step_geology_river_automaton_cell_ms += other.step_geology_river_automaton_cell_ms;
        self.step_geology_river_automaton_queue_ms += other.step_geology_river_automaton_queue_ms;
        self.step_geology_river_network_ms += other.step_geology_river_network_ms;
        self.step_geology_river_sync_ms += other.step_geology_river_sync_ms;
        self.step_geology_river_fallback_ms += other.step_geology_river_fallback_ms;
        self.river_network_rebuild_count = self
            .river_network_rebuild_count
            .saturating_add(other.river_network_rebuild_count);
        self.river_fallback_count = self
            .river_fallback_count
            .saturating_add(other.river_fallback_count);
        self.sink_rebuild_full_count = self
            .sink_rebuild_full_count
            .saturating_add(other.sink_rebuild_full_count);
        self.sink_rebuild_partial_count = self
            .sink_rebuild_partial_count
            .saturating_add(other.sink_rebuild_partial_count);
        self.sink_rebuild_skipped_count = self
            .sink_rebuild_skipped_count
            .saturating_add(other.sink_rebuild_skipped_count);
        self.sink_rebuild_fallback_full_count = self
            .sink_rebuild_fallback_full_count
            .saturating_add(other.sink_rebuild_fallback_full_count);
        self.step_geology_river_sink_incremental_rebuild_ms +=
            other.step_geology_river_sink_incremental_rebuild_ms;
        self.step_geology_river_sink_full_rebuild_ms +=
            other.step_geology_river_sink_full_rebuild_ms;
        self.sink_affected_ratio += other.sink_affected_ratio;
        self.sink_validation_fail_count = self
            .sink_validation_fail_count
            .saturating_add(other.sink_validation_fail_count);
    }
}

impl ExecWorldBreakdownDetailed {
    pub fn accumulate(&mut self, other: &Self) {
        self.breakdown.accumulate(&other.breakdown);
        self.river.accumulate(&other.river);
    }
}

mod feedback;
mod geology;
pub(crate) mod math;
mod modules;
mod pipeline;
mod profiling;
mod profiling_exec;
mod profiling_river;
mod transition;

#[cfg(test)]
mod tests;

pub type HydrologyExecState = Option<crate::sim::erosion::ErosionAutomatonState>;
pub type GeologyExecState = Option<crate::sim::world::GeologyDynamicsState>;

pub use modules::{
    declaration_for_phase, declared_dependencies, declared_phase_order, display_group_key,
    execution_kind_key, feedback_mode_key, first_phase, module_description, module_doc_records,
    module_graph_edge_records, module_graph_record, module_key, module_manifest_lines,
    module_manifests, next_phase_after, phase_accepts_exec_feedback, phase_accepts_module_feedback,
    phase_completes_tick, phase_display_group, phase_execution_kind, phase_key,
    phase_profile_category, profile_category_key, tick_boundary_key, validate_module_declarations,
    world_resource_key, DisplayGroup, ExecutionKind, FeedbackMode, ModuleDeclaration,
    ModuleDependency, ModuleDocRecord, ModuleExecContext, ModuleGraphEdgeRecord, ModuleGraphRecord,
    ModuleManifest, ProfileCategory, WorldResource,
};
pub use pipeline::{
    exec_world, exec_world_slice, exec_world_slice_with_hydrology, exec_world_slice_with_states,
    exec_world_with_feedback, exec_world_with_feedback_and_hydrology,
    exec_world_with_feedback_and_states, ExecWorldPhase, ExecWorldSliceResult,
};
pub use profiling::{ExecWorldBreakdown, ExecWorldBreakdownDetailed};
pub use profiling_exec::{
    exec_world_profiled, exec_world_profiled_detailed, exec_world_profiled_detailed_with_feedback,
    exec_world_profiled_detailed_with_feedback_and_hydrology,
    exec_world_profiled_detailed_with_feedback_and_states,
};

use super::world::EraKind;

pub(crate) const MAX_HEIGHT_DELTA_PER_STEP: f32 = 0.020;
pub(crate) const GEOLOGY_HEIGHT_MIN: f32 = -1.2;
pub(crate) const GEOLOGY_HEIGHT_MAX: f32 = 1.2;
pub(crate) const DEFAULT_DIFFUSION_WEIGHT: f32 = 0.06;
pub(crate) const CONVERGENT_THRESHOLD: f32 = 0.010;
pub(crate) const DIVERGENT_THRESHOLD: f32 = 0.010;
pub(crate) const TRANSFORM_THRESHOLD: f32 = 0.014;
pub(crate) const CRUST_RAIN_LAND: f32 = 0.12;
pub(crate) const CRUST_RAIN_SEA: f32 = 0.04;
pub(crate) const HYDROLOGY_MFD_ACTIVITY_THRESHOLD: f32 = 0.002;

pub(crate) fn geology_river_budget(era: EraKind, geology_budget: u32) -> u32 {
    let scale = match era {
        EraKind::Crust => 1,
        EraKind::Environment => 4,
        EraKind::Life => 3,
        EraKind::Civilization => 2,
        EraKind::History => 1,
    };
    geology_budget.saturating_mul(scale).max(1)
}

pub(crate) fn blend_alpha(budget: u32, base: f32) -> f32 {
    let b = budget.max(1) as f32;
    (1.0 - (1.0 - base).powf(b)).clamp(0.0, 1.0)
}

pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = math::length3(v);
    if len <= 1e-6 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

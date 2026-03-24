mod feedback;
mod geology;
pub(crate) mod math;
mod pipeline;
mod profiling;
mod transition;

#[cfg(test)]
mod tests;

pub use pipeline::exec_world;
pub use profiling::{
    exec_world_profiled, exec_world_profiled_detailed, ExecWorldBreakdown,
    ExecWorldBreakdownDetailed,
};

use super::world::EraKind;

pub(crate) const MAX_HEIGHT_DELTA_PER_STEP: f32 = 0.020;
pub(crate) const DEFAULT_DIFFUSION_WEIGHT: f32 = 0.06;
pub(crate) const CONVERGENT_THRESHOLD: f32 = 0.010;
pub(crate) const DIVERGENT_THRESHOLD: f32 = 0.010;
pub(crate) const TRANSFORM_THRESHOLD: f32 = 0.014;
pub(crate) const CRUST_RAIN_LAND: f32 = 0.12;
pub(crate) const CRUST_RAIN_SEA: f32 = 0.04;

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

pub mod surface;
pub mod precipitation;
pub mod types;

pub(crate) use surface::run_climate_step;
#[allow(unused_imports)]
pub(crate) use precipitation::build_precipitation_map;
#[allow(unused_imports)]
pub use crate::sim::climate::types::*;

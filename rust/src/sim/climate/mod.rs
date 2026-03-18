pub mod precipitation;
pub mod surface;
pub mod types;

#[allow(unused_imports)]
pub use crate::sim::climate::types::*;
#[allow(unused_imports)]
pub(crate) use precipitation::build_precipitation_map;
pub(crate) use surface::run_climate_step;

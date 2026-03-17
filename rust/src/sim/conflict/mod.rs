pub mod types;

#[allow(unused_imports)]
pub use crate::sim::conflict::types::*;

use crate::sim::world::World;

pub(crate) fn update_conflict(_world: &mut World, _budget: u32) {
}

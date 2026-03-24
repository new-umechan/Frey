use serde::{Deserialize, Serialize};

use crate::sim::world::{PolityGroupId, PolityId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PolityRelation {
    pub alliance: f32,
    pub trade: f32,
    pub at_war: bool,
    pub suzerain: Option<PolityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PolityGroup {
    pub id: PolityGroupId,
    pub kind: GroupKind,
    pub members: Vec<PolityId>,
    pub leader: Option<PolityId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GroupKind {
    EconomicZone,
    MilitaryAlliance,
    #[default]
    CulturalSphere,
}

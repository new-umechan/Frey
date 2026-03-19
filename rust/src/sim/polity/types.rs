use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PolityRelation {
    pub alliance: f32,
    pub trade: f32,
    pub at_war: bool,
    pub suzerain: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PolityGroup {
    pub id: u32,
    pub kind: GroupKind,
    pub members: Vec<u32>,
    pub leader: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GroupKind {
    EconomicZone,
    MilitaryAlliance,
    #[default]
    CulturalSphere,
}

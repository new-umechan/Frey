mod entity_state;
mod era;
mod exec;
mod init;
mod metrics;
mod state;

#[cfg(test)]
mod tests;

pub use entity_state::{
    EntityState, EntityStateError, PolityKey, PolityRecord, RegionKey, RegionRecord, SettlementKey,
    SettlementRecord,
};
pub use era::EraKind;
pub use exec::{
    CellFieldId, ClockState, ComponentPatch, EntityBundle, EntityRef, ExecScratchState,
    FeedbackEntry, FeedbackPayload, FeedbackQueue, FieldValue, ModuleId, SubsystemBudgets,
    TargetRef, TransitionState,
};
pub use init::default_target_sea_ratio;
pub use metrics::WorldMetrics;
pub use state::{
    ArchiveState, Biome, BoundaryDynamicsState, BoundaryType, CellId, CellStore, CellStoreMut,
    CivilizationIndicators, CivilizationState, CivilizationStateMut, ClimateState, CoastSide,
    ConflictState, CropBitmap, DomesticatesInternal, DomesticatesState, EcologyInternal,
    EcologyState, GeologyDynamicsState, GeologyState, GeologyStepMetrics, GlaciologyState,
    HydrologyState, LivestockBitmap, PlateKinematicsState, PolityComponent, PolityGroupId,
    PolityId, PolityState, PopulationState, RegionComponent, RegionId, SettlementComponent,
    SettlementId, SettlementState, SubsistenceMix, SubsistenceState, TerrainState,
    VertexCrustState, World, WorldControlState, WorldCore, WorldMesh, WorldState, N_CROPS,
    N_LIVESTOCK,
};

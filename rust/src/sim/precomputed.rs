use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::sim::erosion::ErosionAutomatonState;
use crate::sim::world::{EraKind, GeologyDynamicsState, World, WorldCore};
use crate::GeologyParams;

pub const ALPHA_STAGE_BOUNDARIES: [(&str, u64, EraKind); 4] = [
    ("environment", 800, EraKind::Environment),
    ("life", 1300, EraKind::Life),
    ("civilization", 1395, EraKind::Civilization),
    ("history", 1445, EraKind::History),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlphaSnapshotStage {
    Environment,
    Life,
    Civilization,
    History,
}

impl AlphaSnapshotStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Life => "life",
            Self::Civilization => "civilization",
            Self::History => "history",
        }
    }

    pub fn target_tick(self) -> u64 {
        match self {
            Self::Environment => 800,
            Self::Life => 1300,
            Self::Civilization => 1395,
            Self::History => 1445,
        }
    }
}

impl std::str::FromStr for AlphaSnapshotStage {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "environment" => Ok(Self::Environment),
            "life" => Ok(Self::Life),
            "civilization" => Ok(Self::Civilization),
            "history" => Ok(Self::History),
            _ => Err(format!("unsupported snapshot stage: {value}")),
        }
    }
}

impl Display for AlphaSnapshotStage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecomputedWorldSnapshotEnvelope {
    pub format_version: u32,
    pub seed: String,
    pub mesh_level: u32,
    pub stage: AlphaSnapshotStage,
    pub tick: u64,
    pub era: String,
    pub geology_fingerprint: String,
    pub applied_intervention_seq: u64,
    pub world_core: WorldCore,
    pub hydrology_state: ErosionAutomatonState,
    pub geology_dynamics_state: Option<GeologyDynamicsState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecomputedWorldSnapshotManifestEntry {
    pub stage: AlphaSnapshotStage,
    pub filename: String,
    pub tick: u64,
    pub era: String,
    pub geology_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecomputedWorldSnapshotManifest {
    pub format_version: u32,
    pub seed: String,
    pub mesh_level: u32,
    pub entries: Vec<PrecomputedWorldSnapshotManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecomputedWorldSnapshotView {
    pub format_version: u32,
    pub seed: String,
    pub mesh_level: u32,
    pub stage: AlphaSnapshotStage,
    pub tick: u64,
    pub era: String,
    pub geology_fingerprint: String,
    pub applied_intervention_seq: u64,
    pub cell_count: usize,
    pub polity_count: usize,
    pub settlement_count: usize,
    pub region_count: usize,
    pub polity_relation_count: usize,
    pub polity_group_count: usize,
    pub plate_relation_count: usize,
}

pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

pub fn canonical_cache_dir() -> PathBuf {
    PathBuf::from(".cache/frey/alpha-snapshots")
}

pub fn mirror_dir() -> PathBuf {
    PathBuf::from("web/public/.dev-precomputed/alpha")
}

pub fn geology_fingerprint(params: &GeologyParams) -> Result<String, String> {
    serde_json::to_string(params)
        .map(|json| format!("geology-params-json-v1:{json}"))
        .map_err(|err| format!("failed to serialize geology params fingerprint: {err}"))
}

pub fn stage_filename(stage: AlphaSnapshotStage) -> String {
    format!("{}.bin", stage.as_str())
}

pub fn stage_view_filename(stage: AlphaSnapshotStage) -> String {
    format!("{}.json", stage.as_str())
}

pub fn save_snapshot(
    path: &Path,
    envelope: &PrecomputedWorldSnapshotEnvelope,
) -> Result<(), String> {
    let bytes = bincode::serde::encode_to_vec(envelope, bincode::config::standard())
        .map_err(|err| format!("failed to serialize snapshot: {err}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create snapshot directory {}: {err}",
                parent.display()
            )
        })?;
    }
    fs::write(path, bytes)
        .map_err(|err| format!("failed to write snapshot {}: {err}", path.display()))
}

pub fn save_snapshot_view(
    path: &Path,
    envelope: &PrecomputedWorldSnapshotEnvelope,
) -> Result<(), String> {
    let view = PrecomputedWorldSnapshotView {
        format_version: envelope.format_version,
        seed: envelope.seed.clone(),
        mesh_level: envelope.mesh_level,
        stage: envelope.stage,
        tick: envelope.tick,
        era: envelope.era.clone(),
        geology_fingerprint: envelope.geology_fingerprint.clone(),
        applied_intervention_seq: envelope.applied_intervention_seq,
        cell_count: envelope.world_core.cells.geology.height.len(),
        polity_count: envelope.world_core.entities.polity_components().len(),
        settlement_count: envelope.world_core.entities.settlement_components().len(),
        region_count: envelope.world_core.entities.region_components().len(),
        polity_relation_count: envelope.world_core.relations.polity_relations.len(),
        polity_group_count: envelope.world_core.relations.polity_groups.len(),
        plate_relation_count: envelope.world_core.relations.plate_relations.len(),
    };
    let json = serde_json::to_string_pretty(&view)
        .map_err(|err| format!("failed to serialize snapshot view: {err}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create snapshot view directory {}: {err}",
                parent.display()
            )
        })?;
    }
    fs::write(path, json)
        .map_err(|err| format!("failed to write snapshot view {}: {err}", path.display()))
}

pub fn load_snapshot(path: &Path) -> Result<PrecomputedWorldSnapshotEnvelope, String> {
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read snapshot {}: {err}", path.display()))?;
    let (envelope, _): (PrecomputedWorldSnapshotEnvelope, usize) =
        bincode::serde::decode_from_slice(&bytes, bincode::config::standard())
            .map_err(|err| format!("failed to decode snapshot {}: {err}", path.display()))?;
    Ok(envelope)
}

pub fn save_manifest(
    path: &Path,
    manifest: &PrecomputedWorldSnapshotManifest,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|err| format!("failed to serialize manifest: {err}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create manifest directory {}: {err}",
                parent.display()
            )
        })?;
    }
    fs::write(path, json)
        .map_err(|err| format!("failed to write manifest {}: {err}", path.display()))
}

pub fn restore_world_from_snapshot(
    mut world: World,
    envelope: &PrecomputedWorldSnapshotEnvelope,
) -> Result<(World, ErosionAutomatonState), String> {
    world.apply_core(envelope.world_core.clone());
    world.exec_scratch.geology_dynamics = envelope.geology_dynamics_state.clone();
    world.refresh_terrain_state();
    crate::sim::hydrology::apply_hydrology_state_view(&mut world, &envelope.hydrology_state)?;
    Ok((world, envelope.hydrology_state.clone()))
}

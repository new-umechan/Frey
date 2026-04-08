use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use slotmap::{new_key_type, SlotMap};

use super::state::{
    CellId, PolityComponent, PolityId, RegionComponent, RegionId, SettlementComponent, SettlementId,
};

new_key_type! { pub struct PolityKey; }
new_key_type! { pub struct SettlementKey; }
new_key_type! { pub struct RegionKey; }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolityRecord {
    pub id: PolityId,
    pub capital_cell: CellId,
    pub legitimacy: f32,
    pub centralization: f32,
    pub military_tech: f32,
    pub cells_cache: Vec<CellId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementRecord {
    pub id: SettlementId,
    pub cell: CellId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionRecord {
    pub id: RegionId,
    pub cells: Vec<CellId>,
}

#[derive(Debug, Clone)]
pub struct EntityStore {
    pub polities: SlotMap<PolityKey, PolityRecord>,
    pub settlements: SlotMap<SettlementKey, SettlementRecord>,
    pub regions: SlotMap<RegionKey, RegionRecord>,
    pub polity_by_id: BTreeMap<PolityId, PolityKey>,
    pub settlement_by_id: BTreeMap<SettlementId, SettlementKey>,
    pub region_by_id: BTreeMap<RegionId, RegionKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityStoreError {
    DuplicatePolityId(PolityId),
    DuplicateSettlementId(SettlementId),
    DuplicateRegionId(RegionId),
    DanglingPolityIndex(PolityId),
    DanglingSettlementIndex(SettlementId),
    DanglingRegionIndex(RegionId),
    MismatchedPolityIndex {
        expected: PolityId,
        actual: PolityId,
    },
    MismatchedSettlementIndex {
        expected: SettlementId,
        actual: SettlementId,
    },
    MismatchedRegionIndex {
        expected: RegionId,
        actual: RegionId,
    },
    MissingPolityIndex(PolityId),
    MissingSettlementIndex(SettlementId),
    MissingRegionIndex(RegionId),
    PolityCountMismatch {
        records: usize,
        index: usize,
    },
    SettlementCountMismatch {
        records: usize,
        index: usize,
    },
    RegionCountMismatch {
        records: usize,
        index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EntityStoreSerde {
    polities: Vec<PolityRecord>,
    settlements: Vec<SettlementRecord>,
    regions: Vec<RegionRecord>,
}

impl Default for EntityStore {
    fn default() -> Self {
        Self {
            polities: SlotMap::with_key(),
            settlements: SlotMap::with_key(),
            regions: SlotMap::with_key(),
            polity_by_id: BTreeMap::new(),
            settlement_by_id: BTreeMap::new(),
            region_by_id: BTreeMap::new(),
        }
    }
}

impl PartialEq for EntityStore {
    fn eq(&self, other: &Self) -> bool {
        self.to_serde() == other.to_serde()
    }
}

impl EntityStore {
    pub fn create_polity(&mut self, record: PolityRecord) -> Result<PolityKey, EntityStoreError> {
        if self.polity_by_id.contains_key(&record.id) {
            return Err(EntityStoreError::DuplicatePolityId(record.id));
        }
        let polity_id = record.id;
        let key = self.polities.insert(record);
        self.polity_by_id.insert(polity_id, key);
        Ok(key)
    }

    pub fn get_polity(&self, id: PolityId) -> Option<&PolityRecord> {
        let key = self.polity_by_id.get(&id)?;
        self.polities.get(*key)
    }

    pub fn get_polity_mut(&mut self, id: PolityId) -> Option<&mut PolityRecord> {
        let key = *self.polity_by_id.get(&id)?;
        self.polities.get_mut(key)
    }

    pub fn remove_polity(&mut self, id: PolityId) -> Option<PolityRecord> {
        let key = self.polity_by_id.remove(&id)?;
        self.polities.remove(key)
    }

    pub fn iter_polities(&self) -> impl Iterator<Item = &PolityRecord> {
        self.polities.values()
    }

    pub fn create_settlement(
        &mut self,
        record: SettlementRecord,
    ) -> Result<SettlementKey, EntityStoreError> {
        if self.settlement_by_id.contains_key(&record.id) {
            return Err(EntityStoreError::DuplicateSettlementId(record.id));
        }
        let settlement_id = record.id;
        let key = self.settlements.insert(record);
        self.settlement_by_id.insert(settlement_id, key);
        Ok(key)
    }

    pub fn get_settlement(&self, id: SettlementId) -> Option<&SettlementRecord> {
        let key = self.settlement_by_id.get(&id)?;
        self.settlements.get(*key)
    }

    pub fn get_settlement_mut(&mut self, id: SettlementId) -> Option<&mut SettlementRecord> {
        let key = *self.settlement_by_id.get(&id)?;
        self.settlements.get_mut(key)
    }

    pub fn remove_settlement(&mut self, id: SettlementId) -> Option<SettlementRecord> {
        let key = self.settlement_by_id.remove(&id)?;
        self.settlements.remove(key)
    }

    pub fn iter_settlements(&self) -> impl Iterator<Item = &SettlementRecord> {
        self.settlements.values()
    }

    pub fn create_region(&mut self, record: RegionRecord) -> Result<RegionKey, EntityStoreError> {
        if self.region_by_id.contains_key(&record.id) {
            return Err(EntityStoreError::DuplicateRegionId(record.id));
        }
        let region_id = record.id;
        let key = self.regions.insert(record);
        self.region_by_id.insert(region_id, key);
        Ok(key)
    }

    pub fn get_region(&self, id: RegionId) -> Option<&RegionRecord> {
        let key = self.region_by_id.get(&id)?;
        self.regions.get(*key)
    }

    pub fn get_region_mut(&mut self, id: RegionId) -> Option<&mut RegionRecord> {
        let key = *self.region_by_id.get(&id)?;
        self.regions.get_mut(key)
    }

    pub fn remove_region(&mut self, id: RegionId) -> Option<RegionRecord> {
        let key = self.region_by_id.remove(&id)?;
        self.regions.remove(key)
    }

    pub fn iter_regions(&self) -> impl Iterator<Item = &RegionRecord> {
        self.regions.values()
    }

    pub fn validate(&self) -> Result<(), EntityStoreError> {
        if self.polities.len() != self.polity_by_id.len() {
            return Err(EntityStoreError::PolityCountMismatch {
                records: self.polities.len(),
                index: self.polity_by_id.len(),
            });
        }
        if self.settlements.len() != self.settlement_by_id.len() {
            return Err(EntityStoreError::SettlementCountMismatch {
                records: self.settlements.len(),
                index: self.settlement_by_id.len(),
            });
        }
        if self.regions.len() != self.region_by_id.len() {
            return Err(EntityStoreError::RegionCountMismatch {
                records: self.regions.len(),
                index: self.region_by_id.len(),
            });
        }

        for (id, key) in &self.polity_by_id {
            let Some(record) = self.polities.get(*key) else {
                return Err(EntityStoreError::DanglingPolityIndex(*id));
            };
            if record.id != *id {
                return Err(EntityStoreError::MismatchedPolityIndex {
                    expected: *id,
                    actual: record.id,
                });
            }
        }
        for record in self.polities.values() {
            if !self.polity_by_id.contains_key(&record.id) {
                return Err(EntityStoreError::MissingPolityIndex(record.id));
            }
        }

        for (id, key) in &self.settlement_by_id {
            let Some(record) = self.settlements.get(*key) else {
                return Err(EntityStoreError::DanglingSettlementIndex(*id));
            };
            if record.id != *id {
                return Err(EntityStoreError::MismatchedSettlementIndex {
                    expected: *id,
                    actual: record.id,
                });
            }
        }
        for record in self.settlements.values() {
            if !self.settlement_by_id.contains_key(&record.id) {
                return Err(EntityStoreError::MissingSettlementIndex(record.id));
            }
        }

        for (id, key) in &self.region_by_id {
            let Some(record) = self.regions.get(*key) else {
                return Err(EntityStoreError::DanglingRegionIndex(*id));
            };
            if record.id != *id {
                return Err(EntityStoreError::MismatchedRegionIndex {
                    expected: *id,
                    actual: record.id,
                });
            }
        }
        for record in self.regions.values() {
            if !self.region_by_id.contains_key(&record.id) {
                return Err(EntityStoreError::MissingRegionIndex(record.id));
            }
        }

        Ok(())
    }

    fn to_serde(&self) -> EntityStoreSerde {
        let mut polities = self.iter_polities().cloned().collect::<Vec<_>>();
        polities.sort_by_key(|record| record.id);

        let mut settlements = self.iter_settlements().cloned().collect::<Vec<_>>();
        settlements.sort_by_key(|record| record.id);

        let mut regions = self.iter_regions().cloned().collect::<Vec<_>>();
        regions.sort_by_key(|record| record.id);

        EntityStoreSerde {
            polities,
            settlements,
            regions,
        }
    }

    fn from_serde(value: EntityStoreSerde) -> Result<Self, EntityStoreError> {
        let mut store = Self::default();
        for record in value.polities {
            store.create_polity(record)?;
        }
        for record in value.settlements {
            store.create_settlement(record)?;
        }
        for record in value.regions {
            store.create_region(record)?;
        }
        store.validate()?;
        Ok(store)
    }
}

impl Serialize for EntityStore {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_serde().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EntityStore {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = EntityStoreSerde::deserialize(deserializer)?;
        Self::from_serde(value).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for EntityStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicatePolityId(id) => write!(f, "duplicate polity id {}", id.as_u32()),
            Self::DuplicateSettlementId(id) => {
                write!(f, "duplicate settlement id {}", id.as_u32())
            }
            Self::DuplicateRegionId(id) => write!(f, "duplicate region id {}", id.as_u32()),
            Self::DanglingPolityIndex(id) => write!(f, "dangling polity index {}", id.as_u32()),
            Self::DanglingSettlementIndex(id) => {
                write!(f, "dangling settlement index {}", id.as_u32())
            }
            Self::DanglingRegionIndex(id) => write!(f, "dangling region index {}", id.as_u32()),
            Self::MismatchedPolityIndex { expected, actual } => write!(
                f,
                "mismatched polity index: expected {}, actual {}",
                expected.as_u32(),
                actual.as_u32()
            ),
            Self::MismatchedSettlementIndex { expected, actual } => write!(
                f,
                "mismatched settlement index: expected {}, actual {}",
                expected.as_u32(),
                actual.as_u32()
            ),
            Self::MismatchedRegionIndex { expected, actual } => write!(
                f,
                "mismatched region index: expected {}, actual {}",
                expected.as_u32(),
                actual.as_u32()
            ),
            Self::MissingPolityIndex(id) => write!(f, "missing polity index {}", id.as_u32()),
            Self::MissingSettlementIndex(id) => {
                write!(f, "missing settlement index {}", id.as_u32())
            }
            Self::MissingRegionIndex(id) => write!(f, "missing region index {}", id.as_u32()),
            Self::PolityCountMismatch { records, index } => {
                write!(f, "polity count mismatch: records={records}, index={index}")
            }
            Self::SettlementCountMismatch { records, index } => {
                write!(
                    f,
                    "settlement count mismatch: records={records}, index={index}"
                )
            }
            Self::RegionCountMismatch { records, index } => {
                write!(f, "region count mismatch: records={records}, index={index}")
            }
        }
    }
}

impl std::error::Error for EntityStoreError {}

impl From<PolityComponent> for PolityRecord {
    fn from(value: PolityComponent) -> Self {
        Self {
            id: value.polity_id,
            capital_cell: value.capital_cell,
            legitimacy: value.legitimacy,
            centralization: value.centralization,
            military_tech: value.military_tech,
            cells_cache: value.cells_cache,
        }
    }
}

impl From<PolityRecord> for PolityComponent {
    fn from(value: PolityRecord) -> Self {
        Self {
            polity_id: value.id,
            capital_cell: value.capital_cell,
            legitimacy: value.legitimacy,
            centralization: value.centralization,
            military_tech: value.military_tech,
            cells_cache: value.cells_cache,
        }
    }
}

impl From<SettlementComponent> for SettlementRecord {
    fn from(value: SettlementComponent) -> Self {
        Self {
            id: value.settlement_id,
            cell: value.cell,
        }
    }
}

impl From<SettlementRecord> for SettlementComponent {
    fn from(value: SettlementRecord) -> Self {
        Self {
            settlement_id: value.id,
            cell: value.cell,
        }
    }
}

impl From<RegionComponent> for RegionRecord {
    fn from(value: RegionComponent) -> Self {
        Self {
            id: value.region_id,
            cells: value.cells,
        }
    }
}

impl From<RegionRecord> for RegionComponent {
    fn from(value: RegionRecord) -> Self {
        Self {
            region_id: value.id,
            cells: value.cells,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polity_record(id: u32, capital_cell: u32) -> PolityRecord {
        PolityRecord {
            id: PolityId(id),
            capital_cell: CellId(capital_cell),
            legitimacy: 0.7,
            centralization: 0.5,
            military_tech: 0.3,
            cells_cache: vec![CellId(capital_cell)],
        }
    }

    fn settlement_record(id: u32, cell: u32) -> SettlementRecord {
        SettlementRecord {
            id: SettlementId(id),
            cell: CellId(cell),
        }
    }

    fn region_record(id: u32, cells: &[u32]) -> RegionRecord {
        RegionRecord {
            id: RegionId(id),
            cells: cells.iter().copied().map(CellId).collect(),
        }
    }

    #[test]
    fn create_polity_rejects_duplicate_id() {
        let mut store = EntityStore::default();
        store.create_polity(polity_record(1, 10)).unwrap();

        let error = store.create_polity(polity_record(1, 11)).unwrap_err();
        assert_eq!(error, EntityStoreError::DuplicatePolityId(PolityId(1)));
    }

    #[test]
    fn remove_updates_secondary_indexes() {
        let mut store = EntityStore::default();
        store.create_polity(polity_record(1, 10)).unwrap();
        store.create_settlement(settlement_record(2, 20)).unwrap();
        store.create_region(region_record(3, &[1, 2, 3])).unwrap();

        assert!(store.remove_polity(PolityId(1)).is_some());
        assert!(store.remove_settlement(SettlementId(2)).is_some());
        assert!(store.remove_region(RegionId(3)).is_some());

        assert!(store.get_polity(PolityId(1)).is_none());
        assert!(store.get_settlement(SettlementId(2)).is_none());
        assert!(store.get_region(RegionId(3)).is_none());
        assert!(!store.polity_by_id.contains_key(&PolityId(1)));
        assert!(!store.settlement_by_id.contains_key(&SettlementId(2)));
        assert!(!store.region_by_id.contains_key(&RegionId(3)));
        assert!(store.validate().is_ok());
    }

    #[test]
    fn serde_round_trip_preserves_store() {
        let mut store = EntityStore::default();
        store.create_polity(polity_record(2, 4)).unwrap();
        store.create_polity(polity_record(1, 3)).unwrap();
        store.create_settlement(settlement_record(5, 8)).unwrap();
        store.create_region(region_record(7, &[0, 2, 4])).unwrap();

        let json = serde_json::to_string(&store).unwrap();
        let restored: EntityStore = serde_json::from_str(&json).unwrap();

        assert_eq!(restored, store);
        assert!(restored.validate().is_ok());
    }

    #[test]
    fn clone_preserves_store() {
        let mut store = EntityStore::default();
        store.create_polity(polity_record(1, 2)).unwrap();
        store.create_settlement(settlement_record(2, 3)).unwrap();
        store.create_region(region_record(3, &[4, 5])).unwrap();

        let cloned = store.clone();
        assert_eq!(cloned, store);
        assert!(cloned.validate().is_ok());
    }

    #[test]
    fn validate_detects_missing_index() {
        let mut store = EntityStore::default();
        store.create_polity(polity_record(1, 10)).unwrap();
        store.polity_by_id.clear();

        let error = store.validate().unwrap_err();
        assert_eq!(
            error,
            EntityStoreError::PolityCountMismatch {
                records: 1,
                index: 0,
            }
        );
    }
}

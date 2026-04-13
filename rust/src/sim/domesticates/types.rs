use crate::sim::world::{CropBitmap, LivestockBitmap, N_CROPS, N_LIVESTOCK};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CropKind {
    Wheat = 0,
    Rice = 1,
    Maize = 2,
    Millet = 3,
    Potato = 4,
    Cassava = 5,
    Sorghum = 6,
    Yam = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LivestockKind {
    Cattle = 0,
    Horse = 1,
    Sheep = 2,
    Pig = 3,
    Camel = 4,
}

pub const ALL_CROPS: [CropKind; N_CROPS] = [
    CropKind::Wheat,
    CropKind::Rice,
    CropKind::Maize,
    CropKind::Millet,
    CropKind::Potato,
    CropKind::Cassava,
    CropKind::Sorghum,
    CropKind::Yam,
];

pub const ALL_LIVESTOCK: [LivestockKind; N_LIVESTOCK] = [
    LivestockKind::Cattle,
    LivestockKind::Horse,
    LivestockKind::Sheep,
    LivestockKind::Pig,
    LivestockKind::Camel,
];

#[inline]
pub fn crop_index(kind: CropKind) -> usize {
    kind as usize
}

#[inline]
pub fn livestock_index(kind: LivestockKind) -> usize {
    kind as usize
}

#[inline]
pub fn crop_mask(kind: CropKind) -> CropBitmap {
    1u8 << (kind as u8)
}

#[inline]
pub fn livestock_mask(kind: LivestockKind) -> LivestockBitmap {
    1u8 << (kind as u8)
}

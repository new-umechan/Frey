pub mod types;

#[allow(unused_imports)]
pub use crate::sim::domesticates::types::*;

use crate::sim::world::{World, N_CROPS, N_LIVESTOCK};

pub(crate) fn update_domesticates(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let n = world.state.geology.height.len();
    for i in 0..n {
        if world.state.geology.height[i] <= 0.0 {
            world.state.domesticates.crop_available[i] = 0;
            world.state.domesticates.livestock_available[i] = 0;
            world.state.domesticates.crop_adoption[i] = [0.0; N_CROPS];
            world.state.domesticates.livestock_adoption[i] = [0.0; N_LIVESTOCK];
            continue;
        }
        let tree_cover = world.state.ecology.tree_cover[i].clamp(0.0, 1.0);
        let ground_cover = world.state.ecology.ground_cover[i].clamp(0.0, 1.0);
        let soil_fertility = world.state.ecology.soil_fertility[i].clamp(0.0, 1.0);
        let temp = world.state.climate.temperature[i];
        let precipitation = world.state.climate.precipitation[i];
        let vegetation_proxy =
            (tree_cover + 0.6 * ground_cover * (1.0 - tree_cover)).clamp(0.0, 1.0);
        let eco_suitability = (vegetation_proxy * 0.6 + soil_fertility * 0.4).clamp(0.0, 1.0);
        let crop_bitmap = crop_bitmap_for(temp, precipitation, eco_suitability);
        let livestock_bitmap = livestock_bitmap_for(temp, precipitation, eco_suitability);
        world.state.domesticates.crop_available[i] = crop_bitmap;
        world.state.domesticates.livestock_available[i] = livestock_bitmap;
        world.state.domesticates.crop_adoption[i] =
            adoption_from_bitmap::<N_CROPS>(crop_bitmap, eco_suitability, 1.0);
        world.state.domesticates.livestock_adoption[i] =
            adoption_from_bitmap::<N_LIVESTOCK>(livestock_bitmap, eco_suitability, 0.9);
    }
}

fn crop_bitmap_for(temperature: f32, precipitation: f32, suitability: f32) -> u8 {
    if suitability <= 0.25 {
        return 0;
    }
    let mut bits = 0u8;
    if temperature > -2.0 && precipitation > 260.0 {
        bits |= 1 << 0;
    }
    if temperature > 16.0 && precipitation > 650.0 {
        bits |= 1 << 1;
    }
    if temperature > 10.0 && precipitation > 420.0 {
        bits |= 1 << 2;
    }
    if temperature > 4.0 && precipitation > 220.0 {
        bits |= 1 << 3;
    }
    if temperature > 2.0 && precipitation > 280.0 {
        bits |= 1 << 4;
    }
    if temperature > 6.0 && precipitation > 240.0 {
        bits |= 1 << 5;
    }
    if temperature > 0.0 && precipitation > 200.0 {
        bits |= 1 << 6;
    }
    bits
}

fn livestock_bitmap_for(temperature: f32, precipitation: f32, suitability: f32) -> u8 {
    if suitability <= 0.20 {
        return 0;
    }
    let mut bits = 0u8;
    if temperature > -8.0 && precipitation > 180.0 {
        bits |= 1 << 0;
    }
    if temperature > -4.0 && precipitation > 140.0 {
        bits |= 1 << 1;
    }
    if temperature > -10.0 && precipitation > 120.0 {
        bits |= 1 << 2;
    }
    if temperature > -2.0 && precipitation > 220.0 {
        bits |= 1 << 3;
    }
    if temperature > 8.0 && precipitation < 500.0 {
        bits |= 1 << 4;
    }
    bits
}

fn adoption_from_bitmap<const N: usize>(bitmap: u8, suitability: f32, multiplier: f32) -> [f32; N] {
    let mut adoption = [0.0; N];
    let value = (suitability * multiplier).clamp(0.0, 1.0);
    for (idx, slot) in adoption.iter_mut().enumerate() {
        if (bitmap & (1u8 << idx)) != 0 {
            *slot = value;
        }
    }
    adoption
}

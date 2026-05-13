pub mod types;

#[allow(unused_imports)]
pub use crate::sim::ecology::types::*;

use crate::sim::world::{Biome, World};

const TREE_GROWTH_RATE: f32 = 0.14;
const TREE_DECLINE_RATE: f32 = 0.06;
const GROUND_GROWTH_RATE: f32 = 0.12;
const GROUND_DECLINE_RATE: f32 = 0.06;
const DISTURBANCE_UP_RATE: f32 = 0.18;
const DISTURBANCE_DOWN_RATE: f32 = 0.08;
const ALPINE_THRESHOLD: f32 = 0.68;
const TUNDRA_THRESHOLD: f32 = -3.0;
const DESERT_THRESHOLD: f32 = 180.0;
const WETLAND_THRESHOLD: f32 = 0.52;
const WETLAND_TREE_THRESHOLD: f32 = 0.5;
const TROPICAL_TEMP_THRESHOLD: f32 = 22.0;
const BOREAL_TEMP_THRESHOLD: f32 = 4.0;
const FOREST_THRESHOLD: f32 = 0.52;
const EROSION_FEEDBACK_KEY: &str = "erosion_loss";
const FLOOD_DEPOSITION_FEEDBACK_KEY: &str = "flood_deposition";
const LOGGING_FEEDBACK_KEY: &str = "logging";
const GRAZING_FEEDBACK_KEY: &str = "grazing";
const SLASH_BURN_FEEDBACK_KEY: &str = "slash_burn";
const SOIL_SLASH_BURN_DELTA_FEEDBACK_KEY: &str = "soil_slash_burn_delta";
const FARMING_CONSUMPTION_FEEDBACK_KEY: &str = "farming_consumption";
const POLLUTION_FEEDBACK_KEY: &str = "pollution";

pub(crate) fn run_ecology_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let dt = budget.max(1) as f32;
    let max_flux = world
        .state
        .hydrology
        .river_flow
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    let cell_count = world.state.geology.height.len();
    for i in 0..cell_count {
        let height = world.state.geology.height[i];
        if !world.is_land_cell(i) {
            world.state.ecology.tree_cover[i] = 0.0;
            world.state.ecology.ground_cover[i] = 0.0;
            world.state.ecology.disturbance[i] = 0.0;
            world.state.ecology.soil_fertility[i] = 0.0;
            world.state.ecology.biome[i] = Biome::Desert;
            continue;
        }

        let temp = world.state.climate.temperature[i];
        let precipitation = world.state.climate.precipitation[i];
        let pollution = feedback_value(world, POLLUTION_FEEDBACK_KEY, i).clamp(0.0, 1.0);
        let logging = feedback_value(world, LOGGING_FEEDBACK_KEY, i);
        let grazing = feedback_value(world, GRAZING_FEEDBACK_KEY, i);
        let slash_burn = feedback_value(world, SLASH_BURN_FEEDBACK_KEY, i);
        let farming_consumption = feedback_value(world, FARMING_CONSUMPTION_FEEDBACK_KEY, i);
        let erosion_loss = feedback_value(world, EROSION_FEEDBACK_KEY, i)
            .max(world.state.hydrology.erosion_rate[i].max(0.0) * 0.10);
        let flood_deposition = feedback_value(world, FLOOD_DEPOSITION_FEEDBACK_KEY, i);
        let slash_burn_delta = feedback_value(world, SOIL_SLASH_BURN_DELTA_FEEDBACK_KEY, i);
        let disturbance_target =
            (logging + grazing + slash_burn + pollution * 0.45).clamp(0.0, 1.0);
        let disturbance = converge_toward(
            world.state.ecology.disturbance[i],
            disturbance_target,
            DISTURBANCE_UP_RATE,
            DISTURBANCE_DOWN_RATE,
            dt,
        )
        .clamp(0.0, 1.0);

        let tree_potential = tree_cover_potential(temp, precipitation)
            * (1.0 - disturbance * 0.35)
            * (1.0 - pollution * 0.35);
        let tree_cover = (converge_toward(
            world.state.ecology.tree_cover[i],
            tree_potential,
            TREE_GROWTH_RATE,
            TREE_DECLINE_RATE,
            dt,
        ) - (logging + slash_burn) * dt)
            .clamp(0.0, 1.0);

        let ground_potential = ground_cover_potential(temp, precipitation)
            * (1.0 - disturbance * 0.20)
            * (1.0 - pollution * 0.20);
        let ground_cover = (converge_toward(
            world.state.ecology.ground_cover[i],
            ground_potential,
            GROUND_GROWTH_RATE,
            GROUND_DECLINE_RATE,
            dt,
        ) - (grazing + slash_burn) * dt)
            .clamp(0.0, 1.0);

        let natural_recovery = natural_recovery_rate(tree_cover, ground_cover, temp, precipitation)
            * dt
            * (1.0 - world.state.ecology.soil_fertility[i].clamp(0.0, 1.0));
        let soil_fertility = (world.state.ecology.soil_fertility[i].clamp(0.0, 1.0)
            + natural_recovery
            + flood_deposition * dt
            + slash_burn_delta * dt
            - farming_consumption * dt
            - erosion_loss * dt)
            .clamp(0.0, 1.0);

        world.state.ecology.tree_cover[i] = tree_cover;
        world.state.ecology.ground_cover[i] = ground_cover;
        world.state.ecology.disturbance[i] = disturbance;
        world.state.ecology.soil_fertility[i] = soil_fertility;
        world.state.ecology.biome[i] = classify_biome(
            tree_cover,
            ground_cover,
            temp,
            precipitation,
            world.state.hydrology.river_flow[i],
            height,
            max_flux,
        );
    }
}

fn feedback_value(world: &World, key: &str, index: usize) -> f32 {
    let _ = (world, key, index);
    0.0
}

fn converge_toward(current: f32, potential: f32, rate_up: f32, rate_down: f32, dt: f32) -> f32 {
    let rate = if potential > current {
        rate_up
    } else {
        rate_down
    };
    current + (potential - current) * rate * dt
}

fn tree_cover_potential(temperature: f32, precipitation: f32) -> f32 {
    let temp_factor = ((temperature + 8.0) / 34.0).clamp(0.0, 1.0);
    let precip_factor = ((precipitation - 120.0) / 1_280.0).clamp(0.0, 1.0);
    (temp_factor * precip_factor).clamp(0.0, 1.0)
}

fn ground_cover_potential(temperature: f32, precipitation: f32) -> f32 {
    let temp_factor = ((temperature + 18.0) / 42.0).clamp(0.0, 1.0);
    let precip_factor = ((precipitation + 40.0) / 1_440.0).clamp(0.0, 1.0);
    (precip_factor * 0.75 + temp_factor * 0.25).clamp(0.0, 1.0)
}

fn natural_recovery_rate(
    tree_cover: f32,
    ground_cover: f32,
    temperature: f32,
    precipitation: f32,
) -> f32 {
    let cover = (tree_cover + ground_cover * 0.65).clamp(0.0, 1.0);
    let temp_factor = ((temperature + 6.0) / 30.0).clamp(0.0, 1.0);
    let precip_factor = (precipitation / 1_600.0).clamp(0.0, 1.0);
    cover * temp_factor * precip_factor * 0.02
}

fn classify_biome(
    tree_cover: f32,
    _ground_cover: f32,
    temperature: f32,
    precipitation: f32,
    river_flow: f32,
    height: f32,
    max_flow: f32,
) -> Biome {
    let flooding = derive_flooding(river_flow, height, max_flow);
    if height > ALPINE_THRESHOLD {
        return Biome::Alpine;
    }
    if temperature < TUNDRA_THRESHOLD {
        return Biome::Tundra;
    }
    if precipitation < DESERT_THRESHOLD {
        return Biome::Desert;
    }
    if flooding > WETLAND_THRESHOLD && tree_cover < WETLAND_TREE_THRESHOLD {
        return Biome::Wetland;
    }
    if temperature > TROPICAL_TEMP_THRESHOLD {
        if tree_cover > FOREST_THRESHOLD {
            return Biome::TropicalForest;
        }
        return Biome::Savanna;
    }
    if temperature > BOREAL_TEMP_THRESHOLD {
        if tree_cover > FOREST_THRESHOLD {
            return Biome::TemperateForest;
        }
        return Biome::Grassland;
    }
    Biome::BorealForest
}

fn derive_flooding(river_flow: f32, height: f32, max_flow: f32) -> f32 {
    let normalized_flow = (river_flow / max_flow.max(1e-5)).clamp(0.0, 1.0);
    let lowland_factor = (1.0 - (height / ALPINE_THRESHOLD.max(1e-5))).clamp(0.0, 1.0);
    (normalized_flow * lowland_factor).clamp(0.0, 1.0)
}

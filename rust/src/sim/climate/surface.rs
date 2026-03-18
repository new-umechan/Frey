use std::f32::consts::{PI, TAU};

use crate::sim::exec::{blend_alpha, lerp};
use crate::sim::geo::{
    dot3, east_direction, edge_distance_km, normalize3, project_to_tangent, scale3, sub3,
};
use crate::sim::world::World;
use crate::sim::world::{CoastSide, EraKind};

const CLIMATE_BLEND_BASE: f32 = 0.32;
const LAPSE_RATE_C_PER_KM: f32 = 6.5;
const HEIGHT_TO_METERS: f32 = 6_000.0;
const PRECIP_MIN_MM: f32 = 25.0;
const PRECIP_MAX_MM: f32 = 4_000.0;
const DISTANCE_SCALE_KM: f32 = 1_500.0;
const CONTINENTALITY_GAIN: f32 = 0.4;
const K_OROGRAPHIC: f32 = 1.5;
const K_RAIN_SHADOW: f32 = 2.0;
const OROGRAPHIC_SCALE_M: f32 = 1_000.0;
const RAIN_SHADOW_SCALE_M: f32 = 1_500.0;
const COLD_COAST_GAIN: f32 = 0.8;
const EPS: f32 = 1e-3;

pub(crate) fn run_climate_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    let cell_count = world.state.geology.height.len();
    let alpha = blend_alpha(budget, CLIMATE_BLEND_BASE);
    let mut target_temperature = vec![0.0; cell_count];
    let mut target_precipitation = vec![0.0; cell_count];
    let mut target_ocean_temperature = vec![0.0; cell_count];
    let mut precip_factor = vec![1.0; cell_count];

    for i in 0..cell_count {
        let latitude_deg = world.state.geo.latitude_deg.get(i).copied().unwrap_or(0.0);
        let latitude_abs = latitude_deg.abs();
        let elevation_m = world.state.geology.height[i].max(0.0) * HEIGHT_TO_METERS;
        let temperature =
            base_land_temperature(latitude_deg) - LAPSE_RATE_C_PER_KM * elevation_m / 1_000.0;
        let mut precipitation = latitude_band_precipitation(latitude_abs);
        let wind_sign = prevailing_wind_sign(latitude_deg);
        let (windward_factor, leeward_factor) = orographic_factors(world, i, wind_sign);
        precipitation *= windward_factor * leeward_factor;
        precipitation *= continentality_factor(
            world
                .state
                .geo
                .distance_from_ocean_km
                .get(i)
                .copied()
                .unwrap_or(0.0),
        );
        precipitation = precipitation.clamp(PRECIP_MIN_MM, PRECIP_MAX_MM);

        let mut ocean_temperature = base_ocean_temperature(latitude_deg);
        if world.state.geo.is_coastal.get(i).copied().unwrap_or(false) {
            let coast_side = world
                .state
                .geo
                .coast_side
                .get(i)
                .copied()
                .unwrap_or(CoastSide::None);
            ocean_temperature += ocean_current_offset(latitude_abs, coast_side);
        }

        target_temperature[i] = temperature;
        target_precipitation[i] = precipitation;
        target_ocean_temperature[i] = ocean_temperature;
    }

    apply_cold_coast_precipitation(world, &target_ocean_temperature, &mut precip_factor);

    for i in 0..cell_count {
        let latitude_deg = world.state.geo.latitude_deg.get(i).copied().unwrap_or(0.0);
        let mut precipitation = target_precipitation[i] * precip_factor[i];
        precipitation = precipitation.clamp(PRECIP_MIN_MM, PRECIP_MAX_MM);
        let vegetation_density = vegetation_density_proxy(world, i);
        let pet = annual_pet_mm(target_temperature[i], latitude_deg);
        let evapotranspiration =
            actual_evapotranspiration_mm(precipitation, pet, vegetation_density);
        let runoff = (precipitation - evapotranspiration).max(0.0);
        let aridity = pet / precipitation.max(EPS);

        world.state.climate.temperature[i] = lerp(
            world.state.climate.temperature[i],
            target_temperature[i],
            alpha,
        );
        world.state.climate.precipitation[i] =
            lerp(world.state.climate.precipitation[i], precipitation, alpha);
        world.state.climate.evapotranspiration[i] = lerp(
            world.state.climate.evapotranspiration[i],
            evapotranspiration,
            alpha,
        );
        world.state.climate.runoff[i] = lerp(world.state.climate.runoff[i], runoff, alpha);
        world.state.climate.aridity[i] = lerp(world.state.climate.aridity[i], aridity, alpha);
        world.state.climate.ocean_temperature[i] = lerp(
            world.state.climate.ocean_temperature[i],
            target_ocean_temperature[i],
            alpha,
        );
    }
}

fn base_land_temperature(latitude_deg: f32) -> f32 {
    30.0 * latitude_deg.to_radians().cos() - 5.0
}

fn base_ocean_temperature(latitude_deg: f32) -> f32 {
    28.0 * latitude_deg.to_radians().cos() - 2.0
}

fn latitude_band_precipitation(latitude_abs: f32) -> f32 {
    match latitude_abs {
        x if x < 10.0 => 2_000.0,
        x if x < 30.0 => 300.0,
        x if x < 60.0 => 800.0,
        _ => 200.0,
    }
}

fn prevailing_wind_sign(latitude_deg: f32) -> f32 {
    let mut sign = match latitude_deg.abs() {
        x if x < 30.0 => -1.0,
        x if x < 60.0 => 1.0,
        _ => -1.0,
    };
    if latitude_deg < 0.0 {
        sign *= -1.0;
    }
    sign
}

fn orographic_factors(world: &World, index: usize, wind_sign: f32) -> (f32, f32) {
    let cell_count = world.state.geology.height.len();
    if index >= cell_count {
        return (1.0, 1.0);
    }
    let pos = world
        .mesh
        .positions
        .get(index)
        .copied()
        .unwrap_or([0.0, 0.0, 1.0]);
    let wind_vec = scale3(east_direction(pos), wind_sign);
    let start = world.mesh.nbr_offsets.get(index).copied().unwrap_or(0) as usize;
    let end = world
        .mesh
        .nbr_offsets
        .get(index + 1)
        .copied()
        .unwrap_or(start as u32) as usize;
    let current_elevation_m = world.state.geology.height[index].max(0.0) * HEIGHT_TO_METERS;
    let mut best_rise_m = 0.0;
    let mut best_rise_distance_km = 1.0;
    let mut best_drop_m: f32 = 0.0;

    for &n_u32 in world.mesh.nbrs.get(start..end).unwrap_or(&[]) {
        let n = n_u32 as usize;
        if n >= cell_count {
            continue;
        }
        let neighbor_pos = world
            .mesh
            .positions
            .get(n)
            .copied()
            .unwrap_or([0.0, 0.0, 1.0]);
        let dir = normalize3(project_to_tangent(sub3(neighbor_pos, pos), pos));
        let alignment = dot3(dir, wind_vec);
        let neighbor_elevation_m = world.state.geology.height[n].max(0.0) * HEIGHT_TO_METERS;
        let edge_distance_km = edge_distance_km(pos, neighbor_pos).max(1.0);
        if alignment < -0.15 {
            let rise_m = (current_elevation_m - neighbor_elevation_m).max(0.0);
            if rise_m > best_rise_m {
                best_rise_m = rise_m;
                best_rise_distance_km = edge_distance_km;
            }
        } else if alignment > 0.15 {
            let drop_m = (current_elevation_m - neighbor_elevation_m).max(0.0);
            best_drop_m = best_drop_m.max(drop_m);
        }
    }

    let slope_sin = sin_slope(best_rise_m, best_rise_distance_km * 1_000.0);
    let windward_factor = 1.0 + K_OROGRAPHIC * slope_sin;
    let normalized_descent = (best_drop_m / RAIN_SHADOW_SCALE_M).clamp(0.0, 2.5);
    let leeward_factor = (-K_RAIN_SHADOW * normalized_descent).exp();
    (windward_factor.max(0.4), leeward_factor.clamp(0.15, 1.0))
}

fn continentality_factor(distance_from_ocean_km: f32) -> f32 {
    let continentality = 1.0 - (-distance_from_ocean_km.max(0.0) / DISTANCE_SCALE_KM).exp();
    (1.0 - continentality * CONTINENTALITY_GAIN).clamp(0.35, 1.0)
}

fn ocean_current_offset(latitude_abs: f32, coast_side: CoastSide) -> f32 {
    match (latitude_abs, coast_side) {
        (_, CoastSide::None) => 0.0,
        (x, CoastSide::West) if x < 30.0 => 4.0,
        (x, CoastSide::East) if x < 30.0 => -6.0,
        (x, CoastSide::West) if x < 60.0 => -4.0,
        (x, CoastSide::East) if x < 60.0 => 4.0,
        (_, _) => -2.0,
    }
}

fn apply_cold_coast_precipitation(
    world: &World,
    ocean_temperature: &[f32],
    precip_factor: &mut [f32],
) {
    for i in 0..world.state.geology.height.len() {
        if world.state.geology.height[i] <= 0.0 {
            continue;
        }
        if !world.state.geo.is_coastal.get(i).copied().unwrap_or(false) {
            continue;
        }
        let latitude_deg = world.state.geo.latitude_deg.get(i).copied().unwrap_or(0.0);
        let mean_ocean_temperature = base_ocean_temperature(latitude_deg);
        let cold_anomaly = (mean_ocean_temperature
            - ocean_temperature
                .get(i)
                .copied()
                .unwrap_or(mean_ocean_temperature))
        .max(0.0);
        if cold_anomaly <= 0.0 {
            continue;
        }
        let cold_factor =
            1.0 - COLD_COAST_GAIN * cold_anomaly / mean_ocean_temperature.abs().max(1.0);
        let wind_sign = prevailing_wind_sign(latitude_deg);
        let mut current = i;
        for step in 0..4 {
            let attenuation = 1.0 - (step as f32) * 0.2;
            let step_factor = lerp(
                1.0,
                cold_factor.clamp(0.2, 1.0),
                attenuation.clamp(0.0, 1.0),
            );
            precip_factor[current] *= step_factor;
            let Some(next) = best_downwind_land_neighbor(world, current, wind_sign) else {
                break;
            };
            if next == current {
                break;
            }
            current = next;
        }
    }
}

fn best_downwind_land_neighbor(world: &World, index: usize, wind_sign: f32) -> Option<usize> {
    let pos = world
        .mesh
        .positions
        .get(index)
        .copied()
        .unwrap_or([0.0, 0.0, 1.0]);
    let wind_vec = scale3(east_direction(pos), wind_sign);
    let start = world.mesh.nbr_offsets.get(index).copied().unwrap_or(0) as usize;
    let end = world
        .mesh
        .nbr_offsets
        .get(index + 1)
        .copied()
        .unwrap_or(start as u32) as usize;
    let mut best = None::<(usize, f32)>;

    for &n_u32 in world.mesh.nbrs.get(start..end).unwrap_or(&[]) {
        let n = n_u32 as usize;
        if n >= world.state.geology.height.len() || world.state.geology.height[n] <= 0.0 {
            continue;
        }
        let neighbor_pos = world
            .mesh
            .positions
            .get(n)
            .copied()
            .unwrap_or([0.0, 0.0, 1.0]);
        let dir = normalize3(project_to_tangent(sub3(neighbor_pos, pos), pos));
        let alignment = dot3(dir, wind_vec);
        match best {
            Some((_, best_alignment)) if alignment <= best_alignment => {}
            _ if alignment > 0.15 => best = Some((n, alignment)),
            _ => {}
        }
    }

    best.map(|(n, _)| n)
}

fn vegetation_density_proxy(world: &World, index: usize) -> f32 {
    if world.exec.era == EraKind::Crust || world.exec.era == EraKind::Environment {
        return 0.5;
    }
    let tree_cover = world
        .state
        .ecology
        .tree_cover
        .get(index)
        .copied()
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let ground_cover = world
        .state
        .ecology
        .ground_cover
        .get(index)
        .copied()
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    (tree_cover + 0.6 * ground_cover * (1.0 - tree_cover)).clamp(0.0, 1.0)
}

fn annual_pet_mm(annual_temperature_c: f32, latitude_deg: f32) -> f32 {
    let monthly = monthly_temperatures(annual_temperature_c, latitude_deg);
    let heat_index = monthly
        .iter()
        .copied()
        .filter(|&temp| temp > 0.0)
        .map(|temp| (temp / 5.0).powf(1.514))
        .sum::<f32>();
    if heat_index <= EPS {
        return 0.0;
    }
    let alpha = 6.75e-7 * heat_index.powi(3) - 7.71e-5 * heat_index.powi(2)
        + 1.792e-2 * heat_index
        + 0.49239;
    monthly
        .iter()
        .copied()
        .filter(|&temp| temp > 0.0)
        .map(|temp| 16.0 * (10.0 * temp / heat_index).powf(alpha))
        .sum::<f32>()
}

fn monthly_temperatures(annual_temperature_c: f32, latitude_deg: f32) -> [f32; 12] {
    let amplitude = 3.0 + 17.0 * (latitude_deg.abs() / 90.0);
    let hemisphere_phase = if latitude_deg >= 0.0 { 0.0 } else { PI };
    let mut monthly = [0.0_f32; 12];
    for (month, slot) in monthly.iter_mut().enumerate() {
        let phase = TAU * (month as f32 / 12.0) - PI;
        *slot = annual_temperature_c + amplitude * (phase + hemisphere_phase).cos();
    }
    monthly
}

fn actual_evapotranspiration_mm(
    precipitation_mm: f32,
    pet_mm: f32,
    vegetation_density: f32,
) -> f32 {
    if precipitation_mm <= EPS || pet_mm <= 0.0 {
        return 0.0;
    }
    let phi = pet_mm / precipitation_mm.max(EPS);
    let w = 1.5 + 1.5 * vegetation_density.clamp(0.0, 1.0);
    let inner = (1.0 + phi.powf(-w)).powf(-1.0 / w);
    (precipitation_mm * (1.0 - inner)).clamp(0.0, precipitation_mm)
}

fn sin_slope(vertical_m: f32, horizontal_m: f32) -> f32 {
    if vertical_m <= 0.0 {
        return 0.0;
    }
    let adjusted_horizontal = horizontal_m.max(OROGRAPHIC_SCALE_M);
    vertical_m / (vertical_m.powi(2) + adjusted_horizontal.powi(2)).sqrt()
}

use std::f32::consts::{PI, TAU};

use crate::sim::exec::{blend_alpha, lerp};
use crate::sim::geo::{
    add3, dot3, east_direction, edge_distance_km, normalize3, project_to_tangent, scale3, sub3,
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
const ITCZ_WIDTH_DEG: f32 = 11.0;
const SUBTROPICAL_CENTER_DEG: f32 = 24.0;
const SUBTROPICAL_WIDTH_DEG: f32 = 10.0;
const MIDLAT_CENTER_DEG: f32 = 52.0;
const MIDLAT_WIDTH_DEG: f32 = 13.0;
const BASE_WIND_STRENGTH: f32 = 1.0;
const MOISTURE_SOURCE_BASE: f32 = 240.0;
const MOISTURE_SOURCE_OCEAN_GAIN: f32 = 1_250.0;
const MOISTURE_SOURCE_DISTANCE_KM: f32 = 1_200.0;
const MOISTURE_FLUX_GAIN: f32 = 0.95;
const MOISTURE_CONVERGENCE_GAIN: f32 = 35_000.0;
const CONVERGENCE_MIN_MM: f32 = -260.0;
const CONVERGENCE_MAX_MM: f32 = 480.0;

pub(crate) fn run_climate_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    let cell_count = world.state.geology.height.len();
    ensure_climate_field_lengths(world, cell_count);
    let alpha = blend_alpha(budget, CLIMATE_BLEND_BASE);
    let mut target_temperature = vec![0.0; cell_count];
    let mut target_precipitation = vec![0.0; cell_count];
    let mut target_ocean_temperature = vec![0.0; cell_count];
    let mut target_wind_u = vec![0.0; cell_count];
    let mut target_wind_v = vec![0.0; cell_count];
    let mut target_moisture_flux_u = vec![0.0; cell_count];
    let mut target_moisture_flux_v = vec![0.0; cell_count];
    let mut wind_vectors = vec![[0.0, 0.0, 0.0]; cell_count];
    let mut flux_vectors = vec![[0.0, 0.0, 0.0]; cell_count];
    let mut precip_factor = vec![1.0; cell_count];

    for i in 0..cell_count {
        let latitude = world.state.geo.latitude.get(i).copied().unwrap_or(0.0);
        let latitude_abs = latitude.abs();
        let elevation_m = world.state.geology.height[i].max(0.0) * HEIGHT_TO_METERS;
        let temperature =
            base_land_temperature(latitude) - LAPSE_RATE_C_PER_KM * elevation_m / 1_000.0;
        let mut ocean_temperature = base_ocean_temperature(latitude);
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
        let (wind_u, wind_v) = hadley_wind_components(latitude);
        let wind_vector = local_wind_vector(world, i, wind_u, wind_v);
        let moisture_source = moisture_source_mm(world, i, ocean_temperature);
        let wind_speed = (wind_u * wind_u + wind_v * wind_v).sqrt().max(0.2);
        let flux_magnitude = moisture_source * wind_speed * MOISTURE_FLUX_GAIN;

        target_temperature[i] = temperature;
        target_ocean_temperature[i] = ocean_temperature;
        target_wind_u[i] = wind_u;
        target_wind_v[i] = wind_v;
        wind_vectors[i] = wind_vector;
        target_moisture_flux_u[i] = flux_magnitude * wind_u;
        target_moisture_flux_v[i] = flux_magnitude * wind_v;
        flux_vectors[i] = scale3(wind_vector, flux_magnitude);
    }

    for i in 0..cell_count {
        let latitude = world.state.geo.latitude.get(i).copied().unwrap_or(0.0);
        let latitude_abs = latitude.abs();
        let baseline = latitude_band_precipitation_reference_mm(latitude_abs)
            + 0.35 * hadley_precipitation_anomaly_mm(latitude_abs);
        if world.state.geology.height[i] <= 0.0 {
            target_precipitation[i] = baseline.clamp(PRECIP_MIN_MM, PRECIP_MAX_MM);
            continue;
        }
        let (windward_factor, leeward_factor) = orographic_factors(world, i, wind_vectors[i]);
        let convergence = moisture_convergence_mm(world, i, &flux_vectors);
        let mut precipitation = baseline + 0.18 * convergence;
        precipitation *= windward_factor * leeward_factor;
        precipitation *= continentality_factor(
            world
                .state
                .geo
                .distance_from_ocean
                .get(i)
                .copied()
                .unwrap_or(0.0),
        );
        target_precipitation[i] = precipitation.clamp(PRECIP_MIN_MM, PRECIP_MAX_MM);
    }

    apply_cold_coast_precipitation(world, &target_ocean_temperature, &mut precip_factor);

    for i in 0..cell_count {
        let latitude = world.state.geo.latitude.get(i).copied().unwrap_or(0.0);
        let mut precipitation = target_precipitation[i] * precip_factor[i];
        precipitation = precipitation.clamp(PRECIP_MIN_MM, PRECIP_MAX_MM);
        let vegetation_density = vegetation_density_proxy(world, i);
        let pet = annual_pet_mm(target_temperature[i], latitude);
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
        world.state.climate.wind_u[i] =
            lerp(world.state.climate.wind_u[i], target_wind_u[i], alpha);
        world.state.climate.wind_v[i] =
            lerp(world.state.climate.wind_v[i], target_wind_v[i], alpha);
        world.state.climate.moisture_flux_u[i] = lerp(
            world.state.climate.moisture_flux_u[i],
            target_moisture_flux_u[i],
            alpha,
        );
        world.state.climate.moisture_flux_v[i] = lerp(
            world.state.climate.moisture_flux_v[i],
            target_moisture_flux_v[i],
            alpha,
        );
    }
}

fn ensure_climate_field_lengths(world: &mut World, cell_count: usize) {
    if world.state.climate.wind_u.len() != cell_count {
        world.state.climate.wind_u.resize(cell_count, 0.0);
    }
    if world.state.climate.wind_v.len() != cell_count {
        world.state.climate.wind_v.resize(cell_count, 0.0);
    }
    if world.state.climate.moisture_flux_u.len() != cell_count {
        world.state.climate.moisture_flux_u.resize(cell_count, 0.0);
    }
    if world.state.climate.moisture_flux_v.len() != cell_count {
        world.state.climate.moisture_flux_v.resize(cell_count, 0.0);
    }
}

fn base_land_temperature(latitude: f32) -> f32 {
    30.0 * latitude.to_radians().cos() - 5.0
}

fn base_ocean_temperature(latitude: f32) -> f32 {
    28.0 * latitude.to_radians().cos() - 2.0
}

fn hadley_precipitation_anomaly_mm(latitude_abs: f32) -> f32 {
    let itcz = gaussian(latitude_abs, 0.0, ITCZ_WIDTH_DEG);
    let subtropical_sink = gaussian(latitude_abs, SUBTROPICAL_CENTER_DEG, SUBTROPICAL_WIDTH_DEG);
    let midlat_storm = gaussian(latitude_abs, MIDLAT_CENTER_DEG, MIDLAT_WIDTH_DEG);
    900.0 * itcz + 380.0 * midlat_storm - 560.0 * subtropical_sink
}

fn latitude_band_precipitation_reference_mm(latitude_abs: f32) -> f32 {
    match latitude_abs {
        x if x < 10.0 => 2_000.0,
        x if x < 30.0 => 300.0,
        x if x < 60.0 => 800.0,
        _ => 200.0,
    }
}

fn hadley_wind_components(latitude: f32) -> (f32, f32) {
    let abs_lat = latitude.abs();
    let hemisphere_sign = latitude.signum();
    let trade = 1.0 - smoothstep(18.0, 36.0, abs_lat);
    let westerly = smoothstep(24.0, 48.0, abs_lat) * (1.0 - smoothstep(56.0, 74.0, abs_lat));
    let polar_easterly = smoothstep(60.0, 80.0, abs_lat);
    let zonal = BASE_WIND_STRENGTH * (-0.9 * trade + 0.85 * westerly - 0.55 * polar_easterly);
    let hadley = 1.0 - smoothstep(5.0, 32.0, abs_lat);
    let ferrel = smoothstep(28.0, 44.0, abs_lat) * (1.0 - smoothstep(54.0, 68.0, abs_lat));
    let polar = smoothstep(62.0, 78.0, abs_lat);
    let meridional = BASE_WIND_STRENGTH
        * (-0.45 * hemisphere_sign * hadley + 0.26 * hemisphere_sign * ferrel
            - 0.20 * hemisphere_sign * polar);
    (zonal, meridional)
}

fn local_wind_vector(world: &World, index: usize, wind_u: f32, wind_v: f32) -> [f32; 3] {
    let pos = world
        .mesh
        .positions
        .get(index)
        .copied()
        .unwrap_or([0.0, 0.0, 1.0]);
    let east = east_direction(pos);
    let north = local_north_direction(pos);
    let wind = add3(scale3(east, wind_u), scale3(north, wind_v));
    let normed = normalize3(wind);
    if dot3(normed, normed) > 0.0 {
        normed
    } else {
        east
    }
}

fn local_north_direction(pos: [f32; 3]) -> [f32; 3] {
    let primary = normalize3(project_to_tangent([0.0, 1.0, 0.0], pos));
    if dot3(primary, primary) > 0.0 {
        primary
    } else {
        normalize3(project_to_tangent([0.0, 0.0, 1.0], pos))
    }
}

fn moisture_source_mm(world: &World, index: usize, ocean_temperature: f32) -> f32 {
    let distance_from_ocean = world
        .state
        .geo
        .distance_from_ocean
        .get(index)
        .copied()
        .unwrap_or(0.0)
        .max(0.0);
    let is_ocean = world
        .state
        .geology
        .height
        .get(index)
        .copied()
        .unwrap_or(0.0)
        <= 0.0;
    let oceanity = if is_ocean {
        1.0
    } else {
        (-distance_from_ocean / MOISTURE_SOURCE_DISTANCE_KM).exp()
    };
    let warm_ocean_bonus = ((ocean_temperature + 2.0) / 30.0).clamp(0.0, 1.0);
    MOISTURE_SOURCE_BASE + MOISTURE_SOURCE_OCEAN_GAIN * oceanity * (0.75 + 0.25 * warm_ocean_bonus)
}

fn moisture_convergence_mm(world: &World, index: usize, flux_vectors: &[[f32; 3]]) -> f32 {
    let pos = world
        .mesh
        .positions
        .get(index)
        .copied()
        .unwrap_or([0.0, 0.0, 1.0]);
    let flux_i = flux_vectors.get(index).copied().unwrap_or([0.0, 0.0, 0.0]);
    let start = world.mesh.nbr_offsets.get(index).copied().unwrap_or(0) as usize;
    let end = world
        .mesh
        .nbr_offsets
        .get(index + 1)
        .copied()
        .unwrap_or(start as u32) as usize;
    let mut divergence = 0.0;
    let mut weight_sum = 0.0;

    for &n_u32 in world.mesh.nbrs.get(start..end).unwrap_or(&[]) {
        let n = n_u32 as usize;
        if n >= world.state.geology.height.len() {
            continue;
        }
        let neighbor_pos = world
            .mesh
            .positions
            .get(n)
            .copied()
            .unwrap_or([0.0, 0.0, 1.0]);
        let dir = normalize3(project_to_tangent(sub3(neighbor_pos, pos), pos));
        let edge_km = edge_distance_km(pos, neighbor_pos).max(1.0);
        let flux_n = flux_vectors.get(n).copied().unwrap_or([0.0, 0.0, 0.0]);
        let along_edge = dot3(sub3(flux_n, flux_i), dir);
        let weight = 1.0 / edge_km;
        divergence += along_edge * weight;
        weight_sum += weight;
    }

    if weight_sum <= EPS {
        return 0.0;
    }
    let normalized_divergence = divergence / weight_sum;
    (-normalized_divergence * MOISTURE_CONVERGENCE_GAIN)
        .clamp(CONVERGENCE_MIN_MM, CONVERGENCE_MAX_MM)
}

fn orographic_factors(world: &World, index: usize, wind_vec: [f32; 3]) -> (f32, f32) {
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

fn gaussian(value: f32, center: f32, width: f32) -> f32 {
    let sigma = width.max(1.0);
    (-(value - center).powi(2) / (2.0 * sigma * sigma)).exp()
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(EPS)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn continentality_factor(distance_from_ocean: f32) -> f32 {
    let continentality = 1.0 - (-distance_from_ocean.max(0.0) / DISTANCE_SCALE_KM).exp();
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
        let latitude = world.state.geo.latitude.get(i).copied().unwrap_or(0.0);
        let mean_ocean_temperature = base_ocean_temperature(latitude);
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
        let (wind_u, wind_v) = hadley_wind_components(latitude);
        let mut current = i;
        for step in 0..4 {
            let attenuation = 1.0 - (step as f32) * 0.2;
            let step_factor = lerp(
                1.0,
                cold_factor.clamp(0.2, 1.0),
                attenuation.clamp(0.0, 1.0),
            );
            precip_factor[current] *= step_factor;
            let wind_vector = local_wind_vector(world, current, wind_u, wind_v);
            let Some(next) = best_downwind_land_neighbor(world, current, wind_vector) else {
                break;
            };
            if next == current {
                break;
            }
            current = next;
        }
    }
}

fn best_downwind_land_neighbor(world: &World, index: usize, wind_vec: [f32; 3]) -> Option<usize> {
    let pos = world
        .mesh
        .positions
        .get(index)
        .copied()
        .unwrap_or([0.0, 0.0, 1.0]);
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
    if world.clock.epoch == EraKind::Crust || world.clock.epoch == EraKind::Environment {
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

fn annual_pet_mm(annual_temperature_c: f32, latitude: f32) -> f32 {
    let monthly = monthly_temperatures(annual_temperature_c, latitude);
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

fn monthly_temperatures(annual_temperature_c: f32, latitude: f32) -> [f32; 12] {
    let amplitude = 3.0 + 17.0 * (latitude.abs() / 90.0);
    let hemisphere_phase = if latitude >= 0.0 { 0.0 } else { PI };
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

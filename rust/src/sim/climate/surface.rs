use std::f32::consts::{PI, TAU};

use crate::sim::climate::types::ClimateParams;
use crate::sim::exec::{blend_alpha, lerp};
use crate::sim::geo::{
    add3, dot3, east_direction, edge_distance_km, normalize3, project_to_tangent, scale3, sub3,
};
use crate::sim::world::World;
use crate::sim::world::{CoastSide, EraKind};

const CLIMATE_BLEND_BASE: f32 = 0.32;
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
const DOWNWIND_DEPLETION_PASSES: usize = 2;

#[derive(Debug, Clone, Copy, Default)]
struct OrographicSignal {
    rise_m: f32,
    barrier_m: f32,
    barrier_distance_km: f32,
    ocean_fetch: f32,
}

pub(crate) fn run_climate_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    let cell_count = world.state.geology.height.len();
    let climate_params = ClimateParams::default();
    ensure_climate_field_lengths(world, cell_count);
    let alpha = blend_alpha(budget, CLIMATE_BLEND_BASE);
    let mut target_temperature = vec![0.0; cell_count];
    let mut target_precipitation = vec![0.0; cell_count];
    let mut target_ocean_temperature = vec![0.0; cell_count];
    let mut target_wind_u = vec![0.0; cell_count];
    let mut target_wind_v = vec![0.0; cell_count];
    let mut target_moisture_flux_u = vec![0.0; cell_count];
    let mut target_moisture_flux_v = vec![0.0; cell_count];
    let mut target_moisture_source = vec![0.0; cell_count];
    let mut wind_vectors = vec![[0.0, 0.0, 0.0]; cell_count];
    let mut flux_vectors = vec![[0.0, 0.0, 0.0]; cell_count];
    let mut precip_factor = vec![1.0; cell_count];

    for i in 0..cell_count {
        let latitude = world.state.geo.latitude.get(i).copied().unwrap_or(0.0);
        let latitude_abs = latitude.abs();
        let elevation_m = world.state.geology.height[i].max(0.0) * climate_params.height_to_meters;
        let temperature = base_land_temperature(latitude)
            - climate_params.lapse_rate_c_per_km * elevation_m / 1_000.0;
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
        target_moisture_source[i] = moisture_source;
        wind_vectors[i] = wind_vector;
        target_moisture_flux_u[i] = flux_magnitude * wind_u;
        target_moisture_flux_v[i] = flux_magnitude * wind_v;
        flux_vectors[i] = scale3(wind_vector, flux_magnitude);
    }

    for i in 0..cell_count {
        let latitude = world.state.geo.latitude.get(i).copied().unwrap_or(0.0);
        let latitude_abs = latitude.abs();
        let baseline = latitude_band_precipitation_reference_mm(latitude_abs)
            + climate_params.hadley_anomaly_gain * hadley_precipitation_anomaly_mm(latitude_abs);
        if world.state.geology.height[i] <= 0.0 {
            target_precipitation[i] =
                baseline.clamp(climate_params.precip_min_mm, climate_params.precip_max_mm);
            continue;
        }
        let convergence = moisture_convergence_mm(world, i, &flux_vectors, &climate_params);
        let signal = orographic_signal(world, i, wind_vectors[i], &climate_params);
        let rise_norm =
            (signal.rise_m / climate_params.orographic_rise_scale_m.max(1.0)).clamp(0.0, 3.0);
        let fetch_factor = (0.70 + 0.60 * signal.ocean_fetch).clamp(0.5, 1.3);
        let uplift_mm = climate_params.orographic_uplift_gain_mm * rise_norm * fetch_factor;
        let barrier_norm =
            (signal.barrier_m / climate_params.rain_shadow_scale_m.max(1.0)).clamp(0.0, 3.0);
        let distance_decay = if signal.barrier_distance_km > 0.0 {
            (-signal.barrier_distance_km / climate_params.rain_shadow_distance_km.max(1.0)).exp()
        } else {
            0.0
        };
        let shadow_factor = (-climate_params.rain_shadow_gain * barrier_norm * distance_decay)
            .exp()
            .clamp(0.20, 1.0);
        let mut precipitation =
            baseline + climate_params.convergence_blend * convergence + uplift_mm;
        precipitation *= shadow_factor;
        precipitation *= continentality_factor(
            world
                .state
                .geo
                .distance_from_ocean
                .get(i)
                .copied()
                .unwrap_or(0.0),
            &climate_params,
        );
        let moisture_cap =
            climate_params.precip_cap_from_moisture * target_moisture_source[i].max(0.0);
        precipitation = precipitation.min(moisture_cap.max(climate_params.precip_min_mm));
        target_precipitation[i] =
            precipitation.clamp(climate_params.precip_min_mm, climate_params.precip_max_mm);
    }

    apply_downwind_moisture_depletion_iterative(
        world,
        &wind_vectors,
        &target_moisture_source,
        &mut target_precipitation,
        &climate_params,
    );

    apply_cold_coast_precipitation(
        world,
        &target_ocean_temperature,
        &mut precip_factor,
        &climate_params,
    );

    for i in 0..cell_count {
        let latitude = world.state.geo.latitude.get(i).copied().unwrap_or(0.0);
        let mut precipitation = target_precipitation[i] * precip_factor[i];
        precipitation =
            precipitation.clamp(climate_params.precip_min_mm, climate_params.precip_max_mm);
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

fn moisture_convergence_mm(
    world: &World,
    index: usize,
    flux_vectors: &[[f32; 3]],
    params: &ClimateParams,
) -> f32 {
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
    (-normalized_divergence * params.moisture_convergence_gain)
        .clamp(params.convergence_min_mm, params.convergence_max_mm)
}

fn orographic_signal(
    world: &World,
    index: usize,
    wind_vec: [f32; 3],
    params: &ClimateParams,
) -> OrographicSignal {
    let cell_count = world.state.geology.height.len();
    if index >= cell_count {
        return OrographicSignal::default();
    }
    let mut rise_m = 0.0;
    let mut barrier_m = 0.0;
    let mut barrier_distance_km = 0.0;
    let mut ocean_fetch_weight = 0.0;
    let mut fetch_weight_sum = 0.0;
    let mut traveled_km = 0.0;
    let mut cursor = index;
    let mut cursor_elevation_m =
        world.state.geology.height[index].max(0.0) * params.height_to_meters;
    let base_elevation_m = cursor_elevation_m;
    let upwind_vec = normalize3(scale3(wind_vec, -1.0));

    for step in 0..params.orographic_trace_steps.max(1) {
        let Some((next, edge_km, _alignment)) = best_neighbor_toward(
            world,
            cursor,
            upwind_vec,
            params.orographic_trace_alignment_min,
            false,
        ) else {
            break;
        };
        let next_elevation_m = world.state.geology.height[next].max(0.0) * params.height_to_meters;
        let step_weight = params
            .orographic_step_decay
            .clamp(0.0, 1.0)
            .powi(step as i32);
        rise_m += (cursor_elevation_m - next_elevation_m).max(0.0) * step_weight;
        fetch_weight_sum += step_weight;
        if world.state.geology.height[next] <= 0.0 {
            ocean_fetch_weight += step_weight;
        }
        traveled_km += edge_km;
        let barrier_here = (next_elevation_m - base_elevation_m).max(0.0);
        if barrier_here > barrier_m {
            barrier_m = barrier_here;
            barrier_distance_km = traveled_km;
        }
        cursor = next;
        cursor_elevation_m = next_elevation_m;
    }
    let ocean_fetch = if fetch_weight_sum > 0.0 {
        (ocean_fetch_weight / fetch_weight_sum).clamp(0.0, 1.0)
    } else {
        0.0
    };

    OrographicSignal {
        rise_m,
        barrier_m,
        barrier_distance_km,
        ocean_fetch,
    }
}

fn gaussian(value: f32, center: f32, width: f32) -> f32 {
    let sigma = width.max(1.0);
    (-(value - center).powi(2) / (2.0 * sigma * sigma)).exp()
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(EPS)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn continentality_factor(distance_from_ocean: f32, params: &ClimateParams) -> f32 {
    let continentality =
        1.0 - (-distance_from_ocean.max(0.0) / params.distance_scale_km.max(1.0)).exp();
    (1.0 - continentality * params.continentality_gain).clamp(0.35, 1.0)
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
    params: &ClimateParams,
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
            1.0 - params.cold_coast_gain * cold_anomaly / mean_ocean_temperature.abs().max(1.0);
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
            let Some((next, _, _)) = best_neighbor_toward(world, current, wind_vector, 0.15, true)
            else {
                break;
            };
            if next == current {
                break;
            }
            current = next;
        }
    }
}

fn apply_downwind_moisture_depletion(
    world: &World,
    wind_vectors: &[[f32; 3]],
    moisture_source: &[f32],
    precipitation: &[f32],
    params: &ClimateParams,
) -> Vec<f32> {
    let cell_count = world.state.geology.height.len();
    if wind_vectors.len() != cell_count
        || moisture_source.len() != cell_count
        || precipitation.len() != cell_count
    {
        return vec![0.0_f32; cell_count];
    }
    let mut depletion = vec![0.0_f32; cell_count];
    let step_count = params.downwind_depletion_steps.max(1);
    for i in 0..cell_count {
        if world.state.geology.height[i] <= 0.0 {
            continue;
        }
        let local_source = moisture_source[i].max(1.0);
        let local_cap = (params.precip_cap_from_moisture * local_source).max(params.precip_min_mm);
        let local_span = (local_cap - params.precip_min_mm).max(1.0);
        let precip_norm =
            ((precipitation[i] - params.precip_min_mm).max(0.0) / local_span).clamp(0.0, 1.0);
        let source_weight = (local_source / (local_source + 300.0)).clamp(0.0, 1.0);
        let mut carry = params.downwind_depletion_gain * precip_norm * source_weight;
        if carry <= 0.0 {
            continue;
        }
        let mut current = i;
        for step in 0..step_count {
            let current_wind = wind_vectors
                .get(current)
                .copied()
                .unwrap_or([0.0, 0.0, 0.0]);
            let Some((next, _, _)) = best_neighbor_toward(
                world,
                current,
                current_wind,
                params.downwind_alignment_min,
                true,
            ) else {
                break;
            };
            if next == current {
                break;
            }
            let attenuation = params
                .downwind_depletion_decay
                .clamp(0.0, 1.0)
                .powi(step as i32);
            depletion[next] += carry * attenuation;
            carry *= 0.92;
            if carry < 1e-4 {
                break;
            }
            current = next;
        }
    }
    depletion
}

fn apply_downwind_moisture_depletion_iterative(
    world: &World,
    wind_vectors: &[[f32; 3]],
    moisture_source: &[f32],
    precipitation: &mut [f32],
    params: &ClimateParams,
) {
    let cell_count = world.state.geology.height.len();
    if precipitation.len() != cell_count || moisture_source.len() != cell_count {
        return;
    }

    for _ in 0..DOWNWIND_DEPLETION_PASSES {
        let depletion =
            apply_downwind_moisture_depletion(
                world,
                wind_vectors,
                moisture_source,
                precipitation,
                params,
            );

        for i in 0..cell_count {
            if world.state.geology.height[i] <= 0.0 {
                continue;
            }
            let reduction = depletion[i].clamp(0.0, params.downwind_depletion_max.clamp(0.0, 0.95));
            if reduction > 0.0 {
                precipitation[i] *= 1.0 - reduction;
            }
            let moisture_cap = params.precip_cap_from_moisture * moisture_source[i].max(0.0);
            precipitation[i] = precipitation[i]
                .min(moisture_cap.max(params.precip_min_mm))
                .clamp(params.precip_min_mm, params.precip_max_mm);
        }
    }
}

fn best_neighbor_toward(
    world: &World,
    index: usize,
    direction_vec: [f32; 3],
    min_alignment: f32,
    land_only: bool,
) -> Option<(usize, f32, f32)> {
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
    let mut best = None::<(usize, f32, f32)>;

    for &n_u32 in world.mesh.nbrs.get(start..end).unwrap_or(&[]) {
        let n = n_u32 as usize;
        if n >= world.state.geology.height.len() {
            continue;
        }
        if land_only && world.state.geology.height[n] <= 0.0 {
            continue;
        }
        let neighbor_pos = world
            .mesh
            .positions
            .get(n)
            .copied()
            .unwrap_or([0.0, 0.0, 1.0]);
        let dir = normalize3(project_to_tangent(sub3(neighbor_pos, pos), pos));
        let alignment = dot3(dir, direction_vec);
        let edge_km = edge_distance_km(pos, neighbor_pos).max(1.0);
        match best {
            Some((_, _, best_alignment)) if alignment <= best_alignment => {}
            _ if alignment > min_alignment => best = Some((n, edge_km, alignment)),
            _ => {}
        }
    }

    best
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

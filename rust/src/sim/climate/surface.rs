use std::f32::consts::{PI, TAU};
use std::sync::{Mutex, OnceLock};

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
const LAT_BASE_MM: f32 = 500.0;
const LAT_ITCZ_GAIN_MM: f32 = 1850.000000;
const LAT_MIDLAT_GAIN_MM: f32 = 430.0;
const LAT_SUBTROPICAL_DRY_GAIN_MM: f32 = 680.000000;
const LAT_POLAR_DRY_GAIN_MM: f32 = 260.0;
const LAT_MIN_MM: f32 = 140.0;
const MONSOON_GAIN_MM: f32 = 760.000000;
const MONSOON_DISTANCE_KM: f32 = 1_400.0;
const MONSOON_LAT_CENTER_DEG: f32 = 18.0;
const MONSOON_LAT_WIDTH_DEG: f32 = 16.0;
const CONTINENTAL_RELAX_CONVERGENCE_WEIGHT: f32 = 0.45;
const CONTINENTAL_RELAX_UPLIFT_WEIGHT: f32 = 0.35;
const CONTINENTAL_RELAX_MONSOON_WEIGHT: f32 = 0.25;
const CONTINENTAL_RELAX_MAX: f32 = 0.780000;
const CAP_DYNAMIC_CONVERGENCE_WEIGHT: f32 = 0.50;
const CAP_DYNAMIC_UPLIFT_WEIGHT: f32 = 0.60;
const CAP_DYNAMIC_MONSOON_WEIGHT: f32 = 0.40;
const CAP_DYNAMIC_FETCH_WEIGHT: f32 = 0.25;
const CAP_DYNAMIC_MAX: f32 = 7.400000;
const COLD_RELAX_CONVERGENCE_WEIGHT: f32 = 0.40;
const COLD_RELAX_UPLIFT_WEIGHT: f32 = 0.35;
const COLD_RELAX_MONSOON_WEIGHT: f32 = 0.25;
const COLD_RELAX_MAX: f32 = 0.600000;

#[derive(Debug, Clone, Copy, Default)]
pub struct PrecipDiagnosticsSummary {
    pub continental_reduction_ratio: f32,
    pub cap_reduction_ratio: f32,
    pub depletion_reduction_ratio: f32,
    pub cold_coast_reduction_ratio: f32,
    pub cap_hit_ratio: f32,
    pub mean_monsoon_boost_mm: f32,
    pub mean_hotspot_boost_mm: f32,
    pub mean_stage_source_mm: f32,
    pub mean_stage_transport_mm: f32,
    pub mean_stage_orographic_mm: f32,
    pub mean_stage_correction_factor: f32,
    pub mean_budget_storage_change_mm: f32,
    pub mean_budget_residual_mm: f32,
    pub budget_residual_ratio: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct OrographicSignal {
    rise_m: f32,
    barrier_m: f32,
    barrier_distance_km: f32,
    ocean_fetch: f32,
}

#[derive(Debug, Clone, Copy)]
struct NeighborSample {
    index: usize,
    dir: [f32; 3],
    edge_km: f32,
    is_land: bool,
}

#[derive(Debug, Clone)]
struct NeighborLookup {
    offsets: Vec<usize>,
    entries: Vec<NeighborSample>,
}

#[derive(Debug, Clone)]
struct BaseClimateFields {
    target_temperature: Vec<f32>,
    target_ocean_temperature: Vec<f32>,
    target_wind_u: Vec<f32>,
    target_wind_v: Vec<f32>,
    target_moisture_flux_u: Vec<f32>,
    target_moisture_flux_v: Vec<f32>,
    target_moisture_source: Vec<f32>,
    wind_vectors: Vec<[f32; 3]>,
    flux_vectors: Vec<[f32; 3]>,
}

#[derive(Debug, Clone)]
struct PrecipitationFields {
    target_precipitation: Vec<f32>,
    precip_factor: Vec<f32>,
    convergence_field: Vec<f32>,
    uplift_field: Vec<f32>,
    monsoon_field: Vec<f32>,
    hotspot_field: Vec<f32>,
    continental_pre_sum: f32,
    continental_post_sum: f32,
    cap_pre_sum: f32,
    cap_post_sum: f32,
    depletion_pre_sum: f32,
    depletion_post_sum: f32,
    depletion_source_pre_sum: f32,
    depletion_budget_storage_change_sum: f32,
    depletion_budget_residual_sum: f32,
    cap_hits: usize,
    land_cells: usize,
    monsoon_sum: f32,
    hotspot_sum: f32,
    source_sum: f32,
    transport_sum: f32,
    orographic_sum: f32,
    correction_factor_sum: f32,
}

static LAST_PRECIP_DIAGNOSTICS: OnceLock<Mutex<PrecipDiagnosticsSummary>> = OnceLock::new();

fn diagnostics_store() -> &'static Mutex<PrecipDiagnosticsSummary> {
    LAST_PRECIP_DIAGNOSTICS.get_or_init(|| Mutex::new(PrecipDiagnosticsSummary::default()))
}

fn set_last_precip_diagnostics(summary: PrecipDiagnosticsSummary) {
    let lock = diagnostics_store();
    match lock.lock() {
        Ok(mut guard) => {
            *guard = summary;
        }
        Err(poisoned) => {
            let mut guard = poisoned.into_inner();
            *guard = summary;
        }
    }
}

pub fn last_precip_diagnostics_summary() -> PrecipDiagnosticsSummary {
    let lock = diagnostics_store();
    match lock.lock() {
        Ok(guard) => *guard,
        Err(poisoned) => *poisoned.into_inner(),
    }
}

pub(crate) fn run_climate_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    let cell_count = world.state.geology.height.len();
    let climate_params = ClimateParams::default();
    ensure_climate_field_lengths(world, cell_count);
    let alpha = blend_alpha(budget, CLIMATE_BLEND_BASE);
    let base_fields = compute_base_climate_fields(world, &climate_params, cell_count);
    let neighbor_lookup = build_neighbor_lookup(world);
    let mut precipitation_fields =
        compute_precipitation_fields(
            world,
            &base_fields,
            &neighbor_lookup,
            &climate_params,
            cell_count,
        );

    let cold_input_sum = sum_land_precipitation(world, &precipitation_fields.target_precipitation);
    apply_cold_coast_precipitation(
        world,
        &neighbor_lookup,
        &base_fields.target_ocean_temperature,
        &base_fields.wind_vectors,
        &mut precipitation_fields.precip_factor,
        &precipitation_fields.convergence_field,
        &precipitation_fields.uplift_field,
        &precipitation_fields.monsoon_field,
        &precipitation_fields.hotspot_field,
        &climate_params,
    );
    let cold_output_sum = sum_land_precipitation_with_factor(
        world,
        &precipitation_fields.target_precipitation,
        &precipitation_fields.precip_factor,
    );

    set_last_precip_diagnostics(PrecipDiagnosticsSummary {
        continental_reduction_ratio: reduction_ratio(
            precipitation_fields.continental_pre_sum,
            precipitation_fields.continental_post_sum,
        ),
        cap_reduction_ratio: reduction_ratio(
            precipitation_fields.cap_pre_sum,
            precipitation_fields.cap_post_sum,
        ),
        depletion_reduction_ratio: reduction_ratio(
            precipitation_fields.depletion_pre_sum,
            precipitation_fields.depletion_post_sum,
        ),
        cold_coast_reduction_ratio: reduction_ratio(cold_input_sum, cold_output_sum),
        cap_hit_ratio: if precipitation_fields.land_cells > 0 {
            precipitation_fields.cap_hits as f32 / precipitation_fields.land_cells as f32
        } else {
            0.0
        },
        mean_monsoon_boost_mm: if precipitation_fields.land_cells > 0 {
            precipitation_fields.monsoon_sum / precipitation_fields.land_cells as f32
        } else {
            0.0
        },
        mean_hotspot_boost_mm: if precipitation_fields.land_cells > 0 {
            precipitation_fields.hotspot_sum / precipitation_fields.land_cells as f32
        } else {
            0.0
        },
        mean_stage_source_mm: if precipitation_fields.land_cells > 0 {
            precipitation_fields.source_sum / precipitation_fields.land_cells as f32
        } else {
            0.0
        },
        mean_stage_transport_mm: if precipitation_fields.land_cells > 0 {
            precipitation_fields.transport_sum / precipitation_fields.land_cells as f32
        } else {
            0.0
        },
        mean_stage_orographic_mm: if precipitation_fields.land_cells > 0 {
            precipitation_fields.orographic_sum / precipitation_fields.land_cells as f32
        } else {
            0.0
        },
        mean_stage_correction_factor: if precipitation_fields.land_cells > 0 {
            precipitation_fields.correction_factor_sum / precipitation_fields.land_cells as f32
        } else {
            1.0
        },
        mean_budget_storage_change_mm: if precipitation_fields.land_cells > 0 {
            precipitation_fields.depletion_budget_storage_change_sum
                / precipitation_fields.land_cells as f32
        } else {
            0.0
        },
        mean_budget_residual_mm: if precipitation_fields.land_cells > 0 {
            precipitation_fields.depletion_budget_residual_sum
                / precipitation_fields.land_cells as f32
        } else {
            0.0
        },
        budget_residual_ratio: if precipitation_fields.depletion_source_pre_sum > EPS {
            (precipitation_fields.depletion_budget_residual_sum
                / precipitation_fields.depletion_source_pre_sum)
                .abs()
        } else {
            0.0
        },
    });

    for i in 0..cell_count {
        let latitude = world.state.geo.latitude.get(i).copied().unwrap_or(0.0);
        let mut precipitation =
            precipitation_fields.target_precipitation[i] * precipitation_fields.precip_factor[i];
        precipitation =
            precipitation.clamp(climate_params.precip_min_mm, climate_params.precip_max_mm);
        let vegetation_density = vegetation_density_proxy(world, i);
        let pet = annual_pet_mm(base_fields.target_temperature[i], latitude);
        let evapotranspiration =
            actual_evapotranspiration_mm(precipitation, pet, vegetation_density);
        let runoff = (precipitation - evapotranspiration).max(0.0);
        let aridity = pet / precipitation.max(EPS);

        world.state.climate.temperature[i] = lerp(
            world.state.climate.temperature[i],
            base_fields.target_temperature[i],
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
            base_fields.target_ocean_temperature[i],
            alpha,
        );
        world.state.climate.wind_u[i] =
            lerp(world.state.climate.wind_u[i], base_fields.target_wind_u[i], alpha);
        world.state.climate.wind_v[i] =
            lerp(world.state.climate.wind_v[i], base_fields.target_wind_v[i], alpha);
        world.state.climate.moisture_flux_u[i] = lerp(
            world.state.climate.moisture_flux_u[i],
            base_fields.target_moisture_flux_u[i],
            alpha,
        );
        world.state.climate.moisture_flux_v[i] = lerp(
            world.state.climate.moisture_flux_v[i],
            base_fields.target_moisture_flux_v[i],
            alpha,
        );
    }
}

fn compute_base_climate_fields(
    world: &World,
    climate_params: &ClimateParams,
    cell_count: usize,
) -> BaseClimateFields {
    let mut target_temperature = vec![0.0; cell_count];
    let mut target_ocean_temperature = vec![0.0; cell_count];
    let mut target_wind_u = vec![0.0; cell_count];
    let mut target_wind_v = vec![0.0; cell_count];
    let mut target_moisture_flux_u = vec![0.0; cell_count];
    let mut target_moisture_flux_v = vec![0.0; cell_count];
    let mut target_moisture_source = vec![0.0; cell_count];
    let mut wind_vectors = vec![[0.0, 0.0, 0.0]; cell_count];
    let mut flux_vectors = vec![[0.0, 0.0, 0.0]; cell_count];

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

    BaseClimateFields {
        target_temperature,
        target_ocean_temperature,
        target_wind_u,
        target_wind_v,
        target_moisture_flux_u,
        target_moisture_flux_v,
        target_moisture_source,
        wind_vectors,
        flux_vectors,
    }
}

fn compute_precipitation_fields(
    world: &World,
    base_fields: &BaseClimateFields,
    neighbor_lookup: &NeighborLookup,
    climate_params: &ClimateParams,
    cell_count: usize,
) -> PrecipitationFields {
    let mut target_precipitation = vec![0.0; cell_count];
    let precip_factor = vec![1.0; cell_count];
    let mut convergence_field = vec![0.0; cell_count];
    let mut uplift_field = vec![0.0; cell_count];
    let mut monsoon_field = vec![0.0; cell_count];
    let mut hotspot_field = vec![0.0; cell_count];
    let mut continental_pre_sum = 0.0;
    let mut continental_post_sum = 0.0;
    let mut cap_pre_sum = 0.0;
    let mut cap_post_sum = 0.0;
    let mut cap_hits = 0usize;
    let mut land_cells = 0usize;
    let mut monsoon_sum = 0.0;
    let mut hotspot_sum = 0.0;
    let mut source_sum = 0.0;
    let mut transport_sum = 0.0;
    let mut orographic_sum = 0.0;
    let mut correction_factor_sum = 0.0;

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
        land_cells += 1;
        source_sum += baseline.max(0.0);
        let convergence = moisture_convergence_mm(
            world,
            i,
            &base_fields.flux_vectors,
            climate_params,
        );
        let convergence_mm = climate_params.convergence_blend * convergence;
        let convergence_wet_mm = convergence_mm.max(0.0);
        let divergence_dry_mm = (-convergence_mm).max(0.0);
        transport_sum += convergence_wet_mm;
        let signal = orographic_signal(
            world,
            neighbor_lookup,
            i,
            base_fields.wind_vectors[i],
            climate_params,
        );
        let rise_norm =
            (signal.rise_m / climate_params.orographic_rise_scale_m.max(1.0)).clamp(0.0, 3.0);
        let fetch_factor = (0.70 + 0.60 * signal.ocean_fetch).clamp(0.5, 1.3);
        let uplift_mm = climate_params.orographic_uplift_gain_mm * rise_norm * fetch_factor;
        let monsoon_mm = monsoon_precipitation_boost_mm(
            world,
            neighbor_lookup,
            i,
            base_fields.target_wind_u[i],
            base_fields.wind_vectors[i],
            base_fields.target_temperature[i],
            base_fields.target_ocean_temperature[i],
        );
        let hotspot_mm = marine_orographic_hotspot_boost_mm(
            world,
            i,
            &signal,
            convergence_wet_mm,
            monsoon_mm,
            climate_params,
        );
        convergence_field[i] = convergence_mm;
        uplift_field[i] = uplift_mm;
        monsoon_field[i] = monsoon_mm;
        hotspot_field[i] = hotspot_mm;
        monsoon_sum += monsoon_mm;
        hotspot_sum += hotspot_mm;
        orographic_sum += uplift_mm + monsoon_mm + hotspot_mm;

        let shadow_factor = rain_shadow_factor(&signal, climate_params);
        let mut precipitation = baseline + convergence_wet_mm + uplift_mm + monsoon_mm + hotspot_mm;
        let divergence_dry_factor = (1.0 - 0.40 * smoothstep(35.0, 220.0, divergence_dry_mm))
            .clamp(0.45, 1.0);
        precipitation *= divergence_dry_factor;
        precipitation *= shadow_factor;
        correction_factor_sum += shadow_factor * divergence_dry_factor;

        let precipitation_pre_continental = precipitation.max(climate_params.precip_min_mm);
        let continental_factor = continentality_factor_relaxed(
            world
                .state
                .geo
                .distance_from_ocean
                .get(i)
                .copied()
                .unwrap_or(0.0),
            convergence_wet_mm,
            uplift_mm,
            monsoon_mm + hotspot_mm,
            climate_params,
        );
        precipitation *= continental_factor;
        continental_pre_sum += precipitation_pre_continental;
        continental_post_sum += precipitation.max(climate_params.precip_min_mm);

        let cap_scale = dynamic_precip_cap_scale(
            climate_params,
            convergence_wet_mm,
            uplift_mm + hotspot_mm,
            monsoon_mm,
            signal.ocean_fetch,
        );
        let moisture_cap = cap_scale * base_fields.target_moisture_source[i].max(0.0);
        let precipitation_pre_cap = precipitation.max(climate_params.precip_min_mm);
        if precipitation_pre_cap > moisture_cap.max(climate_params.precip_min_mm) {
            cap_hits += 1;
        }
        precipitation = precipitation.min(moisture_cap.max(climate_params.precip_min_mm));
        cap_pre_sum += precipitation_pre_cap;
        cap_post_sum += precipitation.max(climate_params.precip_min_mm);
        target_precipitation[i] =
            precipitation.clamp(climate_params.precip_min_mm, climate_params.precip_max_mm);
    }

    let depletion_pre_sum = sum_land_precipitation(world, &target_precipitation);
    let depletion_source_pre_sum = sum_land_values(world, &base_fields.target_moisture_source);
    let mut moisture_source_budget = base_fields.target_moisture_source.clone();
    let mut depleted_precipitation = target_precipitation.clone();
    apply_downwind_moisture_depletion_iterative(
        world,
        neighbor_lookup,
        &base_fields.wind_vectors,
        &mut moisture_source_budget,
        &mut depleted_precipitation,
        climate_params,
    );
    for i in 0..cell_count {
        if world.state.geology.height[i] <= 0.0 {
            continue;
        }
        let source_ratio = (moisture_source_budget[i]
            / base_fields.target_moisture_source[i].max(1.0))
            .clamp(0.30, 1.0);
        let moisture_cap =
            climate_params.precip_cap_from_moisture * moisture_source_budget[i].max(0.0);
        target_precipitation[i] = (target_precipitation[i] * source_ratio)
            .min(moisture_cap.max(climate_params.precip_min_mm))
            .clamp(climate_params.precip_min_mm, climate_params.precip_max_mm);
    }
    let depletion_post_sum = sum_land_precipitation(world, &target_precipitation);
    let depletion_source_post_sum = sum_land_values(world, &moisture_source_budget);
    let depletion_budget_storage_change_sum = depletion_source_post_sum - depletion_source_pre_sum;
    let depletion_sink_sum = (depletion_pre_sum - depletion_post_sum).max(0.0);
    let depletion_budget_residual_sum =
        (depletion_source_pre_sum - depletion_source_post_sum) - depletion_sink_sum;

    PrecipitationFields {
        target_precipitation,
        precip_factor,
        convergence_field,
        uplift_field,
        monsoon_field,
        hotspot_field,
        continental_pre_sum,
        continental_post_sum,
        cap_pre_sum,
        cap_post_sum,
        depletion_pre_sum,
        depletion_post_sum,
        depletion_source_pre_sum,
        depletion_budget_storage_change_sum,
        depletion_budget_residual_sum,
        cap_hits,
        land_cells,
        monsoon_sum,
        hotspot_sum,
        source_sum,
        transport_sum,
        orographic_sum,
        correction_factor_sum,
    }
}

fn rain_shadow_factor(signal: &OrographicSignal, climate_params: &ClimateParams) -> f32 {
    let barrier_norm =
        (signal.barrier_m / climate_params.rain_shadow_scale_m.max(1.0)).clamp(0.0, 3.0);
    let distance_decay = if signal.barrier_distance_km > 0.0 {
        (-signal.barrier_distance_km / climate_params.rain_shadow_distance_km.max(1.0)).exp()
    } else {
        0.0
    };
    (-climate_params.rain_shadow_gain * barrier_norm * distance_decay)
        .exp()
        .clamp(0.20, 1.0)
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
    let itcz = gaussian(latitude_abs, 0.0, 10.5);
    let subtropical_dry = gaussian(latitude_abs, 26.0, 8.5);
    let midlat_storm = gaussian(latitude_abs, 50.0, 14.0);
    let polar_dry = gaussian(latitude_abs, 78.0, 11.0);
    (LAT_BASE_MM + LAT_ITCZ_GAIN_MM * itcz + LAT_MIDLAT_GAIN_MM * midlat_storm
        - LAT_SUBTROPICAL_DRY_GAIN_MM * subtropical_dry
        - LAT_POLAR_DRY_GAIN_MM * polar_dry)
        .max(LAT_MIN_MM)
}

fn monsoon_precipitation_boost_mm(
    world: &World,
    neighbor_lookup: &NeighborLookup,
    index: usize,
    wind_u: f32,
    wind_vec: [f32; 3],
    land_temperature: f32,
    ocean_temperature: f32,
) -> f32 {
    if world.state.geology.height.get(index).copied().unwrap_or(0.0) <= 0.0 {
        return 0.0;
    }
    let latitude_abs = world
        .state
        .geo
        .latitude
        .get(index)
        .copied()
        .unwrap_or(0.0)
        .abs();
    let tropical_weight = gaussian(latitude_abs, MONSOON_LAT_CENTER_DEG, MONSOON_LAT_WIDTH_DEG);
    let distance = world
        .state
        .geo
        .distance_from_ocean
        .get(index)
        .copied()
        .unwrap_or(0.0)
        .max(0.0);
    let coastal_weight = (-distance / MONSOON_DISTANCE_KM).exp().clamp(0.0, 1.0);
    let thermal_contrast = ((land_temperature - ocean_temperature + 1.0) / 10.0).clamp(0.0, 1.4);
    let coast_side = world
        .state
        .geo
        .coast_side
        .get(index)
        .copied()
        .unwrap_or(CoastSide::None);
    let zonal_onshore = match coast_side {
        CoastSide::West => wind_u.max(0.0),
        CoastSide::East => (-wind_u).max(0.0),
        CoastSide::None => 0.0,
    }
    .clamp(0.0, 1.2);
    let upwind_ocean = upwind_ocean_fraction(world, neighbor_lookup, index, wind_vec, 3);
    let onshore_weight = (0.65 * zonal_onshore + 0.35 * upwind_ocean).clamp(0.0, 1.3);
    MONSOON_GAIN_MM * tropical_weight * coastal_weight * thermal_contrast * onshore_weight
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
    neighbor_lookup: &NeighborLookup,
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
            neighbor_lookup,
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
    let near_coast = smoothstep(0.0, 0.35, continentality);
    let deep_inland = smoothstep(0.35, 0.90, continentality);
    let inland_weight = (0.30 * near_coast + 0.70 * deep_inland).clamp(0.0, 1.0);
    (1.0 - inland_weight * params.continentality_gain).clamp(0.35, 1.0)
}

fn continentality_factor_relaxed(
    distance_from_ocean: f32,
    convergence_mm: f32,
    uplift_mm: f32,
    monsoon_mm: f32,
    params: &ClimateParams,
) -> f32 {
    let base = continentality_factor(distance_from_ocean, params);
    let relax = (CONTINENTAL_RELAX_CONVERGENCE_WEIGHT * smoothstep(90.0, 320.0, convergence_mm)
        + CONTINENTAL_RELAX_UPLIFT_WEIGHT * smoothstep(70.0, 260.0, uplift_mm)
        + CONTINENTAL_RELAX_MONSOON_WEIGHT * smoothstep(80.0, 300.0, monsoon_mm))
    .clamp(0.0, CONTINENTAL_RELAX_MAX);
    lerp(base, 1.0, relax)
}

fn dynamic_precip_cap_scale(
    params: &ClimateParams,
    convergence_mm: f32,
    uplift_mm: f32,
    monsoon_mm: f32,
    ocean_fetch: f32,
) -> f32 {
    let dynamic_boost = CAP_DYNAMIC_CONVERGENCE_WEIGHT * smoothstep(90.0, 360.0, convergence_mm)
        + CAP_DYNAMIC_UPLIFT_WEIGHT * smoothstep(80.0, 320.0, uplift_mm)
        + CAP_DYNAMIC_MONSOON_WEIGHT * smoothstep(80.0, 300.0, monsoon_mm)
        + CAP_DYNAMIC_FETCH_WEIGHT * ocean_fetch.clamp(0.0, 1.0);
    (params.precip_cap_from_moisture + dynamic_boost)
        .clamp(params.precip_cap_from_moisture, CAP_DYNAMIC_MAX)
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
    neighbor_lookup: &NeighborLookup,
    ocean_temperature: &[f32],
    wind_vectors: &[[f32; 3]],
    precip_factor: &mut [f32],
    convergence_mm: &[f32],
    uplift_mm: &[f32],
    monsoon_mm: &[f32],
    hotspot_mm: &[f32],
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
        let mut current = i;
        for step in 0..4 {
            let attenuation = 1.0 - (step as f32) * 0.2;
            let relax = (COLD_RELAX_CONVERGENCE_WEIGHT
                * smoothstep(
                    100.0,
                    360.0,
                    convergence_mm.get(current).copied().unwrap_or(0.0),
                )
                + COLD_RELAX_UPLIFT_WEIGHT
                    * smoothstep(80.0, 280.0, uplift_mm.get(current).copied().unwrap_or(0.0))
                + COLD_RELAX_MONSOON_WEIGHT
                    * smoothstep(70.0, 240.0, monsoon_mm.get(current).copied().unwrap_or(0.0))
                + params.cold_relax_hotspot_weight
                    * smoothstep(90.0, 320.0, hotspot_mm.get(current).copied().unwrap_or(0.0)))
            .clamp(0.0, COLD_RELAX_MAX);
            let relaxed_cold = lerp(cold_factor.clamp(0.2, 1.0), 1.0, relax);
            let step_factor = lerp(
                1.0,
                relaxed_cold,
                attenuation.clamp(0.0, 1.0),
            );
            precip_factor[current] *= step_factor;
            let wind_vector = wind_vectors
                .get(current)
                .copied()
                .unwrap_or([0.0, 0.0, 0.0]);
            let Some((next, _, _)) =
                best_neighbor_toward(neighbor_lookup, current, wind_vector, 0.15, true)
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

fn marine_orographic_hotspot_boost_mm(
    world: &World,
    index: usize,
    signal: &OrographicSignal,
    convergence_mm: f32,
    monsoon_mm: f32,
    params: &ClimateParams,
) -> f32 {
    if world.state.geology.height.get(index).copied().unwrap_or(0.0) <= 0.0 {
        return 0.0;
    }
    let distance_from_ocean = world
        .state
        .geo
        .distance_from_ocean
        .get(index)
        .copied()
        .unwrap_or(0.0)
        .max(0.0);
    let coast_weight = (-distance_from_ocean / params.hotspot_coast_distance_km.max(1.0))
        .exp()
        .clamp(0.0, 1.0);
    let barrier_norm = (signal.barrier_m / 1_200.0).clamp(0.0, 2.5);
    let rise_norm = (signal.rise_m / 900.0).clamp(0.0, 2.5);
    let fetch_weight = signal.ocean_fetch.clamp(0.0, 1.0);
    let convergence_weight = smoothstep(80.0, 320.0, convergence_mm);
    let monsoon_weight = smoothstep(50.0, 220.0, monsoon_mm);
    let latitude = world
        .state
        .geo
        .latitude
        .get(index)
        .copied()
        .unwrap_or(0.0)
        .abs();
    let latitude_weight =
        (0.70 + 0.20 * gaussian(latitude, 12.0, 18.0) + 0.10 * gaussian(latitude, 48.0, 12.0))
            .clamp(0.0, 1.0);

    params.hotspot_precip_gain_mm
        * coast_weight
        * (params.hotspot_fetch_weight * fetch_weight
            + params.hotspot_convergence_weight * convergence_weight)
        * (0.55 * barrier_norm + 0.45 * rise_norm).clamp(0.0, 2.0)
        * (0.75 + 0.25 * monsoon_weight)
        * latitude_weight
}

fn upwind_ocean_fraction(
    world: &World,
    neighbor_lookup: &NeighborLookup,
    index: usize,
    wind_vec: [f32; 3],
    steps: usize,
) -> f32 {
    if steps == 0 {
        return 0.0;
    }
    let upwind = normalize3(scale3(wind_vec, -1.0));
    let mut cursor = index;
    let mut ocean_hits: f32 = 0.0;
    let mut samples: f32 = 0.0;
    for _ in 0..steps {
        let Some((next, _, _)) = best_neighbor_toward(neighbor_lookup, cursor, upwind, 0.10, false)
        else {
            break;
        };
        if next == cursor {
            break;
        }
        samples += 1.0;
        if world.state.geology.height[next] <= 0.0 {
            ocean_hits += 1.0;
        }
        cursor = next;
    }
    if samples <= 0.0 {
        0.0
    } else {
        (ocean_hits / samples).clamp(0.0, 1.0)
    }
}

fn sum_land_precipitation(world: &World, precipitation: &[f32]) -> f32 {
    precipitation
        .iter()
        .enumerate()
        .filter(|(i, _)| world.state.geology.height.get(*i).copied().unwrap_or(0.0) > 0.0)
        .map(|(_, value)| (*value).max(0.0))
        .sum()
}

fn sum_land_values(world: &World, values: &[f32]) -> f32 {
    values
        .iter()
        .enumerate()
        .filter(|(i, _)| world.state.geology.height.get(*i).copied().unwrap_or(0.0) > 0.0)
        .map(|(_, value)| (*value).max(0.0))
        .sum()
}

fn sum_land_precipitation_with_factor(world: &World, precipitation: &[f32], factor: &[f32]) -> f32 {
    let len = precipitation.len().min(factor.len());
    let mut sum = 0.0;
    for i in 0..len {
        if world.state.geology.height.get(i).copied().unwrap_or(0.0) <= 0.0 {
            continue;
        }
        sum += precipitation[i].max(0.0) * factor[i].clamp(0.0, 1.0);
    }
    sum
}

fn reduction_ratio(before: f32, after: f32) -> f32 {
    if before <= EPS {
        0.0
    } else {
        ((before - after) / before).clamp(0.0, 1.0)
    }
}

fn apply_downwind_moisture_depletion(
    world: &World,
    neighbor_lookup: &NeighborLookup,
    wind_vectors: &[[f32; 3]],
    moisture_source: &[f32],
    precipitation: &[f32],
    depletion: &mut [f32],
    params: &ClimateParams,
) {
    let cell_count = world.state.geology.height.len();
    if wind_vectors.len() != cell_count
        || moisture_source.len() != cell_count
        || precipitation.len() != cell_count
        || depletion.len() != cell_count
    {
        return;
    }
    depletion.fill(0.0);
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
            let Some((primary, secondary)) = top_two_neighbors_toward(
                neighbor_lookup,
                current,
                current_wind,
                params.downwind_alignment_min,
                true,
            ) else {
                break;
            };
            if primary.0 == current {
                break;
            }
            let attenuation = params
                .downwind_depletion_decay
                .clamp(0.0, 1.0)
                .powi(step as i32);
            let primary_share = 0.70;
            let secondary_share = 0.30;
            depletion[primary.0] += carry * attenuation * primary_share;
            if let Some((secondary_idx, _, _)) = secondary {
                depletion[secondary_idx] += carry * attenuation * secondary_share;
            } else {
                depletion[primary.0] += carry * attenuation * secondary_share;
            }
            carry *= 0.92;
            if carry < 1e-4 {
                break;
            }
            current = primary.0;
        }
    }
}

fn apply_downwind_moisture_depletion_iterative(
    world: &World,
    neighbor_lookup: &NeighborLookup,
    wind_vectors: &[[f32; 3]],
    moisture_source: &mut [f32],
    precipitation: &mut [f32],
    params: &ClimateParams,
) {
    let cell_count = world.state.geology.height.len();
    if precipitation.len() != cell_count || moisture_source.len() != cell_count {
        return;
    }

    let pass_count = params.downwind_depletion_passes.max(1) as usize;
    let mut depletion = vec![0.0_f32; cell_count];
    for _ in 0..pass_count {
        apply_downwind_moisture_depletion(
            world,
            neighbor_lookup,
            wind_vectors,
            moisture_source,
            precipitation,
            &mut depletion,
            params,
        );

        for i in 0..cell_count {
            if world.state.geology.height[i] <= 0.0 {
                continue;
            }
            let reduction = depletion[i].clamp(0.0, params.downwind_depletion_max.clamp(0.0, 0.95));
            if reduction > 0.0 {
                precipitation[i] *= 1.0 - reduction;
                moisture_source[i] *= 1.0 - reduction;
            }
            let moisture_cap = params.precip_cap_from_moisture * moisture_source[i].max(0.0);
            precipitation[i] = precipitation[i]
                .min(moisture_cap.max(params.precip_min_mm))
                .clamp(params.precip_min_mm, params.precip_max_mm);
        }
    }
}

fn build_neighbor_lookup(world: &World) -> NeighborLookup {
    let cell_count = world.state.geology.height.len();
    let mut offsets = Vec::with_capacity(cell_count + 1);
    let mut entries = Vec::with_capacity(world.mesh.nbrs.len());
    offsets.push(0);

    for index in 0..cell_count {
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
            let edge_km = edge_distance_km(pos, neighbor_pos).max(1.0);
            entries.push(NeighborSample {
                index: n,
                dir,
                edge_km,
                is_land: world.state.geology.height[n] > 0.0,
            });
        }
        offsets.push(entries.len());
    }

    NeighborLookup { offsets, entries }
}

fn best_neighbor_toward(
    lookup: &NeighborLookup,
    index: usize,
    direction_vec: [f32; 3],
    min_alignment: f32,
    land_only: bool,
) -> Option<(usize, f32, f32)> {
    if index + 1 >= lookup.offsets.len() {
        return None;
    }
    let start = lookup.offsets[index];
    let end = lookup.offsets[index + 1];
    let mut best = None::<(usize, f32, f32)>;

    for sample in lookup.entries.get(start..end).unwrap_or(&[]) {
        if land_only && !sample.is_land {
            continue;
        }
        let alignment = dot3(sample.dir, direction_vec);
        match best {
            Some((_, _, best_alignment)) if alignment <= best_alignment => {}
            _ if alignment > min_alignment => {
                best = Some((sample.index, sample.edge_km, alignment));
            }
            _ => {}
        }
    }

    best
}

fn top_two_neighbors_toward(
    lookup: &NeighborLookup,
    index: usize,
    direction_vec: [f32; 3],
    min_alignment: f32,
    land_only: bool,
) -> Option<((usize, f32, f32), Option<(usize, f32, f32)>)> {
    if index + 1 >= lookup.offsets.len() {
        return None;
    }
    let start = lookup.offsets[index];
    let end = lookup.offsets[index + 1];
    let mut first = None::<(usize, f32, f32)>;
    let mut second = None::<(usize, f32, f32)>;

    for sample in lookup.entries.get(start..end).unwrap_or(&[]) {
        if land_only && !sample.is_land {
            continue;
        }
        let alignment = dot3(sample.dir, direction_vec);
        if alignment <= min_alignment {
            continue;
        }
        let candidate = (sample.index, sample.edge_km, alignment);
        match first {
            Some((_, _, best_alignment)) if alignment <= best_alignment => {
                match second {
                    Some((_, _, second_alignment)) if alignment <= second_alignment => {}
                    _ => {
                        second = Some(candidate);
                    }
                }
            }
            _ => {
                second = first;
                first = Some(candidate);
            }
        }
    }
    first.map(|head| (head, second))
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

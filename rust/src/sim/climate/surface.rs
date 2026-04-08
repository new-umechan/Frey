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
const PRECIPITATION_TURNOVER: f32 = 120.0;
const ITCZ_WIDTH_DEG: f32 = 11.0;
const SUBTROPICAL_CENTER_DEG: f32 = 24.0;
const SUBTROPICAL_WIDTH_DEG: f32 = 10.0;
const MIDLAT_CENTER_DEG: f32 = 52.0;
const MIDLAT_WIDTH_DEG: f32 = 13.0;
const LAT_BASE_MM: f32 = 500.0;
const LAT_ITCZ_GAIN_MM: f32 = 1850.0;
const LAT_MIDLAT_GAIN_MM: f32 = 430.0;
const LAT_SUBTROPICAL_DRY_GAIN_MM: f32 = 680.0;
const LAT_POLAR_DRY_GAIN_MM: f32 = 260.0;
const LAT_MIN_MM: f32 = 140.0;

#[derive(Debug, Clone, Copy, Default)]
pub struct PrecipDiagnosticsSummary {
    pub continental_reduction_ratio: f32,
    pub cap_reduction_ratio: f32,
    pub depletion_reduction_ratio: f32,
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
struct WindFields {
    wind_u: Vec<f32>,
    wind_v: Vec<f32>,
    wind_vectors: Vec<[f32; 3]>,
    vertical_motion: Vec<f32>,
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
    if cell_count == 0 {
        return;
    }

    let climate_params = ClimateParams::default();
    ensure_climate_field_lengths(world, cell_count);
    let alpha = blend_alpha(budget, CLIMATE_BLEND_BASE);
    let land_relax = relaxation_factor(
        world.clock.real_years_per_tick,
        climate_params.core_land_relaxation_years,
    );

    let neighbor_lookup = build_neighbor_lookup(world);

    let mut target_temperature = vec![0.0_f32; cell_count];
    let mut target_ocean_temperature = vec![0.0_f32; cell_count];
    for i in 0..cell_count {
        let latitude = world.latitude(i);
        let elevation_m = world.state.geology.height[i].max(0.0) * climate_params.height_to_meters;
        target_temperature[i] = base_land_temperature(latitude)
            - climate_params.lapse_rate_c_per_km * elevation_m / 1_000.0;
        let coast_side = world.coast_side(i);
        let prev_wind_u = world.state.climate.wind_u.get(i).copied().unwrap_or(0.0);
        let prev_wind_v = world.state.climate.wind_v.get(i).copied().unwrap_or(0.0);
        let has_prev_wind = prev_wind_u != 0.0 || prev_wind_v != 0.0;
        let current_offset = if has_prev_wind {
            ocean_current_offset_from_wind(world, i, prev_wind_u, prev_wind_v)
        } else {
            ocean_current_fallback(latitude, coast_side)
        };
        target_ocean_temperature[i] = base_ocean_temperature(latitude) + current_offset;
    }

    diffuse_scalar(
        world,
        &neighbor_lookup,
        &mut target_temperature,
        climate_params.core_temperature_diffusion_gain,
        1,
    );

    let wind_fields = build_wind_fields(
        world,
        &neighbor_lookup,
        &target_temperature,
        &target_ocean_temperature,
        cell_count,
    );

    let mut qsat = vec![0.0_f32; cell_count];
    for i in 0..cell_count {
        qsat[i] = saturation_capacity_mm(target_temperature[i], &climate_params);
    }

    let spinup_relax = relaxation_factor(world.clock.real_years_per_tick, 200_000.0);
    let mut humidity = vec![0.0_f32; cell_count];
    let mut humidity_initial_sum = 0.0_f32;
    for i in 0..cell_count {
        let fallback = (0.78 * qsat[i]).max(climate_params.core_humidity_floor_mm);
        let prior = world
            .state
            .climate
            .precipitable_water
            .get(i)
            .copied()
            .filter(|value| value.is_finite())
            .map(|value| value.max(climate_params.core_humidity_floor_mm))
            .unwrap_or(fallback);
        humidity[i] = lerp(prior, 0.90 * qsat[i], spinup_relax);
        humidity_initial_sum += humidity[i];
    }

    let mut precip_column = vec![0.0_f32; cell_count];
    let mut source_sum = 0.0_f32;
    let mut transport_sum = 0.0_f32;
    let mut orographic_sum = 0.0_f32;
    let mut condense_sum = 0.0_f32;
    let mut convergence_proxy = vec![0.0_f32; cell_count];
    let mut orographic_base = vec![OrographicSignal::default(); cell_count];
    let mut ascent_gate_base = vec![0.0_f32; cell_count];
    let mut subsidence_gate_base = vec![0.0_f32; cell_count];
    for i in 0..cell_count {
        convergence_proxy[i] =
            local_wind_convergence_proxy(&neighbor_lookup, i, &wind_fields.wind_vectors);
        orographic_base[i] = orographic_signal(
            world,
            &neighbor_lookup,
            i,
            wind_fields.wind_vectors[i],
            &climate_params,
        );
        let signal = orographic_base[i];
        let terrain_lift =
            smoothstep(0.10, 0.90, signal.rise_m) * (signal.rise_m / 2.0).clamp(0.0, 1.5);
        let ascent_proxy = (0.60 * wind_fields.vertical_motion[i]
            + 0.30 * convergence_proxy[i]
            + 0.35 * terrain_lift)
            .clamp(-1.2, 1.2);
        ascent_gate_base[i] = smoothstep(0.02, 0.35, ascent_proxy);
        subsidence_gate_base[i] = smoothstep(0.04, 0.40, -ascent_proxy);
    }

    let extra_substeps = world.clock.real_years_per_tick.max(1.0).log10().floor() as usize;
    let substeps = (climate_params.core_substeps.max(1) as usize + extra_substeps).min(24);
    for _ in 0..substeps {
        for i in 0..cell_count {
            let latitude = world.latitude(i);
            let ocean_qsat = saturation_capacity_mm(target_ocean_temperature[i], &climate_params);
            let is_ocean = world.state.geology.height[i] <= world.control.sea_level_offset;
            let source = if is_ocean {
                climate_params.core_ocean_evaporation_gain * (ocean_qsat - humidity[i]).max(0.0)
            } else {
                let distance = world.distance_from_ocean(i).max(0.0);
                let ocean_reach = (-distance / 1_400.0).exp();
                let previous_et = world
                    .state
                    .climate
                    .evapotranspiration
                    .get(i)
                    .copied()
                    .unwrap_or(0.0)
                    .max(0.0);
                climate_params.core_land_recycle_gain
                    * (0.35 * previous_et
                        + 0.65 * ocean_reach * (ocean_qsat - humidity[i]).max(0.0))
            };
            humidity[i] += source.max(0.0);
            source_sum += source.max(0.0);
            let _ = latitude;
        }

        transport_sum += apply_moisture_advection(
            world,
            &neighbor_lookup,
            &wind_fields.wind_vectors,
            &mut humidity,
            climate_params.core_moisture_transport_gain,
        );

        for i in 0..cell_count {
            let signal = orographic_base[i];
            let excess = (humidity[i] - qsat[i]).max(0.0);
            let humidity_ratio = (humidity[i] / qsat[i].max(1.0)).clamp(0.0, 2.0);
            let near_saturation = smoothstep(0.82, 1.02, humidity_ratio);
            let ascent_gate = ascent_gate_base[i];
            let subsidence_gate = subsidence_gate_base[i];
            let excess_condense =
                climate_params.core_condense_excess_gain * excess * (0.35 + 0.65 * ascent_gate);
            let lift_condense = climate_params.core_condense_excess_gain
                * 0.06
                * humidity[i]
                * near_saturation
                * ascent_gate;
            let orographic_condense = climate_params.core_orographic_condense_gain
                * signal.rise_m.max(0.0)
                * (0.40 + 0.60 * signal.ocean_fetch)
                * humidity[i]
                * (0.30 + 0.70 * ascent_gate);
            let condensation = ((excess_condense + lift_condense + orographic_condense)
                * (1.0 - 0.55 * subsidence_gate).clamp(0.15, 1.0))
            .min(humidity[i])
            .max(0.0);
            humidity[i] -= condensation;
            precip_column[i] += condensation;
            condense_sum += condensation;
            orographic_sum += orographic_condense.min(condensation);
        }

        for value in &mut humidity {
            *value = value.max(climate_params.core_humidity_floor_mm);
        }
    }

    let mut precipitation_raw = vec![0.0_f32; cell_count];
    let mut precipitation_continental = vec![0.0_f32; cell_count];
    let mut precipitation_target = vec![0.0_f32; cell_count];

    let mut continental_pre_sum = 0.0_f32;
    let mut continental_post_sum = 0.0_f32;
    let mut cap_pre_sum = 0.0_f32;
    let mut cap_post_sum = 0.0_f32;
    let mut cap_hits = 0usize;

    for i in 0..cell_count {
        let latitude_abs = world.latitude(i).abs();
        let baseline = latitude_band_precipitation_reference_mm(latitude_abs)
            + climate_params.hadley_anomaly_gain * hadley_precipitation_anomaly_mm(latitude_abs);
        let annualized = precip_column[i] * PRECIPITATION_TURNOVER;
        let distance = world.distance_from_ocean(i).max(0.0);
        let distance_weight = (-distance / 1_350.0).exp();
        let upwind_ocean =
            upwind_ocean_fraction(world, &neighbor_lookup, i, wind_fields.wind_vectors[i], 3);
        let humidity_ratio = (humidity[i] / qsat[i].max(1.0)).clamp(0.0, 2.0);
        let wet_gate = smoothstep(0.55, 1.05, humidity_ratio);
        let wind_convergence = convergence_proxy[i];
        let convergence_gate = smoothstep(0.03, 0.35, wind_convergence);
        let divergence_gate = smoothstep(0.03, 0.35, -wind_convergence);
        let circulation_ascent = wind_fields.vertical_motion[i].max(0.0);
        let circulation_subsidence = (-wind_fields.vertical_motion[i]).max(0.0);
        let onshore_gate = onshore_weight(world, i, wind_fields.wind_u[i], upwind_ocean);
        let subtropical_dry = gaussian(latitude_abs, 26.0, 8.5);
        let boost_gate = smoothstep(220.0, 950.0, baseline)
            * wet_gate
            * onshore_gate
            * (0.40 + 0.80 * circulation_ascent).clamp(0.25, 1.30)
            * (0.25 + 0.75 * convergence_gate)
            * (1.0 - 0.35 * circulation_subsidence).clamp(0.25, 1.0)
            * (1.0 - 0.60 * divergence_gate).clamp(0.15, 1.0);
        let monsoon_boost = 760.0
            * gaussian(latitude_abs, 18.0, 14.0)
            * distance_weight
            * upwind_ocean
            * boost_gate
            * (1.0 - 0.65 * subtropical_dry).clamp(0.20, 1.0);
        let signal = orographic_base[i];
        let hotspot_boost = climate_params.hotspot_precip_gain_mm
            * distance_weight
            * signal.ocean_fetch
            * upwind_ocean.max(0.20)
            * boost_gate
            * (signal.rise_m / 2.2).clamp(0.0, 1.8);
        let annualized_limited = annualized.min(
            2.0 * baseline
                + 1_150.0 * distance_weight
                + 950.0 * signal.ocean_fetch
                + 850.0 * convergence_gate
                + 380.0 * circulation_ascent,
        );
        let equatorial_suppression = smoothstep(5.0, 16.0, latitude_abs);
        let tropical_transition = 0.30 + 0.70 * equatorial_suppression;
        let combined = 0.88 * baseline
            + 0.65 * annualized_limited
            + monsoon_boost * tropical_transition
            + hotspot_boost * tropical_transition
            - 70.0 * circulation_subsidence * subtropical_dry;
        precipitation_raw[i] = combined;
        if world.state.geology.height[i] <= 0.0 {
            precipitation_target[i] =
                combined.clamp(climate_params.precip_min_mm, climate_params.precip_max_mm);
            continue;
        }

        continental_pre_sum += combined.max(climate_params.precip_min_mm);
        let continental_factor = continentality_factor(distance, &climate_params);
        let after_continental = combined * continental_factor;
        precipitation_continental[i] = after_continental;
        continental_post_sum += after_continental.max(climate_params.precip_min_mm);

        cap_pre_sum += after_continental.max(climate_params.precip_min_mm);
        let signal_for_cap = orographic_base[i];
        let cap_scale = climate_params.precip_cap_from_moisture
            * (0.80
                + 0.60 * humidity_ratio
                + 0.40 * signal_for_cap.ocean_fetch
                + 0.20 * distance_weight);
        let cap = cap_scale.max(1.0) * humidity[i].max(0.0) * PRECIPITATION_TURNOVER;
        if after_continental > cap.max(climate_params.precip_min_mm) {
            cap_hits += 1;
        }
        let after_cap = after_continental.min(cap.max(climate_params.precip_min_mm));
        cap_post_sum += after_cap.max(climate_params.precip_min_mm);

        precipitation_target[i] =
            after_cap.clamp(climate_params.precip_min_mm, climate_params.precip_max_mm);
    }

    let mut precipitation_target_sum = 0.0_f32;
    for value in precipitation_target.iter().take(cell_count) {
        precipitation_target_sum += value.max(0.0);
    }
    let condense_supply_sum = (condense_sum * PRECIPITATION_TURNOVER).max(EPS);
    let precipitation_scale =
        (condense_supply_sum / precipitation_target_sum.max(EPS)).clamp(0.55, 1.15);
    for value in &mut precipitation_target {
        *value = (*value * precipitation_scale)
            .clamp(climate_params.precip_min_mm, climate_params.precip_max_mm);
    }

    let mut humidity_final_sum = 0.0_f32;
    for value in &humidity {
        humidity_final_sum += *value;
    }

    let mut land_cells = 0usize;
    let mut storage_change_sum = 0.0_f32;
    let mut land_budget_residual_sum = 0.0_f32;
    let mut land_precip_sum = 0.0_f32;

    for i in 0..cell_count {
        let latitude = world.latitude(i);
        let is_land = world.state.geology.height[i] > world.control.sea_level_offset;

        let target_precip = precipitation_target[i];

        let (target_et, target_runoff, target_storage, aridity) = if is_land {
            land_cells += 1;
            let storage_cap = climate_params.core_land_bucket_capacity_mm.max(1.0);
            let prev_storage_state = (world
                .state
                .climate
                .precipitation
                .get(i)
                .copied()
                .unwrap_or(0.0)
                - world.state.climate.runoff.get(i).copied().unwrap_or(0.0)
                - 0.35
                    * world
                        .state
                        .climate
                        .evapotranspiration
                        .get(i)
                        .copied()
                        .unwrap_or(0.0))
            .clamp(0.0, storage_cap);
            let humidity_ratio = (humidity[i] / qsat[i].max(1.0)).clamp(0.0, 1.6);
            let climate_storage = storage_cap * (0.28 + 0.44 * humidity_ratio);
            let prev_storage = if prev_storage_state <= EPS {
                climate_storage.clamp(0.0, storage_cap)
            } else {
                lerp(prev_storage_state, climate_storage, spinup_relax).clamp(0.0, storage_cap)
            };

            let veg = vegetation_density_proxy(world, i);
            let distance_from_ocean = world.distance_from_ocean(i).max(0.0);
            let atmospheric_demand = atmospheric_evaporative_demand_mm(
                target_temperature[i],
                latitude,
                humidity[i],
                qsat[i],
                target_ocean_temperature[i],
                distance_from_ocean,
            ) * (1.0
                + 0.22 * (-wind_fields.vertical_motion[i]).max(0.0)
                + 0.12 * smoothstep(0.03, 0.35, -convergence_proxy[i]));
            let et_potential =
                atmospheric_demand * (0.16 + 0.84 * veg.clamp(0.0, 1.0)).clamp(0.16, 1.0);
            let available = prev_storage + target_precip;
            let et_eq = et_potential.min(available);
            let relief = local_relief_proxy(world, &neighbor_lookup, i);
            let relief_runoff = ((target_precip - et_eq).max(0.0)
                * (0.10 + 0.32 * relief)
                * (1.0 - humidity_ratio * 0.35).clamp(0.55, 1.0))
            .clamp(0.0, available - et_eq);
            let storage_eq = (available - et_eq - relief_runoff).clamp(0.0, storage_cap);
            let storage_next_raw = lerp(prev_storage, storage_eq, land_relax);
            let max_storage_gain = 0.28 * target_precip;
            let storage_next = (prev_storage
                + (storage_next_raw - prev_storage).clamp(-prev_storage, max_storage_gain))
            .clamp(0.0, storage_cap);
            let storage_change = storage_next - prev_storage;
            let runoff_eq = (target_precip - et_eq - storage_change).max(0.0);
            let residual = target_precip - et_eq - runoff_eq - storage_change;

            storage_change_sum += storage_change;
            land_budget_residual_sum += residual;
            land_precip_sum += target_precip.max(EPS);

            (
                et_eq,
                runoff_eq,
                storage_next,
                atmospheric_demand / target_precip.max(EPS),
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        world.state.climate.temperature[i] = lerp(
            world.state.climate.temperature[i],
            target_temperature[i],
            alpha,
        );
        world.state.climate.ocean_temperature[i] = lerp(
            world.state.climate.ocean_temperature[i],
            target_ocean_temperature[i],
            alpha,
        );
        world.state.climate.precipitation[i] =
            lerp(world.state.climate.precipitation[i], target_precip, alpha);
        world.state.climate.evapotranspiration[i] =
            lerp(world.state.climate.evapotranspiration[i], target_et, alpha);
        world.state.climate.runoff[i] = lerp(world.state.climate.runoff[i], target_runoff, alpha);
        world.state.climate.aridity[i] = lerp(world.state.climate.aridity[i], aridity, alpha);
        world.state.climate.precipitable_water[i] = lerp(
            world.state.climate.precipitable_water[i],
            humidity[i],
            alpha,
        );
        world.state.climate.wind_u[i] =
            lerp(world.state.climate.wind_u[i], wind_fields.wind_u[i], alpha);
        world.state.climate.wind_v[i] =
            lerp(world.state.climate.wind_v[i], wind_fields.wind_v[i], alpha);

        let flux_mag = humidity[i] * 0.75;
        world.state.climate.moisture_flux_u[i] = lerp(
            world.state.climate.moisture_flux_u[i],
            flux_mag * wind_fields.wind_u[i],
            alpha,
        );
        world.state.climate.moisture_flux_v[i] = lerp(
            world.state.climate.moisture_flux_v[i],
            flux_mag * wind_fields.wind_v[i],
            alpha,
        );

        let _ = target_storage;
    }

    let land_cells_f = land_cells.max(1) as f32;
    let atmospheric_residual =
        source_sum - condense_sum - (humidity_final_sum - humidity_initial_sum);
    let atmospheric_residual_ratio = if source_sum > EPS {
        (atmospheric_residual / source_sum).abs()
    } else {
        0.0
    };

    set_last_precip_diagnostics(PrecipDiagnosticsSummary {
        continental_reduction_ratio: reduction_ratio(continental_pre_sum, continental_post_sum),
        cap_reduction_ratio: reduction_ratio(cap_pre_sum, cap_post_sum),
        depletion_reduction_ratio: reduction_ratio(continental_post_sum, cap_post_sum),
        cap_hit_ratio: cap_hits as f32 / land_cells_f,
        mean_monsoon_boost_mm: 0.0,
        mean_hotspot_boost_mm: 0.0,
        mean_stage_source_mm: source_sum / land_cells_f,
        mean_stage_transport_mm: transport_sum / land_cells_f,
        mean_stage_orographic_mm: orographic_sum * PRECIPITATION_TURNOVER / land_cells_f,
        mean_stage_correction_factor: 1.0,
        mean_budget_storage_change_mm: storage_change_sum / land_cells_f,
        mean_budget_residual_mm: land_budget_residual_sum / land_cells_f,
        budget_residual_ratio: (if land_precip_sum > EPS {
            (land_budget_residual_sum / land_precip_sum).abs()
        } else {
            0.0
        } + atmospheric_residual_ratio)
            * 0.5,
    });
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
    if world.state.climate.precipitable_water.len() != cell_count {
        world
            .state
            .climate
            .precipitable_water
            .resize(cell_count, 0.0);
    }
}

fn build_wind_fields(
    world: &World,
    lookup: &NeighborLookup,
    target_temperature: &[f32],
    target_ocean_temperature: &[f32],
    cell_count: usize,
) -> WindFields {
    let mut wind_u = vec![0.0_f32; cell_count];
    let mut wind_v = vec![0.0_f32; cell_count];
    let mut wind_vectors = vec![[0.0_f32, 0.0_f32, 0.0_f32]; cell_count];
    let mut vertical_motion = vec![0.0_f32; cell_count];

    for i in 0..cell_count {
        let latitude = world.latitude(i);
        let (baroclinic_grad, thermal_contrast) = local_dynamic_forcing(
            world,
            lookup,
            i,
            target_temperature,
            target_ocean_temperature.get(i).copied().unwrap_or(0.0),
        );
        let (u, v) = hadley_wind_components(latitude, baroclinic_grad, thermal_contrast);
        wind_u[i] = u;
        wind_v[i] = v;
        wind_vectors[i] = local_wind_vector(world, i, u, v);
        vertical_motion[i] =
            circulation_vertical_motion_proxy(latitude, baroclinic_grad, thermal_contrast, v);
    }

    WindFields {
        wind_u,
        wind_v,
        wind_vectors,
        vertical_motion,
    }
}

fn apply_moisture_advection(
    world: &World,
    lookup: &NeighborLookup,
    wind_vectors: &[[f32; 3]],
    humidity: &mut [f32],
    gain: f32,
) -> f32 {
    let cell_count = world.state.geology.height.len();
    if humidity.len() != cell_count || wind_vectors.len() != cell_count {
        return 0.0;
    }

    let mut delta = vec![0.0_f32; cell_count];
    let mut moved_sum = 0.0_f32;
    let local_gain = gain.clamp(0.0, 0.45);

    for i in 0..cell_count {
        let current = humidity[i].max(0.0);
        if current <= EPS {
            continue;
        }
        let Some((primary, secondary)) =
            top_two_neighbors_toward(lookup, i, wind_vectors[i], 0.05, false)
        else {
            continue;
        };

        let outgoing = current * local_gain;
        if outgoing <= EPS {
            continue;
        }

        let primary_align = primary.2.max(0.0);
        let secondary_align = secondary.map(|sample| sample.2.max(0.0)).unwrap_or(0.0);
        let align_sum = (primary_align + secondary_align).max(EPS);

        delta[i] -= outgoing;
        delta[primary.0] += outgoing * (primary_align / align_sum);
        if let Some((secondary_idx, _, _)) = secondary {
            delta[secondary_idx] += outgoing * (secondary_align / align_sum);
        }
        moved_sum += outgoing;
    }

    for i in 0..cell_count {
        humidity[i] = (humidity[i] + delta[i]).max(0.0);
    }

    moved_sum
}

fn local_dynamic_forcing(
    world: &World,
    lookup: &NeighborLookup,
    index: usize,
    target_temperature: &[f32],
    local_ocean_temperature: f32,
) -> (f32, f32) {
    if index + 1 >= lookup.offsets.len() {
        return (0.0, 0.0);
    }
    let pos = world
        .mesh
        .positions
        .get(index)
        .copied()
        .unwrap_or([0.0, 0.0, 1.0]);
    let north = local_north_direction(pos);
    let start = lookup.offsets[index];
    let end = lookup.offsets[index + 1];
    let neighbors = lookup.entries.get(start..end).unwrap_or(&[]);
    if neighbors.is_empty() {
        return (0.0, 0.0);
    }
    let local_temperature = target_temperature.get(index).copied().unwrap_or(0.0);

    let mut meridional_grad = 0.0_f32;
    let mut weight_sum = 0.0_f32;
    for neighbor in neighbors {
        let n = neighbor.index;
        let neighbor_temp = target_temperature
            .get(n)
            .copied()
            .unwrap_or(local_temperature);
        let align_north = dot3(neighbor.dir, north);
        if align_north.abs() < 0.15 {
            continue;
        }
        let dt = neighbor_temp - local_temperature;
        let weight = align_north.abs() / neighbor.edge_km.max(1.0);
        meridional_grad += dt * align_north.signum() * weight;
        weight_sum += weight;
    }

    let baroclinic_grad = if weight_sum > EPS {
        (meridional_grad / weight_sum).clamp(-18.0, 18.0)
    } else {
        0.0
    };
    let thermal_contrast = (local_temperature - local_ocean_temperature).clamp(-15.0, 20.0);
    (baroclinic_grad, thermal_contrast)
}

fn diffuse_scalar(
    world: &World,
    lookup: &NeighborLookup,
    values: &mut [f32],
    gain: f32,
    iterations: usize,
) {
    if values.is_empty() {
        return;
    }
    let g = gain.clamp(0.0, 0.25);
    if g <= 0.0 {
        return;
    }

    let mut next = values.to_vec();
    for _ in 0..iterations.max(1) {
        for i in 0..values.len() {
            let start = lookup.offsets.get(i).copied().unwrap_or(0);
            let end = lookup.offsets.get(i + 1).copied().unwrap_or(start);
            let neighbors = lookup.entries.get(start..end).unwrap_or(&[]);
            if neighbors.is_empty() {
                next[i] = values[i];
                continue;
            }

            let mut mean = 0.0_f32;
            for neighbor in neighbors {
                mean += values[neighbor.index];
            }
            mean /= neighbors.len() as f32;
            next[i] = lerp(values[i], mean, g);
            let _ = world;
        }
        values.copy_from_slice(&next);
    }
}

fn orographic_signal(
    world: &World,
    lookup: &NeighborLookup,
    index: usize,
    wind_vec: [f32; 3],
    params: &ClimateParams,
) -> OrographicSignal {
    let height_here = world
        .state
        .geology
        .height
        .get(index)
        .copied()
        .unwrap_or(0.0);
    if height_here <= 0.0 {
        return OrographicSignal::default();
    }

    let upwind = normalize3(scale3(wind_vec, -1.0));
    let mut rise_m = 0.0_f32;
    let mut ocean_hits = 0.0_f32;
    let mut samples = 0.0_f32;

    let mut current = index;
    for _ in 0..3 {
        let Some((next, _, _)) = best_neighbor_toward(lookup, current, upwind, 0.10, false) else {
            break;
        };
        if next == current {
            break;
        }
        let h_current = world
            .state
            .geology
            .height
            .get(current)
            .copied()
            .unwrap_or(0.0);
        let h_next = world.state.geology.height.get(next).copied().unwrap_or(0.0);
        rise_m += ((h_current - h_next).max(0.0)) * params.height_to_meters / 1_000.0;
        samples += 1.0;
        if h_next <= 0.0 {
            ocean_hits += 1.0;
        }
        current = next;
    }

    OrographicSignal {
        rise_m,
        ocean_fetch: if samples > 0.0 {
            (ocean_hits / samples).clamp(0.0, 1.0)
        } else {
            0.0
        },
    }
}

fn upwind_ocean_fraction(
    world: &World,
    lookup: &NeighborLookup,
    index: usize,
    wind_vec: [f32; 3],
    steps: usize,
) -> f32 {
    if steps == 0 {
        return 0.0;
    }
    let upwind = normalize3(scale3(wind_vec, -1.0));
    let mut current = index;
    let mut ocean_hits = 0.0_f32;
    let mut samples = 0.0_f32;

    for _ in 0..steps {
        let Some((next, _, _)) = best_neighbor_toward(lookup, current, upwind, 0.05, false) else {
            break;
        };
        if next == current {
            break;
        }
        samples += 1.0;
        if world.state.geology.height.get(next).copied().unwrap_or(0.0) <= 0.0 {
            ocean_hits += 1.0;
        }
        current = next;
    }

    if samples <= 0.0 {
        0.0
    } else {
        (ocean_hits / samples).clamp(0.0, 1.0)
    }
}

fn local_wind_convergence_proxy(
    lookup: &NeighborLookup,
    index: usize,
    wind_vectors: &[[f32; 3]],
) -> f32 {
    if index + 1 >= lookup.offsets.len() || index >= wind_vectors.len() {
        return 0.0;
    }
    let wind_i = wind_vectors[index];
    let start = lookup.offsets[index];
    let end = lookup.offsets[index + 1];
    let neighbors = lookup.entries.get(start..end).unwrap_or(&[]);
    if neighbors.is_empty() {
        return 0.0;
    }

    let mut divergence = 0.0_f32;
    let mut weight_sum = 0.0_f32;
    for neighbor in neighbors {
        if neighbor.index >= wind_vectors.len() {
            continue;
        }
        let wind_n = wind_vectors[neighbor.index];
        let along = dot3(sub3(wind_n, wind_i), neighbor.dir);
        let weight = 1.0 / neighbor.edge_km.max(1.0);
        divergence += along * weight;
        weight_sum += weight;
    }
    if weight_sum <= EPS {
        return 0.0;
    }
    (-divergence / weight_sum * 0.22).clamp(-1.2, 1.2)
}

fn onshore_weight(world: &World, index: usize, wind_u: f32, upwind_ocean: f32) -> f32 {
    let coast_side = world.coast_side(index);
    let zonal_onshore = match coast_side {
        CoastSide::West => wind_u.max(0.0),
        CoastSide::East => (-wind_u).max(0.0),
        CoastSide::None => 0.0,
    }
    .clamp(0.0, 1.2);
    (0.40 + 0.45 * zonal_onshore + 0.35 * upwind_ocean.clamp(0.0, 1.0)).clamp(0.0, 1.25)
}

fn local_relief_proxy(world: &World, lookup: &NeighborLookup, index: usize) -> f32 {
    if index + 1 >= lookup.offsets.len() {
        return 0.0;
    }
    let h0 = world
        .state
        .geology
        .height
        .get(index)
        .copied()
        .unwrap_or(0.0)
        .max(0.0);
    let start = lookup.offsets[index];
    let end = lookup.offsets[index + 1];
    let neighbors = lookup.entries.get(start..end).unwrap_or(&[]);
    if neighbors.is_empty() {
        return 0.0;
    }
    let mut relief = 0.0_f32;
    for neighbor in neighbors {
        let hn = world
            .state
            .geology
            .height
            .get(neighbor.index)
            .copied()
            .unwrap_or(0.0)
            .max(0.0);
        relief += (h0 - hn).abs();
    }
    let mean_relief = relief / neighbors.len() as f32;
    (mean_relief * 5.0).clamp(0.0, 1.0)
}

fn saturation_capacity_mm(temperature_c: f32, params: &ClimateParams) -> f32 {
    let exponent = params.core_humidity_cc_rate_per_c * (temperature_c - 15.0);
    (params.core_humidity_ref_mm * exponent.exp()).clamp(params.core_humidity_floor_mm, 380.0)
}

fn relaxation_factor(delta_years: f32, tau_years: f32) -> f32 {
    if delta_years <= 0.0 {
        return 0.0;
    }
    let tau = tau_years.max(1.0);
    (1.0 - (-delta_years / tau).exp()).clamp(0.0, 1.0)
}

fn base_land_temperature(latitude: f32) -> f32 {
    30.0 * latitude.to_radians().cos() - 5.0
}

fn base_ocean_temperature(latitude: f32) -> f32 {
    28.0 * latitude.to_radians().cos() - 2.0
}

fn hadley_wind_components(
    latitude: f32,
    baroclinic_grad: f32,
    thermal_contrast: f32,
) -> (f32, f32) {
    let abs_lat = latitude.abs();
    let hemisphere_sign = latitude.signum();
    let grad_mag = baroclinic_grad.abs();
    let baroclinic_boost = smoothstep(0.8, 6.0, grad_mag);
    let monsoon_boost =
        smoothstep(2.0, 10.0, thermal_contrast.max(0.0)) * gaussian(abs_lat, 18.0, 16.0);

    let trade = 1.0 - smoothstep(18.0, 36.0, abs_lat);
    let westerly = smoothstep(24.0, 48.0, abs_lat) * (1.0 - smoothstep(56.0, 74.0, abs_lat));
    let polar_easterly = smoothstep(60.0, 80.0, abs_lat);
    let zonal_base = -0.9 * trade + 0.85 * westerly - 0.55 * polar_easterly;
    let zonal_scale =
        1.0 + 0.35 * baroclinic_boost * gaussian(abs_lat, 42.0, 18.0) + 0.12 * monsoon_boost;
    let zonal = zonal_base * zonal_scale;

    let hadley = 1.0 - smoothstep(5.0, 32.0, abs_lat);
    let ferrel = smoothstep(28.0, 44.0, abs_lat) * (1.0 - smoothstep(54.0, 68.0, abs_lat));
    let polar = smoothstep(62.0, 78.0, abs_lat);
    let meridional_base = -0.45 * hemisphere_sign * hadley + 0.26 * hemisphere_sign * ferrel
        - 0.20 * hemisphere_sign * polar;
    let monsoon_direction = -hemisphere_sign;
    let meridional = meridional_base
        + monsoon_direction * (0.18 * monsoon_boost)
        + monsoon_direction * (0.08 * baroclinic_boost * hadley);

    (zonal, meridional)
}

fn circulation_vertical_motion_proxy(
    latitude: f32,
    baroclinic_grad: f32,
    thermal_contrast: f32,
    meridional_wind: f32,
) -> f32 {
    let abs_lat = latitude.abs();
    let itcz_ascent = gaussian(abs_lat, 6.0, 12.0);
    let midlat_ascent = gaussian(abs_lat, 53.0, 11.0);
    let subtropical_descent = gaussian(abs_lat, 28.0, 9.0);
    let polar_descent = gaussian(abs_lat, 76.0, 10.0);

    let cell_structure = 0.95 * itcz_ascent + 0.55 * midlat_ascent
        - 0.90 * subtropical_descent
        - 0.42 * polar_descent;
    let baroclinic_term = 0.16 * baroclinic_grad.abs() * midlat_ascent;
    let monsoon_lift = 0.08 * thermal_contrast.max(0.0) * itcz_ascent;
    let heated_descent = 0.10 * thermal_contrast.max(0.0) * subtropical_descent;
    let overturning = 0.20 * meridional_wind.abs() * (0.65 * itcz_ascent + 0.35 * midlat_ascent);
    (cell_structure + baroclinic_term + monsoon_lift + overturning - heated_descent)
        .clamp(-1.2, 1.2)
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

fn continentality_factor(distance_from_ocean: f32, params: &ClimateParams) -> f32 {
    let continentality =
        1.0 - (-distance_from_ocean.max(0.0) / params.distance_scale_km.max(1.0)).exp();
    let near_coast = smoothstep(0.0, 0.35, continentality);
    let deep_inland = smoothstep(0.35, 0.90, continentality);
    let inland_weight = (0.30 * near_coast + 0.70 * deep_inland).clamp(0.0, 1.0);
    (1.0 - inland_weight * params.continentality_gain).clamp(0.35, 1.0)
}

fn ocean_current_offset_from_wind(world: &World, index: usize, wind_u: f32, wind_v: f32) -> f32 {
    let lat = world.latitude(index);
    let lat_abs = lat.abs();
    let coast_side = world.coast_side(index);

    if coast_side == CoastSide::None {
        return 0.0;
    }

    let is_coastal = world.is_coastal(index);
    if !is_coastal {
        return 0.0;
    }

    let distance = world.distance_from_ocean(index).max(0.0);

    let coastal_decay = (-distance / 600.0).exp().clamp(0.0, 1.0);

    let hemisphere_sign = lat.signum();
    let coriolis = lat_abs.to_radians().sin().abs().max(0.05);

    let coast_sign = match coast_side {
        CoastSide::East => 1.0,
        CoastSide::West => -1.0,
        CoastSide::None => 0.0,
    };

    let alongshore_wind = wind_v * coast_sign;
    let offshore_wind = wind_u * coast_sign;

    let ekman_along = alongshore_wind / coriolis;
    let ekman_offshore = offshore_wind / coriolis;

    let hemisphere_ekman = hemisphere_sign * (ekman_along + ekman_offshore * 0.3);

    let upwelling_signal = -hemisphere_ekman.clamp(-8.0, 8.0) * 0.75;

    let lat_mod = 1.0 + 0.3 * gaussian(lat_abs, 20.0, 15.0);

    (upwelling_signal * lat_mod * coastal_decay).clamp(-8.0, 8.0)
}

fn ocean_current_fallback(latitude: f32, coast_side: CoastSide) -> f32 {
    let lat_abs = latitude.abs();

    if coast_side == CoastSide::None {
        return 0.0;
    }

    let trade_wind_u = -0.9 * (1.0 - smoothstep(18.0, 36.0, lat_abs))
        + 0.85 * smoothstep(24.0, 48.0, lat_abs) * (1.0 - smoothstep(56.0, 74.0, lat_abs))
        - 0.55 * smoothstep(60.0, 80.0, lat_abs);
    let trade_wind_v = -0.45 * latitude.signum() * (1.0 - smoothstep(18.0, 36.0, lat_abs));
    let westerly_wind_v = 0.26
        * latitude.signum()
        * smoothstep(28.0, 44.0, lat_abs)
        * (1.0 - smoothstep(54.0, 68.0, lat_abs));
    let wind_v = trade_wind_v + westerly_wind_v;

    let coast_sign = match coast_side {
        CoastSide::East => 1.0,
        CoastSide::West => -1.0,
        CoastSide::None => 0.0,
    };

    let coriolis = lat_abs.to_radians().sin().abs().max(0.05);
    let hemisphere_sign = latitude.signum();
    let alongshore = wind_v * coast_sign;
    let offshore = trade_wind_u * coast_sign;
    let ekman_along = alongshore / coriolis;
    let ekman_offshore = offshore / coriolis;
    let hemisphere_ekman = hemisphere_sign * (ekman_along + ekman_offshore * 0.3);
    let upwelling_signal = -hemisphere_ekman.clamp(-8.0, 8.0) * 0.75;
    let lat_mod = 1.0 + 0.3 * gaussian(lat_abs, 20.0, 15.0);

    (upwelling_signal * lat_mod).clamp(-8.0, 8.0)
}

fn reduction_ratio(before: f32, after: f32) -> f32 {
    if before <= EPS {
        0.0
    } else {
        ((before - after) / before).clamp(0.0, 1.0)
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(EPS)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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

fn gaussian(value: f32, center: f32, width: f32) -> f32 {
    let sigma = width.max(1.0);
    (-(value - center).powi(2) / (2.0 * sigma * sigma)).exp()
}

fn atmospheric_evaporative_demand_mm(
    temperature_c: f32,
    latitude: f32,
    humidity_mm: f32,
    qsat_mm: f32,
    ocean_temperature_c: f32,
    distance_from_ocean_km: f32,
) -> f32 {
    let abs_lat = latitude.abs();
    let lat_rad = latitude.to_radians();
    let insolation = (0.40 + 0.85 * lat_rad.cos().max(0.0)).clamp(0.18, 1.25);
    let cloudiness = (humidity_mm / qsat_mm.max(1.0)).clamp(0.15, 1.6);
    let cloud_transmittance = (1.12 - 0.36 * cloudiness).clamp(0.45, 1.08);
    let radiation_limit = (210.0 + 1_140.0 * insolation * cloud_transmittance).max(0.0);

    let saturation_deficit = (qsat_mm - humidity_mm).max(0.0) / qsat_mm.max(1.0);
    let coastal_aero = (-distance_from_ocean_km / 900.0).exp();
    let continentality = smoothstep(200.0, 2_600.0, distance_from_ocean_km.max(0.0));
    let subtropical_subsidence = gaussian(abs_lat, 27.0, 9.0);
    let thermal_contrast = ((temperature_c - ocean_temperature_c + 2.0) / 12.0).clamp(0.0, 1.8);
    let dryness_boost =
        (0.70 + 0.45 * continentality + 0.35 * subtropical_subsidence).clamp(0.65, 1.8);
    let aerodynamic_term = 260.0
        * saturation_deficit
        * (0.55 + 0.25 * coastal_aero + 0.45 * continentality)
        * (0.70 + 0.30 * thermal_contrast)
        * dryness_boost;

    let temp_gate = smoothstep(-8.0, 34.0, temperature_c);
    let subsidence_demand = 180.0 * subtropical_subsidence * (0.4 + 0.6 * continentality);
    let demand = (radiation_limit + aerodynamic_term + subsidence_demand) * temp_gate;
    demand.clamp(0.0, 3_200.0)
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

type NeighborCandidate = (usize, f32, f32);

fn top_two_neighbors_toward(
    lookup: &NeighborLookup,
    index: usize,
    direction_vec: [f32; 3],
    min_alignment: f32,
    land_only: bool,
) -> Option<(NeighborCandidate, Option<NeighborCandidate>)> {
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
            Some((_, _, best_alignment)) if alignment <= best_alignment => match second {
                Some((_, _, second_alignment)) if alignment <= second_alignment => {}
                _ => {
                    second = Some(candidate);
                }
            },
            _ => {
                second = first;
                first = Some(candidate);
            }
        }
    }

    first.map(|head| (head, second))
}

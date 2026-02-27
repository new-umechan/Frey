use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

#[derive(Deserialize)]
struct StepLayersBundleInput {
    height_data: Vec<f32>,
    river_flux: Vec<f32>,
    base_positions_y: Vec<f32>,
    climate_temp: Option<Vec<f32>>,
    climate_rain: Option<Vec<f32>>,
    ecology_habitability: Option<Vec<f32>>,
    ecology_productivity: Option<Vec<f32>>,
    civilization_population: Option<Vec<f32>>,
    civilization_state_id: Option<Vec<u32>>,
    climate_steps: u32,
    ecology_steps: u32,
    civilization_steps: u32,
}

#[derive(Serialize)]
pub(crate) struct StepLayersBundleOutput {
    pub(crate) climate_temp: Option<Vec<f32>>,
    pub(crate) climate_rain: Option<Vec<f32>>,
    pub(crate) climate_delta_abs_sum: f32,
    pub(crate) ecology_habitability: Option<Vec<f32>>,
    pub(crate) ecology_productivity: Option<Vec<f32>>,
    pub(crate) ecology_delta_abs_sum: f32,
    pub(crate) civilization_population: Option<Vec<f32>>,
    pub(crate) civilization_state_id: Option<Vec<u32>>,
    pub(crate) civilization_population_delta_sum: f32,
    pub(crate) civilization_polity_change_count: u32,
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn step_climate_once(
    temp: &mut [f32],
    rain: &mut [f32],
    height_data: &[f32],
    river_flux: &[f32],
    base_positions_y: &[f32],
    relax_gain: f32,
) -> f32 {
    let cell_count = temp
        .len()
        .min(rain.len())
        .min(height_data.len())
        .min(river_flux.len())
        .min(base_positions_y.len());

    let mut delta_abs_sum = 0.0f32;
    for i in 0..cell_count {
        let lat_abs = base_positions_y[i].abs().min(1.0);
        let height = height_data[i];
        let flux = river_flux[i].max(0.0);
        let flux_wet = clamp01(flux.ln_1p() * 0.38);
        let oceanic = if height <= 0.0 { 1.0 } else { 0.25 };

        let target_temp = clamp01(1.0 - lat_abs * 0.95 - height.max(0.0) * 0.7);
        let target_rain = clamp01(
            (1.0 - lat_abs) * 0.35 + flux_wet * 0.4 + oceanic * 0.3 - height.max(0.0) * 0.2,
        );

        let prev_temp = temp[i];
        let prev_rain = rain[i];
        let next_temp = prev_temp + (target_temp - prev_temp) * relax_gain;
        let next_rain = prev_rain + (target_rain - prev_rain) * relax_gain;
        temp[i] = next_temp;
        rain[i] = next_rain;
        delta_abs_sum += (next_temp - prev_temp).abs() + (next_rain - prev_rain).abs();
    }

    delta_abs_sum
}

fn step_ecology_once(
    habitability: &mut [f32],
    productivity: &mut [f32],
    temp: &[f32],
    rain: &[f32],
    height_data: &[f32],
    relax_gain: f32,
) -> f32 {
    let cell_count = habitability
        .len()
        .min(productivity.len())
        .min(temp.len())
        .min(rain.len())
        .min(height_data.len());

    let mut delta_abs_sum = 0.0f32;
    for i in 0..cell_count {
        let height = height_data[i];
        let is_land = height > 0.0;
        let local_temp = clamp01(temp[i]);
        let local_rain = clamp01(rain[i]);

        let temperature_suitability = clamp01(1.0 - (local_temp - 0.62).abs() * 1.9);
        let moisture_suitability = clamp01(local_rain * 1.05);
        let terrain_penalty = if is_land { 1.0 } else { 0.0 };
        let target_habitability =
            clamp01(temperature_suitability * 0.55 + moisture_suitability * 0.45) * terrain_penalty;
        let target_productivity =
            clamp01(target_habitability * (local_rain * 0.65 + local_temp * 0.35));

        let prev_habitability = habitability[i];
        let prev_productivity = productivity[i];
        let next_habitability =
            prev_habitability + (target_habitability - prev_habitability) * relax_gain;
        let next_productivity =
            prev_productivity + (target_productivity - prev_productivity) * relax_gain;

        habitability[i] = next_habitability;
        productivity[i] = next_productivity;
        delta_abs_sum +=
            (next_habitability - prev_habitability).abs() + (next_productivity - prev_productivity).abs();
    }

    delta_abs_sum
}

fn step_civilization_once(
    population: &mut [f32],
    state_id: &mut [u32],
    habitability: &[f32],
    productivity: &[f32],
    height_data: &[f32],
    relax_gain: f32,
) -> (f32, u32) {
    let cell_count = population
        .len()
        .min(state_id.len())
        .min(habitability.len())
        .min(productivity.len())
        .min(height_data.len());

    let mut population_delta_sum = 0.0f32;
    let mut polity_change_count = 0u32;

    for i in 0..cell_count {
        let is_land = height_data[i] > 0.0;
        let carrying = if is_land {
            clamp01(habitability[i] * 0.7 + productivity[i] * 0.3)
        } else {
            0.0
        };

        let prev_population = population[i];
        let mut next_population = prev_population + (carrying - prev_population) * relax_gain;
        if carrying > 0.42 && next_population < 0.02 {
            next_population = 0.02;
        }
        if !is_land || carrying < 0.05 {
            next_population = 0.0;
        }
        next_population = clamp01(next_population);
        population[i] = next_population;
        population_delta_sum += (next_population - prev_population).abs();

        let prev_state_id = state_id[i];
        let mut next_state_id = prev_state_id;
        if next_population > 0.18 && prev_state_id == 0 {
            next_state_id = (i as u32) + 1;
        } else if next_population < 0.03 && prev_state_id != 0 {
            next_state_id = 0;
        }

        if next_state_id != prev_state_id {
            state_id[i] = next_state_id;
            polity_change_count += 1;
        }
    }

    (population_delta_sum, polity_change_count)
}

pub(crate) fn step_layers_bundle_from_js(input_js: JsValue) -> Result<StepLayersBundleOutput, String> {
    let mut input = serde_wasm_bindgen::from_value::<StepLayersBundleInput>(input_js)
        .map_err(|err| format!("invalid step_layers_bundle input: {err}"))?;

    let mut climate_delta_abs_sum = 0.0;
    let mut ecology_delta_abs_sum = 0.0;
    let mut civilization_population_delta_sum = 0.0;
    let mut civilization_polity_change_count = 0u32;

    if let (Some(temp), Some(rain)) = (input.climate_temp.as_mut(), input.climate_rain.as_mut()) {
        for _ in 0..input.climate_steps {
            climate_delta_abs_sum += step_climate_once(
                temp,
                rain,
                &input.height_data,
                &input.river_flux,
                &input.base_positions_y,
                0.16,
            );
        }
    }

    if let (Some(habitability), Some(productivity), Some(temp), Some(rain)) = (
        input.ecology_habitability.as_mut(),
        input.ecology_productivity.as_mut(),
        input.climate_temp.as_ref(),
        input.climate_rain.as_ref(),
    ) {
        for _ in 0..input.ecology_steps {
            ecology_delta_abs_sum += step_ecology_once(
                habitability,
                productivity,
                temp,
                rain,
                &input.height_data,
                0.2,
            );
        }
    }

    if let (Some(population), Some(state_id), Some(habitability), Some(productivity)) = (
        input.civilization_population.as_mut(),
        input.civilization_state_id.as_mut(),
        input.ecology_habitability.as_ref(),
        input.ecology_productivity.as_ref(),
    ) {
        for _ in 0..input.civilization_steps {
            let (delta_sum, polity_changes) = step_civilization_once(
                population,
                state_id,
                habitability,
                productivity,
                &input.height_data,
                0.08,
            );
            civilization_population_delta_sum += delta_sum;
            civilization_polity_change_count += polity_changes;
        }
    }

    Ok(StepLayersBundleOutput {
        climate_temp: input.climate_temp,
        climate_rain: input.climate_rain,
        climate_delta_abs_sum,
        ecology_habitability: input.ecology_habitability,
        ecology_productivity: input.ecology_productivity,
        ecology_delta_abs_sum,
        civilization_population: input.civilization_population,
        civilization_state_id: input.civilization_state_id,
        civilization_population_delta_sum,
        civilization_polity_change_count,
    })
}

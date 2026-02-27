use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

#[derive(Deserialize)]
struct ApplyLandRatioFloorInput {
    height_data: Vec<f32>,
    plate_id: Vec<u32>,
    plate_is_ocean: Vec<u8>,
    target_land_ratio: f32,
    floor_scale: f32,
    recovery_gain: f32,
    height_clamp: f32,
}

#[derive(Serialize)]
pub(crate) struct ApplyLandRatioFloorOutput {
    pub(crate) height_data: Vec<f32>,
    pub(crate) delta_abs: f32,
}

pub(crate) fn apply_land_ratio_floor_from_js(
    input_js: JsValue,
) -> Result<ApplyLandRatioFloorOutput, String> {
    let input = serde_wasm_bindgen::from_value::<ApplyLandRatioFloorInput>(input_js)
        .map_err(|err| format!("invalid apply_land_ratio_floor input: {err}"))?;

    Ok(apply_land_ratio_floor_native(input))
}

fn apply_land_ratio_floor_native(input: ApplyLandRatioFloorInput) -> ApplyLandRatioFloorOutput {
    let mut height_data = input.height_data;
    let cell_count = height_data.len().min(input.plate_id.len());

    if cell_count == 0
        || input.target_land_ratio <= 0.0
        || !input.target_land_ratio.is_finite()
        || input.floor_scale <= 0.0
        || !input.floor_scale.is_finite()
        || input.recovery_gain <= 0.0
        || !input.recovery_gain.is_finite()
    {
        return ApplyLandRatioFloorOutput {
            height_data,
            delta_abs: 0.0,
        };
    }

    let mut land_count = 0usize;
    for &h in height_data.iter().take(cell_count) {
        if h > 0.0 {
            land_count += 1;
        }
    }

    let current_land_ratio = land_count as f32 / cell_count.max(1) as f32;
    let floor_land_ratio = input.target_land_ratio * input.floor_scale;
    let land_deficit = (floor_land_ratio - current_land_ratio).max(0.0);
    if land_deficit <= 0.0 {
        return ApplyLandRatioFloorOutput {
            height_data,
            delta_abs: 0.0,
        };
    }

    let mut delta_abs = 0.0f32;
    for i in 0..cell_count {
        let pid = input.plate_id[i] as usize;
        if pid >= input.plate_is_ocean.len() || input.plate_is_ocean[pid] > 0 {
            continue;
        }

        let h = height_data[i];
        if h <= -0.08 {
            continue;
        }

        let coastal_boost = (1.0 - (h.abs() / 0.30).min(1.0)).max(0.0);
        let uplift = land_deficit * input.recovery_gain * (0.30 + coastal_boost);
        if uplift <= 0.0 {
            continue;
        }

        let raised = (h + uplift).min(input.height_clamp);
        let changed = raised - h;
        if changed.abs() < 1e-8 {
            continue;
        }

        height_data[i] = raised;
        delta_abs += changed.abs();
    }

    ApplyLandRatioFloorOutput {
        height_data,
        delta_abs,
    }
}

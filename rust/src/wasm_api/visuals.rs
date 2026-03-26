use serde::Deserialize;
use wasm_bindgen::JsValue;

#[derive(Deserialize)]
struct RenderPositionsInput {
    base_positions: Vec<f32>,
    height_data: Vec<f32>,
    surface_mode: Option<String>,
}

pub(crate) fn build_render_positions_from_js(input_js: JsValue) -> Result<Vec<f32>, String> {
    let input = serde_wasm_bindgen::from_value::<RenderPositionsInput>(input_js)
        .map_err(|err| format!("invalid render positions input: {err}"))?;
    build_render_positions_native(input)
}

fn clamp_scalar(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}

fn build_render_positions_native(input: RenderPositionsInput) -> Result<Vec<f32>, String> {
    if !input.base_positions.len().is_multiple_of(3) {
        return Err("base_positions length must be divisible by 3".to_string());
    }
    let vertex_count = input.base_positions.len() / 3;
    if input.height_data.len() != vertex_count {
        return Err("base_positions and height_data length mismatch".to_string());
    }

    let mut positions = input.base_positions;
    let is_map_mode = input.surface_mode.as_deref() == Some("map");

    for i in (0..positions.len()).step_by(3) {
        let v = i / 3;
        let h = input.height_data[v];
        let x = positions[i];
        let y = positions[i + 1];
        let z = positions[i + 2];
        let render_height = h.clamp(-0.12, 1.2);
        let radius = 1.0 + render_height * 0.08;

        if is_map_mode {
            let len = (x * x + y * y + z * z).sqrt().max(1e-6);
            let nx = x / len;
            let ny = y / len;
            let nz = z / len;
            let longitude = nz.atan2(nx);
            let latitude = clamp_scalar(ny, -1.0, 1.0).asin();
            positions[i] = longitude / std::f32::consts::PI;
            positions[i + 1] = latitude / std::f32::consts::PI;
            positions[i + 2] = 0.0;
            continue;
        }

        positions[i] = x * radius;
        positions[i + 1] = y * radius;
        positions[i + 2] = z * radius;
    }

    Ok(positions)
}

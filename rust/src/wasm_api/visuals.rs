use serde::Deserialize;
use wasm_bindgen::JsValue;

#[derive(Deserialize)]
struct RenderPositionsInput {
    base_positions: Vec<f32>,
    height_data: Vec<f32>,
    surface_mode: Option<String>,
    view_mode: Option<String>,
    cell_metric: Option<String>,
    metric_data: Option<Vec<f32>>,
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

    let metric_displacement = build_metric_displacement(&input, vertex_count);
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

        let displacement = metric_displacement
            .as_ref()
            .and_then(|data| data.get(v))
            .copied()
            .unwrap_or(0.0);
        let displaced_radius = radius + displacement;
        positions[i] = x * displaced_radius;
        positions[i + 1] = y * displaced_radius;
        positions[i + 2] = z * displaced_radius;
    }

    Ok(positions)
}

const METRIC_DISPLACEMENT_SCALE: f32 = 0.06;

fn build_metric_displacement(
    input: &RenderPositionsInput,
    vertex_count: usize,
) -> Option<Vec<f32>> {
    if input.view_mode.as_deref() != Some("metric") {
        return None;
    }
    let metric_key = input.cell_metric.as_deref().unwrap_or("height");
    let (min, max) = metric_displacement_range(metric_key)?;
    let metric_data = input.metric_data.as_ref()?;
    if metric_data.len() != vertex_count {
        return None;
    }
    let span = (max - min).max(1e-6);
    let mut displacements = Vec::with_capacity(vertex_count);
    for value in metric_data {
        let normalized = ((*value - min) / span).clamp(0.0, 1.0);
        displacements.push(normalized * METRIC_DISPLACEMENT_SCALE);
    }
    Some(displacements)
}

fn metric_displacement_range(metric_key: &str) -> Option<(f32, f32)> {
    match metric_key {
        "temperature" => Some((-30.0, 45.0)),
        "precipitation" => Some((0.0, 4000.0)),
        "evapotranspiration" => Some((0.0, 2500.0)),
        "aridity" => Some((0.0, 4.0)),
        "runoff" => Some((0.0, 3000.0)),
        "river_flux" => Some((0.0, 1.0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_render_positions_native, RenderPositionsInput};

    fn sample_base_positions() -> Vec<f32> {
        vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
    }

    #[test]
    fn metric_displacement_changes_radius_in_globe_metric_mode() {
        let input = RenderPositionsInput {
            base_positions: sample_base_positions(),
            height_data: vec![0.2, 0.2],
            surface_mode: Some("globe".to_string()),
            view_mode: Some("metric".to_string()),
            cell_metric: Some("temperature".to_string()),
            metric_data: Some(vec![-30.0, 45.0]),
        };
        let positions = build_render_positions_native(input).expect("positions");
        assert!((positions[0] - 1.016).abs() < 1e-6);
        assert!((positions[4] - 1.076).abs() < 1e-6);
    }

    #[test]
    fn map_mode_keeps_flat_projection_even_with_metric_displacement_input() {
        let input = RenderPositionsInput {
            base_positions: sample_base_positions(),
            height_data: vec![0.2, 0.2],
            surface_mode: Some("map".to_string()),
            view_mode: Some("metric".to_string()),
            cell_metric: Some("precipitation".to_string()),
            metric_data: Some(vec![0.0, 4000.0]),
        };
        let positions = build_render_positions_native(input).expect("positions");
        assert!((positions[2] - 0.0).abs() < 1e-6);
        assert!((positions[5] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn unsupported_metric_keeps_previous_geometry_behavior() {
        let baseline_input = RenderPositionsInput {
            base_positions: sample_base_positions(),
            height_data: vec![0.2, 0.2],
            surface_mode: Some("globe".to_string()),
            view_mode: Some("metric".to_string()),
            cell_metric: Some("height".to_string()),
            metric_data: Some(vec![0.0, 1.0]),
        };
        let baseline_positions = build_render_positions_native(baseline_input).expect("baseline");

        let without_metric_input = RenderPositionsInput {
            base_positions: sample_base_positions(),
            height_data: vec![0.2, 0.2],
            surface_mode: Some("globe".to_string()),
            view_mode: Some("normal".to_string()),
            cell_metric: Some("height".to_string()),
            metric_data: None,
        };
        let without_metric_positions =
            build_render_positions_native(without_metric_input).expect("no metric");
        assert_eq!(baseline_positions, without_metric_positions);
    }
}

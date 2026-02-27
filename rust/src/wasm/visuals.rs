use serde::Deserialize;
use wasm_bindgen::JsValue;

#[derive(Deserialize)]
struct RenderPositionsInput {
    base_positions: Vec<f32>,
    height_data: Vec<f32>,
    surface_mode: Option<String>,
}

#[derive(Deserialize)]
struct VertexColorsInput {
    height_data: Vec<f32>,
    plate_id: Vec<u32>,
    river_flux: Vec<f32>,
    lake_depth: Option<Vec<f32>>,
    view_mode: String,
    debug_enabled: bool,
    tectonic_debug: Option<TectonicDebugInput>,
}

#[derive(Deserialize)]
struct TectonicDebugInput {
    trench: Option<Vec<f32>>,
    arc: Option<Vec<f32>>,
    backarc: Option<Vec<f32>>,
    #[serde(rename = "oceanOceanArc")]
    ocean_ocean_arc: Option<Vec<f32>>,
}

#[derive(Copy, Clone)]
struct Rgb {
    r: f32,
    g: f32,
    b: f32,
}

impl Rgb {
    const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    fn lerp(self, target: Self, t: f32) -> Self {
        let clamped = clamp_scalar(t, 0.0, 1.0);
        Self {
            r: self.r + (target.r - self.r) * clamped,
            g: self.g + (target.g - self.g) * clamped,
            b: self.b + (target.b - self.b) * clamped,
        }
    }
}

pub(crate) fn build_render_positions_from_js(input_js: JsValue) -> Result<Vec<f32>, String> {
    let input = serde_wasm_bindgen::from_value::<RenderPositionsInput>(input_js)
        .map_err(|err| format!("invalid render positions input: {err}"))?;
    build_render_positions_native(input)
}

pub(crate) fn build_vertex_colors_from_js(input_js: JsValue) -> Result<Vec<f32>, String> {
    let input = serde_wasm_bindgen::from_value::<VertexColorsInput>(input_js)
        .map_err(|err| format!("invalid vertex colors input: {err}"))?;
    build_vertex_colors_native(input)
}

fn color_deep_ocean() -> Rgb {
    rgb_from_hex(0x12406a)
}

fn color_plate_ocean_mix() -> Rgb {
    rgb_from_hex(0x0e2847)
}

fn color_lake() -> Rgb {
    rgb_from_hex(0x2f82c7)
}

fn color_river() -> Rgb {
    rgb_from_hex(0x4ca3dd)
}

fn color_debug_trench() -> Rgb {
    rgb_from_hex(0xff355e)
}

fn color_debug_backarc() -> Rgb {
    rgb_from_hex(0x7b61ff)
}

fn color_debug_arc() -> Rgb {
    rgb_from_hex(0xffb000)
}

fn color_debug_ocean_ocean_arc() -> Rgb {
    rgb_from_hex(0x2aff7a)
}

fn srgb_channel_to_linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn rgb_from_hex(hex: u32) -> Rgb {
    let r_srgb = ((hex >> 16) & 0xff) as f32 / 255.0;
    let g_srgb = ((hex >> 8) & 0xff) as f32 / 255.0;
    let b_srgb = (hex & 0xff) as f32 / 255.0;
    Rgb {
        r: srgb_channel_to_linear(r_srgb),
        g: srgb_channel_to_linear(g_srgb),
        b: srgb_channel_to_linear(b_srgb),
    }
}

fn clamp_scalar(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}

fn lerp_scalar(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep_scalar(value: f32, edge0: f32, edge1: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = clamp_scalar((value - edge0) / (edge1 - edge0), 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Rgb {
    let hue = h.rem_euclid(1.0);
    let sat = clamp_scalar(s, 0.0, 1.0);
    let light = clamp_scalar(l, 0.0, 1.0);
    if sat <= f32::EPSILON {
        return Rgb::new(light, light, light);
    }
    let q = if light < 0.5 {
        light * (1.0 + sat)
    } else {
        light + sat - light * sat
    };
    let p = 2.0 * light - q;
    Rgb::new(
        hue_to_rgb(p, q, hue + 1.0 / 3.0),
        hue_to_rgb(p, q, hue),
        hue_to_rgb(p, q, hue - 1.0 / 3.0),
    )
}

fn plate_mode_color(plate: u32, height_value: f32) -> Rgb {
    let hue = (((plate as f32) * 137.508) % 360.0) / 360.0;
    let saturation = 0.58;
    let lightness = if height_value > 0.0 { 0.54 } else { 0.38 };
    hsl_to_rgb(hue, saturation, lightness)
}

fn build_vertex_colors_native(input: VertexColorsInput) -> Result<Vec<f32>, String> {
    let vertex_count = input.height_data.len();
    if input.plate_id.len() != vertex_count || input.river_flux.len() != vertex_count {
        return Err("height_data/plate_id/river_flux length mismatch".to_string());
    }
    if let Some(lake_depth) = &input.lake_depth {
        if lake_depth.len() != vertex_count {
            return Err("lake_depth length mismatch".to_string());
        }
    }

    let mut colors = vec![0.0; vertex_count * 3];
    let is_plate_mode = input.view_mode == "plates";
    let is_normal_mode = input.view_mode == "normal";

    for v in 0..vertex_count {
        let h = input.height_data[v];
        let river = input.river_flux[v];
        let lake = input.lake_depth.as_ref().map_or(0.0, |depth| depth[v]);
        let mut color = if is_plate_mode {
            let mut c = plate_mode_color(input.plate_id[v], h);
            if h <= 0.0 {
                c = c.lerp(color_plate_ocean_mix(), 0.25);
            }
            c
        } else if h <= 0.0 {
            color_deep_ocean()
        } else {
            let t = h.min(1.0);
            let mut c = Rgb::new(
                lerp_scalar(0.18, 0.62, t),
                lerp_scalar(0.42, 0.56, t),
                lerp_scalar(0.20, 0.48, t),
            );
            if lake > 0.0 && h < 0.55 {
                let lake_depth_factor = smoothstep_scalar(lake, 0.008, 0.050);
                let lake_water_factor = smoothstep_scalar(river, 0.012, 0.080);
                let lake_mix = 0.75 * lake_depth_factor * lake_water_factor;
                if lake_mix > 0.01 {
                    c = c.lerp(color_lake(), lake_mix);
                }
            }
            if river > 0.10 && h < 0.45 {
                c = c.lerp(color_river(), (river * 0.45).min(0.35));
            }
            c
        };

        if input.debug_enabled && is_normal_mode {
            if let Some(debug) = &input.tectonic_debug {
                let trench = debug
                    .trench
                    .as_ref()
                    .map_or(0.0, |vals| vals.get(v).copied().unwrap_or(0.0));
                let arc = debug
                    .arc
                    .as_ref()
                    .map_or(0.0, |vals| vals.get(v).copied().unwrap_or(0.0));
                let backarc = debug
                    .backarc
                    .as_ref()
                    .map_or(0.0, |vals| vals.get(v).copied().unwrap_or(0.0));
                let ocean_ocean_arc = debug
                    .ocean_ocean_arc
                    .as_ref()
                    .map_or(0.0, |vals| vals.get(v).copied().unwrap_or(0.0));

                if trench > 0.01 {
                    color = color.lerp(color_debug_trench(), (trench * 0.90).min(0.80));
                }
                if backarc > 0.01 {
                    color = color.lerp(color_debug_backarc(), (backarc * 0.60).min(0.55));
                }
                if arc > 0.01 {
                    color = color.lerp(color_debug_arc(), (arc * 0.95).min(0.85));
                }
                if ocean_ocean_arc > 0.01 {
                    color = color.lerp(color_debug_ocean_ocean_arc(), ocean_ocean_arc.min(0.95));
                }
            }
        }

        let i = v * 3;
        colors[i] = color.r;
        colors[i + 1] = color.g;
        colors[i + 2] = color.b;
    }

    Ok(colors)
}

fn build_render_positions_native(input: RenderPositionsInput) -> Result<Vec<f32>, String> {
    if input.base_positions.len() % 3 != 0 {
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
        let render_height = if h > 0.0 { h } else { 0.0 };
        let radius = 1.0 + render_height * 0.04;

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

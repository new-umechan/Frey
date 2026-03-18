use crate::common::geom::{add3, clamp, dot3, length3, mul3, normalize3, project_to_tangent, sub3};

#[allow(dead_code)]
pub(crate) fn build_precipitation_map(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    river_rain_base: f32,
) -> Vec<f32> {
    let mut rain = vec![0.0_f32; positions.len()];

    for i in 0..positions.len() {
        let p = positions[i];
        let lat = clamp(p[1], -1.0_f32, 1.0_f32).asin();

        let lat_factor = (1.0_f32 - lat.abs() / (std::f32::consts::PI * 0.5_f32)).max(0.0_f32);
        let altitude_factor = 1.0_f32 + 0.20_f32 * height[i].max(0.0_f32);

        let wind_dir = prevailing_wind_dir(p, lat);
        let (upwind_h, downwind_h) =
            directional_neighbor_heights(i, positions, nbr_offsets, nbrs, height, wind_dir);

        let slope_signal = clamp((downwind_h - upwind_h) / 0.20_f32, -1.0_f32, 1.0_f32);
        let windward_boost = slope_signal.max(0.0_f32);
        let leeward_drop = (-slope_signal).max(0.0_f32);
        let barrier_strength = upwind_h.max(0.0_f32);

        let orographic_factor = clamp(
            1.0_f32 + 0.60_f32 * windward_boost * (1.0_f32 + 0.6_f32 * height[i].max(0.0_f32))
                - 1.10_f32 * leeward_drop * (1.0_f32 + 0.8_f32 * barrier_strength),
            0.12_f32,
            2.20_f32,
        );

        rain[i] = river_rain_base * lat_factor * altitude_factor * orographic_factor;
    }

    rain
}

#[allow(dead_code)]
fn prevailing_wind_dir(p: [f32; 3], lat: f32) -> [f32; 3] {
    let abs_lat = lat.abs();
    let zonal_sign = if abs_lat < std::f32::consts::FRAC_PI_6 {
        -1.0_f32
    } else if abs_lat < std::f32::consts::PI / 3.0_f32 {
        1.0_f32
    } else {
        -1.0_f32
    };

    let mut east = [-p[2], 0.0_f32, p[0]];
    if length3(east) < 1e-6_f32 {
        east = project_to_tangent([1.0_f32, 0.0_f32, 0.0_f32], p);
    }
    east = normalize3(east);

    let pole = if lat >= 0.0_f32 {
        [0.0_f32, 1.0_f32, 0.0_f32]
    } else {
        [0.0_f32, -1.0_f32, 0.0_f32]
    };
    let meridional = normalize3(project_to_tangent(pole, p));
    let meridional_sign = if abs_lat < std::f32::consts::FRAC_PI_6 {
        -1.0_f32
    } else {
        0.35_f32
    };

    normalize3(add3(
        mul3(east, zonal_sign),
        mul3(meridional, 0.25_f32 * meridional_sign),
    ))
}

#[allow(dead_code)]
fn directional_neighbor_heights(
    i: usize,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    wind_dir: [f32; 3],
) -> (f32, f32) {
    let p = positions[i];
    let start = nbr_offsets[i] as usize;
    let end = nbr_offsets[i + 1] as usize;

    let mut up_sum = 0.0_f32;
    let mut up_w = 0.0_f32;
    let mut down_sum = 0.0_f32;
    let mut down_w = 0.0_f32;

    for &n in &nbrs[start..end] {
        let n = n as usize;
        let edge = sub3(positions[n], p);
        let tangent = project_to_tangent(edge, p);
        let len = length3(tangent);
        if len < 1e-6_f32 {
            continue;
        }
        let dir = [tangent[0] / len, tangent[1] / len, tangent[2] / len];
        let score = dot3(dir, wind_dir);

        if score > 0.15_f32 {
            let w = score * score;
            down_sum += height[n] * w;
            down_w += w;
        } else if score < -0.15_f32 {
            let w = score * score;
            up_sum += height[n] * w;
            up_w += w;
        }
    }

    let upwind_h = if up_w > 0.0_f32 {
        up_sum / up_w
    } else {
        height[i]
    };
    let downwind_h = if down_w > 0.0_f32 {
        down_sum / down_w
    } else {
        height[i]
    };
    (upwind_h, downwind_h)
}

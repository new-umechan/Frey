use super::*;

fn build_precipitation_map(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    river_rain_base: f32,
) -> Vec<f32> {
    let mut rain = vec![0.0; positions.len()];

    for i in 0..positions.len() {
        let p = positions[i];
        let lat = clamp(p[1], -1.0, 1.0).asin();

        let lat_factor = (1.0 - lat.abs() / (std::f32::consts::PI * 0.5)).max(0.0);
        let altitude_factor = 1.0 + 0.20 * height[i].max(0.0);

        let wind_dir = prevailing_wind_dir(p, lat);
        let (upwind_h, downwind_h) =
            directional_neighbor_heights(i, positions, nbr_offsets, nbrs, height, wind_dir);

        let slope_signal = clamp((downwind_h - upwind_h) / 0.20, -1.0, 1.0);
        let windward_boost = slope_signal.max(0.0);
        let leeward_drop = (-slope_signal).max(0.0);
        let barrier_strength = upwind_h.max(0.0);

        let orographic_factor = clamp(
            1.0 + 0.60 * windward_boost * (1.0 + 0.6 * height[i].max(0.0))
                - 1.10 * leeward_drop * (1.0 + 0.8 * barrier_strength),
            0.12,
            2.20,
        );

        rain[i] = river_rain_base * lat_factor * altitude_factor * orographic_factor;
    }

    rain
}

fn prevailing_wind_dir(p: [f32; 3], lat: f32) -> [f32; 3] {
    let abs_lat = lat.abs();
    let zonal_sign = if abs_lat < std::f32::consts::FRAC_PI_6 {
        -1.0
    } else if abs_lat < std::f32::consts::PI / 3.0 {
        1.0
    } else {
        -1.0
    };

    let mut east = [-p[2], 0.0, p[0]];
    if length3(east) < 1e-6 {
        east = project_to_tangent([1.0, 0.0, 0.0], p);
    }
    east = normalize3(east);

    let pole = if lat >= 0.0 {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, -1.0, 0.0]
    };
    let meridional = normalize3(project_to_tangent(pole, p));
    let meridional_sign = if abs_lat < std::f32::consts::FRAC_PI_6 {
        -1.0
    } else {
        0.35
    };

    normalize3(add3(
        mul3(east, zonal_sign),
        mul3(meridional, 0.25 * meridional_sign),
    ))
}

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

    let mut up_sum = 0.0;
    let mut up_w = 0.0;
    let mut down_sum = 0.0;
    let mut down_w = 0.0;

    for &n in &nbrs[start..end] {
        let n = n as usize;
        let edge = sub3(positions[n], p);
        let tangent = project_to_tangent(edge, p);
        let len = length3(tangent);
        if len < 1e-6 {
            continue;
        }
        let dir = [tangent[0] / len, tangent[1] / len, tangent[2] / len];
        let score = dot3(dir, wind_dir);

        if score > 0.15 {
            let w = score * score;
            down_sum += height[n] * w;
            down_w += w;
        } else if score < -0.15 {
            let w = score * score;
            up_sum += height[n] * w;
            up_w += w;
        }
    }

    let upwind_h = if up_w > 0.0 { up_sum / up_w } else { height[i] };
    let downwind_h = if down_w > 0.0 {
        down_sum / down_w
    } else {
        height[i]
    };
    (upwind_h, downwind_h)
}

fn earth_preset(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    river_rain_base: f32,
) -> GeologyOutput {
    let mut height = vec![0.0; positions.len()];
    let mut plate_id = vec![0u32; positions.len()];

    let continents = [
        ([0.90, 0.15, 0.35], 0.55, 0.75),
        ([-0.70, 0.35, 0.10], 0.48, 0.70),
        ([0.10, -0.20, -0.95], 0.52, 0.72),
        ([-0.20, -0.65, 0.70], 0.42, 0.68),
    ];

    for (i, p) in positions.iter().enumerate() {
        let lat = clamp(p[1], -1.0, 1.0).asin();
        let mut land_signal = -0.35 + 0.10 * (2.0 * lat).cos();

        for (center, amp, width) in continents {
            let d = chord_distance(*p, center);
            let influence = amp * (-(d * d) / (2.0 * width * width)).exp();
            land_signal += influence;
        }

        let ridge = 0.08 * (6.0 * p[0]).sin() * (5.0 * p[2]).cos();
        height[i] = clamp(land_signal + ridge, -1.0, 1.0);

        plate_id[i] = if height[i] > 0.35 {
            1
        } else if height[i] > 0.05 {
            2
        } else if p[0] > 0.0 {
            0
        } else {
            3
        };
    }

    let (river_flux, river_next) = generate_rivers(
        positions,
        nbr_offsets,
        nbrs,
        &height,
        river_rain_base,
        0.015,
    );
    let lake_depth = compute_lake_depth_map(positions, nbr_offsets, nbrs, &height);
    let plate_count = {
        let mut unique = std::collections::HashSet::with_capacity(plate_id.len());
        for &pid in &plate_id {
            unique.insert(pid);
        }
        unique.len() as u32
    };
    let land_count = height.iter().filter(|&&h| h > 0.0).count();
    let land_ratio = land_count as f32 / (height.len().max(1) as f32);

    GeologyOutput {
        height,
        plate_id,
        plate_count,
        land_ratio,
        river_flux,
        river_next,
        lake_depth,
        vertex_weight: vec![0.66, 0.24, 0.20, 0.61],
        plate_is_ocean: vec![1, 0, 0, 1],
        plate_base_height: vec![-0.06, 0.14, 0.08, -0.03],
        plate_base_weight: vec![0.66, 0.24, 0.20, 0.61],
        vertex_age_norm: vec![0.4, 0.0, 0.0, 0.6],
        vertex_buoyancy: vec![-0.08, 0.14, 0.08, -0.10],
        debug_trench_strength: vec![0.0; positions.len()],
        debug_arc_strength: vec![0.0; positions.len()],
        debug_backarc_strength: vec![0.0; positions.len()],
        debug_ocean_ocean_arc_strength: vec![0.0; positions.len()],
    }
}

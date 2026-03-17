pub(crate) const EARTH_RADIUS_KM: f32 = 6_371.0;

pub(crate) fn edge_distance_km(a: [f32; 3], b: [f32; 3]) -> f32 {
    dot3(a, b).clamp(-1.0, 1.0).acos() * EARTH_RADIUS_KM
}

pub(crate) fn east_direction(pos: [f32; 3]) -> [f32; 3] {
    let east = [-pos[2], 0.0, pos[0]];
    let norm = length3(east);
    if norm > 1e-6 {
        [east[0] / norm, east[1] / norm, east[2] / norm]
    } else {
        [1.0, 0.0, 0.0]
    }
}

pub(crate) fn project_to_tangent(v: [f32; 3], normal: [f32; 3]) -> [f32; 3] {
    let scale = dot3(v, normal);
    [
        v[0] - normal[0] * scale,
        v[1] - normal[1] * scale,
        v[2] - normal[2] * scale,
    ]
}

pub(crate) fn scale3(v: [f32; 3], scale: f32) -> [f32; 3] {
    [v[0] * scale, v[1] * scale, v[2] * scale]
}

pub(crate) fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn length3(v: [f32; 3]) -> f32 {
    dot3(v, v).sqrt()
}

pub(crate) fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let norm = length3(v);
    if norm > 1e-6 {
        [v[0] / norm, v[1] / norm, v[2] / norm]
    } else {
        [0.0, 0.0, 0.0]
    }
}

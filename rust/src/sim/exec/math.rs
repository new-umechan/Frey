pub(crate) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn length3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

pub(crate) fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn fract01(v: f32) -> f32 {
    v - v.floor()
}

pub(crate) fn hash01(seed: u32) -> f32 {
    let s = ((seed as f32) * 12.9898 + 78.233).sin();
    fract01(s * 43_758.547)
}

pub(crate) fn seeded_axis(seed: u32) -> [f32; 3] {
    let z = 2.0 * hash01(seed ^ 0x7feb_352d) - 1.0;
    let phi = std::f32::consts::TAU * hash01(seed ^ 0x846c_a68b);
    let r = (1.0 - z * z).max(0.0).sqrt();
    [r * phi.cos(), z, r * phi.sin()]
}

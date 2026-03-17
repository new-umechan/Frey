fn edge_noise_signed(a: usize, b: usize, plate: usize) -> f32 {
    let (lo, hi) = if a <= b { (a as u64, b as u64) } else { (b as u64, a as u64) };
    let mut x = lo
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(hi.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add((plate as u64).wrapping_mul(0x94D0_49BB_1331_11EB));
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    let u = ((x >> 40) as u32) as f32 / 16_777_215.0;
    2.0 * u - 1.0
}

fn generate_plate_cost_warp_basis(
    count: usize,
    nbr_offsets: &[u32],
    nbrs: &[u32],
    rng: &mut DeterministicRng,
) -> [Vec<f32>; 3] {
    let mut a = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 6, 15, rng);
    let mut b = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 2, 6, rng);
    let mut c = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 1, 3, rng);
    normalize_zscore_if_var(&mut a);
    normalize_zscore_if_var(&mut b);
    normalize_zscore_if_var(&mut c);
    smooth_scalar_field(nbr_offsets, nbrs, &mut a, 1);
    smooth_scalar_field(nbr_offsets, nbrs, &mut b, 1);
    smooth_scalar_field(nbr_offsets, nbrs, &mut c, 1);
    normalize_zscore_if_var(&mut a);
    normalize_zscore_if_var(&mut b);
    normalize_zscore_if_var(&mut c);
    for i in 0..count {
        b[i] *= 1.30;
        c[i] *= 1.15;
    }
    [a, b, c]
}

fn sample_plate_warp_mid(
    profile: &PlateGrowthProfile,
    basis: &[Vec<f32>; 3],
    v0: usize,
    v1: usize,
) -> f32 {
    let mut acc = 0.0;
    for i in 0..3 {
        let mid = 0.5 * (basis[i][v0] + basis[i][v1]);
        acc += profile.warp_weights[i] * mid;
    }
    acc
}

fn local_preferred_tangent_axis(
    profile: &PlateGrowthProfile,
    position: [f32; 3],
    edge_dir: [f32; 3],
) -> [f32; 3] {
    let blend = 0.5 + 0.5 * clamp(dot3(position, profile.axis_blend_axis), -1.0, 1.0);
    let mixed = normalize3(add3(
        mul3(profile.preferred_axis, 1.0 - blend),
        mul3(profile.secondary_axis, blend),
    ));
    let tangent = normalize3(project_to_tangent(mixed, position));
    if length3(tangent) <= 1e-6 {
        let fallback = normalize3(project_to_tangent(profile.preferred_axis, position));
        if length3(fallback) <= 1e-6 {
            edge_dir
        } else {
            fallback
        }
    } else {
        tangent
    }
}

fn random_unit_vector3(rng: &mut DeterministicRng) -> [f32; 3] {
    let v = [
        rng.standard_normal(),
        rng.standard_normal(),
        rng.standard_normal(),
    ];
    let n = normalize3(v);
    if length3(n) <= 1e-6 {
        [1.0, 0.0, 0.0]
    } else {
        n
    }
}

fn local_plate_velocity(attr: &PlateAttr, plate: usize, position: [f32; 3]) -> [f32; 3] {
    let base = project_to_tangent(attr.velocity, position);
    let base_mag = length3(base);

    let blend = 0.5 + 0.5 * clamp(dot3(position, attr.drift_mix_axis), -1.0, 1.0);
    let mixed_axis = normalize3(add3(
        mul3(attr.drift_axis_primary, 1.0 - blend),
        mul3(attr.drift_axis_secondary, blend),
    ));
    let drift_axis = project_to_tangent(mixed_axis, position);
    let drift_mag = length3(drift_axis);

    let seed = plate as u32;
    let local_hash = 2.0 * trig_hash01(position, seed ^ 0x9e37_79b9) - 1.0;
    let local_scale = attr.drift_variability * local_hash;

    if drift_mag <= 1e-6 {
        return base;
    }
    let drift_dir = mul3(drift_axis, 1.0 / drift_mag);
    let mixed = add3(base, mul3(drift_dir, base_mag * local_scale));
    let tangent = project_to_tangent(mixed, position);
    if length3(tangent) <= 1e-6 {
        base
    } else {
        tangent
    }
}


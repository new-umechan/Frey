use super::*;

pub(super) fn compute_spherical_coords(positions: &[[f32; 3]]) -> Vec<(f32, f32)> {
    positions
        .iter()
        .map(|p| {
            let theta = clamp(p[1], -1.0, 1.0).acos();
            let lambda = p[2].atan2(p[0]);
            (theta, lambda)
        })
        .collect()
}

pub(super) fn evaluate_phi(
    spherical: &[(f32, f32)],
    harmonic_max_l: u32,
    spectral_alpha: f32,
    rng: &mut DeterministicRng,
) -> Vec<f32> {
    let mut coeffs: Vec<Vec<f32>> = Vec::with_capacity((harmonic_max_l + 1) as usize);
    coeffs.push(vec![0.0]);
    coeffs.push(vec![0.0, 0.0, 0.0]);

    for l in 2..=harmonic_max_l {
        let sigma = 1.0 / (l as f32).powf(spectral_alpha);
        let len = (2 * l + 1) as usize;
        let mut arr = vec![0.0; len];
        for value in &mut arr {
            let z = rng.standard_normal();
            *value = sigma * z;
        }
        coeffs.push(arr);
    }

    let mut phi = vec![0.0; spherical.len()];
    for (i, (theta, lambda)) in spherical.iter().enumerate() {
        let mut sum = 0.0;
        for l in 2..=harmonic_max_l {
            for m in -(l as i32)..=(l as i32) {
                let c = coeffs[l as usize][(m + l as i32) as usize];
                sum += c * real_spherical_harmonic(l as i32, m, *theta, *lambda);
            }
        }
        phi[i] = sum;
    }
    phi
}

pub(super) fn real_spherical_harmonic(l: i32, m: i32, theta: f32, lambda: f32) -> f32 {
    let abs_m = m.abs();
    let x = theta.cos();
    let p_lm = associated_legendre(l, abs_m, x);

    let normalization = (((2 * l + 1) as f32 / (4.0 * std::f32::consts::PI))
        * factorial((l - abs_m) as u32)
        / factorial((l + abs_m) as u32))
    .sqrt();

    if m > 0 {
        (2.0_f32).sqrt() * normalization * p_lm * (abs_m as f32 * lambda).cos()
    } else if m < 0 {
        (2.0_f32).sqrt() * normalization * p_lm * (abs_m as f32 * lambda).sin()
    } else {
        normalization * p_lm
    }
}

pub(super) fn associated_legendre(l: i32, m: i32, x: f32) -> f32 {
    if m > l {
        return 0.0;
    }

    let mut p_mm = 1.0;
    if m > 0 {
        let root = (1.0 - x * x).max(0.0).sqrt();
        let mut factor = 1.0;
        for _ in 1..=m {
            p_mm *= -factor * root;
            factor += 2.0;
        }
    }

    if l == m {
        return p_mm;
    }

    let p_m1m = x * (2 * m + 1) as f32 * p_mm;
    if l == m + 1 {
        return p_m1m;
    }

    let mut p_prev = p_mm;
    let mut p_curr = p_m1m;

    for ll in (m + 2)..=l {
        let p_next =
            (((2 * ll - 1) as f32) * x * p_curr - ((ll + m - 1) as f32) * p_prev) / (ll - m) as f32;
        p_prev = p_curr;
        p_curr = p_next;
    }

    p_curr
}

pub(super) fn factorial(n: u32) -> f32 {
    if n <= 1 {
        return 1.0;
    }
    (2..=n).fold(1.0, |acc, v| acc * v as f32)
}

pub(super) fn normalize_zscore(data: &mut [f32]) {
    let mean = data.iter().sum::<f32>() / data.len() as f32;
    let variance = data
        .iter()
        .map(|v| {
            let d = *v - mean;
            d * d
        })
        .sum::<f32>()
        / data.len() as f32;
    let std = variance.sqrt().max(1e-6);

    for v in data {
        *v = (*v - mean) / std;
    }
}

pub(super) fn normalize_zscore_if_var(data: &mut [f32]) {
    if data.is_empty() {
        return;
    }
    let mean = data.iter().sum::<f32>() / data.len() as f32;
    let variance = data
        .iter()
        .map(|v| {
            let d = *v - mean;
            d * d
        })
        .sum::<f32>()
        / data.len() as f32;
    if variance < 1e-8 {
        data.fill(0.0);
        return;
    }
    let std = variance.sqrt();
    for v in data {
        *v = (*v - mean) / std;
    }
}

pub(super) fn generate_frequency_bands(
    spherical: &[(f32, f32)],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    harmonic_max_l: u32,
    spectral_alpha: f32,
    rng: &mut DeterministicRng,
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let low_max = harmonic_max_l.clamp(3, 5);
    let mid_max = (harmonic_max_l + 3).max(low_max + 1).min(10);

    let mut low = evaluate_phi_band(spherical, 2, low_max, spectral_alpha + 0.35, rng);
    let mut mid = evaluate_phi_band(spherical, low_max + 1, mid_max, spectral_alpha, rng);
    if mid.iter().all(|v| v.abs() < 1e-7) {
        mid = generate_smoothed_noise_band(spherical.len(), nbr_offsets, nbrs, 3, 1, rng);
    }

    let mut high = generate_smoothed_noise_band(spherical.len(), nbr_offsets, nbrs, 1, 4, rng);

    normalize_zscore_if_var(&mut low);
    normalize_zscore_if_var(&mut mid);
    normalize_zscore_if_var(&mut high);

    (low, mid, high)
}

pub(super) fn evaluate_phi_band(
    spherical: &[(f32, f32)],
    l_min: u32,
    harmonic_max_l: u32,
    spectral_alpha: f32,
    rng: &mut DeterministicRng,
) -> Vec<f32> {
    if l_min > harmonic_max_l {
        return vec![0.0; spherical.len()];
    }

    let mut coeffs: Vec<Vec<f32>> = vec![Vec::new(); (harmonic_max_l + 1) as usize];
    for l in l_min..=harmonic_max_l {
        let sigma = 1.0 / (l as f32).powf(spectral_alpha.max(0.1));
        let len = (2 * l + 1) as usize;
        let mut arr = vec![0.0; len];
        for value in &mut arr {
            *value = sigma * rng.standard_normal();
        }
        coeffs[l as usize] = arr;
    }

    let mut out = vec![0.0; spherical.len()];
    for (i, (theta, lambda)) in spherical.iter().enumerate() {
        let mut sum = 0.0;
        for l in l_min..=harmonic_max_l {
            for m in -(l as i32)..=(l as i32) {
                let c = coeffs[l as usize][(m + l as i32) as usize];
                sum += c * real_spherical_harmonic(l as i32, m, *theta, *lambda);
            }
        }
        out[i] = sum;
    }
    out
}

pub(super) fn generate_smoothed_noise_band(
    count: usize,
    nbr_offsets: &[u32],
    nbrs: &[u32],
    smooth_short: u32,
    smooth_long: u32,
    rng: &mut DeterministicRng,
) -> Vec<f32> {
    let mut raw = vec![0.0; count];
    for v in &mut raw {
        *v = rng.gen_range_f32(-1.0, 1.0);
    }

    let mut a = raw.clone();
    let mut b = raw;
    smooth_scalar_field(nbr_offsets, nbrs, &mut a, smooth_short);
    smooth_scalar_field(nbr_offsets, nbrs, &mut b, smooth_long.max(smooth_short));

    a.iter().zip(b.iter()).map(|(x, y)| x - y).collect()
}

pub(super) fn smooth_scalar_field(nbr_offsets: &[u32], nbrs: &[u32], field: &mut [f32], iter: u32) {
    if iter == 0 || field.is_empty() {
        return;
    }
    let mut buf = field.to_vec();
    for _ in 0..iter {
        for v in 0..field.len() {
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            if start == end {
                buf[v] = field[v];
                continue;
            }
            let mut sum = field[v];
            let mut wsum = 1.0;
            for &n in &nbrs[start..end] {
                sum += field[n as usize];
                wsum += 1.0;
            }
            buf[v] = sum / wsum;
        }
        field.copy_from_slice(&buf);
    }
}

pub(super) fn compute_plate_boundary_proximity(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    max_hops: u32,
) -> Vec<f32> {
    let mut dist = vec![u32::MAX; plate_id.len()];
    let mut frontier = Vec::<usize>::new();

    for v in 0..plate_id.len() {
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        for &n in &nbrs[start..end] {
            if plate_id[v] != plate_id[n as usize] {
                dist[v] = 0;
                frontier.push(v);
                break;
            }
        }
    }

    let mut head = 0usize;
    while head < frontier.len() {
        let v = frontier[head];
        head += 1;
        let d = dist[v];
        if d >= max_hops {
            continue;
        }
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        for &n in &nbrs[start..end] {
            let n = n as usize;
            if dist[n] > d + 1 {
                dist[n] = d + 1;
                frontier.push(n);
            }
        }
    }

    dist.iter()
        .map(|&d| {
            if d == u32::MAX {
                0.0
            } else {
                (1.0 - d as f32 / (max_hops.max(1) as f32 + 1.0)).max(0.0)
            }
        })
        .collect()
}

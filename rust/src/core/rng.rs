use crate::TerrainParams;

pub(super) struct DeterministicRng {
    state: u64,
    cached_normal: Option<f32>,
}

impl DeterministicRng {
    fn from_seed_bytes(seed: [u8; 16]) -> Self {
        let mut lo = [0u8; 8];
        let mut hi = [0u8; 8];
        lo.copy_from_slice(&seed[..8]);
        hi.copy_from_slice(&seed[8..]);
        let mut state = u64::from_le_bytes(lo) ^ u64::from_le_bytes(hi).rotate_left(7);
        if state == 0 {
            state = 0x9E37_79B9_7F4A_7C15;
        }
        Self {
            state,
            cached_normal: None,
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn next_f32(&mut self) -> f32 {
        let v = (self.next_u64() >> 40) as u32;
        v as f32 / 16_777_216.0
    }

    pub(super) fn gen_range_f32(&mut self, min: f32, max: f32) -> f32 {
        if min >= max {
            min
        } else {
            min + (max - min) * self.next_f32()
        }
    }

    pub(super) fn gen_range_u32_inclusive(&mut self, min: u32, max: u32) -> u32 {
        if min >= max {
            min
        } else {
            min + (self.next_u64() % (max - min + 1) as u64) as u32
        }
    }

    pub(super) fn gen_range_usize(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            min
        } else {
            min + (self.next_u64() % (max - min) as u64) as usize
        }
    }

    pub(super) fn bernoulli(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }

    pub(super) fn standard_normal(&mut self) -> f32 {
        if let Some(v) = self.cached_normal.take() {
            return v;
        }

        let u1 = self.next_f32().max(1e-7);
        let u2 = self.next_f32();
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * std::f32::consts::PI * u2;
        let z0 = r * theta.cos();
        let z1 = r * theta.sin();
        self.cached_normal = Some(z1);
        z0
    }
}

pub(super) fn rng_from_seed(seed: &str, params: &TerrainParams) -> DeterministicRng {
    let canonical = format!(
        "{{\"l_max\":{},\"alpha\":{:.8},\"num_plates_min\":{},\"num_plates_max\":{},\"ocean_plate_ratio\":{:.8},\"boundary_band\":{:.8},\"uplift_gain\":{:.8},\"subduct_gain\":{:.8},\"divergent_gain\":{:.8},\"smooth_iter\":{},\"smooth_lambda\":{:.8},\"river_rain_base\":{:.8},\"river_accum_threshold\":{:.8},\"erosion_iter\":{},\"hydraulic_erode_rate\":{:.8},\"hydraulic_deposit_rate\":{:.8},\"sediment_capacity_gain\":{:.8},\"erosion_min_slope\":{:.8},\"erosion_max_delta_per_iter\":{:.8},\"coastal_deposit_rate\":{:.8},\"shallow_sea_floor\":{:.8}}}",
        params.l_max,
        params.alpha,
        params.num_plates_min,
        params.num_plates_max,
        params.ocean_plate_ratio,
        params.boundary_band,
        params.uplift_gain,
        params.subduct_gain,
        params.divergent_gain,
        params.smooth_iter,
        params.smooth_lambda,
        params.river_rain_base,
        params.river_accum_threshold,
        params.erosion_iter,
        params.hydraulic_erode_rate,
        params.hydraulic_deposit_rate,
        params.sediment_capacity_gain,
        params.erosion_min_slope,
        params.erosion_max_delta_per_iter,
        params.coastal_deposit_rate,
        params.shallow_sea_floor,
    );

    let mut source = Vec::new();
    source.extend_from_slice(seed.as_bytes());
    source.extend_from_slice(canonical.as_bytes());
    let digest = pseudo_sha256(&source);

    let mut seed16 = [0u8; 16];
    seed16.copy_from_slice(&digest[..16]);
    DeterministicRng::from_seed_bytes(seed16)
}

fn pseudo_sha256(input: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in 0..4u64 {
        let mut block = Vec::with_capacity(input.len() + 8);
        block.extend_from_slice(input);
        block.extend_from_slice(&i.to_le_bytes());
        let h = fnv1a64(&block).wrapping_add(0x9E37_79B9_7F4A_7C15_u64.wrapping_mul(i + 1));
        out[(i as usize) * 8..(i as usize + 1) * 8].copy_from_slice(&h.to_le_bytes());
    }
    out
}

fn fnv1a64(input: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for b in input {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3_u64);
    }
    hash
}

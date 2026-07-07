use std::env;
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::sim::geology_types::GeologyParams;

const OCEANIC_AGE_DEPTH_MAX_MYR: f32 = 100.0;
const RIDGE_DISTANCE_BIN_WIDTH_KM: f32 = 250.0;
const CONTINENTAL_MASK_MAGIC: &[u8; 8] = b"GEOCNTL1";
const HEIGHT_TO_METERS: f32 = 6000.0;
const INUNDATION_SCENARIOS_METERS: [f32; 5] = [1.0, 5.0, 10.0, 20.0, 50.0];

#[derive(Debug, Clone)]
struct OceanicAgeRef {
    age: Vec<f32>,
}

#[derive(Debug, Clone)]
struct TerrainRef {
    height: Vec<f32>,
}

#[derive(Debug, Clone)]
struct PlateBoundaryRef {
    ridge_distance_km: Vec<f32>,
}

#[derive(Debug, Clone)]
struct ContinentalMaskRef {
    mask: Vec<u8>,
}

#[derive(Clone)]
struct BenchRunMetadata {
    run_id: String,
    repeat_index: Option<u32>,
    repeat_total: Option<u32>,
    git_commit: Option<String>,
}

#[derive(Clone)]
struct AgeDepthMetrics {
    oceanic_age_spearman: Option<f32>,
    oceanic_age_bin_spearman: Option<f32>,
    oceanic_age_coverage_ratio: Option<f32>,
    oceanic_age_valid_cells: usize,
    oceanic_age_total_cells: usize,
    oceanic_age_bin_count: usize,
    oceanic_age_populated_bins: usize,
}

#[derive(Clone)]
struct Diagnostics {
    generated_land_ratio: f32,
    oceanic_age_min_myr: Option<f32>,
    oceanic_age_max_myr: Option<f32>,
    mean_depth: Option<f32>,
    inundation: Vec<InundationScenario>,
}

#[derive(Clone)]
struct InundationScenario {
    sea_level_rise_m: f32,
    generated_land_ratio: f32,
    reference_land_ratio: f32,
    generated_newly_inundated_ratio: f32,
    reference_newly_inundated_ratio: f32,
}

#[derive(Clone)]
struct OceanicAgeBins {
    bin_count: usize,
    populated_bins: usize,
}

#[derive(Clone)]
struct RidgeDistanceMetrics {
    ridge_distance_spearman: Option<f32>,
    ridge_distance_bin_spearman: Option<f32>,
    ridge_distance_coverage_ratio: Option<f32>,
    ridge_distance_valid_cells: usize,
    ridge_distance_total_cells: usize,
    ridge_distance_bin_count: usize,
    ridge_distance_populated_bins: usize,
}

#[derive(Clone)]
struct RidgeDistanceBins {
    bin_count: usize,
    populated_bins: usize,
}

#[derive(Clone)]
struct ContinentalHypsometryMetrics {
    continental_mean_height: Option<f32>,
    ocean_mean_height: Option<f32>,
    continental_median_height: Option<f32>,
    ocean_median_height: Option<f32>,
    continental_ocean_mean_gap: Option<f32>,
    continental_ocean_median_gap: Option<f32>,
    continental_ocean_overlap_ratio: Option<f32>,
    continental_valid_cells: usize,
    ocean_valid_cells: usize,
}

fn main() {
    let geology_params = GeologyParams {
        level: 6,
        ..Default::default()
    };
    let mesh_level = geology_params.level;
    let seed = env::var("GEOLOGY_BENCH_SEED")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "earth".to_string());
    let run_id = env::var("GEOLOGY_BENCH_RUN_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(default_run_id);
    let repeat_index = parse_env_u32("GEOLOGY_BENCH_REPEAT_INDEX");
    let repeat_total = parse_env_u32("GEOLOGY_BENCH_REPEAT_TOTAL");
    let git_commit = env::var("GEOLOGY_BENCH_GIT_COMMIT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(resolve_git_commit);

    let terrain_ref_path = match find_cache("terrain_ref.bin") {
        Some(path) => path,
        None => {
            println!("=== Geology Solo Bench ===");
            println!("-- Terrain Input: SKIPPED (benches/data/terrain_ref.bin not found) --");
            return;
        }
    };
    let terrain_ref = match load_terrain_ref(&terrain_ref_path) {
        Ok(value) => value,
        Err(error) => {
            println!("=== Geology Solo Bench ===");
            println!(
                "-- Terrain Input: ERROR ({}: {}) --",
                terrain_ref_path.display(),
                error
            );
            return;
        }
    };

    let age_ref_path = match find_cache("oceanic_crust_age_ref.bin") {
        Some(path) => path,
        None => {
            println!("=== Geology Solo Bench ===");
            println!(
                "-- Oceanic Age Input: SKIPPED (benches/data/oceanic_crust_age_ref.bin not found) --"
            );
            println!("To generate:");
            println!("  pnpm bench:resample:geology-age");
            return;
        }
    };
    let age_ref = match load_oceanic_age_ref(&age_ref_path) {
        Ok(value) => value,
        Err(error) => {
            println!("=== Geology Solo Bench ===");
            println!(
                "-- Oceanic Age Input: ERROR ({}: {}) --",
                age_ref_path.display(),
                error
            );
            return;
        }
    };
    let ridge_ref_path = find_cache("plate_boundary_ref.bin");
    let ridge_ref = match ridge_ref_path.as_ref() {
        Some(path) => match load_plate_boundary_ref(path) {
            Ok(value) => Some(value),
            Err(error) => {
                println!("=== Geology Solo Bench ===");
                println!("-- Terrain Source: {} --", terrain_ref_path.display());
                println!("-- Oceanic Age Source: {} --", age_ref_path.display());
                println!(
                    "-- Plate Boundary Input: ERROR ({}: {}) --",
                    path.display(),
                    error
                );
                return;
            }
        },
        None => None,
    };
    let continental_ref_path = find_cache("continental_mask_ref.bin");
    let continental_ref = match continental_ref_path.as_ref() {
        Some(path) => match load_continental_mask_ref(path) {
            Ok(value) => Some(value),
            Err(error) => {
                println!("=== Geology Solo Bench ===");
                println!("-- Terrain Source: {} --", terrain_ref_path.display());
                println!("-- Oceanic Age Source: {} --", age_ref_path.display());
                if let Some(ridge_path) = ridge_ref_path.as_ref() {
                    println!("-- Plate Boundary Source: {} --", ridge_path.display());
                }
                println!(
                    "-- Continental Mask Input: ERROR ({}: {}) --",
                    path.display(),
                    error
                );
                return;
            }
        },
        None => None,
    };

    let build_started_at = Instant::now();
    let (terrain, positions, _nbr_offsets, _nbrs) =
        sim::build_geology_with_mesh(seed.as_str(), geology_params.clone());
    let geology_build_ms = build_started_at.elapsed().as_secs_f64() * 1000.0;
    let cell_count = positions.len();

    if terrain_ref.height.len() != cell_count {
        println!("=== Geology Solo Bench ===");
        println!("-- Terrain Source: {} --", terrain_ref_path.display());
        println!("-- Oceanic Age Source: {} --", age_ref_path.display());
        println!(
            "-- Input: ERROR (terrain cell count mismatch: terrain_ref={} cell_count={}) --",
            terrain_ref.height.len(),
            cell_count
        );
        return;
    }
    if age_ref.age.len() != cell_count {
        println!("=== Geology Solo Bench ===");
        println!("-- Terrain Source: {} --", terrain_ref_path.display());
        println!("-- Oceanic Age Source: {} --", age_ref_path.display());
        println!(
            "-- Input: ERROR (cell count mismatch: age_ref={} cell_count={}) --",
            age_ref.age.len(),
            cell_count
        );
        return;
    }
    if let Some(ridge_ref) = ridge_ref.as_ref() {
        if ridge_ref.ridge_distance_km.len() != cell_count {
            println!("=== Geology Solo Bench ===");
            println!("-- Terrain Source: {} --", terrain_ref_path.display());
            println!("-- Oceanic Age Source: {} --", age_ref_path.display());
            if let Some(path) = ridge_ref_path.as_ref() {
                println!("-- Plate Boundary Source: {} --", path.display());
            }
            println!(
                "-- Input: ERROR (cell count mismatch: ridge_ref={} cell_count={}) --",
                ridge_ref.ridge_distance_km.len(),
                cell_count
            );
            return;
        }
    }
    if let Some(continental_ref) = continental_ref.as_ref() {
        if continental_ref.mask.len() != cell_count {
            println!("=== Geology Solo Bench ===");
            println!("-- Terrain Source: {} --", terrain_ref_path.display());
            println!("-- Oceanic Age Source: {} --", age_ref_path.display());
            if let Some(path) = ridge_ref_path.as_ref() {
                println!("-- Plate Boundary Source: {} --", path.display());
            }
            if let Some(path) = continental_ref_path.as_ref() {
                println!("-- Continental Mask Source: {} --", path.display());
            }
            println!(
                "-- Input: ERROR (cell count mismatch: continental_mask_ref={} cell_count={}) --",
                continental_ref.mask.len(),
                cell_count
            );
            return;
        }
    }

    let age_metrics = compute_age_depth_metrics(&terrain_ref.height, &age_ref.age);
    let diagnostics = compute_diagnostics(&terrain.height, &terrain_ref.height, &age_ref.age);
    let ridge_metrics = ridge_ref.as_ref().map(|ridge_ref| {
        compute_ridge_distance_depth_metrics(
            &terrain_ref.height,
            &age_ref.age,
            &ridge_ref.ridge_distance_km,
        )
    });
    let continental_metrics = continental_ref.as_ref().map(|continental_ref| {
        compute_continental_hypsometry_metrics(&terrain_ref.height, &continental_ref.mask)
    });
    let run_metadata = BenchRunMetadata {
        run_id,
        repeat_index,
        repeat_total,
        git_commit,
    };

    println!("=== Geology Solo Bench ===");
    println!("seed={}", seed);
    println!("-- Terrain Source: {} --", terrain_ref_path.display());
    println!("-- Oceanic Age Source: {} --", age_ref_path.display());
    println!(
        "runtime: geology_build_ms={:.3} mesh_level={} cell_count={}",
        geology_build_ms, mesh_level, cell_count
    );
    match age_metrics.oceanic_age_spearman {
        Some(value) => println!("oceanic_age_depth_spearman={:.3}", value),
        None => println!("oceanic_age_depth_spearman=n/a"),
    }
    match age_metrics.oceanic_age_bin_spearman {
        Some(value) => println!("oceanic_age_bin_spearman={:.3}", value),
        None => println!("oceanic_age_bin_spearman=n/a"),
    }
    match age_metrics.oceanic_age_coverage_ratio {
        Some(value) => println!("oceanic_age_coverage_ratio={:.3}", value),
        None => println!("oceanic_age_coverage_ratio=n/a"),
    }
    if let Some(ridge_metrics) = ridge_metrics.as_ref() {
        match ridge_metrics.ridge_distance_spearman {
            Some(value) => println!("ridge_distance_depth_spearman={:.3}", value),
            None => println!("ridge_distance_depth_spearman=n/a"),
        }
        match ridge_metrics.ridge_distance_bin_spearman {
            Some(value) => println!("ridge_distance_bin_spearman={:.3}", value),
            None => println!("ridge_distance_bin_spearman=n/a"),
        }
        match ridge_metrics.ridge_distance_coverage_ratio {
            Some(value) => println!("ridge_distance_coverage_ratio={:.3}", value),
            None => println!("ridge_distance_coverage_ratio=n/a"),
        }
    } else {
        println!("ridge_distance_depth_spearman=SKIPPED");
        println!("ridge_distance_bin_spearman=SKIPPED");
        println!("ridge_distance_coverage_ratio=SKIPPED");
    }
    if let Some(continental_metrics) = continental_metrics.as_ref() {
        match continental_metrics.continental_ocean_mean_gap {
            Some(value) => println!("continental_ocean_mean_gap={:.3}", value),
            None => println!("continental_ocean_mean_gap=n/a"),
        }
        match continental_metrics.continental_ocean_median_gap {
            Some(value) => println!("continental_ocean_median_gap={:.3}", value),
            None => println!("continental_ocean_median_gap=n/a"),
        }
        match continental_metrics.continental_ocean_overlap_ratio {
            Some(value) => println!("continental_ocean_overlap_ratio={:.3}", value),
            None => println!("continental_ocean_overlap_ratio=n/a"),
        }
    } else {
        println!("continental_ocean_mean_gap=SKIPPED");
        println!("continental_ocean_median_gap=SKIPPED");
        println!("continental_ocean_overlap_ratio=SKIPPED");
    }
    println!(
        "diagnostics: generated_land_ratio={:.3} oceanic_valid_cells={} oceanic_total_cells={}",
        diagnostics.generated_land_ratio,
        age_metrics.oceanic_age_valid_cells,
        age_metrics.oceanic_age_total_cells
    );
    for scenario in &diagnostics.inundation {
        println!(
            "inundation_{}m: generated_land_ratio={:.3} reference_land_ratio={:.3} generated_newly_inundated_ratio={:.3} reference_newly_inundated_ratio={:.3}",
            scenario.sea_level_rise_m as i32,
            scenario.generated_land_ratio,
            scenario.reference_land_ratio,
            scenario.generated_newly_inundated_ratio,
            scenario.reference_newly_inundated_ratio
        );
    }

    if let Err(error) = append_score_record_jsonl(
        &run_metadata,
        seed.as_str(),
        mesh_level,
        cell_count,
        geology_build_ms,
        &age_metrics,
        ridge_metrics.as_ref(),
        continental_metrics.as_ref(),
        &diagnostics,
    ) {
        println!("score_save=ERROR ({})", error);
    } else {
        println!("score_save=OK");
    }
}

fn parse_env_u32(key: &str) -> Option<u32> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u32>().ok())
}

fn default_run_id() -> String {
    format!(
        "default-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    )
}

fn resolve_git_commit() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|commit| commit.trim().to_string())
}

fn find_cache(filename: &str) -> Option<PathBuf> {
    let candidates = [
        Path::new("benches/data").join(filename),
        Path::new("../benches/data").join(filename),
        Path::new("data").join(filename),
        Path::new("../data").join(filename),
    ];
    candidates
        .iter()
        .find(|path| path.exists())
        .map(|path| (*path).to_path_buf())
}

fn load_oceanic_age_ref(path: &Path) -> Result<OceanicAgeRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    decode_oceanic_age_ref(&mut reader)
        .map_err(|error| format!("failed to decode {}: {}", path.display(), error))
}

fn load_terrain_ref(path: &Path) -> Result<TerrainRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    decode_terrain_ref(&mut reader)
        .map_err(|error| format!("failed to decode {}: {}", path.display(), error))
}

fn load_plate_boundary_ref(path: &Path) -> Result<PlateBoundaryRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    decode_plate_boundary_ref(&mut reader)
        .map_err(|error| format!("failed to decode {}: {}", path.display(), error))
}

fn load_continental_mask_ref(path: &Path) -> Result<ContinentalMaskRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    decode_continental_mask_ref(&mut reader)
        .map_err(|error| format!("failed to decode {}: {}", path.display(), error))
}

fn decode_oceanic_age_ref<R: Read>(reader: &mut R) -> Result<OceanicAgeRef, String> {
    const MAGIC: &[u8; 8] = b"GEOAG001";
    let mut magic = [0_u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read magic: {}", error))?;
    if &magic != MAGIC {
        return Err("invalid magic (expected GEOAG001)".to_string());
    }

    let version = read_u32_le(reader)?;
    if version != 1 {
        return Err(format!("unsupported version: {}", version));
    }

    let cell_count = read_u64_le(reader)? as usize;
    let age = read_f32_vec(reader, cell_count)?;
    Ok(OceanicAgeRef { age })
}

fn decode_terrain_ref<R: Read>(reader: &mut R) -> Result<TerrainRef, String> {
    const MAGIC: &[u8; 8] = b"TERRREF1";
    let mut magic = [0_u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read magic: {}", error))?;
    if &magic != MAGIC {
        return Err("invalid magic (expected TERRREF1)".to_string());
    }

    let version = read_u32_le(reader)?;
    if version != 1 {
        return Err(format!("unsupported version: {}", version));
    }

    let cell_count = read_u64_le(reader)? as usize;
    let height = read_f32_vec(reader, cell_count)?;
    Ok(TerrainRef { height })
}

fn decode_plate_boundary_ref<R: Read>(reader: &mut R) -> Result<PlateBoundaryRef, String> {
    const MAGIC: &[u8; 8] = b"GEORIDG1";
    let mut magic = [0_u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read magic: {}", error))?;
    if &magic != MAGIC {
        return Err("invalid magic (expected GEORIDG1)".to_string());
    }

    let version = read_u32_le(reader)?;
    if version != 1 {
        return Err(format!("unsupported version: {}", version));
    }

    let cell_count = read_u64_le(reader)? as usize;
    let ridge_distance_km = read_f32_vec(reader, cell_count)?;
    Ok(PlateBoundaryRef { ridge_distance_km })
}

fn decode_continental_mask_ref<R: Read>(reader: &mut R) -> Result<ContinentalMaskRef, String> {
    let mut magic = [0u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read continental mask magic: {}", error))?;
    if &magic != CONTINENTAL_MASK_MAGIC {
        return Err("invalid magic (expected GEOCNTL1)".to_string());
    }

    let version = read_u32_le(reader)?;
    if version != 1 {
        return Err(format!("unsupported version: {}", version));
    }

    let cell_count = read_u64_le(reader)? as usize;
    let mask = read_u8_vec(reader, cell_count)?;
    Ok(ContinentalMaskRef { mask })
}

fn read_u32_le<R: Read>(reader: &mut R) -> Result<u32, String> {
    let mut bytes = [0_u8; 4];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("failed to read u32: {}", error))?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64_le<R: Read>(reader: &mut R) -> Result<u64, String> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("failed to read u64: {}", error))?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_u8_vec<R: Read>(reader: &mut R, len: usize) -> Result<Vec<u8>, String> {
    let mut values = vec![0_u8; len];
    reader
        .read_exact(&mut values)
        .map_err(|error| format!("failed to read u8 vec: {}", error))?;
    Ok(values)
}

fn read_f32_vec<R: Read>(reader: &mut R, len: usize) -> Result<Vec<f32>, String> {
    let mut bytes = vec![0_u8; len.saturating_mul(4)];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("failed to read f32 vec: {}", error))?;
    let mut values = Vec::with_capacity(len);
    for chunk in bytes.chunks_exact(4) {
        values.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Ok(values)
}

fn compute_age_depth_metrics(height: &[f32], age: &[f32]) -> AgeDepthMetrics {
    let mut samples_age = Vec::new();
    let mut samples_depth = Vec::new();
    let mut oceanic_total = 0usize;
    let mut oceanic_valid = 0usize;
    let mut oceanic_min = f32::INFINITY;
    let mut oceanic_max = f32::NEG_INFINITY;

    for i in 0..height.len().min(age.len()) {
        let age_v = age[i];
        if !age_v.is_finite() || age_v < 0.0 || age_v > OCEANIC_AGE_DEPTH_MAX_MYR {
            continue;
        }
        oceanic_total += 1;
        oceanic_min = oceanic_min.min(age_v);
        oceanic_max = oceanic_max.max(age_v);
        if height[i].is_finite() && height[i] <= 0.0 {
            let depth = -height[i];
            samples_age.push(age_v);
            samples_depth.push(depth);
            oceanic_valid += 1;
        }
    }

    let oceanic_age_spearman = spearman(&samples_age, &samples_depth);
    let (oceanic_age_bin_spearman, bins) = binned_spearman(&samples_age, &samples_depth, 20.0);
    let oceanic_age_coverage_ratio = if oceanic_total > 0 {
        Some(oceanic_valid as f32 / oceanic_total as f32)
    } else {
        None
    };

    AgeDepthMetrics {
        oceanic_age_spearman,
        oceanic_age_bin_spearman,
        oceanic_age_coverage_ratio,
        oceanic_age_valid_cells: oceanic_valid,
        oceanic_age_total_cells: oceanic_total,
        oceanic_age_bin_count: bins.bin_count,
        oceanic_age_populated_bins: bins.populated_bins,
    }
}

fn compute_diagnostics(height: &[f32], reference_height: &[f32], age: &[f32]) -> Diagnostics {
    let mut land_cells = 0usize;
    let mut oceanic_depth_sum = 0.0_f32;
    let mut oceanic_depth_count = 0usize;
    for i in 0..height.len().min(age.len()) {
        if height[i].is_finite() && height[i] > 0.0 {
            land_cells += 1;
        }
        if age[i].is_finite()
            && age[i] >= 0.0
            && age[i] <= OCEANIC_AGE_DEPTH_MAX_MYR
            && height[i].is_finite()
            && height[i] <= 0.0
        {
            oceanic_depth_sum += -height[i];
            oceanic_depth_count += 1;
        }
    }

    let cell_count = height.len().max(1);
    let generated_land_ratio = land_cells as f32 / cell_count as f32;
    let mean_depth = if oceanic_depth_count > 0 {
        Some(oceanic_depth_sum / oceanic_depth_count as f32)
    } else {
        None
    };

    let mut oceanic_min = None;
    let mut oceanic_max = None;
    for &value in age {
        if value.is_finite() && value >= 0.0 && value <= OCEANIC_AGE_DEPTH_MAX_MYR {
            oceanic_min = Some(oceanic_min.map_or(value, |current: f32| current.min(value)));
            oceanic_max = Some(oceanic_max.map_or(value, |current: f32| current.max(value)));
        }
    }

    Diagnostics {
        generated_land_ratio,
        oceanic_age_min_myr: oceanic_min,
        oceanic_age_max_myr: oceanic_max,
        mean_depth,
        inundation: compute_inundation_scenarios(height, reference_height),
    }
}

fn compute_inundation_scenarios(
    generated_height: &[f32],
    reference_height: &[f32],
) -> Vec<InundationScenario> {
    let count = generated_height.len().min(reference_height.len());
    if count == 0 {
        return Vec::new();
    }

    INUNDATION_SCENARIOS_METERS
        .iter()
        .copied()
        .map(|sea_level_rise_m| {
            let offset = sea_level_rise_m / HEIGHT_TO_METERS;
            let mut generated_land = 0usize;
            let mut reference_land = 0usize;
            let mut generated_newly_inundated = 0usize;
            let mut reference_newly_inundated = 0usize;

            for i in 0..count {
                let generated = generated_height[i];
                let reference = reference_height[i];
                if generated.is_finite() {
                    if generated > offset {
                        generated_land += 1;
                    } else if generated > 0.0 {
                        generated_newly_inundated += 1;
                    }
                }
                if reference.is_finite() {
                    if reference > offset {
                        reference_land += 1;
                    } else if reference > 0.0 {
                        reference_newly_inundated += 1;
                    }
                }
            }

            let denom = count as f32;
            InundationScenario {
                sea_level_rise_m,
                generated_land_ratio: generated_land as f32 / denom,
                reference_land_ratio: reference_land as f32 / denom,
                generated_newly_inundated_ratio: generated_newly_inundated as f32 / denom,
                reference_newly_inundated_ratio: reference_newly_inundated as f32 / denom,
            }
        })
        .collect()
}

fn compute_ridge_distance_depth_metrics(
    height: &[f32],
    age: &[f32],
    ridge_distance_km: &[f32],
) -> RidgeDistanceMetrics {
    let mut samples_distance = Vec::new();
    let mut samples_depth = Vec::new();
    let mut oceanic_total = 0usize;
    let mut oceanic_valid = 0usize;

    for i in 0..height.len().min(age.len()).min(ridge_distance_km.len()) {
        let age_v = age[i];
        if !age_v.is_finite() || age_v < 0.0 || age_v > OCEANIC_AGE_DEPTH_MAX_MYR {
            continue;
        }
        oceanic_total += 1;

        let depth_v = height[i];
        let ridge_v = ridge_distance_km[i];
        if depth_v.is_finite() && depth_v <= 0.0 && ridge_v.is_finite() && ridge_v >= 0.0 {
            samples_distance.push(ridge_v);
            samples_depth.push(-depth_v);
            oceanic_valid += 1;
        }
    }

    let ridge_distance_spearman = spearman(&samples_distance, &samples_depth);
    let (ridge_distance_bin_spearman, bins) = binned_distance_spearman(
        &samples_distance,
        &samples_depth,
        RIDGE_DISTANCE_BIN_WIDTH_KM,
    );
    let ridge_distance_coverage_ratio = if oceanic_total > 0 {
        Some(oceanic_valid as f32 / oceanic_total as f32)
    } else {
        None
    };

    RidgeDistanceMetrics {
        ridge_distance_spearman,
        ridge_distance_bin_spearman,
        ridge_distance_coverage_ratio,
        ridge_distance_valid_cells: oceanic_valid,
        ridge_distance_total_cells: oceanic_total,
        ridge_distance_bin_count: bins.bin_count,
        ridge_distance_populated_bins: bins.populated_bins,
    }
}

fn compute_continental_hypsometry_metrics(
    height: &[f32],
    mask: &[u8],
) -> ContinentalHypsometryMetrics {
    let mut continental = Vec::new();
    let mut ocean = Vec::new();

    for i in 0..height.len().min(mask.len()) {
        let value = height[i];
        if !value.is_finite() {
            continue;
        }
        if mask[i] == 1 {
            continental.push(value);
        } else {
            ocean.push(value);
        }
    }

    let continental_mean_height = if continental.is_empty() {
        None
    } else {
        Some(continental.iter().sum::<f32>() / continental.len() as f32)
    };
    let ocean_mean_height = if ocean.is_empty() {
        None
    } else {
        Some(ocean.iter().sum::<f32>() / ocean.len() as f32)
    };
    let continental_median_height = if continental.is_empty() {
        None
    } else {
        Some(median(&continental))
    };
    let ocean_median_height = if ocean.is_empty() {
        None
    } else {
        Some(median(&ocean))
    };
    let continental_ocean_mean_gap = match (continental_mean_height, ocean_mean_height) {
        (Some(continent), Some(ocean)) => Some(continent - ocean),
        _ => None,
    };
    let continental_ocean_median_gap = match (continental_median_height, ocean_median_height) {
        (Some(continent), Some(ocean)) => Some(continent - ocean),
        _ => None,
    };
    let continental_ocean_overlap_ratio = overlap_coefficient(&continental, &ocean);

    ContinentalHypsometryMetrics {
        continental_mean_height,
        ocean_mean_height,
        continental_median_height,
        ocean_median_height,
        continental_ocean_mean_gap,
        continental_ocean_median_gap,
        continental_ocean_overlap_ratio,
        continental_valid_cells: continental.len(),
        ocean_valid_cells: ocean.len(),
    }
}

fn binned_spearman(
    age: &[f32],
    depth: &[f32],
    bin_width_myr: f32,
) -> (Option<f32>, OceanicAgeBins) {
    if age.len() != depth.len() || age.is_empty() || !(bin_width_myr > 0.0) {
        return (
            None,
            OceanicAgeBins {
                bin_count: 0,
                populated_bins: 0,
            },
        );
    }

    let max_age = age
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .fold(None, |acc: Option<f32>, value| {
            Some(acc.map_or(value, |current| current.max(value)))
        });
    let Some(max_age) = max_age else {
        return (
            None,
            OceanicAgeBins {
                bin_count: 0,
                populated_bins: 0,
            },
        );
    };

    let bin_count = (max_age / bin_width_myr).ceil() as usize + 1;
    let mut bucket_depths = vec![Vec::<f32>::new(); bin_count];
    for i in 0..age.len() {
        let age_v = age[i];
        let depth_v = depth[i];
        if !age_v.is_finite() || !depth_v.is_finite() || age_v < 0.0 {
            continue;
        }
        let bin = (age_v / bin_width_myr).floor() as usize;
        if let Some(bucket) = bucket_depths.get_mut(bin.min(bin_count - 1)) {
            bucket.push(depth_v);
        }
    }

    let mut centers = Vec::new();
    let mut medians = Vec::new();
    for (index, values) in bucket_depths.iter().enumerate() {
        if values.is_empty() {
            continue;
        }
        centers.push((index as f32 + 0.5) * bin_width_myr);
        medians.push(median(values));
    }

    let populated_bins = medians.len();
    (
        spearman(&centers, &medians),
        OceanicAgeBins {
            bin_count,
            populated_bins,
        },
    )
}

fn binned_distance_spearman(
    distance: &[f32],
    depth: &[f32],
    bin_width_km: f32,
) -> (Option<f32>, RidgeDistanceBins) {
    if distance.len() != depth.len() || distance.is_empty() || !(bin_width_km > 0.0) {
        return (
            None,
            RidgeDistanceBins {
                bin_count: 0,
                populated_bins: 0,
            },
        );
    }

    let max_distance = distance
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .fold(None, |acc: Option<f32>, value| {
            Some(acc.map_or(value, |current| current.max(value)))
        });
    let Some(max_distance) = max_distance else {
        return (
            None,
            RidgeDistanceBins {
                bin_count: 0,
                populated_bins: 0,
            },
        );
    };

    let bin_count = (max_distance / bin_width_km).ceil() as usize + 1;
    let mut bucket_depths = vec![Vec::<f32>::new(); bin_count];
    for i in 0..distance.len() {
        let distance_v = distance[i];
        let depth_v = depth[i];
        if !distance_v.is_finite() || !depth_v.is_finite() || distance_v < 0.0 {
            continue;
        }
        let bin = (distance_v / bin_width_km).floor() as usize;
        if let Some(bucket) = bucket_depths.get_mut(bin.min(bin_count - 1)) {
            bucket.push(depth_v);
        }
    }

    let mut centers = Vec::new();
    let mut medians = Vec::new();
    for (index, values) in bucket_depths.iter().enumerate() {
        if values.is_empty() {
            continue;
        }
        centers.push((index as f32 + 0.5) * bin_width_km);
        medians.push(median(values));
    }

    let populated_bins = medians.len();
    (
        spearman(&centers, &medians),
        RidgeDistanceBins {
            bin_count,
            populated_bins,
        },
    )
}

fn median(values: &[f32]) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) * 0.5
    } else {
        sorted[mid]
    }
}

fn overlap_coefficient(a: &[f32], b: &[f32]) -> Option<f32> {
    let a_valid: Vec<f32> = a
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    let b_valid: Vec<f32> = b
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if a_valid.len() < 2 || b_valid.len() < 2 {
        return None;
    }

    let min_value = a_valid
        .iter()
        .chain(b_valid.iter())
        .copied()
        .fold(f32::INFINITY, |current, value| current.min(value));
    let max_value = a_valid
        .iter()
        .chain(b_valid.iter())
        .copied()
        .fold(f32::NEG_INFINITY, |current, value| current.max(value));
    if !min_value.is_finite() || !max_value.is_finite() || max_value <= min_value {
        return None;
    }

    let bin_count = 64usize;
    let bin_width = (max_value - min_value) / bin_count as f32;
    if !(bin_width > 0.0) {
        return None;
    }

    let mut hist_a = vec![0.0_f32; bin_count];
    let mut hist_b = vec![0.0_f32; bin_count];
    for value in a_valid {
        let index = (((value - min_value) / bin_width).floor() as isize)
            .clamp(0, (bin_count - 1) as isize) as usize;
        hist_a[index] += 1.0;
    }
    for value in b_valid {
        let index = (((value - min_value) / bin_width).floor() as isize)
            .clamp(0, (bin_count - 1) as isize) as usize;
        hist_b[index] += 1.0;
    }

    let sum_a = hist_a.iter().sum::<f32>();
    let sum_b = hist_b.iter().sum::<f32>();
    if sum_a <= 0.0 || sum_b <= 0.0 {
        return None;
    }
    let mut overlap = 0.0_f32;
    for index in 0..bin_count {
        overlap += (hist_a[index] / sum_a).min(hist_b[index] / sum_b);
    }
    Some(overlap.clamp(0.0, 1.0))
}

fn spearman(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.len() < 2 {
        return None;
    }
    let mut filtered_a = Vec::new();
    let mut filtered_b = Vec::new();
    for i in 0..a.len() {
        if a[i].is_finite() && b[i].is_finite() {
            filtered_a.push(a[i]);
            filtered_b.push(b[i]);
        }
    }
    if filtered_a.len() < 2 {
        return None;
    }
    let rank_a = rank_with_ties(&filtered_a);
    let rank_b = rank_with_ties(&filtered_b);
    pearson_corr(&rank_a, &rank_b)
}

fn rank_with_ties(values: &[f32]) -> Vec<f32> {
    let mut indexed = values
        .iter()
        .copied()
        .enumerate()
        .collect::<Vec<(usize, f32)>>();
    indexed.sort_by(|left, right| {
        left.1
            .partial_cmp(&right.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut ranks = vec![0.0_f32; values.len()];
    let mut start = 0usize;
    while start < indexed.len() {
        let mut end = start + 1;
        while end < indexed.len() && (indexed[end].1 - indexed[start].1).abs() <= 1e-12 {
            end += 1;
        }
        let avg_rank = (start + 1 + end) as f32 / 2.0;
        for i in start..end {
            ranks[indexed[i].0] = avg_rank;
        }
        start = end;
    }
    ranks
}

fn pearson_corr(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.len() < 2 {
        return None;
    }
    let n = a.len() as f32;
    let mean_a = a.iter().sum::<f32>() / n;
    let mean_b = b.iter().sum::<f32>() / n;
    let mut numerator = 0.0_f32;
    let mut denom_a = 0.0_f32;
    let mut denom_b = 0.0_f32;
    for i in 0..a.len() {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        numerator += da * db;
        denom_a += da * da;
        denom_b += db * db;
    }
    let denom = (denom_a * denom_b).sqrt();
    if denom <= 1e-12 {
        return None;
    }
    Some((numerator / denom).clamp(-1.0, 1.0))
}

fn score_output_path() -> PathBuf {
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let candidate =
            PathBuf::from(manifest_dir).join("../results/geology_solo_main_scores.jsonl");
        if let Some(parent) = candidate.parent() {
            if parent.exists() {
                return candidate;
            }
        }
    }
    let candidates = [
        Path::new("benches/results/geology_solo_main_scores.jsonl"),
        Path::new("results/geology_solo_main_scores.jsonl"),
        Path::new("../benches/results/geology_solo_main_scores.jsonl"),
        Path::new("../results/geology_solo_main_scores.jsonl"),
        Path::new("../../benches/results/geology_solo_main_scores.jsonl"),
    ];
    for candidate in candidates {
        if let Some(parent) = candidate.parent() {
            if parent.exists() {
                return candidate.to_path_buf();
            }
        }
    }
    candidates[0].to_path_buf()
}

fn json_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn format_json_number(value: Option<f32>) -> String {
    if let Some(numeric) = value {
        if numeric.is_finite() {
            return format!("{:.6}", numeric);
        }
    }
    "null".to_string()
}

fn append_score_record_jsonl(
    run_metadata: &BenchRunMetadata,
    seed: &str,
    mesh_level: u32,
    cell_count: usize,
    geology_build_ms: f64,
    age_metrics: &AgeDepthMetrics,
    ridge_metrics: Option<&RidgeDistanceMetrics>,
    continental_metrics: Option<&ContinentalHypsometryMetrics>,
    diagnostics: &Diagnostics,
) -> Result<(), String> {
    let timestamp_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system time error: {}", error))?
        .as_millis();

    let mut line = String::new();
    write!(
        &mut line,
        "{{\"schema_version\":1,\"timestamp_unix_ms\":{},",
        timestamp_unix_ms
    )
    .map_err(|error| format!("failed to write json: {}", error))?;
    write!(
        &mut line,
        "\"bench\":\"geology_solo\",\"run_id\":\"{}\",",
        json_escape(&run_metadata.run_id)
    )
    .map_err(|error| format!("failed to write json: {}", error))?;
    write!(
        &mut line,
        "\"repeat_index\":{},\"repeat_total\":{},",
        run_metadata
            .repeat_index
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string()),
        run_metadata
            .repeat_total
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string())
    )
    .map_err(|error| format!("failed to write json: {}", error))?;
    write!(
        &mut line,
        "\"git_commit\":{},\"seed\":\"{}\",",
        run_metadata
            .git_commit
            .as_ref()
            .map(|value| format!("\"{}\"", json_escape(value)))
            .unwrap_or_else(|| "null".to_string()),
        json_escape(seed)
    )
    .map_err(|error| format!("failed to write json: {}", error))?;
    write!(
        &mut line,
        "\"mesh_level\":{},\"cell_count\":{},\"runtime\":{{\"geology_build_ms\":{:.3}}},",
        mesh_level, cell_count, geology_build_ms
    )
    .map_err(|error| format!("failed to write json: {}", error))?;
    write!(
        &mut line,
        "\"phase2\":{{\"state\":\"ready\",\"metrics\":{{\"oceanic_age_depth_spearman\":{},\"oceanic_age_bin_spearman\":{},\"oceanic_age_coverage_ratio\":{}",
        format_json_number(age_metrics.oceanic_age_spearman),
        format_json_number(age_metrics.oceanic_age_bin_spearman),
        format_json_number(age_metrics.oceanic_age_coverage_ratio)
    )
    .map_err(|error| format!("failed to write json: {}", error))?;
    if let Some(ridge_metrics) = ridge_metrics {
        write!(
            &mut line,
            ",\"ridge_distance_depth_spearman\":{},\"ridge_distance_bin_spearman\":{},\"ridge_distance_coverage_ratio\":{}",
            format_json_number(ridge_metrics.ridge_distance_spearman),
            format_json_number(ridge_metrics.ridge_distance_bin_spearman),
            format_json_number(ridge_metrics.ridge_distance_coverage_ratio)
        )
        .map_err(|error| format!("failed to write json: {}", error))?;
    }
    if let Some(continental_metrics) = continental_metrics {
        write!(
            &mut line,
            ",\"continental_ocean_mean_gap\":{},\"continental_ocean_median_gap\":{},\"continental_ocean_overlap_ratio\":{}",
            format_json_number(continental_metrics.continental_ocean_mean_gap),
            format_json_number(continental_metrics.continental_ocean_median_gap),
            format_json_number(continental_metrics.continental_ocean_overlap_ratio)
        )
        .map_err(|error| format!("failed to write json: {}", error))?;
    }
    write!(&mut line, "}}}},").map_err(|error| format!("failed to write json: {}", error))?;
    write!(
        &mut line,
        "\"diagnostics\":{{\"generated_land_ratio\":{:.6},\"oceanic_age_min_myr\":{},\"oceanic_age_max_myr\":{},\"mean_depth\":{},\"oceanic_age_valid_cells\":{},\"oceanic_age_total_cells\":{},\"oceanic_age_bin_count\":{},\"oceanic_age_populated_bins\":{}",
        diagnostics.generated_land_ratio,
        format_json_number(diagnostics.oceanic_age_min_myr),
        format_json_number(diagnostics.oceanic_age_max_myr),
        format_json_number(diagnostics.mean_depth),
        age_metrics.oceanic_age_valid_cells,
        age_metrics.oceanic_age_total_cells,
        age_metrics.oceanic_age_bin_count,
        age_metrics.oceanic_age_populated_bins
    )
    .map_err(|error| format!("failed to write json: {}", error))?;
    if let Some(ridge_metrics) = ridge_metrics {
        write!(
            &mut line,
            ",\"ridge_distance_valid_cells\":{},\"ridge_distance_total_cells\":{},\"ridge_distance_bin_count\":{},\"ridge_distance_populated_bins\":{}",
            ridge_metrics.ridge_distance_valid_cells,
            ridge_metrics.ridge_distance_total_cells,
            ridge_metrics.ridge_distance_bin_count,
            ridge_metrics.ridge_distance_populated_bins
        )
        .map_err(|error| format!("failed to write json: {}", error))?;
    }
    if let Some(continental_metrics) = continental_metrics {
        write!(
            &mut line,
            ",\"continental_valid_cells\":{},\"continental_ocean_cells\":{},\"continental_mean_height\":{},\"ocean_mean_height\":{},\"continental_median_height\":{},\"ocean_median_height\":{}",
            continental_metrics.continental_valid_cells,
            continental_metrics.ocean_valid_cells,
            format_json_number(continental_metrics.continental_mean_height),
            format_json_number(continental_metrics.ocean_mean_height),
            format_json_number(continental_metrics.continental_median_height),
            format_json_number(continental_metrics.ocean_median_height)
        )
        .map_err(|error| format!("failed to write json: {}", error))?;
    }
    write!(&mut line, ",\"coastal_inundation_response\":[")
        .map_err(|error| format!("failed to write json: {}", error))?;
    for (index, scenario) in diagnostics.inundation.iter().enumerate() {
        if index > 0 {
            write!(&mut line, ",").map_err(|error| format!("failed to write json: {}", error))?;
        }
        write!(
            &mut line,
            "{{\"sea_level_rise_m\":{:.1},\"generated_land_ratio\":{:.6},\"reference_land_ratio\":{:.6},\"land_ratio_gap\":{:.6},\"generated_newly_inundated_ratio\":{:.6},\"reference_newly_inundated_ratio\":{:.6},\"newly_inundated_ratio_gap\":{:.6}}}",
            scenario.sea_level_rise_m,
            scenario.generated_land_ratio,
            scenario.reference_land_ratio,
            scenario.generated_land_ratio - scenario.reference_land_ratio,
            scenario.generated_newly_inundated_ratio,
            scenario.reference_newly_inundated_ratio,
            scenario.generated_newly_inundated_ratio - scenario.reference_newly_inundated_ratio
        )
        .map_err(|error| format!("failed to write json: {}", error))?;
    }
    write!(&mut line, "]").map_err(|error| format!("failed to write json: {}", error))?;
    write!(&mut line, "}}}}\n").map_err(|error| format!("failed to write json: {}", error))?;

    let output_path = score_output_path();
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {}", parent.display(), error))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&output_path)
        .map_err(|error| format!("failed to open {}: {}", output_path.display(), error))?;
    file.write_all(line.as_bytes())
        .map_err(|error| format!("failed to write {}: {}", output_path.display(), error))
}

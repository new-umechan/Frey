use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::sim::geology_types::GeologyParams;
use frey_wasm::world;

#[derive(Clone, Copy)]
struct Region {
    id: &'static str,
    lat: f32,
    lon: f32,
}

#[derive(Clone, Copy)]
struct RankAssertion {
    left: &'static str,
    right: &'static str,
}

#[derive(Clone, Copy)]
struct BiomeAssertion {
    region: &'static str,
    expected: u8,
}

struct RankOutcome {
    passed: bool,
}

struct BiomeOutcome {
    passed: bool,
}

#[derive(Debug, Clone)]
struct TerrainRef {
    height: Vec<f32>,
}

#[derive(Debug, Clone)]
struct ClimateRef {
    temperature: Vec<f32>,
    precipitation: Vec<f32>,
}

#[derive(Debug, Clone)]
struct HydroRef {
    river_flow: Vec<f32>,
}

#[derive(Debug, Clone)]
struct EcologyRef {
    tree_cover: Vec<f32>,
    ground_cover: Vec<f32>,
    soil_fertility: Vec<f32>,
    biome: Vec<u8>,
    natural_mask: Vec<u8>,
    open_canopy_mask: Vec<u8>,
}

struct MetricSummary {
    matched: usize,
    total: usize,
    coverage_ratio: f32,
}

struct RunState {
    converged: bool,
    ticks_to_converge: usize,
}

const REGIONS: &[Region] = &[
    Region {
        id: "amazon_core",
        lat: -3.0,
        lon: -60.0,
    },
    Region {
        id: "congo_core",
        lat: -1.0,
        lon: 24.0,
    },
    Region {
        id: "serengeti",
        lat: -2.5,
        lon: 34.8,
    },
    Region {
        id: "great_plains",
        lat: 44.0,
        lon: -101.0,
    },
    Region {
        id: "sahara_core",
        lat: 23.0,
        lon: 13.0,
    },
    Region {
        id: "europe_temperate",
        lat: 49.0,
        lon: 14.0,
    },
    Region {
        id: "siberia_taiga",
        lat: 61.0,
        lon: 105.0,
    },
    Region {
        id: "yamal_tundra",
        lat: 70.0,
        lon: 70.0,
    },
    Region {
        id: "pantanal",
        lat: -17.0,
        lon: -57.0,
    },
    Region {
        id: "tibet_alpine",
        lat: 32.0,
        lon: 86.0,
    },
];

const BIOME_ASSERTIONS: &[BiomeAssertion] = &[
    BiomeAssertion {
        region: "amazon_core",
        expected: 0,
    },
    BiomeAssertion {
        region: "congo_core",
        expected: 0,
    },
    BiomeAssertion {
        region: "serengeti",
        expected: 1,
    },
    BiomeAssertion {
        region: "great_plains",
        expected: 3,
    },
    BiomeAssertion {
        region: "sahara_core",
        expected: 2,
    },
    BiomeAssertion {
        region: "europe_temperate",
        expected: 4,
    },
    BiomeAssertion {
        region: "siberia_taiga",
        expected: 5,
    },
    BiomeAssertion {
        region: "yamal_tundra",
        expected: 6,
    },
    BiomeAssertion {
        region: "pantanal",
        expected: 7,
    },
    BiomeAssertion {
        region: "tibet_alpine",
        expected: 8,
    },
];

const TREE_ASSERTIONS: &[RankAssertion] = &[
    RankAssertion {
        left: "amazon_core",
        right: "serengeti",
    },
    RankAssertion {
        left: "congo_core",
        right: "great_plains",
    },
    RankAssertion {
        left: "europe_temperate",
        right: "sahara_core",
    },
    RankAssertion {
        left: "siberia_taiga",
        right: "yamal_tundra",
    },
];

const GROUND_ASSERTIONS: &[RankAssertion] = &[
    RankAssertion {
        left: "serengeti",
        right: "sahara_core",
    },
    RankAssertion {
        left: "great_plains",
        right: "sahara_core",
    },
    RankAssertion {
        left: "pantanal",
        right: "tibet_alpine",
    },
];

fn main() {
    let geology_params = GeologyParams {
        level: 6,
        ..Default::default()
    };
    let mesh_level = geology_params.level as u32;
    let seed = "earth";

    let (mut terrain, positions, nbr_offsets, nbrs) =
        sim::build_geology_with_mesh(seed, geology_params);
    let cell_count = positions.len();

    let terrain_ref = match find_cache("terrain_ref.bin")
        .and_then(|path| load_terrain_ref(&path).ok().map(|r| (path, r)))
    {
        Some((path, r)) => {
            println!("=== Ecology Solo Bench ===");
            println!("-- Terrain Source: {} --", path.display());
            r
        }
        None => {
            println!("=== Ecology Solo Bench ===");
            println!("-- Terrain Input: SKIPPED (benches/data/terrain_ref.bin not found or decode failed) --");
            return;
        }
    };
    let climate_ref = match find_cache("climate_ref.bin")
        .and_then(|path| load_climate_ref(&path).ok().map(|r| (path, r)))
    {
        Some((path, r)) => {
            println!("-- Climate Source: {} --", path.display());
            r
        }
        None => {
            println!("-- Climate Input: SKIPPED (benches/data/climate_ref.bin not found or decode failed) --");
            return;
        }
    };
    let hydro_ref = match find_cache("hydro_ref.bin")
        .and_then(|path| load_hydro_ref(&path).ok().map(|r| (path, r)))
    {
        Some((path, r)) => {
            println!("-- Hydro Source: {} --", path.display());
            r
        }
        None => {
            println!(
                "-- Hydro Input: SKIPPED (benches/data/hydro_ref.bin not found or decode failed) --"
            );
            return;
        }
    };
    let (ecology_ref_path, ecology_ref) = match find_cache("ecology_ref.bin")
        .and_then(|path| load_ecology_ref(&path).ok().map(|r| (path, r)))
    {
        Some((path, r)) => {
            println!("-- Ecology Reference Source: {} --", path.display());
            (path, r)
        }
        None => {
            println!("-- Ecology Reference: SKIPPED (benches/data/ecology_ref.bin not found or decode failed) --");
            return;
        }
    };

    if terrain_ref.height.len() != cell_count
        || climate_ref.temperature.len() != cell_count
        || climate_ref.precipitation.len() != cell_count
        || hydro_ref.river_flow.len() != cell_count
        || ecology_ref.tree_cover.len() != cell_count
    {
        println!("-- Input: ERROR (cell count mismatch) --");
        return;
    }

    terrain.height = terrain_ref.height;
    let plate_id = terrain
        .plate_id
        .iter()
        .copied()
        .map(world::PlateId)
        .collect::<Vec<_>>();
    let geology = world::GeologyState {
        height: terrain.height,
        plate_id,
        erosion_rate: vec![0.0; cell_count],
        deposition_rate: vec![0.0; cell_count],
        volcanism: terrain.volcanism,
        vertex_buoyancy: terrain.vertex_buoyancy,
        geology_internal: vec![world::GeologyInternal::default(); cell_count],
        boundary_condition: vec![0.0; cell_count],
    };
    let mesh = world::WorldMesh {
        positions,
        nbr_offsets,
        nbrs,
    };
    let mut sim_world = world::World::new(mesh, geology);
    sim_world.clock.epoch = world::EraKind::Life;
    sim_world.clock.real_years_per_tick = world::EraKind::Life.real_years_per_tick();
    sim_world.clock.runtime_tick_ms = world::EraKind::Life.runtime_tick_ms();
    sim_world.clock.budgets = world::EraKind::Life.budgets();
    sim_world.state.climate.temperature = climate_ref.temperature;
    sim_world.state.climate.precipitation = climate_ref.precipitation;
    sim_world.state.hydrology.river_flow = hydro_ref.river_flow;
    sim_world.state.ecology.tree_cover.fill(0.0);
    sim_world.state.ecology.ground_cover.fill(0.0);
    sim_world.state.ecology.disturbance.fill(0.0);
    sim_world.state.ecology.soil_fertility.fill(0.35);

    let run_state = run_until_converged(&mut sim_world);
    println!();
    println!("-- Run State --");
    println!("converged:        {}", run_state.converged);
    println!("ticks_to_converge: {}", run_state.ticks_to_converge);
    if !run_state.converged {
        println!("run_state:        NOT_CONVERGED");
    }

    println!();
    println!("-- Main Evaluation --");
    let tree_rho = spearman_masked(
        &sim_world.state.ecology.tree_cover,
        &ecology_ref.tree_cover,
        &sim_world.state.geology.height,
        &ecology_ref.natural_mask,
    )
    .unwrap_or(f32::NAN);
    println!("tree_cover:       rho={:.3}", tree_rho);

    let ground_rho = spearman_masked_with_two_masks(
        &sim_world.state.ecology.ground_cover,
        &ecology_ref.ground_cover,
        &sim_world.state.geology.height,
        &ecology_ref.natural_mask,
        &ecology_ref.open_canopy_mask,
    )
    .unwrap_or(f32::NAN);
    println!("ground_cover:     rho={:.3}", ground_rho);

    let model_biome = sim_world
        .state
        .ecology
        .biome
        .iter()
        .copied()
        .map(biome_to_u8)
        .collect::<Vec<_>>();
    let (biome_macro_f1, biome_accuracy) = macro_f1_and_accuracy(
        &model_biome,
        &ecology_ref.biome,
        &sim_world.state.geology.height,
        &ecology_ref.natural_mask,
    );
    println!(
        "biome:            macro_f1={:.3} accuracy={:.3}",
        biome_macro_f1, biome_accuracy
    );

    println!();
    println!("-- Reference Evaluation --");
    let soil_rho = spearman_masked(
        &sim_world.state.ecology.soil_fertility,
        &ecology_ref.soil_fertility,
        &sim_world.state.geology.height,
        &ecology_ref.natural_mask,
    )
    .unwrap_or(f32::NAN);
    println!("soil_fertility:   rho={:.3}", soil_rho);

    let selection = build_region_selection(&sim_world.mesh.positions);
    let biome_diag = run_biome_assertions(&selection, &model_biome, BIOME_ASSERTIONS);
    let tree_diag = run_rank_assertions(
        &selection,
        &sim_world.state.ecology.tree_cover,
        TREE_ASSERTIONS,
    );
    let ground_diag = run_rank_assertions(
        &selection,
        &sim_world.state.ecology.ground_cover,
        GROUND_ASSERTIONS,
    );
    let biome_summary = summarize_biome(&biome_diag);
    let tree_summary = summarize_rank(&tree_diag);
    let ground_summary = summarize_rank(&ground_diag);

    println!();
    println!("-- Diagnostic Evaluation: Assertions --");
    println!(
        "[biome]           matched={}/{} coverage_ratio={:.3}",
        biome_summary.matched,
        biome_summary.total,
        biome_summary.coverage_ratio
    );
    println!(
        "[tree_cover]      matched={}/{} coverage_ratio={:.3}",
        tree_summary.matched,
        tree_summary.total,
        tree_summary.coverage_ratio
    );
    println!(
        "[ground_cover]    matched={}/{} coverage_ratio={:.3}",
        ground_summary.matched,
        ground_summary.total,
        ground_summary.coverage_ratio
    );

    let mean_coverage_ratio = (
        biome_summary.coverage_ratio + tree_summary.coverage_ratio + ground_summary.coverage_ratio
    ) / 3.0;
    println!();
    println!("-- Main Evaluation Summary: metrics_reported=3 --");
    println!("-- Reference Evaluation Summary: metrics_reported=1 --");
    println!(
        "-- Diagnostic Evaluation Summary: metrics=3 mean_coverage_ratio={:.3} --",
        mean_coverage_ratio
    );
    if run_state.converged {
        println!("-- Main Evaluation State: READY --");
    } else {
        println!("-- Main Evaluation State: NOT_CONVERGED --");
    }
    if let Err(error) = append_score_record_jsonl(
        seed,
        mesh_level,
        cell_count,
        &run_state,
        &ecology_ref_path,
        tree_rho,
        ground_rho,
        biome_macro_f1,
        biome_accuracy,
        soil_rho,
        &biome_summary,
        &tree_summary,
        &ground_summary,
    ) {
        println!("-- Score Save: ERROR ({}) --", error);
    } else {
        println!("-- Score Save: OK --");
    }
}

fn run_until_converged(world: &mut world::World) -> RunState {
    let ecology_budget = world.clock.budgets.ecology.max(1);
    let mut stable_ticks = 0usize;
    let max_ticks = 256usize;
    let mut ticks = 0usize;

    while ticks < max_ticks {
        let prev_tree = world.state.ecology.tree_cover.clone();
        let prev_ground = world.state.ecology.ground_cover.clone();
        let prev_soil = world.state.ecology.soil_fertility.clone();
        let prev_biome = world
            .state
            .ecology
            .biome
            .iter()
            .copied()
            .map(biome_to_u8)
            .collect::<Vec<_>>();

        sim::run_ecology_step_for_bench(world, ecology_budget);
        ticks += 1;

        let mut tree_delta = Vec::new();
        let mut ground_delta = Vec::new();
        let mut soil_delta = Vec::new();
        let mut biome_changed = 0usize;
        let mut land_count = 0usize;
        for i in 0..world.state.geology.height.len() {
            if world.state.geology.height[i] <= 0.0 {
                continue;
            }
            land_count += 1;
            tree_delta.push((world.state.ecology.tree_cover[i] - prev_tree[i]).abs());
            ground_delta.push((world.state.ecology.ground_cover[i] - prev_ground[i]).abs());
            soil_delta.push((world.state.ecology.soil_fertility[i] - prev_soil[i]).abs());
            if biome_to_u8(world.state.ecology.biome[i]) != prev_biome[i] {
                biome_changed += 1;
            }
        }

        let tree_p95 = percentile_sorted(&mut tree_delta, 0.95);
        let ground_p95 = percentile_sorted(&mut ground_delta, 0.95);
        let soil_p95 = percentile_sorted(&mut soil_delta, 0.95);
        let biome_change_ratio = if land_count > 0 {
            biome_changed as f32 / land_count as f32
        } else {
            0.0
        };

        let stable = tree_p95 < 0.002
            && ground_p95 < 0.002
            && soil_p95 < 0.001
            && biome_change_ratio < 0.001;
        if stable {
            stable_ticks += 1;
            if stable_ticks >= 8 {
                return RunState {
                    converged: true,
                    ticks_to_converge: ticks,
                };
            }
        } else {
            stable_ticks = 0;
        }
    }

    RunState {
        converged: false,
        ticks_to_converge: max_ticks,
    }
}

fn find_cache(name: &str) -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(format!("benches/data/{name}")),
        PathBuf::from(format!("../benches/data/{name}")),
    ];
    candidates.into_iter().find(|path| path.exists())
}

fn score_output_path() -> PathBuf {
    let candidates = [
        Path::new("benches/results/ecology_main_scores.jsonl"),
        Path::new("../benches/results/ecology_main_scores.jsonl"),
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

fn format_json_number(value: f32) -> String {
    if value.is_finite() {
        format!("{:.6}", value)
    } else {
        "null".to_string()
    }
}

#[allow(clippy::too_many_arguments)]
fn append_score_record_jsonl(
    seed: &str,
    mesh_level: u32,
    cell_count: usize,
    run_state: &RunState,
    reference_path: &Path,
    tree_rho: f32,
    ground_rho: f32,
    biome_macro_f1: f32,
    biome_accuracy: f32,
    soil_rho: f32,
    biome_summary: &MetricSummary,
    tree_summary: &MetricSummary,
    ground_summary: &MetricSummary,
) -> Result<(), String> {
    let timestamp_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system time error: {}", error))?
        .as_millis();

    let main_state_label = if run_state.converged {
        "ready"
    } else {
        "not_converged"
    };
    let line = format!(
        "{{\"timestamp_unix_ms\":{},\"bench\":\"ecology_solo\",\"seed\":\"{}\",\"mesh_level\":{},\"cell_count\":{},\"run_state\":{{\"converged\":{},\"ticks_to_converge\":{}}},\"main_evaluation\":{{\"state\":\"{}\",\"ref_path\":\"{}\",\"error\":null,\"metrics\":{{\"tree_cover_rho\":{},\"ground_cover_rho\":{},\"biome_macro_f1\":{},\"biome_accuracy\":{}}}}},\"reference_evaluation\":{{\"state\":\"ready\",\"metrics\":{{\"soil_fertility_rho\":{}}}}},\"diagnostic_evaluation\":{{\"biome_assertions\":{{\"matched\":{},\"total\":{},\"coverage_ratio\":{}}},\"tree_cover_assertions\":{{\"matched\":{},\"total\":{},\"coverage_ratio\":{}}},\"ground_cover_assertions\":{{\"matched\":{},\"total\":{},\"coverage_ratio\":{}}}}}\n",
        timestamp_unix_ms,
        json_escape(seed),
        mesh_level,
        cell_count,
        run_state.converged,
        run_state.ticks_to_converge,
        main_state_label,
        json_escape(&reference_path.display().to_string()),
        format_json_number(tree_rho),
        format_json_number(ground_rho),
        format_json_number(biome_macro_f1),
        format_json_number(biome_accuracy),
        format_json_number(soil_rho),
        biome_summary.matched,
        biome_summary.total,
        format_json_number(biome_summary.coverage_ratio),
        tree_summary.matched,
        tree_summary.total,
        format_json_number(tree_summary.coverage_ratio),
        ground_summary.matched,
        ground_summary.total,
        format_json_number(ground_summary.coverage_ratio),
    );

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
        .map_err(|error| format!("failed to write {}: {}", output_path.display(), error))?;
    Ok(())
}

fn load_terrain_ref(path: &Path) -> Result<TerrainRef, String> {
    let mut reader =
        BufReader::new(File::open(path).map_err(|e| format!("open {}: {}", path.display(), e))?);
    decode_terrain_ref(&mut reader)
}

fn load_climate_ref(path: &Path) -> Result<ClimateRef, String> {
    let mut reader =
        BufReader::new(File::open(path).map_err(|e| format!("open {}: {}", path.display(), e))?);
    decode_climate_ref(&mut reader)
}

fn load_hydro_ref(path: &Path) -> Result<HydroRef, String> {
    let mut reader =
        BufReader::new(File::open(path).map_err(|e| format!("open {}: {}", path.display(), e))?);
    decode_hydro_ref(&mut reader)
}

fn load_ecology_ref(path: &Path) -> Result<EcologyRef, String> {
    let mut reader =
        BufReader::new(File::open(path).map_err(|e| format!("open {}: {}", path.display(), e))?);
    decode_ecology_ref(&mut reader)
}

fn decode_terrain_ref<R: Read>(reader: &mut R) -> Result<TerrainRef, String> {
    expect_magic(reader, b"TERRREF1")?;
    expect_version(reader, 1)?;
    let cell_count = read_u64_le(reader)? as usize;
    Ok(TerrainRef {
        height: read_f32_vec(reader, cell_count)?,
    })
}

fn decode_climate_ref<R: Read>(reader: &mut R) -> Result<ClimateRef, String> {
    expect_magic(reader, b"CLIMREF1")?;
    expect_version(reader, 1)?;
    let cell_count = read_u64_le(reader)? as usize;
    Ok(ClimateRef {
        temperature: read_f32_vec(reader, cell_count)?,
        precipitation: read_f32_vec(reader, cell_count)?,
    })
}

fn decode_hydro_ref<R: Read>(reader: &mut R) -> Result<HydroRef, String> {
    expect_magic(reader, b"HYDROREF1")?;
    expect_version(reader, 1)?;
    let cell_count = read_u64_le(reader)? as usize;
    Ok(HydroRef {
        river_flow: read_f32_vec(reader, cell_count)?,
    })
}

fn decode_ecology_ref<R: Read>(reader: &mut R) -> Result<EcologyRef, String> {
    expect_magic(reader, b"ECOREF01")?;
    expect_version(reader, 1)?;
    let cell_count = read_u64_le(reader)? as usize;
    Ok(EcologyRef {
        tree_cover: read_f32_vec(reader, cell_count)?,
        ground_cover: read_f32_vec(reader, cell_count)?,
        soil_fertility: read_f32_vec(reader, cell_count)?,
        biome: read_u8_vec(reader, cell_count)?,
        natural_mask: read_u8_vec(reader, cell_count)?,
        open_canopy_mask: read_u8_vec(reader, cell_count)?,
    })
}

fn expect_magic<R: Read>(reader: &mut R, expected: &[u8]) -> Result<(), String> {
    let mut buf = vec![0_u8; expected.len()];
    reader
        .read_exact(&mut buf)
        .map_err(|e| format!("failed to read magic: {}", e))?;
    if buf != expected {
        return Err("invalid magic".to_string());
    }
    Ok(())
}

fn expect_version<R: Read>(reader: &mut R, expected: u32) -> Result<(), String> {
    let version = read_u32_le(reader)?;
    if version != expected {
        return Err(format!("unsupported version: {}", version));
    }
    Ok(())
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

fn read_u8_vec<R: Read>(reader: &mut R, len: usize) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0_u8; len];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("failed to read u8 vec: {}", error))?;
    Ok(bytes)
}

fn build_region_selection(positions: &[[f32; 3]]) -> Vec<(&'static str, usize)> {
    REGIONS
        .iter()
        .map(|region| (region.id, nearest_cell(positions, region.lat, region.lon)))
        .collect::<Vec<_>>()
}

fn nearest_cell(positions: &[[f32; 3]], lat: f32, lon: f32) -> usize {
    positions
        .iter()
        .enumerate()
        .map(|(index, pos)| {
            let cell_lat = pos[1].clamp(-1.0, 1.0).asin().to_degrees();
            let cell_lon = pos[2].atan2(pos[0]).to_degrees();
            let dist = haversine_km(cell_lat, cell_lon, lat, lon);
            (index, dist)
        })
        .min_by(|(_, l), (_, r)| l.total_cmp(r))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn haversine_km(lat_a: f32, lon_a: f32, lat_b: f32, lon_b: f32) -> f32 {
    let earth_radius_km = 6_371.0_f32;
    let lat1 = lat_a.to_radians();
    let lon1 = lon_a.to_radians();
    let lat2 = lat_b.to_radians();
    let lon2 = lon_b.to_radians();
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;

    let sin_dlat = (dlat * 0.5).sin();
    let sin_dlon = (dlon * 0.5).sin();
    let a = sin_dlat * sin_dlat + lat1.cos() * lat2.cos() * sin_dlon * sin_dlon;
    let c = 2.0 * a.clamp(0.0, 1.0).sqrt().asin();
    earth_radius_km * c
}

fn run_rank_assertions(
    selection: &[(&'static str, usize)],
    values: &[f32],
    assertions: &[RankAssertion],
) -> Vec<RankOutcome> {
    assertions
        .iter()
        .map(|assertion| {
            let left_index = lookup_selection(selection, assertion.left);
            let right_index = lookup_selection(selection, assertion.right);
            let left_value = left_index
                .and_then(|idx| values.get(idx).copied())
                .unwrap_or(f32::NAN);
            let right_value = right_index
                .and_then(|idx| values.get(idx).copied())
                .unwrap_or(f32::NAN);
            let passed =
                left_value.is_finite() && right_value.is_finite() && left_value > right_value;
            RankOutcome { passed }
        })
        .collect()
}

fn run_biome_assertions(
    selection: &[(&'static str, usize)],
    values: &[u8],
    assertions: &[BiomeAssertion],
) -> Vec<BiomeOutcome> {
    assertions
        .iter()
        .map(|assertion| {
            let index = lookup_selection(selection, assertion.region);
            let actual = index
                .and_then(|idx| values.get(idx).copied())
                .unwrap_or(255);
            BiomeOutcome {
                passed: actual == assertion.expected,
            }
        })
        .collect()
}

fn lookup_selection(selection: &[(&'static str, usize)], id: &str) -> Option<usize> {
    selection
        .iter()
        .find(|(region_id, _)| *region_id == id)
        .map(|(_, index)| *index)
}

fn summarize_rank(outcomes: &[RankOutcome]) -> MetricSummary {
    let total = outcomes.len();
    let matched = outcomes.iter().filter(|item| item.passed).count();
    let coverage_ratio = if total > 0 {
        matched as f32 / total as f32
    } else {
        0.0
    };
    MetricSummary {
        matched,
        total,
        coverage_ratio,
    }
}

fn summarize_biome(outcomes: &[BiomeOutcome]) -> MetricSummary {
    let total = outcomes.len();
    let matched = outcomes.iter().filter(|item| item.passed).count();
    let coverage_ratio = if total > 0 {
        matched as f32 / total as f32
    } else {
        0.0
    };
    MetricSummary {
        matched,
        total,
        coverage_ratio,
    }
}

fn spearman_masked(model: &[f32], reference: &[f32], height: &[f32], mask: &[u8]) -> Option<f32> {
    let len = model
        .len()
        .min(reference.len())
        .min(height.len())
        .min(mask.len());
    let mut left = Vec::with_capacity(len);
    let mut right = Vec::with_capacity(len);
    for i in 0..len {
        if height[i] <= 0.0 || mask[i] == 0 {
            continue;
        }
        if model[i].is_finite() && reference[i].is_finite() {
            left.push(model[i]);
            right.push(reference[i]);
        }
    }
    if left.len() < 3 {
        return None;
    }
    spearman(&left, &right)
}

fn spearman_masked_with_two_masks(
    model: &[f32],
    reference: &[f32],
    height: &[f32],
    mask_a: &[u8],
    mask_b: &[u8],
) -> Option<f32> {
    let len = model
        .len()
        .min(reference.len())
        .min(height.len())
        .min(mask_a.len())
        .min(mask_b.len());
    let mut left = Vec::with_capacity(len);
    let mut right = Vec::with_capacity(len);
    for i in 0..len {
        if height[i] <= 0.0 || mask_a[i] == 0 || mask_b[i] == 0 {
            continue;
        }
        if model[i].is_finite() && reference[i].is_finite() {
            left.push(model[i]);
            right.push(reference[i]);
        }
    }
    if left.len() < 3 {
        return None;
    }
    spearman(&left, &right)
}

fn macro_f1_and_accuracy(
    model: &[u8],
    truth: &[u8],
    height: &[f32],
    natural_mask: &[u8],
) -> (f32, f32) {
    let len = model
        .len()
        .min(truth.len())
        .min(height.len())
        .min(natural_mask.len());
    let mut total = 0usize;
    let mut correct = 0usize;
    for i in 0..len {
        if height[i] <= 0.0 || natural_mask[i] == 0 || truth[i] == 255 {
            continue;
        }
        total += 1;
        if model[i] == truth[i] {
            correct += 1;
        }
    }
    let mut per_class_f1 = Vec::new();

    for class_id in 0_u8..=8_u8 {
        let mut tp = 0f32;
        let mut fp = 0f32;
        let mut fnn = 0f32;
        for i in 0..len {
            if height[i] <= 0.0 || natural_mask[i] == 0 || truth[i] == 255 {
                continue;
            }
            let pred = model[i] == class_id;
            let gt = truth[i] == class_id;
            if pred && gt {
                tp += 1.0;
            } else if pred && !gt {
                fp += 1.0;
            } else if !pred && gt {
                fnn += 1.0;
            }
        }
        if tp + fp + fnn <= 0.0 {
            continue;
        }
        let precision = if tp + fp > 0.0 { tp / (tp + fp) } else { 0.0 };
        let recall = if tp + fnn > 0.0 { tp / (tp + fnn) } else { 0.0 };
        let f1 = if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        };
        per_class_f1.push(f1);
    }

    let macro_f1 = if per_class_f1.is_empty() {
        f32::NAN
    } else {
        per_class_f1.iter().copied().sum::<f32>() / per_class_f1.len() as f32
    };
    let accuracy = if total == 0 {
        f32::NAN
    } else {
        correct as f32 / total as f32
    };
    (macro_f1, accuracy)
}

fn spearman(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.len() < 3 {
        return None;
    }
    let rank_a = rank_with_ties(a);
    let rank_b = rank_with_ties(b);
    pearson_corr(&rank_a, &rank_b)
}

fn rank_with_ties(values: &[f32]) -> Vec<f32> {
    let mut indexed = values
        .iter()
        .copied()
        .enumerate()
        .collect::<Vec<(usize, f32)>>();
    indexed.sort_by(|(_, l), (_, r)| l.total_cmp(r));

    let mut ranks = vec![0.0_f32; values.len()];
    let mut i = 0usize;
    while i < indexed.len() {
        let start = i;
        let value = indexed[i].1;
        while i < indexed.len() && indexed[i].1 == value {
            i += 1;
        }
        let end = i;
        let avg_rank = (start + 1 + end) as f32 / 2.0;
        for j in start..end {
            ranks[indexed[j].0] = avg_rank;
        }
    }
    ranks
}

fn pearson_corr(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let n = a.len() as f32;
    let mean_a = a.iter().copied().sum::<f32>() / n;
    let mean_b = b.iter().copied().sum::<f32>() / n;
    let mut numerator = 0.0f32;
    let mut denom_a = 0.0f32;
    let mut denom_b = 0.0f32;
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

fn percentile_sorted(values: &mut Vec<f32>, quantile: f32) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    values.sort_by(|l, r| l.total_cmp(r));
    if values.len() == 1 {
        return values[0];
    }
    let q = quantile.clamp(0.0, 1.0);
    let pos = q * ((values.len() - 1) as f32);
    let lower = pos.floor() as usize;
    let upper = pos.ceil() as usize;
    if lower == upper {
        return values[lower];
    }
    let w = pos - lower as f32;
    values[lower] * (1.0 - w) + values[upper] * w
}

fn biome_to_u8(value: world::Biome) -> u8 {
    match value {
        world::Biome::TropicalForest => 0,
        world::Biome::Savanna => 1,
        world::Biome::Desert => 2,
        world::Biome::Grassland => 3,
        world::Biome::TemperateForest => 4,
        world::Biome::BorealForest => 5,
        world::Biome::Tundra => 6,
        world::Biome::Wetland => 7,
        world::Biome::Alpine => 8,
    }
}

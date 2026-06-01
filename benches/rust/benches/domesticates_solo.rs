use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::sim::domesticates::{CropKind, DomesticatesBenchDiagnostics, LivestockKind};
use frey_wasm::sim::geology_types::{GeologyInternal, GeologyParams};
use frey_wasm::world;

const CROP_SPECIES: &[(CropKind, &str)] = &[
    (CropKind::Wheat, "Wheat"),
    (CropKind::Rice, "Rice"),
    (CropKind::Maize, "Maize"),
    (CropKind::Millet, "Millet"),
    (CropKind::Potato, "Potato"),
    (CropKind::Cassava, "Cassava"),
    (CropKind::Sorghum, "Sorghum"),
];

const LIVESTOCK_SPECIES: &[(LivestockKind, &str)] = &[
    (LivestockKind::Cattle, "Cattle"),
    (LivestockKind::Horse, "Horse"),
    (LivestockKind::Sheep, "Sheep"),
    (LivestockKind::Pig, "Pig"),
];

const REGIONS: &[Region] = &[
    Region {
        id: "se_asia_lowland",
        lat: 15.0,
        lon: 105.0,
    },
    Region {
        id: "tibetan_highland",
        lat: 32.0,
        lon: 86.0,
    },
    Region {
        id: "sahel",
        lat: 14.0,
        lon: 0.0,
    },
    Region {
        id: "amazon",
        lat: -4.0,
        lon: -62.0,
    },
    Region {
        id: "andes",
        lat: -14.0,
        lon: -72.0,
    },
    Region {
        id: "europe_plain",
        lat: 49.0,
        lon: 14.0,
    },
    Region {
        id: "steppe",
        lat: 47.0,
        lon: 75.0,
    },
    Region {
        id: "arabia",
        lat: 23.0,
        lon: 45.0,
    },
    Region {
        id: "congo_edge",
        lat: 0.5,
        lon: 24.5,
    },
];

const ASSERTIONS: &[RegionalAssertion] = &[
    RegionalAssertion {
        id: "rice_lowland_over_highland",
        species: SpeciesRef::Crop(CropKind::Rice),
        left: "se_asia_lowland",
        right: "tibetan_highland",
    },
    RegionalAssertion {
        id: "millet_sahel_over_amazon",
        species: SpeciesRef::Crop(CropKind::Millet),
        left: "sahel",
        right: "amazon",
    },
    RegionalAssertion {
        id: "sorghum_sahel_over_amazon",
        species: SpeciesRef::Crop(CropKind::Sorghum),
        left: "sahel",
        right: "amazon",
    },
    RegionalAssertion {
        id: "potato_andes_over_amazon",
        species: SpeciesRef::Crop(CropKind::Potato),
        left: "andes",
        right: "amazon",
    },
    RegionalAssertion {
        id: "wheat_europe_over_amazon",
        species: SpeciesRef::Crop(CropKind::Wheat),
        left: "europe_plain",
        right: "amazon",
    },
    RegionalAssertion {
        id: "horse_steppe_over_congo",
        species: SpeciesRef::Livestock(LivestockKind::Horse),
        left: "steppe",
        right: "congo_edge",
    },
    RegionalAssertion {
        id: "sheep_arabia_over_amazon",
        species: SpeciesRef::Livestock(LivestockKind::Sheep),
        left: "arabia",
        right: "amazon",
    },
    RegionalAssertion {
        id: "pig_congo_over_arabia",
        species: SpeciesRef::Livestock(LivestockKind::Pig),
        left: "congo_edge",
        right: "arabia",
    },
    // v1 gate対象外 species も診断 assertion だけ残す。
    RegionalAssertion {
        id: "yam_amazon_over_sahel",
        species: SpeciesRef::Crop(CropKind::Yam),
        left: "amazon",
        right: "sahel",
    },
    RegionalAssertion {
        id: "camel_arabia_over_congo",
        species: SpeciesRef::Livestock(LivestockKind::Camel),
        left: "arabia",
        right: "congo_edge",
    },
];

#[derive(Clone, Copy)]
struct Region {
    id: &'static str,
    lat: f32,
    lon: f32,
}

#[derive(Clone, Copy)]
enum SpeciesRef {
    Crop(CropKind),
    Livestock(LivestockKind),
}

#[derive(Clone, Copy)]
struct RegionalAssertion {
    id: &'static str,
    species: SpeciesRef,
    left: &'static str,
    right: &'static str,
}

struct RegionalAssertionOutcome {
    id: &'static str,
    left_value: f32,
    right_value: f32,
    passed: bool,
}

struct SummaryMetric {
    matched: usize,
    total: usize,
    coverage_ratio: f32,
}

#[derive(Debug, Clone)]
struct TerrainRef {
    height: Vec<f32>,
}

#[derive(Debug, Clone)]
struct ClimateRef {
    temperature: Vec<f32>,
    precipitation: Vec<f32>,
    aridity: Vec<f32>,
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
}

#[derive(Debug, Clone)]
struct DomesticatesRef {
    crop_observed_intensity: Vec<f32>,
    livestock_observed_intensity: Vec<f32>,
    crop_observed_presence: Vec<u8>,
    livestock_observed_presence: Vec<u8>,
    crop_eval_mask: Vec<u8>,
    livestock_eval_mask: Vec<u8>,
}

struct SpeciesMetric {
    name: &'static str,
    value: f32,
}

fn main() {
    let geology_params = GeologyParams {
        level: 6,
        ..Default::default()
    };
    let mesh_level = geology_params.level;
    let seed = "earth";

    let (mut terrain, positions, nbr_offsets, nbrs) =
        sim::build_geology_with_mesh(seed, geology_params);
    let cell_count = positions.len();

    let (terrain_ref_path, terrain_ref) = match find_cache("terrain_ref.bin")
        .and_then(|path| load_terrain_ref(&path).ok().map(|r| (path, r)))
    {
        Some(pair) => pair,
        None => {
            println!("=== Domesticates Solo Bench ===");
            println!();
            println!("-- Terrain Input: SKIPPED (benches/data/terrain_ref.bin not found) --");
            println!("To generate:");
            println!("  1) pnpm bench:dump-centroids");
            println!("  2) pnpm bench:resample:terrain -- --height <path>");
            return;
        }
    };
    let (climate_ref_path, climate_ref) = match find_cache("climate_ref.bin")
        .and_then(|path| load_climate_ref(&path).ok().map(|r| (path, r)))
    {
        Some(pair) => pair,
        None => {
            println!("=== Domesticates Solo Bench ===");
            println!();
            println!("-- Climate Input: SKIPPED (benches/data/climate_ref.bin not found) --");
            return;
        }
    };
    let (hydro_ref_path, hydro_ref) = match find_cache("hydro_ref.bin")
        .and_then(|path| load_hydro_ref(&path).ok().map(|r| (path, r)))
    {
        Some(pair) => pair,
        None => {
            println!("=== Domesticates Solo Bench ===");
            println!();
            println!("-- Hydro Input: SKIPPED (benches/data/hydro_ref.bin not found) --");
            return;
        }
    };
    let (ecology_ref_path, ecology_ref) = match find_cache("ecology_ref.bin")
        .and_then(|path| load_ecology_ref(&path).ok().map(|r| (path, r)))
    {
        Some(pair) => pair,
        None => {
            println!("=== Domesticates Solo Bench ===");
            println!();
            println!("-- Ecology Input: SKIPPED (benches/data/ecology_ref.bin not found) --");
            return;
        }
    };
    let (domesticates_ref_path, domesticates_ref) = match find_cache("domesticates_ref.bin")
        .and_then(|path| load_domesticates_ref(&path).ok().map(|r| (path, r)))
    {
        Some(pair) => pair,
        None => {
            println!("=== Domesticates Solo Bench ===");
            println!();
            println!(
                "-- Domesticates Ref: SKIPPED (benches/data/domesticates_ref.bin not found) --"
            );
            println!("To generate:");
            println!(
                "  pnpm bench:resample:domesticates-ref -- --manifest benches/raw/domesticates/manifest.json"
            );
            return;
        }
    };

    if terrain_ref.height.len() != cell_count
        || climate_ref.temperature.len() != cell_count
        || climate_ref.precipitation.len() != cell_count
        || climate_ref.aridity.len() != cell_count
        || hydro_ref.river_flow.len() != cell_count
        || ecology_ref.tree_cover.len() != cell_count
        || ecology_ref.ground_cover.len() != cell_count
        || ecology_ref.soil_fertility.len() != cell_count
        || ecology_ref.biome.len() != cell_count
        || domesticates_ref.crop_observed_presence.len() != cell_count
        || domesticates_ref.livestock_observed_presence.len() != cell_count
    {
        println!("=== Domesticates Solo Bench ===");
        println!();
        println!("-- Input: ERROR (cell count mismatch) --");
        return;
    }

    println!("=== Domesticates Solo Bench ===");
    println!("-- Terrain Source: {} --", terrain_ref_path.display());
    println!("-- Climate Source: {} --", climate_ref_path.display());
    println!("-- Hydro Source: {} --", hydro_ref_path.display());
    println!("-- Ecology Source: {} --", ecology_ref_path.display());
    println!(
        "-- Domesticates Source: {} --",
        domesticates_ref_path.display()
    );

    terrain.height = terrain_ref.height;
    let plate_id = terrain.plate_id.clone();
    let geology = world::GeologyState {
        height: terrain.height,
        lake_depth: vec![0.0; cell_count],
        plate_id,
        volcanism: terrain.volcanism,
        vertex_buoyancy: terrain.vertex_buoyancy,
        geology_internal: vec![GeologyInternal::default(); cell_count],
        boundary_condition: vec![0.0; cell_count],
        smoothing_limited_cells_ratio: 0.0,
        mean_smoothing_factor: 1.0,
        zero_mean_adjusted_cells_ratio: 0.0,
        zero_mean_mean_abs_correction: 0.0,
        zero_mean_std_delta: 0.0,
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
    sim_world.state.climate.aridity = climate_ref.aridity;
    sim_world.state.hydrology.river_flow = hydro_ref.river_flow;
    sim_world.state.ecology.tree_cover = ecology_ref.tree_cover;
    sim_world.state.ecology.ground_cover = ecology_ref.ground_cover;
    sim_world.state.ecology.soil_fertility = ecology_ref.soil_fertility;
    sim_world.state.ecology.biome = ecology_ref
        .biome
        .iter()
        .copied()
        .map(decode_biome)
        .collect();

    let started = Instant::now();
    let diagnostics = sim::run_domesticates_step_with_diagnostics_for_bench(&mut sim_world, 2);
    let domesticates_step_ms = started.elapsed().as_secs_f64() as f32 * 1000.0;

    let crop_intensity_metrics = evaluate_crop_intensity(
        &diagnostics,
        &domesticates_ref,
        &sim_world.state.geology.height,
    );
    let crop_presence_metrics = evaluate_crop_presence(
        &sim_world,
        &domesticates_ref,
        &sim_world.state.geology.height,
    );
    let livestock_intensity_metrics = evaluate_livestock_intensity(
        &diagnostics,
        &domesticates_ref,
        &sim_world.state.geology.height,
    );
    let livestock_presence_metrics = evaluate_livestock_presence(
        &sim_world,
        &domesticates_ref,
        &sim_world.state.geology.height,
    );

    let crop_intensity_rho = mean_metric(&crop_intensity_metrics);
    let crop_presence_f1 = mean_metric(&crop_presence_metrics);
    let livestock_intensity_rho = mean_metric(&livestock_intensity_metrics);
    let livestock_presence_f1 = mean_metric(&livestock_presence_metrics);

    let selection = build_region_selection(&sim_world.mesh().positions);
    let assertion_outcomes = run_regional_assertions(&selection, &diagnostics);
    let regional_assertion = summarize_assertions(&assertion_outcomes);

    let overall_score = mean_metric(&[
        SpeciesMetric {
            name: "crop_intensity_rho",
            value: crop_intensity_rho,
        },
        SpeciesMetric {
            name: "crop_presence_f1",
            value: crop_presence_f1,
        },
        SpeciesMetric {
            name: "livestock_intensity_rho",
            value: livestock_intensity_rho,
        },
        SpeciesMetric {
            name: "livestock_presence_f1",
            value: livestock_presence_f1,
        },
        SpeciesMetric {
            name: "regional_assertion_coverage",
            value: regional_assertion.coverage_ratio,
        },
    ]);

    println!();
    println!("-- Main Evaluation --");
    println!("crop_intensity_rho:        {:.3}", crop_intensity_rho);
    println!("crop_presence_f1:          {:.3}", crop_presence_f1);
    println!("livestock_intensity_rho:   {:.3}", livestock_intensity_rho);
    println!("livestock_presence_f1:     {:.3}", livestock_presence_f1);
    println!(
        "regional_assertion_coverage: {:.3}",
        regional_assertion.coverage_ratio
    );
    println!("overall_score:             {:.3}", overall_score);
    println!("runtime_ms:                {:.3}", domesticates_step_ms);

    println!();
    println!("-- Crop Intensity Detail --");
    print_species_metrics(&crop_intensity_metrics);
    println!("-- Crop Presence Detail --");
    print_species_metrics(&crop_presence_metrics);
    println!("-- Livestock Intensity Detail --");
    print_species_metrics(&livestock_intensity_metrics);
    println!("-- Livestock Presence Detail --");
    print_species_metrics(&livestock_presence_metrics);

    println!();
    println!("-- Diagnostic Assertions --");
    println!(
        "matched={}/{} coverage_ratio={:.3}",
        regional_assertion.matched, regional_assertion.total, regional_assertion.coverage_ratio
    );
    for outcome in &assertion_outcomes {
        println!(
            "{}: {} ({:.3} > {:.3})",
            outcome.id,
            if outcome.passed { "PASS" } else { "FAIL" },
            outcome.left_value,
            outcome.right_value
        );
    }

    if let Err(error) = append_score_record_jsonl(
        mesh_level,
        cell_count,
        domesticates_step_ms,
        crop_intensity_rho,
        crop_presence_f1,
        livestock_intensity_rho,
        livestock_presence_f1,
        regional_assertion.coverage_ratio,
        overall_score,
    ) {
        println!("-- Score Save: ERROR ({}) --", error);
    } else {
        println!("-- Score Save: OK --");
    }
}

fn find_cache(name: &str) -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(format!("benches/data/{name}")),
        PathBuf::from(format!("../data/{name}")),
        PathBuf::from(format!("../benches/data/{name}")),
        PathBuf::from(format!("../../benches/data/{name}")),
    ];
    candidates.into_iter().find(|path| path.exists())
}

fn load_terrain_ref(path: &Path) -> Result<TerrainRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    expect_magic(&mut reader, b"TERRREF1")?;
    let version = read_u32_le(&mut reader)?;
    if version != 1 {
        return Err(format!("unsupported terrain_ref version: {}", version));
    }
    let cell_count = read_u64_le(&mut reader)? as usize;
    let height = read_f32_vec(&mut reader, cell_count)?;
    Ok(TerrainRef { height })
}

fn load_climate_ref(path: &Path) -> Result<ClimateRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    expect_magic(&mut reader, b"CLIMREF1")?;
    let version = read_u32_le(&mut reader)?;
    if version != 1 {
        return Err(format!("unsupported climate_ref version: {}", version));
    }
    let cell_count = read_u64_le(&mut reader)? as usize;
    let temperature = read_f32_vec(&mut reader, cell_count)?;
    let precipitation = read_f32_vec(&mut reader, cell_count)?;
    let _evapotranspiration = read_f32_vec(&mut reader, cell_count)?;
    let _runoff = read_f32_vec(&mut reader, cell_count)?;
    let aridity = read_f32_vec(&mut reader, cell_count)?;
    Ok(ClimateRef {
        temperature,
        precipitation,
        aridity,
    })
}

fn load_hydro_ref(path: &Path) -> Result<HydroRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    expect_magic(&mut reader, b"HYDROREF1")?;
    let version = read_u32_le(&mut reader)?;
    if version != 1 {
        return Err(format!("unsupported hydro_ref version: {}", version));
    }
    let cell_count = read_u64_le(&mut reader)? as usize;
    let river_flow = read_f32_vec(&mut reader, cell_count)?;
    let _is_lake = read_u8_vec(&mut reader, cell_count)?;
    Ok(HydroRef { river_flow })
}

fn load_ecology_ref(path: &Path) -> Result<EcologyRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    expect_magic(&mut reader, b"ECOREF01")?;
    let version = read_u32_le(&mut reader)?;
    if version != 1 {
        return Err(format!("unsupported ecology_ref version: {}", version));
    }
    let cell_count = read_u64_le(&mut reader)? as usize;
    let tree_cover = read_f32_vec(&mut reader, cell_count)?;
    let ground_cover = read_f32_vec(&mut reader, cell_count)?;
    let soil_fertility = read_f32_vec(&mut reader, cell_count)?;
    let biome = read_u8_vec(&mut reader, cell_count)?;
    let _natural_mask = read_u8_vec(&mut reader, cell_count)?;
    let _open_canopy_mask = read_u8_vec(&mut reader, cell_count)?;
    Ok(EcologyRef {
        tree_cover,
        ground_cover,
        soil_fertility,
        biome,
    })
}

fn load_domesticates_ref(path: &Path) -> Result<DomesticatesRef, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open {}: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    expect_magic(&mut reader, b"DOMEREF2")?;
    let version = read_u32_le(&mut reader)?;
    if version != 1 {
        return Err(format!("unsupported domesticates_ref version: {}", version));
    }
    let cell_count = read_u64_le(&mut reader)? as usize;
    let crop_observed_intensity = read_f32_vec(&mut reader, cell_count * CROP_SPECIES.len())?;
    let livestock_observed_intensity =
        read_f32_vec(&mut reader, cell_count * LIVESTOCK_SPECIES.len())?;
    let crop_observed_presence = read_u8_vec(&mut reader, cell_count)?;
    let livestock_observed_presence = read_u8_vec(&mut reader, cell_count)?;
    let crop_eval_mask = read_u8_vec(&mut reader, cell_count * CROP_SPECIES.len())?;
    let livestock_eval_mask = read_u8_vec(&mut reader, cell_count * LIVESTOCK_SPECIES.len())?;
    Ok(DomesticatesRef {
        crop_observed_intensity,
        livestock_observed_intensity,
        crop_observed_presence,
        livestock_observed_presence,
        crop_eval_mask,
        livestock_eval_mask,
    })
}

fn expect_magic<R: Read>(reader: &mut R, expected: &[u8]) -> Result<(), String> {
    let mut magic = vec![0_u8; expected.len()];
    reader
        .read_exact(&mut magic)
        .map_err(|error| format!("failed to read magic: {}", error))?;
    if magic.as_slice() != expected {
        return Err(format!(
            "invalid magic (expected {})",
            String::from_utf8_lossy(expected)
        ));
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
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

fn read_u8_vec<R: Read>(reader: &mut R, len: usize) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0_u8; len];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("failed to read u8 vec: {}", error))?;
    Ok(bytes)
}

fn decode_biome(code: u8) -> world::Biome {
    match code {
        0 => world::Biome::TropicalForest,
        1 => world::Biome::Savanna,
        2 => world::Biome::Desert,
        3 => world::Biome::Grassland,
        4 => world::Biome::TemperateForest,
        5 => world::Biome::BorealForest,
        6 => world::Biome::Tundra,
        7 => world::Biome::Wetland,
        8 => world::Biome::Alpine,
        _ => world::Biome::TemperateForest,
    }
}

fn evaluate_crop_intensity(
    diagnostics: &DomesticatesBenchDiagnostics,
    reference: &DomesticatesRef,
    geology_height: &[f32],
) -> Vec<SpeciesMetric> {
    CROP_SPECIES
        .iter()
        .enumerate()
        .map(|(ref_idx, (kind, name))| SpeciesMetric {
            name,
            value: spearman_masked(
                &extract_crop_series(diagnostics, *kind),
                &reference_column(
                    &reference.crop_observed_intensity,
                    CROP_SPECIES.len(),
                    ref_idx,
                ),
                geology_height,
                &reference_column_u8(&reference.crop_eval_mask, CROP_SPECIES.len(), ref_idx),
            )
            .unwrap_or(f32::NAN),
        })
        .collect()
}

fn evaluate_livestock_intensity(
    diagnostics: &DomesticatesBenchDiagnostics,
    reference: &DomesticatesRef,
    geology_height: &[f32],
) -> Vec<SpeciesMetric> {
    LIVESTOCK_SPECIES
        .iter()
        .enumerate()
        .map(|(ref_idx, (kind, name))| SpeciesMetric {
            name,
            value: spearman_masked(
                &extract_livestock_series(diagnostics, *kind),
                &reference_column(
                    &reference.livestock_observed_intensity,
                    LIVESTOCK_SPECIES.len(),
                    ref_idx,
                ),
                geology_height,
                &reference_column_u8(
                    &reference.livestock_eval_mask,
                    LIVESTOCK_SPECIES.len(),
                    ref_idx,
                ),
            )
            .unwrap_or(f32::NAN),
        })
        .collect()
}

fn evaluate_crop_presence(
    world: &world::World,
    reference: &DomesticatesRef,
    geology_height: &[f32],
) -> Vec<SpeciesMetric> {
    CROP_SPECIES
        .iter()
        .enumerate()
        .map(|(ref_idx, (kind, name))| SpeciesMetric {
            name,
            value: f1_on_masked_presence(
                &world.state.domesticates.crop_available,
                1u8 << (*kind as u8),
                &reference.crop_observed_presence,
                1u8 << ref_idx,
                geology_height,
                &reference_column_u8(&reference.crop_eval_mask, CROP_SPECIES.len(), ref_idx),
            ),
        })
        .collect()
}

fn evaluate_livestock_presence(
    world: &world::World,
    reference: &DomesticatesRef,
    geology_height: &[f32],
) -> Vec<SpeciesMetric> {
    LIVESTOCK_SPECIES
        .iter()
        .enumerate()
        .map(|(ref_idx, (kind, name))| SpeciesMetric {
            name,
            value: f1_on_masked_presence(
                &world.state.domesticates.livestock_available,
                1u8 << (*kind as u8),
                &reference.livestock_observed_presence,
                1u8 << ref_idx,
                geology_height,
                &reference_column_u8(
                    &reference.livestock_eval_mask,
                    LIVESTOCK_SPECIES.len(),
                    ref_idx,
                ),
            ),
        })
        .collect()
}

fn extract_crop_series(diagnostics: &DomesticatesBenchDiagnostics, kind: CropKind) -> Vec<f32> {
    diagnostics
        .crop_niche
        .iter()
        .map(|values| values[kind as usize])
        .collect()
}

fn extract_livestock_series(
    diagnostics: &DomesticatesBenchDiagnostics,
    kind: LivestockKind,
) -> Vec<f32> {
    diagnostics
        .livestock_niche
        .iter()
        .map(|values| values[kind as usize])
        .collect()
}

fn reference_column(values: &[f32], width: usize, col: usize) -> Vec<f32> {
    values
        .chunks_exact(width)
        .map(|row| row[col])
        .collect::<Vec<_>>()
}

fn reference_column_u8(values: &[u8], width: usize, col: usize) -> Vec<u8> {
    values
        .chunks_exact(width)
        .map(|row| row[col])
        .collect::<Vec<_>>()
}

fn spearman_masked(
    model: &[f32],
    reference: &[f32],
    geology_height: &[f32],
    eval_mask: &[u8],
) -> Option<f32> {
    let mut model_values = Vec::new();
    let mut ref_values = Vec::new();
    for i in 0..model
        .len()
        .min(reference.len())
        .min(geology_height.len())
        .min(eval_mask.len())
    {
        if geology_height[i] <= 0.0 || eval_mask[i] == 0 {
            continue;
        }
        let mv = model[i];
        let rv = reference[i];
        if !mv.is_finite() || !rv.is_finite() {
            continue;
        }
        model_values.push(mv);
        ref_values.push(rv);
    }
    spearman(&model_values, &ref_values)
}

fn spearman(left: &[f32], right: &[f32]) -> Option<f32> {
    if left.len() != right.len() || left.len() < 3 {
        return None;
    }
    let left_rank = rank(left);
    let right_rank = rank(right);
    pearson(&left_rank, &right_rank)
}

fn rank(values: &[f32]) -> Vec<f32> {
    let mut pairs = values
        .iter()
        .copied()
        .enumerate()
        .collect::<Vec<(usize, f32)>>();
    pairs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut ranks = vec![0.0; values.len()];
    let mut start = 0usize;
    while start < pairs.len() {
        let mut end = start + 1;
        while end < pairs.len() && (pairs[end].1 - pairs[start].1).abs() <= f32::EPSILON {
            end += 1;
        }
        let avg_rank = (start + end - 1) as f32 * 0.5;
        for index in start..end {
            ranks[pairs[index].0] = avg_rank;
        }
        start = end;
    }
    ranks
}

fn pearson(left: &[f32], right: &[f32]) -> Option<f32> {
    if left.len() != right.len() || left.len() < 2 {
        return None;
    }
    let count = left.len() as f32;
    let mean_left = left.iter().sum::<f32>() / count;
    let mean_right = right.iter().sum::<f32>() / count;
    let mut cov = 0.0f32;
    let mut var_left = 0.0f32;
    let mut var_right = 0.0f32;
    for i in 0..left.len() {
        let dl = left[i] - mean_left;
        let dr = right[i] - mean_right;
        cov += dl * dr;
        var_left += dl * dl;
        var_right += dr * dr;
    }
    if var_left <= 1e-12 || var_right <= 1e-12 {
        return None;
    }
    Some((cov / (var_left.sqrt() * var_right.sqrt())).clamp(-1.0, 1.0))
}

fn f1_on_masked_presence(
    model_bitmap: &[u8],
    model_mask: u8,
    reference_bitmap: &[u8],
    reference_mask: u8,
    geology_height: &[f32],
    eval_mask: &[u8],
) -> f32 {
    let mut tp = 0.0f32;
    let mut fp = 0.0f32;
    let mut fnn = 0.0f32;
    for i in 0..model_bitmap
        .len()
        .min(reference_bitmap.len())
        .min(geology_height.len())
        .min(eval_mask.len())
    {
        if geology_height[i] <= 0.0 || eval_mask[i] == 0 {
            continue;
        }
        let model = (model_bitmap[i] & model_mask) != 0;
        let reference = (reference_bitmap[i] & reference_mask) != 0;
        match (model, reference) {
            (true, true) => tp += 1.0,
            (true, false) => fp += 1.0,
            (false, true) => fnn += 1.0,
            (false, false) => {}
        }
    }
    let precision = if tp + fp > 0.0 { tp / (tp + fp) } else { 0.0 };
    let recall = if tp + fnn > 0.0 { tp / (tp + fnn) } else { 0.0 };
    if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    }
}

fn build_region_selection(positions: &[[f32; 3]]) -> Vec<(&'static str, usize)> {
    REGIONS
        .iter()
        .map(|region| (region.id, nearest_cell(positions, region.lat, region.lon)))
        .collect()
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
        .min_by(|(_, left), (_, right)| {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        })
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

fn run_regional_assertions(
    selection: &[(&'static str, usize)],
    diagnostics: &DomesticatesBenchDiagnostics,
) -> Vec<RegionalAssertionOutcome> {
    ASSERTIONS
        .iter()
        .map(|assertion| {
            let left_index = lookup_index(selection, assertion.left);
            let right_index = lookup_index(selection, assertion.right);
            let (left_value, right_value) = match assertion.species {
                SpeciesRef::Crop(kind) => (
                    diagnostics.crop_niche[left_index][kind as usize],
                    diagnostics.crop_niche[right_index][kind as usize],
                ),
                SpeciesRef::Livestock(kind) => (
                    diagnostics.livestock_niche[left_index][kind as usize],
                    diagnostics.livestock_niche[right_index][kind as usize],
                ),
            };
            RegionalAssertionOutcome {
                id: assertion.id,
                left_value,
                right_value,
                passed: left_value > right_value,
            }
        })
        .collect()
}

fn lookup_index(selection: &[(&'static str, usize)], id: &str) -> usize {
    selection
        .iter()
        .find(|(region_id, _)| *region_id == id)
        .map(|(_, index)| *index)
        .unwrap_or(0)
}

fn summarize_assertions(outcomes: &[RegionalAssertionOutcome]) -> SummaryMetric {
    let matched = outcomes.iter().filter(|outcome| outcome.passed).count();
    let total = outcomes.len();
    SummaryMetric {
        matched,
        total,
        coverage_ratio: if total > 0 {
            matched as f32 / total as f32
        } else {
            0.0
        },
    }
}

fn print_species_metrics(metrics: &[SpeciesMetric]) {
    for metric in metrics {
        println!("{:<16} {:.3}", format!("{}:", metric.name), metric.value);
    }
}

fn mean_metric(metrics: &[SpeciesMetric]) -> f32 {
    let values = metrics
        .iter()
        .map(|metric| metric.value)
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return f32::NAN;
    }
    values.iter().sum::<f32>() / values.len() as f32
}

fn score_output_path() -> PathBuf {
    let candidates = [
        Path::new("../../benches/results/domesticates_main_scores.jsonl"),
        Path::new("benches/results/domesticates_main_scores.jsonl"),
        Path::new("../benches/results/domesticates_main_scores.jsonl"),
        Path::new("../results/domesticates_main_scores.jsonl"),
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

fn format_json_number(value: f32) -> String {
    if value.is_finite() {
        format!("{:.6}", value)
    } else {
        "null".to_string()
    }
}

fn append_score_record_jsonl(
    mesh_level: u32,
    cell_count: usize,
    domesticates_step_ms: f32,
    crop_intensity_rho: f32,
    crop_presence_f1: f32,
    livestock_intensity_rho: f32,
    livestock_presence_f1: f32,
    regional_assertion_coverage: f32,
    overall_score: f32,
) -> Result<(), String> {
    let timestamp_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system time error: {}", error))?
        .as_millis();
    let line = format!(
        "{{\"schema_version\":2,\"timestamp_unix_ms\":{},\"bench\":\"domesticates_solo\",\"seed\":\"earth\",\"mesh_level\":{},\"cell_count\":{},\"runtime\":{{\"domesticates_step_ms\":{}}},\"metrics\":{{\"crop_intensity_rho\":{},\"crop_presence_f1\":{},\"livestock_intensity_rho\":{},\"livestock_presence_f1\":{},\"regional_assertion_coverage\":{},\"overall_score\":{}}}}}\n",
        timestamp_unix_ms,
        mesh_level,
        cell_count,
        format_json_number(domesticates_step_ms),
        format_json_number(crop_intensity_rho),
        format_json_number(crop_presence_f1),
        format_json_number(livestock_intensity_rho),
        format_json_number(livestock_presence_f1),
        format_json_number(regional_assertion_coverage),
        format_json_number(overall_score),
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
        .map_err(|error| format!("failed to write {}: {}", output_path.display(), error))
}

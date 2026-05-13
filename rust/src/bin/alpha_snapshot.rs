use std::fs;
use std::path::PathBuf;

use frey_wasm::sim;
use frey_wasm::sim::precomputed::{
    canonical_cache_dir, geology_fingerprint, load_snapshot, mirror_dir, restore_world_from_snapshot,
    save_manifest, save_snapshot, save_snapshot_view, stage_filename, stage_view_filename,
    AlphaSnapshotStage, PrecomputedWorldSnapshotEnvelope, PrecomputedWorldSnapshotManifest,
    PrecomputedWorldSnapshotManifestEntry, SNAPSHOT_FORMAT_VERSION,
};
use frey_wasm::sim::world::FeedbackQueue;
use frey_wasm::GeologyParams;

const SEED: &str = "alpha";
const LEVEL: u32 = 6;

struct Args {
    level: u32,
    cache_dir: PathBuf,
    mirror_dir: PathBuf,
    resume_from: Option<AlphaSnapshotStage>,
    resume_path: Option<PathBuf>,
    until_stage: Option<AlphaSnapshotStage>,
    at_tick: Option<u64>,
    output_path: Option<PathBuf>,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut level = LEVEL;
    let mut cache_dir = canonical_cache_dir();
    let mut mirror_dir = mirror_dir();
    let mut resume_from = None;
    let mut resume_path = None;
    let mut until_stage = None;
    let mut at_tick = None;
    let mut output_path = None;
    let mut i = 0usize;
    while i < argv.len() {
        match argv[i].as_str() {
            "--level" => {
                let raw = argv
                    .get(i + 1)
                    .ok_or_else(|| "--level requires value".to_string())?;
                level = raw
                    .parse::<u32>()
                    .map_err(|_| "--level must be integer".to_string())?;
                i += 2;
            }
            "--cache-dir" => {
                let raw = argv
                    .get(i + 1)
                    .ok_or_else(|| "--cache-dir requires value".to_string())?;
                cache_dir = PathBuf::from(raw);
                i += 2;
            }
            "--mirror-dir" => {
                let raw = argv
                    .get(i + 1)
                    .ok_or_else(|| "--mirror-dir requires value".to_string())?;
                mirror_dir = PathBuf::from(raw);
                i += 2;
            }
            "--resume-from" => {
                let raw = argv
                    .get(i + 1)
                    .ok_or_else(|| "--resume-from requires value".to_string())?;
                resume_from = Some(raw.parse::<AlphaSnapshotStage>()?);
                i += 2;
            }
            "--resume-path" => {
                let raw = argv
                    .get(i + 1)
                    .ok_or_else(|| "--resume-path requires value".to_string())?;
                resume_path = Some(PathBuf::from(raw));
                i += 2;
            }
            "--until-stage" => {
                let raw = argv
                    .get(i + 1)
                    .ok_or_else(|| "--until-stage requires value".to_string())?;
                until_stage = Some(raw.parse::<AlphaSnapshotStage>()?);
                i += 2;
            }
            "--at-tick" => {
                let raw = argv
                    .get(i + 1)
                    .ok_or_else(|| "--at-tick requires value".to_string())?;
                at_tick = Some(
                    raw.parse::<u64>()
                        .map_err(|_| "--at-tick must be integer".to_string())?,
                );
                i += 2;
            }
            "--output-path" => {
                let raw = argv
                    .get(i + 1)
                    .ok_or_else(|| "--output-path requires value".to_string())?;
                output_path = Some(PathBuf::from(raw));
                i += 2;
            }
            "--help" => {
                eprintln!("Usage: cargo run --manifest-path rust/Cargo.toml --bin alpha_snapshot -- [options]");
                eprintln!("  --level <n>");
                eprintln!("  --cache-dir <path>");
                eprintln!("  --mirror-dir <path>");
                eprintln!("  --resume-from <environment|life|civilization|history>");
                eprintln!("  --resume-path <path>");
                eprintln!("  --until-stage <environment|life|civilization|history>");
                eprintln!("  --at-tick <n>");
                eprintln!("  --output-path <path>");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        level,
        cache_dir,
        mirror_dir,
        resume_from,
        resume_path,
        until_stage,
        at_tick,
        output_path,
    })
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let argv = std::env::args().skip(1).collect::<Vec<_>>();
    let args = parse_args(&argv)?;
    let level = args.level;
    let cache_dir = args.cache_dir;
    let mirror_dir = args.mirror_dir;
    let resume_path = args.resume_path;
    let until_stage = args.until_stage;
    let at_tick = args.at_tick;
    let output_path = args.output_path;
    let geology_params = GeologyParams {
        level,
        ..GeologyParams::default()
    };
    let geology_fp = geology_fingerprint(&geology_params)?;

    let (mut world, base_erosion_state) = if let Some(path) = resume_path.as_ref() {
        sim::headless::init_world_for_headless_runner_from_snapshot_path(
            SEED,
            level,
            geology_params.clone(),
            path,
            None,
        )?
    } else {
        sim::headless::init_world_for_headless_runner(SEED, level, geology_params.clone())?
    };
    let mut hydrology_state = Some(base_erosion_state);
    if let Some(stage) = args.resume_from {
        let snapshot_path = cache_dir.join(stage_filename(stage));
        let envelope = load_snapshot(&snapshot_path)?;
        if envelope.format_version != SNAPSHOT_FORMAT_VERSION {
            return Err(format!(
                "resume snapshot format mismatch at {}: expected {}, got {}",
                snapshot_path.display(),
                SNAPSHOT_FORMAT_VERSION,
                envelope.format_version
            ));
        }
        if envelope.seed != SEED || envelope.mesh_level != level || envelope.stage != stage {
            return Err(format!(
                "resume snapshot metadata mismatch at {}",
                snapshot_path.display()
            ));
        }
        if envelope.geology_fingerprint != geology_fp {
            return Err(format!(
                "resume snapshot geology fingerprint mismatch at {}",
                snapshot_path.display()
            ));
        }
        let restored = restore_world_from_snapshot(world, &envelope)?;
        world = restored.0;
        hydrology_state = Some(restored.1);
    }
    let mut feedback = FeedbackQueue::new(world.cell_count());

    if let Some(target_tick) = at_tick {
        while world.clock.tick < target_tick {
            sim::exec_world_with_feedback_and_hydrology(
                &mut world,
                &mut feedback,
                &mut hydrology_state,
            );
            if let Some(state) = hydrology_state.as_mut() {
                sim::hydrology::sync_hydrology_state_for_headless_runner(
                    &mut world,
                    state,
                    &geology_params,
                );
            }
        }
        let hydrology_state_value = hydrology_state
            .as_ref()
            .cloned()
            .ok_or_else(|| "hydrology state is missing".to_string())?;
        let stage = stage_for_tick(world.clock.tick);
        let envelope = PrecomputedWorldSnapshotEnvelope {
            format_version: SNAPSHOT_FORMAT_VERSION,
            seed: SEED.to_string(),
            mesh_level: level,
            stage,
            tick: world.clock.tick,
            era: world.clock.epoch.as_key().to_string(),
            geology_fingerprint: geology_fp,
            applied_intervention_seq: 0,
            world_core: world.core_owned(),
            hydrology_state: hydrology_state_value,
            geology_dynamics_state: world.exec_scratch.geology_dynamics.clone(),
        };
        let output_path = output_path
            .unwrap_or_else(|| cache_dir.join(format!("diagnostic-{}.bin", target_tick)));
        save_snapshot(&output_path, &envelope)?;
        let view_path = output_path.with_extension("json");
        save_snapshot_view(&view_path, &envelope)?;
        eprintln!(
            "alpha snapshot generated: path={} tick={} era={}",
            output_path.display(),
            envelope.tick,
            envelope.era
        );
        return Ok(());
    }

    let stages = [
        AlphaSnapshotStage::Environment,
        AlphaSnapshotStage::Life,
        AlphaSnapshotStage::Civilization,
        AlphaSnapshotStage::History,
    ];

    let mut manifest_entries = Vec::new();
    for stage in stages {
        if let Some(limit_stage) = until_stage {
            if stage.target_tick() > limit_stage.target_tick() {
                break;
            }
        }
        if world.clock.tick > stage.target_tick() {
            continue;
        }
        while world.clock.tick < stage.target_tick() {
            sim::exec_world_with_feedback_and_hydrology(
                &mut world,
                &mut feedback,
                &mut hydrology_state,
            );
            if let Some(state) = hydrology_state.as_mut() {
                sim::hydrology::sync_hydrology_state_for_headless_runner(
                    &mut world,
                    state,
                    &geology_params,
                );
            }
        }
        let hydrology_state_value = hydrology_state
            .as_ref()
            .cloned()
            .ok_or_else(|| "hydrology state is missing".to_string())?;
        let envelope = PrecomputedWorldSnapshotEnvelope {
            format_version: SNAPSHOT_FORMAT_VERSION,
            seed: SEED.to_string(),
            mesh_level: level,
            stage,
            tick: world.clock.tick,
            era: world.clock.epoch.as_key().to_string(),
            geology_fingerprint: geology_fp.clone(),
            applied_intervention_seq: 0,
            world_core: world.core_owned(),
            hydrology_state: hydrology_state_value,
            geology_dynamics_state: world.exec_scratch.geology_dynamics.clone(),
        };
        let filename = stage_filename(stage);
        let view_filename = stage_view_filename(stage);
        save_snapshot(&cache_dir.join(&filename), &envelope)?;
        save_snapshot_view(&cache_dir.join(&view_filename), &envelope)?;
        manifest_entries.push(PrecomputedWorldSnapshotManifestEntry {
            stage,
            filename,
            tick: envelope.tick,
            era: envelope.era,
            geology_fingerprint: geology_fp.clone(),
        });
    }

    let manifest = PrecomputedWorldSnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        seed: SEED.to_string(),
        mesh_level: level,
        entries: manifest_entries.clone(),
    };
    save_manifest(&cache_dir.join("manifest.json"), &manifest)?;

    fs::create_dir_all(&mirror_dir).map_err(|err| {
        format!(
            "failed to create mirror directory {}: {err}",
            mirror_dir.display()
        )
    })?;
    for entry in &manifest_entries {
        let src = cache_dir.join(&entry.filename);
        let dst = mirror_dir.join(&entry.filename);
        fs::copy(&src, &dst).map_err(|err| {
            format!(
                "failed to mirror snapshot {} -> {}: {err}",
                src.display(),
                dst.display()
            )
        })?;
        let src_view = cache_dir.join(stage_view_filename(entry.stage));
        let dst_view = mirror_dir.join(stage_view_filename(entry.stage));
        fs::copy(&src_view, &dst_view).map_err(|err| {
            format!(
                "failed to mirror snapshot view {} -> {}: {err}",
                src_view.display(),
                dst_view.display()
            )
        })?;
    }
    save_manifest(&mirror_dir.join("manifest.json"), &manifest)?;
    eprintln!(
        "alpha snapshots generated: cache={} mirror={} resume_from={} resume_path={} until_stage={}",
        cache_dir.display(),
        mirror_dir.display(),
        args.resume_from
            .map(|stage| stage.as_str().to_string())
            .unwrap_or_else(|| "none".to_string()),
        resume_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        until_stage
            .map(|stage| stage.as_str().to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    Ok(())
}

fn stage_for_tick(tick: u64) -> AlphaSnapshotStage {
    if tick >= AlphaSnapshotStage::History.target_tick() {
        AlphaSnapshotStage::History
    } else if tick >= AlphaSnapshotStage::Civilization.target_tick() {
        AlphaSnapshotStage::Civilization
    } else if tick >= AlphaSnapshotStage::Life.target_tick() {
        AlphaSnapshotStage::Life
    } else {
        AlphaSnapshotStage::Environment
    }
}

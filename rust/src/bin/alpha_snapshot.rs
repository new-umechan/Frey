use std::fs;
use std::path::PathBuf;

use frey_wasm::sim;
use frey_wasm::sim::precomputed::{
    canonical_cache_dir, geology_fingerprint, mirror_dir, save_manifest, save_snapshot,
    save_snapshot_view, stage_filename, stage_view_filename, AlphaSnapshotStage,
    PrecomputedWorldSnapshotEnvelope, PrecomputedWorldSnapshotManifest,
    PrecomputedWorldSnapshotManifestEntry, SNAPSHOT_FORMAT_VERSION,
};
use frey_wasm::sim::world::FeedbackQueue;
use frey_wasm::GeologyParams;

const SEED: &str = "alpha";
const LEVEL: u32 = 6;

fn parse_args(argv: &[String]) -> Result<(u32, PathBuf, PathBuf), String> {
    let mut level = LEVEL;
    let mut cache_dir = canonical_cache_dir();
    let mut mirror_dir = mirror_dir();
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
            "--help" => {
                eprintln!("Usage: cargo run --manifest-path rust/Cargo.toml --bin alpha_snapshot -- [options]");
                eprintln!("  --level <n>");
                eprintln!("  --cache-dir <path>");
                eprintln!("  --mirror-dir <path>");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok((level, cache_dir, mirror_dir))
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let argv = std::env::args().skip(1).collect::<Vec<_>>();
    let (level, cache_dir, mirror_dir) = parse_args(&argv)?;
    let geology_params = GeologyParams {
        level,
        ..GeologyParams::default()
    };
    let geology_fp = geology_fingerprint(&geology_params)?;

    let (mut world, erosion_state) =
        sim::headless::init_world_for_headless_runner(SEED, level, geology_params.clone())?;
    let mut hydrology_state = Some(erosion_state);
    let mut feedback = FeedbackQueue::new(world.cell_count());

    let stages = [
        AlphaSnapshotStage::Environment,
        AlphaSnapshotStage::Life,
        AlphaSnapshotStage::Civilization,
        AlphaSnapshotStage::History,
    ];

    let mut manifest_entries = Vec::new();
    for stage in stages {
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
        "alpha snapshots generated: cache={} mirror={}",
        cache_dir.display(),
        mirror_dir.display()
    );
    Ok(())
}

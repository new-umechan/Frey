#![cfg(feature = "precompute_server")]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::File;
use std::io::{Cursor, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use crate::application::world_dto::{
    CheckpointTicksResponse, ExecWorldSliceResponse, FieldResponse, InitWorldConfig,
    MetricsResponse, TimelineConfig, TimelineStateResponse, ViewDeltaFieldResponse, ViewDeltaQuery,
    ViewDeltaResponse,
};
use crate::application::{world_query_use_cases, world_use_cases};
use crate::application::world_service::WorldService;
use crate::sim::precomputed::geology_fingerprint;
use crate::sim::{module_doc_records, module_graph_record};
use crate::{generate_mesh_core, GeologyParams};

const STORE_FORMAT_VERSION: u32 = 1;
const DEFAULT_STORE_DIR: &str = "data/precomputed/worlds";
const DEFAULT_MAX_TICK: u32 = 1600;
const DEFAULT_KEYFRAME_INTERVAL: u32 = 64;
const DEFAULT_FRAME_COMPRESSION: FrameCompression = FrameCompression::Zstd;
const ZSTD_LEVEL: i32 = 3;

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<ServerState>>,
}

struct ServerState {
    store: PrecomputedStore,
    sessions: BTreeMap<String, WorldSession>,
    seed_worlds: BTreeMap<String, String>,
    requests: BTreeMap<String, GenerationRequestRecord>,
    next_world_seq: u64,
    next_request_seq: u64,
}

struct WorldSession {
    seed: String,
    frame: MaterializedFrame,
}

#[derive(Clone, Serialize)]
struct PrecomputedWorldRecord {
    seed: String,
    world_id: String,
    mesh_level: u32,
    max_tick: u32,
    ticks: Vec<u32>,
    status: &'static str,
}

#[derive(Clone, Serialize)]
struct GenerationRequestRecord {
    request_id: String,
    seed: String,
    mesh_level: u32,
    status: &'static str,
}

#[derive(Serialize)]
struct SeedsResponse {
    mesh_level: u32,
    max_tick: u32,
    seeds: Vec<PrecomputedWorldRecord>,
    pending_requests: Vec<GenerationRequestRecord>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize)]
struct GenerationRequestedResponse {
    request_id: String,
    seed: String,
    mesh_level: u32,
    status: &'static str,
    message: String,
}

#[derive(Serialize)]
struct MeshResponse {
    positions: Vec<f32>,
    indices: Vec<u32>,
    cell_overlay_positions: Vec<f32>,
    cell_overlay_cell_ids: Vec<u32>,
    cell_overlay_lift: Vec<f32>,
}

#[derive(Deserialize)]
struct InitWorldRequest {
    seed: String,
    mesh_level: u32,
    #[serde(default)]
    config: Option<InitWorldConfig>,
}

#[derive(Deserialize)]
struct AdvanceRequest {
    #[serde(default = "default_tick_count")]
    tick_count: u32,
}

#[derive(Deserialize)]
struct SliceRequest {
    #[serde(default = "default_work_budget")]
    work_budget: u32,
    #[serde(default)]
    options: Option<ViewDeltaQuery>,
}

#[derive(Deserialize)]
struct FieldQuery {
    #[serde(default = "default_lod")]
    lod: u32,
}

#[derive(Deserialize)]
struct ViewDeltaRequest {
    #[serde(default)]
    options: Option<ViewDeltaQuery>,
}

#[derive(Deserialize)]
struct SeekRequest {
    tick: u32,
}

#[derive(Deserialize)]
struct RateRequest {
    #[serde(rename = "rate")]
    _rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrecomputedStoreManifest {
    format_version: u32,
    seed: String,
    mesh_level: u32,
    max_tick: u32,
    keyframe_interval: u32,
    #[serde(default)]
    frame_compression: FrameCompression,
    geology_fingerprint: String,
    field_kinds: Vec<String>,
    frames: Vec<FrameManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FrameManifestEntry {
    tick: u32,
    kind: FrameKind,
    filename: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FrameKind {
    Keyframe,
    Delta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FrameCompression {
    None,
    Zstd,
}

impl Default for FrameCompression {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct KeyframeEnvelope {
    format_version: u32,
    seed: String,
    mesh_level: u32,
    tick: u32,
    frame: MaterializedFrame,
}

#[derive(Clone, Serialize, Deserialize)]
struct DeltaEnvelope {
    format_version: u32,
    seed: String,
    mesh_level: u32,
    from_tick: u32,
    to_tick: u32,
    delta: ViewDeltaResponse,
}

#[derive(Clone, Serialize, Deserialize)]
struct MaterializedFrame {
    world_id: String,
    tick: u32,
    head_tick: u32,
    metrics: MetricsResponse,
    timeline: TimelineStateResponse,
    fields: BTreeMap<String, FieldResponse>,
}

struct PrecomputedStore {
    seeds: BTreeMap<String, SeedStore>,
}

struct SeedStore {
    root_dir: PathBuf,
    manifest: PrecomputedStoreManifest,
    keyframes: BTreeMap<u32, FrameManifestEntry>,
    deltas: BTreeMap<u32, FrameManifestEntry>,
}

fn default_tick_count() -> u32 {
    1
}

fn default_work_budget() -> u32 {
    1
}

fn default_lod() -> u32 {
    1
}

pub async fn run_from_env() -> Result<(), String> {
    let bind = std::env::var("FREY_PRECOMPUTE_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let addr = bind
        .parse::<SocketAddr>()
        .map_err(|err| format!("invalid FREY_PRECOMPUTE_BIND={bind}: {err}"))?;
    let store_dir = env_path("FREY_PRECOMPUTE_STORE_DIR", DEFAULT_STORE_DIR);
    let store = PrecomputedStore::load(&store_dir)?;
    let state = ServerState::from_store(store);
    let app = router(AppState {
        inner: Arc::new(Mutex::new(state)),
    });

    eprintln!(
        "frey precompute server reading {} and listening on http://{addr}",
        store_dir.display()
    );
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| format!("failed to bind {addr}: {err}"))?;
    axum::serve(listener, app)
        .await
        .map_err(|err| format!("server failed: {err}"))
}

pub fn run_precompute_world_from_env() -> Result<(), String> {
    let args = PrecomputeArgs::parse(&std::env::args().skip(1).collect::<Vec<_>>())?;
    precompute_world(args)
}

struct PrecomputeArgs {
    seed: String,
    level: u32,
    ticks: u32,
    out_dir: PathBuf,
    keyframe_interval: u32,
    compression: FrameCompression,
}

impl PrecomputeArgs {
    fn parse(argv: &[String]) -> Result<Self, String> {
        let mut seed = "alpha".to_string();
        let mut level = 6u32;
        let mut ticks = DEFAULT_MAX_TICK;
        let mut out_dir = PathBuf::from(DEFAULT_STORE_DIR);
        let mut keyframe_interval = DEFAULT_KEYFRAME_INTERVAL;
        let mut compression = DEFAULT_FRAME_COMPRESSION;
        let mut i = 0usize;
        while i < argv.len() {
            match argv[i].as_str() {
                "--seed" => {
                    seed = required_arg(argv, i, "--seed")?;
                    i += 2;
                }
                "--level" => {
                    level = parse_u32_arg(argv, i, "--level")?;
                    i += 2;
                }
                "--ticks" => {
                    ticks = parse_u32_arg(argv, i, "--ticks")?;
                    i += 2;
                }
                "--out-dir" => {
                    out_dir = PathBuf::from(required_arg(argv, i, "--out-dir")?);
                    i += 2;
                }
                "--keyframe-interval" => {
                    keyframe_interval = parse_u32_arg(argv, i, "--keyframe-interval")?;
                    i += 2;
                }
                "--compression" => {
                    compression = parse_frame_compression_arg(argv, i, "--compression")?;
                    i += 2;
                }
                "--help" => {
                    eprintln!("Usage: cargo run --manifest-path rust/Cargo.toml --features precompute_server --bin precompute_world -- [options]");
                    eprintln!("  --seed <seed>");
                    eprintln!("  --level <n>");
                    eprintln!("  --ticks <n>");
                    eprintln!("  --out-dir <path>");
                    eprintln!("  --keyframe-interval <n>");
                    eprintln!("  --compression <none|zstd>");
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        if keyframe_interval == 0 {
            return Err("--keyframe-interval must be greater than 0".to_string());
        }
        Ok(Self {
            seed,
            level,
            ticks,
            out_dir,
            keyframe_interval,
            compression,
        })
    }
}

fn required_arg(argv: &[String], i: usize, name: &str) -> Result<String, String> {
    argv.get(i + 1)
        .cloned()
        .ok_or_else(|| format!("{name} requires value"))
}

fn parse_u32_arg(argv: &[String], i: usize, name: &str) -> Result<u32, String> {
    required_arg(argv, i, name)?
        .parse::<u32>()
        .map_err(|_| format!("{name} must be an unsigned integer"))
}

fn parse_frame_compression_arg(
    argv: &[String],
    i: usize,
    name: &str,
) -> Result<FrameCompression, String> {
    match required_arg(argv, i, name)?.as_str() {
        "none" => Ok(FrameCompression::None),
        "zstd" => Ok(FrameCompression::Zstd),
        other => Err(format!("{name} must be one of: none, zstd (got {other})")),
    }
}

fn env_path(name: &str, default_value: &str) -> PathBuf {
    std::env::var(name)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default_value))
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/precomputed/seeds", get(list_seeds))
        .route("/api/precompute-requests", post(create_generation_request))
        .route("/api/mesh/:level", get(generate_mesh))
        .route("/api/worlds", post(init_world))
        .route("/api/worlds/:world_id/advance", post(advance_world))
        .route(
            "/api/worlds/:world_id/advance-slice-and-delta",
            post(advance_slice_and_delta),
        )
        .route("/api/worlds/:world_id/view-delta", post(get_view_delta))
        .route("/api/worlds/:world_id/metrics", get(get_metrics))
        .route("/api/worlds/:world_id/timeline", get(get_timeline_state))
        .route("/api/worlds/:world_id/field/:field_kind", get(get_field))
        .route("/api/worlds/:world_id/checkpoints", get(list_checkpoint_ticks))
        .route("/api/worlds/:world_id/seek", post(seek_world))
        .route("/api/worlds/:world_id/rewind", post(rewind_world))
        .route("/api/worlds/:world_id/simulation-rate", post(set_simulation_rate))
        .route("/api/worlds/:world_id/profiled", post(exec_world_profiled))
        .route("/api/exec-modules", get(get_exec_modules))
        .route("/api/exec-module-graph", get(get_exec_module_graph))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

impl ServerState {
    fn from_store(store: PrecomputedStore) -> Self {
        Self {
            store,
            sessions: BTreeMap::new(),
            seed_worlds: BTreeMap::new(),
            requests: BTreeMap::new(),
            next_world_seq: 1,
            next_request_seq: 1,
        }
    }

    fn create_request(&mut self, seed: String, mesh_level: u32) -> GenerationRequestRecord {
        if let Some(existing) = self.requests.values().find(|request| request.seed == seed) {
            return existing.clone();
        }
        let request_id = format!("request-{:06}", self.next_request_seq);
        self.next_request_seq = self.next_request_seq.saturating_add(1);
        let record = GenerationRequestRecord {
            request_id: request_id.clone(),
            seed,
            mesh_level,
            status: "queued",
        };
        self.requests.insert(request_id, record.clone());
        record
    }

    fn init_session(&mut self, seed: String, mesh_level: u32) -> Result<String, GenerationRequestRecord> {
        let Some(seed_store) = self.store.seeds.get(&seed) else {
            return Err(self.create_request(seed, mesh_level));
        };
        if seed_store.manifest.mesh_level != mesh_level {
            return Err(self.create_request(seed, mesh_level));
        }
        if let Some(world_id) = self.seed_worlds.get(&seed) {
            return Ok(world_id.clone());
        }
        let world_id = format!("precomputed-{:06}", self.next_world_seq);
        self.next_world_seq = self.next_world_seq.saturating_add(1);
        let mut frame = self
            .store
            .materialize(&seed, 0)
            .unwrap_or_else(|err| panic!("failed to materialize precomputed seed {seed}: {err}"));
        frame.world_id = world_id.clone();
        frame.metrics.world_id = world_id.clone();
        frame.timeline.world_id = world_id.clone();
        self.sessions.insert(
            world_id.clone(),
            WorldSession {
                seed: seed.clone(),
                frame,
            },
        );
        self.seed_worlds.insert(seed, world_id.clone());
        Ok(world_id)
    }

    fn session(&self, world_id: &str) -> Result<&WorldSession, String> {
        self.sessions
            .get(world_id)
            .ok_or_else(|| format!("world not found: {world_id}"))
    }

    fn session_mut(&mut self, world_id: &str) -> Result<&mut WorldSession, String> {
        self.sessions
            .get_mut(world_id)
            .ok_or_else(|| format!("world not found: {world_id}"))
    }
}

impl PrecomputedStore {
    fn load(root_dir: &Path) -> Result<Self, String> {
        let mut seeds = BTreeMap::new();
        if root_dir.exists() {
            for entry in fs::read_dir(root_dir)
                .map_err(|err| format!("failed to read store dir {}: {err}", root_dir.display()))?
            {
                let entry = entry.map_err(|err| format!("failed to read store entry: {err}"))?;
                if !entry
                    .file_type()
                    .map_err(|err| format!("failed to read file type: {err}"))?
                    .is_dir()
                {
                    continue;
                }
                let seed_root = entry.path();
                let manifest_path = seed_root.join("manifest.json");
                if !manifest_path.exists() {
                    continue;
                }
                let manifest = load_manifest(&manifest_path)?;
                if manifest.format_version != STORE_FORMAT_VERSION {
                    return Err(format!(
                        "store format mismatch in {}: expected {}, got {}",
                        manifest_path.display(),
                        STORE_FORMAT_VERSION,
                        manifest.format_version
                    ));
                }
                let mut keyframes = BTreeMap::new();
                let mut deltas = BTreeMap::new();
                for frame in &manifest.frames {
                    match frame.kind {
                        FrameKind::Keyframe => {
                            keyframes.insert(frame.tick, frame.clone());
                        }
                        FrameKind::Delta => {
                            deltas.insert(frame.tick, frame.clone());
                        }
                    }
                }
                seeds.insert(
                    manifest.seed.clone(),
                    SeedStore {
                        root_dir: seed_root,
                        manifest,
                        keyframes,
                        deltas,
                    },
                );
            }
        }
        Ok(Self {
            seeds,
        })
    }

    fn records(&self) -> Vec<PrecomputedWorldRecord> {
        self.seeds
            .values()
            .map(|seed_store| PrecomputedWorldRecord {
                seed: seed_store.manifest.seed.clone(),
                world_id: String::new(),
                mesh_level: seed_store.manifest.mesh_level,
                max_tick: seed_store.manifest.max_tick,
                ticks: (0..=seed_store.manifest.max_tick).collect(),
                status: "ready",
            })
            .collect()
    }

    fn max_tick(&self) -> u32 {
        self.seeds
            .values()
            .map(|seed| seed.manifest.max_tick)
            .max()
            .unwrap_or(0)
    }

    fn mesh_level(&self) -> u32 {
        self.seeds
            .values()
            .map(|seed| seed.manifest.mesh_level)
            .next()
            .unwrap_or(6)
    }

    fn materialize(&self, seed: &str, tick: u32) -> Result<MaterializedFrame, String> {
        let seed_store = self
            .seeds
            .get(seed)
            .ok_or_else(|| format!("precomputed seed not found: {seed}"))?;
        if tick > seed_store.manifest.max_tick {
            return Err(format!("tick {tick} is not precomputed for seed={seed}"));
        }
        let (&key_tick, key_entry) = seed_store
            .keyframes
            .range(..=tick)
            .next_back()
            .ok_or_else(|| format!("keyframe not found for seed={seed} tick={tick}"))?;
        let mut frame = load_keyframe(
            &seed_store.root_dir.join(&key_entry.filename),
            seed_store.manifest.frame_compression,
        )?
        .frame;
        for next_tick in key_tick.saturating_add(1)..=tick {
            let delta = self.load_delta(seed, next_tick)?;
            apply_delta_to_frame(&mut frame, &delta);
        }
        frame.tick = tick;
        Ok(frame)
    }

    fn load_delta(&self, seed: &str, tick: u32) -> Result<ViewDeltaResponse, String> {
        let seed_store = self
            .seeds
            .get(seed)
            .ok_or_else(|| format!("precomputed seed not found: {seed}"))?;
        let entry = seed_store
            .deltas
            .get(&tick)
            .ok_or_else(|| format!("delta not found for seed={seed} tick={tick}"))?;
        Ok(load_delta(
            &seed_store.root_dir.join(&entry.filename),
            seed_store.manifest.frame_compression,
        )?
        .delta)
    }
}

fn load_manifest(path: &Path) -> Result<PrecomputedStoreManifest, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read manifest {}: {err}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|err| format!("failed to decode manifest {}: {err}", path.display()))
}

fn save_manifest(path: &Path, manifest: &PrecomputedStoreManifest) -> Result<(), String> {
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|err| format!("failed to encode manifest: {err}"))?;
    atomic_write(path, json.as_bytes())
}

fn load_keyframe(path: &Path, compression: FrameCompression) -> Result<KeyframeEnvelope, String> {
    load_envelope(path, compression, "keyframe")
}

fn save_keyframe(
    path: &Path,
    compression: FrameCompression,
    envelope: &KeyframeEnvelope,
) -> Result<(), String> {
    let bytes = encode_envelope(envelope, compression, "keyframe")?;
    atomic_write(path, &bytes)
}

fn load_delta(path: &Path, compression: FrameCompression) -> Result<DeltaEnvelope, String> {
    load_envelope(path, compression, "delta")
}

fn save_delta(
    path: &Path,
    compression: FrameCompression,
    envelope: &DeltaEnvelope,
) -> Result<(), String> {
    let bytes = encode_envelope(envelope, compression, "delta")?;
    atomic_write(path, &bytes)
}

fn load_envelope<T>(
    path: &Path,
    compression: FrameCompression,
    label: &str,
) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read {label} {}: {err}", path.display()))?;
    decode_envelope_bytes(bytes, compression, label, path)
}

fn encode_envelope<T>(
    envelope: &T,
    compression: FrameCompression,
    label: &str,
) -> Result<Vec<u8>, String>
where
    T: Serialize,
{
    let bytes = bincode::serde::encode_to_vec(envelope, bincode::config::standard())
        .map_err(|err| format!("failed to encode {label}: {err}"))?;
    match compression {
        FrameCompression::None => Ok(bytes),
        FrameCompression::Zstd => zstd::stream::encode_all(Cursor::new(bytes), ZSTD_LEVEL)
            .map_err(|err| format!("failed to compress {label}: {err}")),
    }
}

fn decode_envelope_bytes<T>(
    bytes: Vec<u8>,
    compression: FrameCompression,
    label: &str,
    path: &Path,
) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let decoded = match compression {
        FrameCompression::None => bytes,
        FrameCompression::Zstd => zstd::stream::decode_all(Cursor::new(bytes))
            .map_err(|err| format!("failed to decompress {label} {}: {err}", path.display()))?,
    };
    let (envelope, _): (T, usize) =
        bincode::serde::decode_from_slice(&decoded, bincode::config::standard())
            .map_err(|err| format!("failed to decode {label} {}: {err}", path.display()))?;
    Ok(envelope)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("missing parent directory for {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|err| format!("failed to create directory {}: {err}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid file name for {}", path.display()))?;
    let tmp_path = parent.join(format!(".{file_name}.tmp"));
    let result = (|| -> Result<(), String> {
        let mut file = File::create(&tmp_path)
            .map_err(|err| format!("failed to create {}: {err}", tmp_path.display()))?;
        file.write_all(bytes)
            .map_err(|err| format!("failed to write {}: {err}", tmp_path.display()))?;
        file.sync_all()
            .map_err(|err| format!("failed to sync {}: {err}", tmp_path.display()))?;
        drop(file);
        fs::rename(&tmp_path, path)
            .map_err(|err| format!("failed to rename {}: {err}", path.display()))
    })();
    if result.is_err() && tmp_path.exists() {
        let _ = fs::remove_file(&tmp_path);
    }
    result
}

fn precompute_world(args: PrecomputeArgs) -> Result<(), String> {
    let seed_dir = args.out_dir.join(&args.seed);
    fs::create_dir_all(seed_dir.join("keyframes"))
        .map_err(|err| format!("failed to create keyframe dir: {err}"))?;
    fs::create_dir_all(seed_dir.join("deltas"))
        .map_err(|err| format!("failed to create delta dir: {err}"))?;

    let geology_params = GeologyParams {
        level: args.level,
        ..GeologyParams::default()
    };
    let geology_fp = geology_fingerprint(&geology_params)?;
    let mut service = WorldService::new();
    let init = world_use_cases::init_world(
        &mut service,
        args.seed.clone(),
        args.level,
        InitWorldConfig {
            geology_params: Some(geology_params),
            simulation_rate: Some(1.0),
            verification_mode: None,
            timeline: Some(TimelineConfig {
                checkpoint_interval: Some(1),
                checkpoint_limit: Some(args.ticks as usize + 2),
                undo_log_limit: Some(args.ticks as usize + 2),
                undo_future_prune_grace_ticks: None,
                max_estimated_bytes: None,
            }),
        },
    )?;
    let world_id = init.world_id;
    let mut frames = Vec::new();

    save_current_keyframe(
        &service,
        &world_id,
        &args.seed,
        args.level,
        args.ticks,
        &seed_dir,
        0,
        args.compression,
        &mut frames,
    )?;

    for tick in 1..=args.ticks {
        world_use_cases::advance_timeline(&mut service, world_id.clone(), 1)?;
        let delta = world_query_use_cases::get_view_delta(
            &mut service,
            world_id.clone(),
            Some(default_field_kinds().into_iter().collect()),
        )?;
        let filename = frame_filename("deltas", tick, args.compression);
        save_delta(
            &seed_dir.join(&filename),
            args.compression,
            &DeltaEnvelope {
                format_version: STORE_FORMAT_VERSION,
                seed: args.seed.clone(),
                mesh_level: args.level,
                from_tick: tick.saturating_sub(1),
                to_tick: tick,
                delta,
            },
        )?;
        frames.push(FrameManifestEntry {
            tick,
            kind: FrameKind::Delta,
            filename,
        });
        if tick % args.keyframe_interval == 0 || tick == args.ticks {
            save_current_keyframe(
                &service,
                &world_id,
                &args.seed,
                args.level,
                args.ticks,
                &seed_dir,
                tick,
                args.compression,
                &mut frames,
            )?;
        }
    }

    let manifest = PrecomputedStoreManifest {
        format_version: STORE_FORMAT_VERSION,
        seed: args.seed,
        mesh_level: args.level,
        max_tick: args.ticks,
        keyframe_interval: args.keyframe_interval,
        frame_compression: args.compression,
        geology_fingerprint: geology_fp,
        field_kinds: default_field_kinds(),
        frames,
    };
    save_manifest(&seed_dir.join("manifest.json"), &manifest)?;
    eprintln!(
        "precomputed seed={} ticks={} out={}",
        manifest.seed,
        manifest.max_tick,
        seed_dir.display()
    );
    Ok(())
}

fn save_current_keyframe(
    service: &WorldService,
    world_id: &str,
    seed: &str,
    mesh_level: u32,
    head_tick: u32,
    seed_dir: &Path,
    tick: u32,
    compression: FrameCompression,
    frames: &mut Vec<FrameManifestEntry>,
) -> Result<(), String> {
    let filename = frame_filename("keyframes", tick, compression);
    let frame = build_materialized_frame(service, world_id, head_tick)?;
    save_keyframe(
        &seed_dir.join(&filename),
        compression,
        &KeyframeEnvelope {
            format_version: STORE_FORMAT_VERSION,
            seed: seed.to_string(),
            mesh_level,
            tick,
            frame,
        },
    )?;
    frames.push(FrameManifestEntry {
        tick,
        kind: FrameKind::Keyframe,
        filename,
    });
    Ok(())
}

fn frame_filename(dir: &str, tick: u32, compression: FrameCompression) -> String {
    let extension = match compression {
        FrameCompression::None => "bin",
        FrameCompression::Zstd => "bin.zst",
    };
    format!("{dir}/tick-{tick:06}.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct SampleEnvelope {
        tick: u32,
        label: String,
        values: Vec<u32>,
    }

    #[test]
    fn encode_decode_roundtrip_without_compression() {
        let envelope = SampleEnvelope {
            tick: 7,
            label: "alpha".to_string(),
            values: vec![1, 2, 3, 5, 8],
        };
        let bytes = encode_envelope(&envelope, FrameCompression::None, "sample")
            .expect("encode without compression");
        let (decoded, _): (SampleEnvelope, usize) =
            bincode::serde::decode_from_slice(&bytes, bincode::config::standard())
                .expect("decode raw bincode");
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn encode_decode_roundtrip_with_zstd() {
        let envelope = SampleEnvelope {
            tick: 42,
            label: "compressed".to_string(),
            values: (0..256).collect(),
        };
        let bytes = encode_envelope(&envelope, FrameCompression::Zstd, "sample")
            .expect("encode with zstd");
        let decoded: SampleEnvelope = decode_envelope_bytes(
            bytes,
            FrameCompression::Zstd,
            "sample",
            Path::new("sample.bin.zst"),
        )
        .expect("decode with zstd");
        assert_eq!(decoded, envelope);
    }
}

fn build_materialized_frame(
    service: &WorldService,
    world_id: &str,
    head_tick: u32,
) -> Result<MaterializedFrame, String> {
    let metrics = world_query_use_cases::get_metrics(service, world_id.to_string())?;
    let mut timeline = world_query_use_cases::get_timeline_state(service, world_id.to_string())?;
    timeline.head_tick = head_tick as f64;
    let mut fields = BTreeMap::new();
    for field_kind in default_field_kinds() {
        let field = world_query_use_cases::get_field(service, world_id, field_kind.clone(), 1)?;
        fields.insert(field_kind, field);
    }
    Ok(MaterializedFrame {
        world_id: world_id.to_string(),
        tick: metrics.tick.max(0.0).floor() as u32,
        head_tick,
        metrics,
        timeline,
        fields,
    })
}

fn lock_state(
    state: &AppState,
) -> Result<std::sync::MutexGuard<'_, ServerState>, (StatusCode, Json<ErrorResponse>)> {
    state.inner.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "server state lock poisoned".to_string(),
            }),
        )
    })
}

fn error_response(status: StatusCode, error: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: error.into() }))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn list_seeds(
    State(state): State<AppState>,
) -> Result<Json<SeedsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let state = lock_state(&state)?;
    let mut seeds = state.store.records();
    for record in &mut seeds {
        if let Some(world_id) = state.seed_worlds.get(&record.seed) {
            record.world_id = world_id.clone();
        }
    }
    Ok(Json(SeedsResponse {
        mesh_level: state.store.mesh_level(),
        max_tick: state.store.max_tick(),
        seeds,
        pending_requests: state.requests.values().cloned().collect(),
    }))
}

async fn create_generation_request(
    State(state): State<AppState>,
    Json(request): Json<InitWorldRequest>,
) -> Result<(StatusCode, Json<GenerationRequestedResponse>), (StatusCode, Json<ErrorResponse>)> {
    let mut state = lock_state(&state)?;
    let record = state.create_request(request.seed.clone(), request.mesh_level);
    Ok((
        StatusCode::ACCEPTED,
        Json(GenerationRequestedResponse {
            request_id: record.request_id,
            seed: record.seed,
            mesh_level: record.mesh_level,
            status: record.status,
            message: format!("precompute requested for seed={}", request.seed),
        }),
    ))
}

async fn generate_mesh(
    AxumPath(level): AxumPath<u32>,
) -> Result<Json<MeshResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mesh = generate_mesh_core(level).map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    Ok(Json(MeshResponse {
        positions: mesh.positions,
        indices: mesh.indices,
        cell_overlay_positions: mesh.cell_overlay_positions,
        cell_overlay_cell_ids: mesh.cell_overlay_cell_ids,
        cell_overlay_lift: mesh.cell_overlay_lift,
    }))
}

async fn init_world(
    State(state): State<AppState>,
    Json(request): Json<InitWorldRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ErrorResponse>)> {
    let mut state = lock_state(&state)?;
    let _ = request.config;
    match state.init_session(request.seed.clone(), request.mesh_level) {
        Ok(world_id) => {
            let session = state
                .session(&world_id)
                .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
            Ok((
                StatusCode::OK,
                Json(serde_json::json!({
                    "world_id": world_id,
                    "tick": session.frame.tick,
                    "head_tick": session.frame.head_tick,
                    "era": session.frame.metrics.era,
                    "cell_count": session.frame.metrics.cell_count,
                    "precompute_status": "ready"
                })),
            ))
        }
        Err(record) => Ok((
            StatusCode::ACCEPTED,
            Json(serde_json::json!({
                "request_id": record.request_id,
                "seed": record.seed,
                "mesh_level": record.mesh_level,
                "precompute_status": record.status,
                "message": "precompute requested"
            })),
        )),
    }
}

async fn advance_world(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
    Json(request): Json<AdvanceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let mut state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let target = session
        .frame
        .tick
        .saturating_add(request.tick_count)
        .min(session.frame.head_tick);
    let seed = session.seed.clone();
    let frame = state
        .store
        .materialize(&seed, target)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let previous = session.frame.tick;
    state
        .session_mut(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?
        .frame = with_world_id(frame, &world_id);
    Ok(Json(serde_json::json!({
        "world_id": world_id,
        "tick": target,
        "head_tick": state.store.seeds.get(&seed).map(|store| store.manifest.max_tick).unwrap_or(target),
        "advanced_ticks": target.saturating_sub(previous)
    })))
}

async fn advance_slice_and_delta(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
    Json(request): Json<SliceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let mut state = lock_state(&state)?;
    let _ = request.work_budget;
    let (seed, current, head_tick) = {
        let session = state
            .session(&world_id)
            .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
        (session.seed.clone(), session.frame.tick, session.frame.head_tick)
    };
    let target = current.saturating_add(1).min(head_tick);
    if target == current {
        let slice = ExecWorldSliceResponse {
            world_id,
            processed_ticks: 0,
            busy: false,
            phase: "precomputed".to_string(),
            tick: current as f64,
            head_tick: head_tick as f64,
            tick_boundary: "completed_tick".to_string(),
        };
        return Ok(Json(serde_json::json!({ "slice": slice, "delta": null })));
    }
    let mut delta = state
        .store
        .load_delta(&seed, target)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    delta.world_id = world_id.clone();
    delta.head_tick = head_tick as f64;
    filter_delta_fields(&mut delta, request.options);
    {
        let session = state
            .session_mut(&world_id)
            .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
        apply_delta_to_frame(&mut session.frame, &delta);
        session.frame.world_id = world_id.clone();
    }
    let slice = ExecWorldSliceResponse {
        world_id,
        processed_ticks: target.saturating_sub(current),
        busy: false,
        phase: "precomputed".to_string(),
        tick: target as f64,
        head_tick: head_tick as f64,
        tick_boundary: "completed_tick".to_string(),
    };
    Ok(Json(serde_json::json!({ "slice": slice, "delta": delta })))
}

async fn get_view_delta(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
    Json(request): Json<ViewDeltaRequest>,
) -> Result<Json<ViewDeltaResponse>, (StatusCode, Json<ErrorResponse>)> {
    let state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let mut delta = full_delta_from_frame(&session.frame);
    filter_delta_fields(&mut delta, request.options);
    Ok(Json(delta))
}

async fn get_metrics(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
) -> Result<Json<MetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    Ok(Json(session.frame.metrics.clone()))
}

async fn get_timeline_state(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
) -> Result<Json<TimelineStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    let state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    Ok(Json(session.frame.timeline.clone()))
}

async fn get_field(
    State(state): State<AppState>,
    AxumPath((world_id, field_kind)): AxumPath<(String, String)>,
    Query(query): Query<FieldQuery>,
) -> Result<Json<FieldResponse>, (StatusCode, Json<ErrorResponse>)> {
    let state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let field = session
        .frame
        .fields
        .get(&field_kind)
        .cloned()
        .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, format!("unknown field: {field_kind}")))?;
    Ok(Json(sample_field(field, query.lod)))
}

async fn list_checkpoint_ticks(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
) -> Result<Json<CheckpointTicksResponse>, (StatusCode, Json<ErrorResponse>)> {
    let state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    Ok(Json(CheckpointTicksResponse {
        world_id,
        interval: 1,
        ticks: (0..=session.frame.head_tick).map(|tick| tick as f64).collect(),
    }))
}

async fn seek_world(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
    Json(request): Json<SeekRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let mut state = lock_state(&state)?;
    let seed = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?
        .seed
        .clone();
    let frame = state
        .store
        .materialize(&seed, request.tick)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    state
        .session_mut(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?
        .frame = with_world_id(frame, &world_id);
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    Ok(Json(serde_json::json!({
        "world_id": world_id,
        "tick": session.frame.tick,
        "head_tick": session.frame.head_tick
    })))
}

async fn rewind_world(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
    Json(request): Json<AdvanceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let mut state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let target = session.frame.tick.saturating_sub(request.tick_count);
    let seed = session.seed.clone();
    let frame = state
        .store
        .materialize(&seed, target)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let previous = session.frame.tick;
    state
        .session_mut(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?
        .frame = with_world_id(frame, &world_id);
    Ok(Json(serde_json::json!({
        "world_id": world_id,
        "tick": target,
        "head_tick": state.store.seeds.get(&seed).map(|store| store.manifest.max_tick).unwrap_or(target),
        "rewound_ticks": previous.saturating_sub(target)
    })))
}

async fn set_simulation_rate(
    State(_state): State<AppState>,
    AxumPath(_world_id): AxumPath<String>,
    Json(_request): Json<RateRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    Ok(StatusCode::NO_CONTENT)
}

async fn exec_world_profiled(
    State(_state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
    Json(request): Json<AdvanceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(serde_json::json!({
        "world_id": world_id,
        "steps": request.tick_count,
        "exec_feedback_ms": 0.0,
        "exec_geology_terrain_ms": 0.0,
        "exec_climate_ms": 0.0,
        "exec_glaciology_ms": 0.0,
        "exec_hydrology_ms": 0.0,
        "exec_ecology_ms": 0.0,
        "exec_society_ms": 0.0,
        "exec_transition_ms": 0.0,
        "step_sync_erosion_ms": 0.0,
        "step_observe_world_change_ms": 0.0,
        "step_history_snapshot_ms": 0.0
    })))
}

async fn get_exec_modules() -> Json<serde_json::Value> {
    Json(serde_json::to_value(module_doc_records()).unwrap_or_else(|_| serde_json::json!([])))
}

async fn get_exec_module_graph() -> Json<serde_json::Value> {
    Json(serde_json::to_value(module_graph_record()).unwrap_or_else(|_| {
        serde_json::json!({ "modules": [], "edges": [] })
    }))
}

fn with_world_id(mut frame: MaterializedFrame, world_id: &str) -> MaterializedFrame {
    frame.world_id = world_id.to_string();
    frame.metrics.world_id = world_id.to_string();
    frame.timeline.world_id = world_id.to_string();
    frame
}

fn full_delta_from_frame(frame: &MaterializedFrame) -> ViewDeltaResponse {
    ViewDeltaResponse {
        world_id: frame.world_id.clone(),
        tick: frame.tick as f64,
        head_tick: frame.head_tick as f64,
        era: frame.metrics.era.clone(),
        real_years_per_tick: frame.metrics.real_years_per_tick,
        runtime_tick_ms: frame.metrics.runtime_tick_ms,
        budgets: frame.metrics.budgets.clone(),
        deltas: frame
            .fields
            .values()
            .map(|field| ViewDeltaFieldResponse {
                field_kind: field.field_kind.clone(),
                mode: "full".to_string(),
                ranges: vec![],
                dirty_bitmap: None,
                f32_data: field.f32_data.clone(),
                u32_data: field.u32_data.clone(),
                i32_data: field.i32_data.clone(),
            })
            .collect(),
    }
}

fn filter_delta_fields(delta: &mut ViewDeltaResponse, options: Option<ViewDeltaQuery>) {
    let Some(fields) = options.and_then(|query| query.include_fields) else {
        return;
    };
    let include = fields.into_iter().collect::<BTreeSet<_>>();
    delta
        .deltas
        .retain(|field_delta| include.contains(&field_delta.field_kind));
}

fn apply_delta_to_frame(frame: &mut MaterializedFrame, delta: &ViewDeltaResponse) {
    frame.tick = delta.tick.max(0.0).floor() as u32;
    frame.metrics.tick = delta.tick;
    frame.metrics.era = delta.era.clone();
    frame.metrics.real_years_per_tick = delta.real_years_per_tick;
    frame.metrics.runtime_tick_ms = delta.runtime_tick_ms;
    frame.metrics.budgets = delta.budgets.clone();
    frame.timeline.current_tick = delta.tick;
    frame.timeline.head_tick = delta.head_tick;
    for field_delta in &delta.deltas {
        if let Some(field) = frame.fields.get_mut(&field_delta.field_kind) {
            apply_field_delta(field, field_delta);
        } else {
            frame.fields.insert(
                field_delta.field_kind.clone(),
                field_from_delta(field_delta, 0),
            );
        }
    }
}

fn field_from_delta(delta: &ViewDeltaFieldResponse, cell_count: u32) -> FieldResponse {
    let sampled_count = delta
        .f32_data
        .as_ref()
        .map(|values| values.len())
        .or_else(|| delta.u32_data.as_ref().map(|values| values.len()))
        .or_else(|| delta.i32_data.as_ref().map(|values| values.len()))
        .unwrap_or(0) as u32;
    FieldResponse {
        field_kind: delta.field_kind.clone(),
        stride: 1,
        cell_count: cell_count.max(sampled_count),
        sampled_count,
        f32_data: delta.f32_data.clone(),
        u32_data: delta.u32_data.clone(),
        i32_data: delta.i32_data.clone(),
    }
}

fn apply_field_delta(field: &mut FieldResponse, delta: &ViewDeltaFieldResponse) {
    if let Some(values) = delta.f32_data.as_ref() {
        if field.f32_data.is_none() {
            field.f32_data = Some(vec![0.0; field.cell_count as usize]);
        }
        if let Some(target) = field.f32_data.as_mut() {
            apply_numeric_delta(target, values, delta);
        }
    }
    if let Some(values) = delta.u32_data.as_ref() {
        if field.u32_data.is_none() {
            field.u32_data = Some(vec![0; field.cell_count as usize]);
        }
        if let Some(target) = field.u32_data.as_mut() {
            apply_numeric_delta(target, values, delta);
        }
    }
    if let Some(values) = delta.i32_data.as_ref() {
        if field.i32_data.is_none() {
            field.i32_data = Some(vec![0; field.cell_count as usize]);
        }
        if let Some(target) = field.i32_data.as_mut() {
            apply_numeric_delta(target, values, delta);
        }
    }
}

fn apply_numeric_delta<T>(target: &mut Vec<T>, values: &[T], delta: &ViewDeltaFieldResponse)
where
    T: Copy + Default,
{
    if delta.mode == "full" {
        target.clear();
        target.extend_from_slice(values);
        return;
    }
    if delta.mode == "bitmap" {
        let Some(bitmap) = delta.dirty_bitmap.as_ref() else {
            return;
        };
        let mut value_offset = 0usize;
        for (word_index, mut word) in bitmap.iter().copied().enumerate() {
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                let cell_index = word_index * 32 + bit;
                if cell_index >= target.len() || value_offset >= values.len() {
                    return;
                }
                target[cell_index] = values[value_offset];
                value_offset += 1;
                word &= word - 1;
            }
        }
        return;
    }
    let mut offset = 0usize;
    for range in &delta.ranges {
        let start = range.start as usize;
        let end = (range.end as usize).min(target.len());
        if end <= start {
            continue;
        }
        let len = end - start;
        let copy_len = len.min(values.len().saturating_sub(offset));
        for i in 0..copy_len {
            target[start + i] = values[offset + i];
        }
        offset = offset.saturating_add(len);
    }
}

fn sample_field(mut field: FieldResponse, lod: u32) -> FieldResponse {
    let stride = lod.max(1) as usize;
    if stride <= 1 {
        return field;
    }
    field.stride = stride as u32;
    if let Some(values) = field.f32_data.take() {
        field.sampled_count = sampled_len(values.len(), stride as u32);
        field.f32_data = Some(values.into_iter().step_by(stride).collect());
    }
    if let Some(values) = field.u32_data.take() {
        field.sampled_count = sampled_len(values.len(), stride as u32);
        field.u32_data = Some(values.into_iter().step_by(stride).collect());
    }
    if let Some(values) = field.i32_data.take() {
        field.sampled_count = sampled_len(values.len(), stride as u32);
        field.i32_data = Some(values.into_iter().step_by(stride).collect());
    }
    field
}

fn sampled_len(len: usize, stride: u32) -> u32 {
    if len == 0 {
        return 0;
    }
    let stride = stride.max(1) as usize;
    len.div_ceil(stride) as u32
}

fn default_field_kinds() -> Vec<String> {
    [
        "height",
        "lake_depth",
        "plate_id",
        "river_flux",
        "river_next",
        "mantle_heat",
        "erosion_rate",
        "deposition_rate",
        "temperature",
        "precipitation",
        "evapotranspiration",
        "aridity",
        "runoff",
        "ice_pressure",
        "ocean_temperature",
        "wind_u",
        "wind_v",
        "moisture_flux_u",
        "moisture_flux_v",
        "biome",
        "river_transport_cost",
        "crop_adoption_wheat",
        "crop_adoption_rice",
        "crop_adoption_maize",
        "crop_adoption_millet",
        "crop_adoption_potato",
        "crop_adoption_cassava",
        "crop_adoption_sorghum",
        "crop_adoption_yam",
        "crop_available_wheat",
        "crop_available_rice",
        "crop_available_maize",
        "crop_available_millet",
        "crop_available_potato",
        "crop_available_cassava",
        "crop_available_sorghum",
        "crop_available_yam",
        "livestock_adoption_cattle",
        "livestock_adoption_horse",
        "livestock_adoption_sheep",
        "livestock_adoption_pig",
        "livestock_adoption_camel",
        "livestock_available_cattle",
        "livestock_available_horse",
        "livestock_available_sheep",
        "livestock_available_pig",
        "livestock_available_camel",
    ]
    .iter()
    .map(|field| (*field).to_string())
    .collect()
}

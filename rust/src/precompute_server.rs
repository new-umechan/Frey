#![cfg(feature = "precompute_server")]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::File;
use std::io::{Cursor, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

use crate::application::world_dto::{
    CheckpointTicksResponse, ExecWorldSliceResponse, FieldResponse, InitWorldConfig,
    MetricsResponse, TimelineConfig, TimelineStateResponse, ViewDeltaFieldResponse, ViewDeltaQuery,
    ViewDeltaResponse,
};
use crate::application::world_service::WorldService;
use crate::application::{world_query_use_cases, world_use_cases};
use crate::sim::{module_doc_records, module_graph_record};
use crate::{generate_mesh_core, GeologyParams};

// Bump this whenever bincode-serialized frame DTOs change. Serde defaults do
// not make bincode struct additions backward compatible.
const STORE_FORMAT_VERSION: u32 = 2;
const DEFAULT_STORE_DIR: &str = "data/precomputed/worlds";
const DEFAULT_MAX_TICK: u32 = 1600;
const DEFAULT_KEYFRAME_INTERVAL: u32 = 64;
const DEFAULT_PRECOMPUTE_RETENTION_TICKS: usize = 2;
const DEFAULT_FRAME_COMPRESSION: FrameCompression = FrameCompression::Zstd;
const ZSTD_LEVEL: i32 = 3;
const DEFAULT_PRECOMPUTE_PROGRESS_INTERVAL: u32 = 16;
const DEFAULT_STREAM_RADIUS: u32 = 2;
const MAX_STREAM_RADIUS: u32 = 8;
const DEFAULT_COARSE_KEYFRAME_INTERVAL: u32 = 256;
const MIN_COARSE_KEYFRAME_INTERVAL: u32 = 64;
const MAX_COARSE_KEYFRAME_INTERVAL: u32 = 512;
const COARSE_STREAM_FIELDS: [&str; 6] = [
    "height",
    "lake_depth",
    "plate_id",
    "river_flux",
    "river_next",
    "mantle_heat",
];

fn geology_fingerprint(params: &GeologyParams) -> Result<String, String> {
    serde_json::to_string(params)
        .map(|json| format!("geology-params-json-v1:{json}"))
        .map_err(|err| format!("failed to serialize geology params fingerprint: {err}"))
}

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<ServerState>>,
    config: ServerConfig,
}

#[derive(Clone, Default)]
struct ServerConfig {
    public_seeds: Option<BTreeSet<String>>,
    public_mesh_level: Option<u32>,
    max_mesh_level: Option<u32>,
    max_tick: Option<u32>,
    max_lod: Option<u32>,
    disable_precompute_requests: bool,
    cors_origins: Option<Vec<HeaderValue>>,
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

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TickStreamRequest {
    Subscribe {
        request_id: u64,
        center_tick: u32,
        #[serde(default = "default_stream_radius")]
        radius: u32,
        #[serde(default)]
        known_exact_ticks: Vec<u32>,
        #[serde(default)]
        known_coarse_ticks: Vec<u32>,
        #[serde(default = "default_coarse_keyframe_interval")]
        coarse_interval: u32,
        #[serde(default)]
        include_coarse: bool,
    },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TickStreamResponse {
    Catalog {
        world_id: String,
        head_tick: u32,
        exact_radius_limit: u32,
        coarse_interval: u32,
    },
    ExactAnchor {
        request_id: u64,
        tick: u32,
        metrics: MetricsResponse,
        timeline: TimelineStateResponse,
        frame: ViewDeltaResponse,
    },
    ExactDelta {
        request_id: u64,
        tick: u32,
        delta: ViewDeltaResponse,
    },
    CoarseFrame {
        request_id: u64,
        tick: u32,
        metrics: MetricsResponse,
        timeline: TimelineStateResponse,
        frame: ViewDeltaResponse,
    },
    Complete {
        request_id: u64,
        center_tick: u32,
        window_start: u32,
        window_end: u32,
    },
    Error {
        request_id: Option<u64>,
        message: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
struct ExactStreamPlan {
    window_start: u32,
    window_end: u32,
    anchor_tick: Option<u32>,
    delta_start: u32,
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

#[derive(Clone)]
struct PrecomputedStore {
    root_dir: PathBuf,
    seeds: BTreeMap<String, SeedStore>,
}

#[derive(Clone)]
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

fn default_stream_radius() -> u32 {
    DEFAULT_STREAM_RADIUS
}

fn default_coarse_keyframe_interval() -> u32 {
    DEFAULT_COARSE_KEYFRAME_INTERVAL
}

pub async fn run_from_env() -> Result<(), String> {
    let bind =
        std::env::var("FREY_PRECOMPUTE_BIND").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let addr = bind
        .parse::<SocketAddr>()
        .map_err(|err| format!("invalid FREY_PRECOMPUTE_BIND={bind}: {err}"))?;
    let store_dir = env_path("FREY_PRECOMPUTE_STORE_DIR", DEFAULT_STORE_DIR);
    let config = ServerConfig::from_env()?;
    let store = PrecomputedStore::load(&store_dir)?;
    let state = ServerState::from_store(store);
    let app = router(AppState {
        inner: Arc::new(Mutex::new(state)),
        config,
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

impl ServerConfig {
    fn from_env() -> Result<Self, String> {
        Self::from_env_vars(std::env::vars())
    }

    fn from_env_vars<I, K, V>(vars: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let vars = vars
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect::<BTreeMap<_, _>>();
        Ok(Self {
            public_seeds: parse_csv_set(vars.get("FREY_PUBLIC_SEEDS")),
            public_mesh_level: parse_optional_u32(&vars, "FREY_PUBLIC_MESH_LEVEL")?,
            max_mesh_level: parse_optional_u32(&vars, "FREY_MAX_MESH_LEVEL")?,
            max_tick: parse_optional_u32(&vars, "FREY_MAX_TICK")?,
            max_lod: parse_optional_u32(&vars, "FREY_MAX_LOD")?,
            disable_precompute_requests: parse_bool_flag(
                vars.get("FREY_DISABLE_PRECOMPUTE_REQUESTS"),
            )?,
            cors_origins: parse_cors_origins(vars.get("FREY_CORS_ORIGINS"))?,
        })
    }

    fn allows_seed(&self, seed: &str) -> bool {
        self.public_seeds
            .as_ref()
            .map_or(true, |seeds| seeds.contains(seed))
    }

    fn validate_seed(&self, seed: &str) -> Result<(), String> {
        if self.allows_seed(seed) {
            return Ok(());
        }
        Err(format!("seed is not public: {seed}"))
    }

    fn validate_mesh_level(&self, mesh_level: u32) -> Result<(), String> {
        if let Some(public_mesh_level) = self.public_mesh_level {
            if mesh_level != public_mesh_level {
                return Err(format!(
                    "mesh level {mesh_level} is not public; expected {public_mesh_level}"
                ));
            }
        }
        if let Some(max_mesh_level) = self.max_mesh_level {
            if mesh_level > max_mesh_level {
                return Err(format!(
                    "mesh level {mesh_level} exceeds maximum {max_mesh_level}"
                ));
            }
        }
        Ok(())
    }

    fn validate_tick(&self, tick: u32) -> Result<(), String> {
        if let Some(max_tick) = self.max_tick {
            if tick > max_tick {
                return Err(format!("tick {tick} exceeds maximum {max_tick}"));
            }
        }
        Ok(())
    }

    fn validate_lod(&self, lod: u32) -> Result<(), String> {
        if let Some(max_lod) = self.max_lod {
            if lod > max_lod {
                return Err(format!("lod {lod} exceeds maximum {max_lod}"));
            }
        }
        Ok(())
    }

    fn capped_head_tick(&self, head_tick: u32) -> u32 {
        self.max_tick
            .map_or(head_tick, |max_tick| head_tick.min(max_tick))
    }
}

fn parse_optional_u32(vars: &BTreeMap<String, String>, name: &str) -> Result<Option<u32>, String> {
    let Some(value) = vars.get(name).map(|value| value.trim()) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<u32>()
        .map(Some)
        .map_err(|_| format!("{name} must be an unsigned integer"))
}

fn parse_bool_flag(value: Option<&String>) -> Result<bool, String> {
    let Some(value) = value.map(|value| value.trim().to_ascii_lowercase()) else {
        return Ok(false);
    };
    match value.as_str() {
        "" | "0" | "false" | "no" | "off" => Ok(false),
        "1" | "true" | "yes" | "on" => Ok(true),
        _ => Err("FREY_DISABLE_PRECOMPUTE_REQUESTS must be a boolean".to_string()),
    }
}

fn parse_csv_set(value: Option<&String>) -> Option<BTreeSet<String>> {
    let values = value?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn parse_cors_origins(value: Option<&String>) -> Result<Option<Vec<HeaderValue>>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let origins = value
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(|origin| {
            HeaderValue::from_str(origin)
                .map_err(|err| format!("invalid FREY_CORS_ORIGINS origin {origin}: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if origins.is_empty() {
        Ok(None)
    } else {
        Ok(Some(origins))
    }
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
    retention_ticks: usize,
}

#[derive(Default)]
struct PrecomputeTiming {
    init: Duration,
    initial_keyframe: Duration,
    advance: Duration,
    delta_query: Duration,
    delta_write: Duration,
    keyframe_write: Duration,
    manifest_write: Duration,
    exec_geology_terrain: Duration,
    exec_climate: Duration,
    exec_glaciology: Duration,
    exec_hydrology: Duration,
    exec_ecology: Duration,
    exec_society: Duration,
    exec_transition: Duration,
}

impl PrecomputeTiming {
    fn print(&self, ticks: u32) {
        let total = self.init
            + self.initial_keyframe
            + self.advance
            + self.delta_query
            + self.delta_write
            + self.keyframe_write
            + self.manifest_write;
        let tick_count = ticks.max(1) as f64;
        eprintln!(
            concat!(
                "precompute timing ticks={} total_ms={:.3} ",
                "init_ms={:.3} initial_keyframe_ms={:.3} ",
                "advance_ms={:.3} delta_query_ms={:.3} delta_write_ms={:.3} ",
                "keyframe_write_ms={:.3} manifest_write_ms={:.3} ",
                "advance_ms_per_tick={:.3} delta_query_ms_per_tick={:.3} delta_write_ms_per_tick={:.3}"
            ),
            ticks,
            duration_ms(total),
            duration_ms(self.init),
            duration_ms(self.initial_keyframe),
            duration_ms(self.advance),
            duration_ms(self.delta_query),
            duration_ms(self.delta_write),
            duration_ms(self.keyframe_write),
            duration_ms(self.manifest_write),
            duration_ms(self.advance) / tick_count,
            duration_ms(self.delta_query) / tick_count,
            duration_ms(self.delta_write) / tick_count,
        );
    }

    fn print_progress(&self, completed_ticks: u32, total_ticks: u32, elapsed: Duration) {
        let completed = completed_ticks.max(1) as f64;
        let elapsed_ms = duration_ms(elapsed);
        let remaining_ticks = total_ticks.saturating_sub(completed_ticks) as f64;
        let estimated_remaining_ms = elapsed_ms / completed * remaining_ticks;
        eprintln!(
            concat!(
                "precompute progress tick={}/{} elapsed_ms={:.3} eta_ms={:.3} ",
                "advance_ms={:.3} delta_query_ms={:.3} delta_write_ms={:.3} ",
                "keyframe_write_ms={:.3} geology_ms={:.3} climate_ms={:.3} ",
                "glaciology_ms={:.3} hydrology_ms={:.3} ecology_ms={:.3} ",
                "society_ms={:.3} transition_ms={:.3}"
            ),
            completed_ticks,
            total_ticks,
            elapsed_ms,
            estimated_remaining_ms,
            duration_ms(self.advance),
            duration_ms(self.delta_query),
            duration_ms(self.delta_write),
            duration_ms(self.keyframe_write),
            duration_ms(self.exec_geology_terrain),
            duration_ms(self.exec_climate),
            duration_ms(self.exec_glaciology),
            duration_ms(self.exec_hydrology),
            duration_ms(self.exec_ecology),
            duration_ms(self.exec_society),
            duration_ms(self.exec_transition),
        );
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

impl PrecomputeArgs {
    fn parse(argv: &[String]) -> Result<Self, String> {
        let mut seed = "alpha".to_string();
        let mut level = 6u32;
        let mut ticks = DEFAULT_MAX_TICK;
        let mut out_dir = PathBuf::from(DEFAULT_STORE_DIR);
        let mut keyframe_interval = DEFAULT_KEYFRAME_INTERVAL;
        let mut compression = DEFAULT_FRAME_COMPRESSION;
        let mut retention_ticks = DEFAULT_PRECOMPUTE_RETENTION_TICKS;
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
                "--retention-ticks" => {
                    retention_ticks = parse_usize_arg(argv, i, "--retention-ticks")?;
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
                    eprintln!("  --retention-ticks <n>");
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
            retention_ticks,
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

fn parse_usize_arg(argv: &[String], i: usize, name: &str) -> Result<usize, String> {
    required_arg(argv, i, name)?
        .parse::<usize>()
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
    let cors_layer = match &state.config.cors_origins {
        Some(origins) => CorsLayer::new()
            .allow_origin(origins.clone())
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers(Any),
        None => CorsLayer::permissive(),
    };
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
        .route("/api/worlds/:world_id/stream", get(stream_world_ticks))
        .route("/api/worlds/:world_id/field/:field_kind", get(get_field))
        .route(
            "/api/worlds/:world_id/checkpoints",
            get(list_checkpoint_ticks),
        )
        .route("/api/worlds/:world_id/seek", post(seek_world))
        .route("/api/worlds/:world_id/rewind", post(rewind_world))
        .route(
            "/api/worlds/:world_id/simulation-rate",
            post(set_simulation_rate),
        )
        .route("/api/worlds/:world_id/profiled", post(exec_world_profiled))
        .route("/api/exec-modules", get(get_exec_modules))
        .route("/api/exec-module-graph", get(get_exec_module_graph))
        .layer(cors_layer)
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

    fn reload_store(&mut self) -> Result<(), String> {
        let root_dir = self.store.root_dir.clone();
        self.store = PrecomputedStore::load(&root_dir)?;
        Ok(())
    }

    fn init_session(&mut self, seed: String, mesh_level: u32) -> Result<String, InitSessionError> {
        self.reload_store().map_err(InitSessionError::Store)?;
        let Some(seed_store) = self.store.seeds.get(&seed) else {
            let record = self.create_request(seed, mesh_level);
            return Err(InitSessionError::Pending(record));
        };
        if seed_store.manifest.mesh_level != mesh_level {
            let record = self.create_request(seed, mesh_level);
            return Err(InitSessionError::Pending(record));
        }
        let world_id = format!("precomputed-{:06}", self.next_world_seq);
        self.next_world_seq = self.next_world_seq.saturating_add(1);
        let mut frame = self
            .store
            .materialize(&seed, 0)
            .map_err(InitSessionError::Store)?;
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

enum InitSessionError {
    Pending(GenerationRequestRecord),
    Store(String),
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
                    eprintln!(
                        "skipping stale precomputed store {}: expected format {}, got {}; regenerate this seed",
                        manifest_path.display(),
                        STORE_FORMAT_VERSION,
                        manifest.format_version
                    );
                    continue;
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
            root_dir: root_dir.to_path_buf(),
            seeds,
        })
    }

    fn records(&self, config: &ServerConfig) -> Vec<PrecomputedWorldRecord> {
        self.seeds
            .values()
            .filter(|seed_store| config.allows_seed(&seed_store.manifest.seed))
            .map(|seed_store| PrecomputedWorldRecord {
                seed: seed_store.manifest.seed.clone(),
                world_id: String::new(),
                mesh_level: seed_store.manifest.mesh_level,
                max_tick: config.capped_head_tick(seed_store.manifest.max_tick),
                ticks: (0..=config.capped_head_tick(seed_store.manifest.max_tick)).collect(),
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

    fn mesh_level(&self, config: &ServerConfig) -> u32 {
        if let Some(public_mesh_level) = config.public_mesh_level {
            return public_mesh_level;
        }
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

    fn has_precomputed(&self, seed: &str, mesh_level: u32) -> bool {
        self.seeds
            .get(seed)
            .is_some_and(|seed_store| seed_store.manifest.mesh_level == mesh_level)
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

fn load_envelope<T>(path: &Path, compression: FrameCompression, label: &str) -> Result<T, String>
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
            .map_err(|err| {
                format!(
                    "failed to decode {label} {}: {err}; precomputed frames may be stale, regenerate this seed",
                    path.display()
                )
            })?;
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
    let mut timing = PrecomputeTiming::default();
    let overall_start = Instant::now();
    let progress_interval = std::env::var("FREY_PRECOMPUTE_PROGRESS_INTERVAL")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_PRECOMPUTE_PROGRESS_INTERVAL)
        .max(1);
    let profile_modules =
        std::env::var("FREY_PRECOMPUTE_PROFILE_MODULES").is_ok_and(|value| value == "true");
    let seed_dir = args.out_dir.join(&args.seed);
    if seed_dir.exists() {
        fs::remove_dir_all(&seed_dir).map_err(|err| {
            format!(
                "failed to remove existing seed dir {}: {err}",
                seed_dir.display()
            )
        })?;
    }
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
    let start = Instant::now();
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
                checkpoint_limit: Some(args.retention_ticks),
                undo_log_limit: Some(args.retention_ticks),
                undo_future_prune_grace_ticks: None,
                max_estimated_bytes: None,
            }),
        },
    )?;
    timing.init += start.elapsed();
    let world_id = init.world_id;
    let mut frames = Vec::new();

    let start = Instant::now();
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
    timing.initial_keyframe += start.elapsed();

    for tick in 1..=args.ticks {
        let start = Instant::now();
        if profile_modules {
            let profile = world_use_cases::exec_world_profiled(&mut service, world_id.clone(), 1)?;
            timing.exec_geology_terrain +=
                Duration::from_secs_f64(profile.exec_geology_terrain_ms / 1000.0);
            timing.exec_climate += Duration::from_secs_f64(profile.exec_climate_ms / 1000.0);
            timing.exec_glaciology += Duration::from_secs_f64(profile.exec_glaciology_ms / 1000.0);
            timing.exec_hydrology += Duration::from_secs_f64(profile.exec_hydrology_ms / 1000.0);
            timing.exec_ecology += Duration::from_secs_f64(profile.exec_ecology_ms / 1000.0);
            timing.exec_society += Duration::from_secs_f64(profile.exec_society_ms / 1000.0);
            timing.exec_transition += Duration::from_secs_f64(profile.exec_transition_ms / 1000.0);
        } else {
            world_use_cases::advance_timeline(&mut service, world_id.clone(), 1)?;
        }
        timing.advance += start.elapsed();
        let start = Instant::now();
        let delta = world_query_use_cases::get_view_delta(
            &mut service,
            world_id.clone(),
            Some(default_field_kinds().into_iter().collect()),
        )?;
        timing.delta_query += start.elapsed();
        let filename = frame_filename("deltas", tick, args.compression);
        let start = Instant::now();
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
        timing.delta_write += start.elapsed();
        frames.push(FrameManifestEntry {
            tick,
            kind: FrameKind::Delta,
            filename,
        });
        if tick % args.keyframe_interval == 0 || tick == args.ticks {
            let start = Instant::now();
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
            timing.keyframe_write += start.elapsed();
        }
        if tick % progress_interval == 0 || tick == args.ticks {
            timing.print_progress(tick, args.ticks, overall_start.elapsed());
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
    let start = Instant::now();
    save_manifest(&seed_dir.join("manifest.json"), &manifest)?;
    timing.manifest_write += start.elapsed();
    eprintln!(
        "precomputed seed={} ticks={} out={}",
        manifest.seed,
        manifest.max_tick,
        seed_dir.display()
    );
    timing.print(manifest.max_tick);
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
    use std::time::{SystemTime, UNIX_EPOCH};

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
        let bytes =
            encode_envelope(&envelope, FrameCompression::Zstd, "sample").expect("encode with zstd");
        let decoded: SampleEnvelope = decode_envelope_bytes(
            bytes,
            FrameCompression::Zstd,
            "sample",
            Path::new("sample.bin.zst"),
        )
        .expect("decode with zstd");
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn server_config_parses_public_demo_limits() {
        let config = ServerConfig::from_env_vars([
            ("FREY_PUBLIC_SEEDS", "alpha, beta"),
            ("FREY_PUBLIC_MESH_LEVEL", "6"),
            ("FREY_MAX_MESH_LEVEL", "7"),
            ("FREY_MAX_TICK", "1600"),
            ("FREY_MAX_LOD", "2"),
            ("FREY_DISABLE_PRECOMPUTE_REQUESTS", "true"),
            ("FREY_CORS_ORIGINS", "https://demo.example.com"),
        ])
        .expect("parse public demo config");

        assert!(config.allows_seed("alpha"));
        assert!(config.allows_seed("beta"));
        assert!(!config.allows_seed("gamma"));
        assert!(config.validate_mesh_level(6).is_ok());
        assert!(config.validate_mesh_level(7).is_err());
        assert!(config.validate_tick(1600).is_ok());
        assert!(config.validate_tick(1601).is_err());
        assert!(config.validate_lod(2).is_ok());
        assert!(config.validate_lod(3).is_err());
        assert!(config.disable_precompute_requests);
        assert_eq!(config.cors_origins.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn server_config_defaults_keep_development_behavior() {
        let config =
            ServerConfig::from_env_vars([] as [(&str, &str); 0]).expect("parse empty config");

        assert!(config.allows_seed("any-seed"));
        assert!(config.validate_mesh_level(64).is_ok());
        assert!(config.validate_tick(u32::MAX).is_ok());
        assert!(config.validate_lod(u32::MAX).is_ok());
        assert!(!config.disable_precompute_requests);
        assert!(config.cors_origins.is_none());
    }

    #[test]
    fn precomputed_store_skips_stale_manifest_format() {
        let root = unique_test_dir("stale-manifest");
        let seed_dir = root.join("alpha");
        fs::create_dir_all(&seed_dir).expect("create stale seed dir");
        let manifest = PrecomputedStoreManifest {
            format_version: STORE_FORMAT_VERSION.saturating_sub(1),
            seed: "alpha".to_string(),
            mesh_level: 3,
            max_tick: 0,
            keyframe_interval: 64,
            frame_compression: FrameCompression::Zstd,
            geology_fingerprint: "test".to_string(),
            field_kinds: Vec::new(),
            frames: Vec::new(),
        };
        save_manifest(&seed_dir.join("manifest.json"), &manifest).expect("write stale manifest");

        let store = PrecomputedStore::load(&root).expect("load store with stale seed");
        assert!(!store.has_precomputed("alpha", 3));
        assert!(store.seeds.is_empty());

        fs::remove_dir_all(&root).expect("remove test store");
    }

    #[test]
    fn exact_stream_uses_anchor_when_window_start_is_not_cached() {
        let plan = plan_exact_stream(57, 2, 1600, &[54, 56]);

        assert_eq!(
            plan,
            ExactStreamPlan {
                window_start: 55,
                window_end: 59,
                anchor_tick: Some(55),
                delta_start: 56,
            }
        );
    }

    #[test]
    fn exact_stream_continues_after_cached_prefix() {
        let plan = plan_exact_stream(58, 2, 1600, &[56, 57, 58, 59]);

        assert_eq!(
            plan,
            ExactStreamPlan {
                window_start: 56,
                window_end: 60,
                anchor_tick: None,
                delta_start: 60,
            }
        );
    }

    #[test]
    fn exact_stream_clamps_radius_and_head() {
        let plan = plan_exact_stream(1598, 99, 1600, &[]);

        assert_eq!(plan.window_start, 1590);
        assert_eq!(plan.window_end, 1600);
        assert_eq!(plan.anchor_tick, Some(1590));
        assert_eq!(plan.delta_start, 1591);
    }

    #[test]
    fn coarse_ticks_include_head_once() {
        assert_eq!(coarse_ticks(640, 256), vec![0, 256, 512, 640]);
        assert_eq!(coarse_ticks(512, 256), vec![0, 256, 512]);
        assert_eq!(coarse_ticks(0, 256), vec![0]);
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "frey-precompute-{label}-{}-{nanos}",
            std::process::id()
        ))
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

fn error_response(
    status: StatusCode,
    error: impl Into<String>,
) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            error: error.into(),
        }),
    )
}

fn cap_session_head_tick(
    state: &mut ServerState,
    world_id: &str,
    config: &ServerConfig,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let session = state
        .session_mut(world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let capped = config.capped_head_tick(session.frame.head_tick);
    session.frame.head_tick = capped;
    session.frame.timeline.head_tick = capped as f64;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn list_seeds(
    State(state): State<AppState>,
) -> Result<Json<SeedsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let config = state.config.clone();
    let state = lock_state(&state)?;
    let mut seeds = state.store.records(&config);
    for record in &mut seeds {
        if let Some(world_id) = state.seed_worlds.get(&record.seed) {
            record.world_id = world_id.clone();
        }
    }
    Ok(Json(SeedsResponse {
        mesh_level: state.store.mesh_level(&config),
        max_tick: config.capped_head_tick(state.store.max_tick()),
        seeds,
        pending_requests: state.requests.values().cloned().collect(),
    }))
}

async fn create_generation_request(
    State(state): State<AppState>,
    Json(request): Json<InitWorldRequest>,
) -> Result<(StatusCode, Json<GenerationRequestedResponse>), (StatusCode, Json<ErrorResponse>)> {
    state
        .config
        .validate_seed(&request.seed)
        .map_err(|err| error_response(StatusCode::FORBIDDEN, err))?;
    state
        .config
        .validate_mesh_level(request.mesh_level)
        .map_err(|err| error_response(StatusCode::FORBIDDEN, err))?;
    if state.config.disable_precompute_requests {
        return Err(error_response(
            StatusCode::FORBIDDEN,
            "precompute requests are disabled",
        ));
    }
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
    State(state): State<AppState>,
    AxumPath(level): AxumPath<u32>,
) -> Result<Json<MeshResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .config
        .validate_mesh_level(level)
        .map_err(|err| error_response(StatusCode::FORBIDDEN, err))?;
    let mesh =
        generate_mesh_core(level).map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
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
    state
        .config
        .validate_seed(&request.seed)
        .map_err(|err| error_response(StatusCode::FORBIDDEN, err))?;
    state
        .config
        .validate_mesh_level(request.mesh_level)
        .map_err(|err| error_response(StatusCode::FORBIDDEN, err))?;
    let config = state.config.clone();
    let mut state = lock_state(&state)?;
    let _ = request.config;
    if config.disable_precompute_requests
        && !state
            .store
            .has_precomputed(&request.seed, request.mesh_level)
    {
        return Err(error_response(
            StatusCode::FORBIDDEN,
            "precomputed world is not available",
        ));
    }
    match state.init_session(request.seed.clone(), request.mesh_level) {
        Ok(world_id) => {
            cap_session_head_tick(&mut state, &world_id, &config)?;
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
        Err(InitSessionError::Pending(record)) => Ok((
            StatusCode::ACCEPTED,
            Json(serde_json::json!({
                "request_id": record.request_id,
                "seed": record.seed,
                "mesh_level": record.mesh_level,
                "precompute_status": record.status,
                "message": "precompute requested"
            })),
        )),
        Err(InitSessionError::Store(err)) => {
            Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, err))
        }
    }
}

async fn advance_world(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
    Json(request): Json<AdvanceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let config = state.config.clone();
    let mut state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let head_tick = config.capped_head_tick(session.frame.head_tick);
    let target = session
        .frame
        .tick
        .saturating_add(request.tick_count)
        .min(head_tick);
    config
        .validate_tick(target)
        .map_err(|err| error_response(StatusCode::FORBIDDEN, err))?;
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
    cap_session_head_tick(&mut state, &world_id, &config)?;
    Ok(Json(serde_json::json!({
        "world_id": world_id,
        "tick": target,
        "head_tick": config.capped_head_tick(state.store.seeds.get(&seed).map(|store| store.manifest.max_tick).unwrap_or(target)),
        "advanced_ticks": target.saturating_sub(previous)
    })))
}

async fn advance_slice_and_delta(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
    Json(request): Json<SliceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let config = state.config.clone();
    let mut state = lock_state(&state)?;
    let _ = request.work_budget;
    let (seed, current, head_tick) = {
        let session = state
            .session(&world_id)
            .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
        (
            session.seed.clone(),
            session.frame.tick,
            config.capped_head_tick(session.frame.head_tick),
        )
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
        session.frame.head_tick = head_tick;
        session.frame.timeline.head_tick = head_tick as f64;
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
    let config = state.config.clone();
    let state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let mut delta = full_delta_from_frame(&session.frame);
    delta.head_tick = config.capped_head_tick(session.frame.head_tick) as f64;
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
    let config = state.config.clone();
    let state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let mut timeline = session.frame.timeline.clone();
    timeline.head_tick = config.capped_head_tick(session.frame.head_tick) as f64;
    Ok(Json(timeline))
}

async fn stream_world_ticks(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let (head_tick, coarse_interval) = {
        let locked = lock_state(&state)?;
        let session = locked
            .session(&world_id)
            .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
        (
            state.config.capped_head_tick(session.frame.head_tick),
            DEFAULT_COARSE_KEYFRAME_INTERVAL,
        )
    };
    Ok(ws.on_upgrade(move |socket| {
        handle_tick_stream(socket, state, world_id, head_tick, coarse_interval)
    }))
}

async fn handle_tick_stream(
    mut socket: WebSocket,
    state: AppState,
    world_id: String,
    head_tick: u32,
    coarse_interval: u32,
) {
    let catalog = TickStreamResponse::Catalog {
        world_id: world_id.clone(),
        head_tick,
        exact_radius_limit: MAX_STREAM_RADIUS,
        coarse_interval,
    };
    if send_tick_stream_response(&mut socket, &catalog)
        .await
        .is_err()
    {
        return;
    }

    while let Some(message) = socket.recv().await {
        let Ok(message) = message else {
            break;
        };
        let Message::Text(text) = message else {
            if matches!(message, Message::Close(_)) {
                break;
            }
            continue;
        };
        let request = match serde_json::from_str::<TickStreamRequest>(&text) {
            Ok(request) => request,
            Err(err) => {
                let response = TickStreamResponse::Error {
                    request_id: None,
                    message: format!("invalid stream request: {err}"),
                };
                if send_tick_stream_response(&mut socket, &response)
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
        };
        let prepare_state = state.clone();
        let prepare_world_id = world_id.clone();
        let responses = tokio::task::spawn_blocking(move || {
            prepare_tick_stream_responses(&prepare_state, &prepare_world_id, request)
        })
        .await
        .unwrap_or_else(|err| Err((None, format!("stream worker failed: {err}"))));
        match responses {
            Ok(responses) => {
                for response in responses {
                    if send_tick_stream_response(&mut socket, &response)
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
            Err((request_id, message)) => {
                let response = TickStreamResponse::Error {
                    request_id,
                    message,
                };
                if send_tick_stream_response(&mut socket, &response)
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }
    }
}

async fn send_tick_stream_response(
    socket: &mut WebSocket,
    response: &TickStreamResponse,
) -> Result<(), String> {
    let payload = serde_json::to_string(response)
        .map_err(|err| format!("failed to encode stream response: {err}"))?;
    socket
        .send(Message::Text(payload.into()))
        .await
        .map_err(|err| format!("failed to send stream response: {err}"))
}

fn prepare_tick_stream_responses(
    state: &AppState,
    world_id: &str,
    request: TickStreamRequest,
) -> Result<Vec<TickStreamResponse>, (Option<u64>, String)> {
    let TickStreamRequest::Subscribe {
        request_id,
        center_tick,
        radius,
        known_exact_ticks,
        known_coarse_ticks,
        coarse_interval,
        include_coarse,
    } = request;
    state
        .config
        .validate_tick(center_tick)
        .map_err(|err| (Some(request_id), err))?;
    let (store, seed, head_tick) = {
        let locked = state
            .inner
            .lock()
            .map_err(|_| (Some(request_id), "server state lock poisoned".to_string()))?;
        let session = locked
            .session(world_id)
            .map_err(|err| (Some(request_id), err))?;
        (
            locked.store.clone(),
            session.seed.clone(),
            state.config.capped_head_tick(session.frame.head_tick),
        )
    };
    if center_tick > head_tick {
        return Err((
            Some(request_id),
            format!("tick {center_tick} exceeds precomputed head {head_tick}"),
        ));
    }

    let plan = plan_exact_stream(center_tick, radius, head_tick, &known_exact_ticks);
    let mut responses = Vec::new();
    if let Some(anchor_tick) = plan.anchor_tick {
        let mut frame = store
            .materialize(&seed, anchor_tick)
            .map_err(|err| (Some(request_id), err))?;
        frame = with_world_id(frame, world_id);
        frame.head_tick = head_tick;
        frame.timeline.head_tick = head_tick as f64;
        let mut full = full_delta_from_frame(&frame);
        full.head_tick = head_tick as f64;
        responses.push(TickStreamResponse::ExactAnchor {
            request_id,
            tick: anchor_tick,
            metrics: frame.metrics,
            timeline: frame.timeline,
            frame: full,
        });
    }
    for tick in plan.delta_start..=plan.window_end {
        let mut delta = store
            .load_delta(&seed, tick)
            .map_err(|err| (Some(request_id), err))?;
        delta.world_id = world_id.to_string();
        delta.head_tick = head_tick as f64;
        responses.push(TickStreamResponse::ExactDelta {
            request_id,
            tick,
            delta,
        });
    }

    if include_coarse {
        let interval =
            coarse_interval.clamp(MIN_COARSE_KEYFRAME_INTERVAL, MAX_COARSE_KEYFRAME_INTERVAL);
        let known = known_coarse_ticks.into_iter().collect::<BTreeSet<_>>();
        for tick in coarse_ticks(head_tick, interval) {
            if known.contains(&tick) {
                continue;
            }
            let mut frame = store
                .materialize(&seed, tick)
                .map_err(|err| (Some(request_id), err))?;
            frame = with_world_id(frame, world_id);
            frame.head_tick = head_tick;
            frame.timeline.head_tick = head_tick as f64;
            let mut full = full_delta_from_frame(&frame);
            full.head_tick = head_tick as f64;
            filter_delta_fields(
                &mut full,
                Some(ViewDeltaQuery {
                    include_fields: Some(
                        COARSE_STREAM_FIELDS
                            .iter()
                            .map(|field| (*field).to_string())
                            .collect(),
                    ),
                }),
            );
            responses.push(TickStreamResponse::CoarseFrame {
                request_id,
                tick,
                metrics: frame.metrics,
                timeline: frame.timeline,
                frame: full,
            });
        }
    }
    responses.push(TickStreamResponse::Complete {
        request_id,
        center_tick,
        window_start: plan.window_start,
        window_end: plan.window_end,
    });
    Ok(responses)
}

fn plan_exact_stream(
    center_tick: u32,
    radius: u32,
    head_tick: u32,
    known_exact_ticks: &[u32],
) -> ExactStreamPlan {
    let radius = radius.min(MAX_STREAM_RADIUS);
    let window_start = center_tick.saturating_sub(radius);
    let window_end = center_tick.saturating_add(radius).min(head_tick);
    let known = known_exact_ticks.iter().copied().collect::<BTreeSet<_>>();
    if !known.contains(&window_start) {
        return ExactStreamPlan {
            window_start,
            window_end,
            anchor_tick: Some(window_start),
            delta_start: window_start.saturating_add(1),
        };
    }
    let mut contiguous_end = window_start;
    while contiguous_end < window_end && known.contains(&contiguous_end.saturating_add(1)) {
        contiguous_end = contiguous_end.saturating_add(1);
    }
    ExactStreamPlan {
        window_start,
        window_end,
        anchor_tick: None,
        delta_start: contiguous_end.saturating_add(1),
    }
}

fn coarse_ticks(head_tick: u32, interval: u32) -> Vec<u32> {
    let interval = interval.max(1);
    let mut ticks = (0..=head_tick)
        .step_by(interval as usize)
        .collect::<Vec<_>>();
    if ticks.last().copied() != Some(head_tick) {
        ticks.push(head_tick);
    }
    ticks
}

async fn get_field(
    State(state): State<AppState>,
    AxumPath((world_id, field_kind)): AxumPath<(String, String)>,
    Query(query): Query<FieldQuery>,
) -> Result<Json<FieldResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .config
        .validate_lod(query.lod)
        .map_err(|err| error_response(StatusCode::FORBIDDEN, err))?;
    let state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let field = session
        .frame
        .fields
        .get(&field_kind)
        .cloned()
        .ok_or_else(|| {
            error_response(
                StatusCode::BAD_REQUEST,
                format!("unknown field: {field_kind}"),
            )
        })?;
    Ok(Json(sample_field(field, query.lod)))
}

async fn list_checkpoint_ticks(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
) -> Result<Json<CheckpointTicksResponse>, (StatusCode, Json<ErrorResponse>)> {
    let config = state.config.clone();
    let state = lock_state(&state)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    let head_tick = config.capped_head_tick(session.frame.head_tick);
    Ok(Json(CheckpointTicksResponse {
        world_id,
        interval: 1,
        ticks: (0..=head_tick).map(|tick| tick as f64).collect(),
    }))
}

async fn seek_world(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
    Json(request): Json<SeekRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    state
        .config
        .validate_tick(request.tick)
        .map_err(|err| error_response(StatusCode::FORBIDDEN, err))?;
    let config = state.config.clone();
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
    cap_session_head_tick(&mut state, &world_id, &config)?;
    let session = state
        .session(&world_id)
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, err))?;
    Ok(Json(serde_json::json!({
        "world_id": world_id,
        "tick": session.frame.tick,
        "head_tick": config.capped_head_tick(session.frame.head_tick)
    })))
}

async fn rewind_world(
    State(state): State<AppState>,
    AxumPath(world_id): AxumPath<String>,
    Json(request): Json<AdvanceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let config = state.config.clone();
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
    cap_session_head_tick(&mut state, &world_id, &config)?;
    Ok(Json(serde_json::json!({
        "world_id": world_id,
        "tick": target,
        "head_tick": config.capped_head_tick(state.store.seeds.get(&seed).map(|store| store.manifest.max_tick).unwrap_or(target)),
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
    Json(
        serde_json::to_value(module_graph_record())
            .unwrap_or_else(|_| serde_json::json!({ "modules": [], "edges": [] })),
    )
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

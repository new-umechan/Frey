import initWasm, {
    WorldSimController,
    build_render_positions,
    generate_mesh,
} from "../interface/wasm.js";
import { createTickPerfRecorder } from "../app/perf-benchmark.js";

const STEP_BREAKDOWN_METRIC_NAMES = [
    "step_feedback",
    "step_geology_terrain",
    "step_climate",
    "step_geology_river",
    "step_ecology",
    "step_civilization",
    "step_transition",
    "step_sync_erosion",
    "step_observe_world_change",
    "step_history_snapshot",
];

const FLOAT32_FIELDS = new Set([
    "height",
    "river_flux",
    "mantle_heat",
    "temperature",
    "precipitation",
    "runoff",
    "ocean_temperature",
]);

const DELTA_FIELD_KIND_BY_VIEW = Object.freeze({
    normal: ["height", "river_flux", "river_next"],
    plates: ["height", "river_flux", "river_next"],
    mantle: ["height", "river_flux", "river_next", "mantle_heat"],
});

const CLIMATE_FIELD_KIND_BY_METRIC = Object.freeze({
    temperature: "temperature",
    precipitation: "precipitation",
});

let wasmReadyPromise = null;
let emittedWarning = false;

function postProgress(runId, payload = {}) {
    const done = Math.max(0, Math.floor(payload.done ?? 0));
    const total = Math.max(1, Math.floor(payload.total ?? 1));
    const percent = Math.max(0, Math.min(100, Math.floor(payload.percent ?? ((done / total) * 100))));
    self.postMessage({
        type: "progress",
        runId,
        done,
        total,
        percent,
        status: payload.status ?? `Running ${done}/${total} ticks... (${percent}%)`,
    });
}

function formatError(error) {
    if (error instanceof Error) {
        return `${error.name}: ${error.message}`;
    }
    return String(error);
}

function notifyWarning(runId, message) {
    if (emittedWarning) {
        return;
    }
    emittedWarning = true;
    postProgress(runId, {
        done: 0,
        total: 1,
        percent: 0,
        status: `Worker warning: ${message}`,
    });
}

function ensureWasmReady() {
    if (!wasmReadyPromise) {
        wasmReadyPromise = initWasm();
    }
    return wasmReadyPromise;
}

function getDeltaFieldKindsForProfile(profile) {
    if (profile?.viewMode === "climate") {
        const climateField = CLIMATE_FIELD_KIND_BY_METRIC[profile?.climateMetric] ?? "temperature";
        return ["height", "river_flux", "river_next", climateField];
    }
    return DELTA_FIELD_KIND_BY_VIEW[profile?.viewMode] ?? DELTA_FIELD_KIND_BY_VIEW.normal;
}

function getFieldData(controller, worldId, fieldKind) {
    const response = controller.get_field(worldId, fieldKind, 1);
    if (FLOAT32_FIELDS.has(fieldKind)) {
        return new Float32Array(response?.f32_data ?? []);
    }
    if (fieldKind === "plate_id") {
        return new Uint32Array(response?.u32_data ?? []);
    }
    return new Int32Array(response?.i32_data ?? []);
}

function buildCoreBuffers(controller, worldId) {
    return {
        heightData: getFieldData(controller, worldId, "height"),
        riverFlux: getFieldData(controller, worldId, "river_flux"),
        riverNext: getFieldData(controller, worldId, "river_next"),
        mantleHeat: getFieldData(controller, worldId, "mantle_heat"),
        temperature: getFieldData(controller, worldId, "temperature"),
        precipitation: getFieldData(controller, worldId, "precipitation"),
    };
}

function applyNumericDelta(target, fieldDelta) {
    const ranges = Array.isArray(fieldDelta?.ranges) ? fieldDelta.ranges : [];
    const values = fieldDelta?.f32_data ?? fieldDelta?.i32_data ?? [];
    const canFastCopy = typeof target?.set === "function" && ArrayBuffer.isView(values);
    if (fieldDelta?.mode === "full") {
        const copyLength = Math.min(target.length, values.length);
        if (canFastCopy) {
            target.set(values.subarray(0, copyLength), 0);
            return copyLength > 0;
        }
        for (let i = 0; i < copyLength; i += 1) {
            target[i] = values[i];
        }
        return copyLength > 0;
    }

    let offset = 0;
    for (const range of ranges) {
        const start = Math.max(0, Math.floor(range?.start ?? 0));
        const end = Math.min(target.length, Math.floor(range?.end ?? 0));
        if (end <= start) {
            continue;
        }
        const rangeLength = end - start;
        const copyLength = Math.max(0, Math.min(rangeLength, values.length - offset));
        if (canFastCopy && copyLength > 0) {
            target.set(values.subarray(offset, offset + copyLength), start);
            offset += rangeLength;
            continue;
        }
        for (let i = 0; i < copyLength; i += 1) {
            target[start + i] = values[offset + i];
        }
        offset += rangeLength;
    }
    return ranges.length > 0;
}

function applyWorldDeltaToCore(core, worldDelta) {
    const changes = {
        height: false,
        river: false,
        mantleHeat: false,
        climate: false,
    };
    for (const delta of worldDelta?.deltas ?? []) {
        switch (delta?.field_kind) {
        case "height":
            changes.height = applyNumericDelta(core.heightData, delta);
            break;
        case "river_flux":
            changes.river = applyNumericDelta(core.riverFlux, delta) || changes.river;
            break;
        case "river_next":
            changes.river = applyNumericDelta(core.riverNext, delta) || changes.river;
            break;
        case "mantle_heat":
            changes.mantleHeat = applyNumericDelta(core.mantleHeat, delta);
            break;
        case "temperature":
            changes.climate = applyNumericDelta(core.temperature, delta) || changes.climate;
            break;
        case "precipitation":
            changes.climate = applyNumericDelta(core.precipitation, delta) || changes.climate;
            break;
        default:
            break;
        }
    }
    return changes;
}

function estimateRiverMaskUpdate(riverNext, riverFlux) {
    let activeSegments = 0;
    for (let i = 0; i < riverNext.length; i += 1) {
        const next = riverNext[i];
        if (next < 0 || next >= riverNext.length) {
            continue;
        }
        if (Number.isFinite(riverFlux[i]) && riverFlux[i] > 0) {
            activeSegments += 1;
        }
    }
    return activeSegments;
}

function pushStepBreakdownSamples(recorder, profiledResult) {
    if (!profiledResult) {
        return;
    }
    const steps = Math.max(1, Math.floor(profiledResult.steps ?? 1));
    for (const metricName of STEP_BREAKDOWN_METRIC_NAMES) {
        const rawValue = profiledResult[`${metricName}_ms`];
        if (!Number.isFinite(rawValue)) {
            continue;
        }
        recorder.pushSample(metricName, rawValue / steps);
    }
}

function createControllerState(profile, level, terrainParams) {
    const controller = new WorldSimController();
    const initResult = controller.init_world(profile.seed ?? "alpha", level, {
        terrain_params: terrainParams,
    });
    const worldId = initResult?.world_id;
    if (!worldId) {
        throw new Error("worker benchmark failed: missing world id");
    }
    return {
        controller,
        worldId,
        core: buildCoreBuffers(controller, worldId),
    };
}

function rebuildControllerState(profile, level, terrainParams, completedTicks, deltaFieldKinds) {
    const state = createControllerState(profile, level, terrainParams);
    if (completedTicks > 0) {
        state.controller.step_world(state.worldId, completedTicks);
    }
    // Replay直後の未消費deltaを捨て、次tick以降のdelta計測を正常化する。
    state.controller.get_world_delta(state.worldId, {
        include_fields: deltaFieldKinds,
    });
    state.core = buildCoreBuffers(state.controller, state.worldId);
    return state;
}

async function runBenchmark(message) {
    emittedWarning = false;
    await ensureWasmReady();
    const profile = message?.profile ?? {};
    const level = Number.isFinite(message?.level) ? message.level : 3;
    const terrainParams = message?.terrainParams ?? {};
    const sampleInterval = Math.max(1, Math.floor(message?.sampleInterval ?? 4));
    const totalTicks = Math.max(1, Math.floor(profile?.tickCount ?? 32));

    const deltaFieldKinds = getDeltaFieldKindsForProfile(profile);
    let controllerState = createControllerState(profile, level, terrainParams);
    let controller = controllerState.controller;
    let worldId = controllerState.worldId;

    let basePositions = null;
    try {
        const baseMesh = generate_mesh(level);
        basePositions = new Float32Array(baseMesh?.positions ?? []);
    } catch (error) {
        notifyWarning(message.runId, `geometry mesh unavailable (${formatError(error)})`);
    }
    let core = controllerState.core;
    const recorder = createTickPerfRecorder();
    const tickStart = Math.floor(controller.get_metrics(worldId)?.tick ?? 0);
    const wallStartedAt = performance.now();

    for (let i = 0; i < totalTicks; i += 1) {
        const tickTotalStart = performance.now();
        const tickIndex = i + 1;
        const shouldSampleBreakdown = tickIndex % sampleInterval === 0 || tickIndex === totalTicks;
        postProgress(message.runId, {
            done: i,
            total: totalTicks,
            status: shouldSampleBreakdown
                ? `Running ${i}/${totalTicks} ticks... Collecting step breakdown for tick ${tickIndex}/${totalTicks}`
                : `Running ${i}/${totalTicks} ticks... stepping tick ${tickIndex}/${totalTicks}`,
        });

        const stepStart = performance.now();
        if (shouldSampleBreakdown) {
            try {
                const profiled = controller.step_world_profiled(worldId, 1);
                pushStepBreakdownSamples(recorder, profiled);
            } catch (error) {
                notifyWarning(message.runId, `step profiling trap recovered (${formatError(error)})`);
                try {
                    controllerState = rebuildControllerState(profile, level, terrainParams, i, deltaFieldKinds);
                    controller = controllerState.controller;
                    worldId = controllerState.worldId;
                    core = controllerState.core;
                    controller.step_world(worldId, 1);
                } catch (recoverError) {
                    throw new Error(
                        `step profiling recovery failed at tick ${tickIndex}: ${formatError(recoverError)} (profile error: ${formatError(error)})`,
                    );
                }
            }
        } else {
            try {
                controller.step_world(worldId, 1);
            } catch (error) {
                throw new Error(`step_world failed at tick ${tickIndex}: ${formatError(error)}`);
            }
        }
        recorder.pushSample("step_world", performance.now() - stepStart);

        const deltaStart = performance.now();
        let changes = {
            height: false,
            river: false,
            mantleHeat: false,
            climate: false,
        };
        try {
            const worldDelta = controller.get_world_delta(worldId, {
                include_fields: deltaFieldKinds,
            });
            changes = applyWorldDeltaToCore(core, worldDelta);
        } catch (error) {
            notifyWarning(message.runId, `delta sync skipped (${formatError(error)})`);
        }
        recorder.pushSample("delta_sync", performance.now() - deltaStart);

        if (changes.height && basePositions) {
            const geometryStart = performance.now();
            try {
                build_render_positions({
                    base_positions: basePositions,
                    height_data: core.heightData,
                    surface_mode: profile?.surfaceMode ?? "globe",
                });
            } catch (error) {
                notifyWarning(message.runId, `geometry update skipped (${formatError(error)})`);
            }
            recorder.pushSample("geometry_update", performance.now() - geometryStart);
        }

        if (changes.river) {
            const riverStart = performance.now();
            estimateRiverMaskUpdate(core.riverNext, core.riverFlux);
            recorder.pushSample("river_mask_update", performance.now() - riverStart);
        }

        recorder.pushSample("tick_total", performance.now() - tickTotalStart);
        const percent = Math.floor((tickIndex / totalTicks) * 100);
        const sampleLabel = shouldSampleBreakdown ? " | breakdown sample" : "";
        postProgress(message.runId, {
            done: tickIndex,
            total: totalTicks,
            percent,
            status: `Running ${tickIndex}/${totalTicks} ticks... (${percent}%)${sampleLabel}`,
        });
    }

    const tickEnd = Math.floor(controller.get_metrics(worldId)?.tick ?? tickStart);
    const wallTimeMs = performance.now() - wallStartedAt;
    const result = {
        meta: {
            generated_at: new Date().toISOString(),
            user_agent: message?.meta?.user_agent ?? self.navigator?.userAgent ?? "worker",
            timezone: message?.meta?.timezone ?? "unknown",
        },
        profile: {
            ...profile,
            tickStart,
            tickEnd,
        },
        totals: {
            wall_time_ms: Math.round(wallTimeMs * 1000) / 1000,
            processed_ticks: totalTicks,
        },
        metrics: recorder.buildSummary(),
    };
    self.postMessage({
        type: "done",
        runId: message.runId,
        result,
    });
}

self.addEventListener("message", async (event) => {
    const message = event.data ?? {};
    if (message.type !== "run") {
        return;
    }
    try {
        await runBenchmark(message);
    } catch (error) {
        self.postMessage({
            type: "error",
            runId: message.runId,
            message: error instanceof Error ? error.message : String(error),
        });
    }
});

import { createTickPerfRecorder } from "./perf-benchmark.js";

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

const RIVER_BREAKDOWN_METRIC_NAMES = [
    "step_geology_river_prepare",
    "step_geology_river_automaton",
    "step_geology_river_network",
    "step_geology_river_sync",
    "step_geology_river_fallback",
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

function defaultNowMs() {
    if (globalThis.performance && typeof globalThis.performance.now === "function") {
        return globalThis.performance.now();
    }
    return Date.now();
}

function roundMs(value) {
    if (!Number.isFinite(value)) {
        return 0;
    }
    return Math.round(value * 1000) / 1000;
}

function roundRatio(value) {
    if (!Number.isFinite(value)) {
        return 0;
    }
    return Math.round(value * 1000000) / 1000000;
}

function formatError(error) {
    if (error instanceof Error) {
        return `${error.name}: ${error.message}`;
    }
    return String(error);
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

function pushRiverBreakdownSamples(recorder, profiledResult) {
    if (!profiledResult) {
        return;
    }
    const steps = Math.max(1, Math.floor(profiledResult.steps ?? 1));
    for (const metricName of RIVER_BREAKDOWN_METRIC_NAMES) {
        const rawValue = profiledResult[`${metricName}_ms`];
        if (!Number.isFinite(rawValue)) {
            continue;
        }
        recorder.pushSample(metricName, rawValue / steps);
    }
}

function createControllerState(WorldSimController, profile, level, terrainParams) {
    const controller = new WorldSimController();
    const initResult = controller.init_world(profile.seed ?? "alpha", level, {
        terrain_params: terrainParams,
    });
    const worldId = initResult?.world_id;
    if (!worldId) {
        throw new Error("benchmark failed: missing world id");
    }
    return {
        controller,
        worldId,
        core: buildCoreBuffers(controller, worldId),
    };
}

function rebuildControllerState(
    WorldSimController,
    profile,
    level,
    terrainParams,
    completedTicks,
    deltaFieldKinds,
) {
    const state = createControllerState(WorldSimController, profile, level, terrainParams);
    if (completedTicks > 0) {
        state.controller.step_world(state.worldId, completedTicks);
    }
    state.controller.get_world_delta(state.worldId, {
        include_fields: deltaFieldKinds,
    });
    state.core = buildCoreBuffers(state.controller, state.worldId);
    return state;
}

export function createPerfBenchmarkRunner(deps = {}) {
    const {
        WorldSimController,
        build_render_positions,
        generate_mesh,
        nowMs = defaultNowMs,
    } = deps;

    if (typeof WorldSimController !== "function") {
        throw new Error("WorldSimController is required");
    }
    if (typeof build_render_positions !== "function") {
        throw new Error("build_render_positions is required");
    }
    if (typeof generate_mesh !== "function") {
        throw new Error("generate_mesh is required");
    }

    async function runBenchmark(options = {}) {
        const {
            runId = "bench",
            profile = {},
            level = 3,
            terrainParams = {},
            sampleInterval = 4,
            meta = {},
            onProgress,
            onWarning,
        } = options;

        let warningEmitted = false;
        const notifyWarning = (message) => {
            if (warningEmitted) {
                return;
            }
            warningEmitted = true;
            if (typeof onWarning === "function") {
                onWarning(message);
            }
        };

        const postProgress = (payload = {}) => {
            if (typeof onProgress !== "function") {
                return;
            }
            const done = Math.max(0, Math.floor(payload.done ?? 0));
            const total = Math.max(1, Math.floor(payload.total ?? 1));
            const percent = Math.max(0, Math.min(100, Math.floor(payload.percent ?? ((done / total) * 100))));
            onProgress({
                runId,
                done,
                total,
                percent,
                status: payload.status ?? `Running ${done}/${total} ticks... (${percent}%)`,
            });
        };

        const normalizedSampleInterval = Math.max(1, Math.floor(sampleInterval));
        const totalTicks = Math.max(1, Math.floor(profile?.tickCount ?? 32));
        const deltaFieldKinds = getDeltaFieldKindsForProfile(profile);
        let controllerState = createControllerState(WorldSimController, profile, level, terrainParams);
        let controller = controllerState.controller;
        let worldId = controllerState.worldId;

        let basePositions = null;
        try {
            const baseMesh = generate_mesh(level);
            basePositions = new Float32Array(baseMesh?.positions ?? []);
        } catch (error) {
            notifyWarning(`geometry mesh unavailable (${formatError(error)})`);
        }

        let core = controllerState.core;
        const recorder = createTickPerfRecorder();
        const tickStart = Math.floor(controller.get_metrics(worldId)?.tick ?? 0);
        const wallStartedAt = nowMs();
        const diagnostics = {
            profile_attempt_count: 0,
            profile_success_count: 0,
            profile_fallback_count: 0,
            replay_ticks_total: 0,
            replay_time_ms_total: 0,
            step_world_time_ms_total: 0,
            river_network_rebuild_count_total: 0,
            river_fallback_count_total: 0,
        };

        for (let i = 0; i < totalTicks; i += 1) {
            const tickTotalStart = nowMs();
            const tickIndex = i + 1;
            const shouldSampleBreakdown = tickIndex % normalizedSampleInterval === 0 || tickIndex === totalTicks;
            postProgress({
                done: i,
                total: totalTicks,
                status: shouldSampleBreakdown
                    ? `Running ${i}/${totalTicks} ticks... Collecting step breakdown for tick ${tickIndex}/${totalTicks}`
                    : `Running ${i}/${totalTicks} ticks... stepping tick ${tickIndex}/${totalTicks}`,
            });

            const stepStart = nowMs();
            if (shouldSampleBreakdown) {
                diagnostics.profile_attempt_count += 1;
                try {
                    const profiled = typeof controller.step_world_profiled_detail === "function"
                        ? controller.step_world_profiled_detail(worldId, 1)
                        : controller.step_world_profiled(worldId, 1);
                    pushStepBreakdownSamples(recorder, profiled);
                    pushRiverBreakdownSamples(recorder, profiled);
                    diagnostics.river_network_rebuild_count_total += Math.max(
                        0,
                        Math.floor(Number(profiled?.river_network_rebuild_count) || 0),
                    );
                    diagnostics.river_fallback_count_total += Math.max(
                        0,
                        Math.floor(Number(profiled?.river_fallback_count) || 0),
                    );
                    diagnostics.profile_success_count += 1;
                } catch (error) {
                    diagnostics.profile_fallback_count += 1;
                    diagnostics.replay_ticks_total += i;
                    notifyWarning(`step profiling trap recovered (${formatError(error)})`);
                    try {
                        const replayStart = nowMs();
                        controllerState = rebuildControllerState(
                            WorldSimController,
                            profile,
                            level,
                            terrainParams,
                            i,
                            deltaFieldKinds,
                        );
                        controller = controllerState.controller;
                        worldId = controllerState.worldId;
                        core = controllerState.core;
                        controller.step_world(worldId, 1);
                        diagnostics.replay_time_ms_total += nowMs() - replayStart;
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
            const stepElapsedMs = nowMs() - stepStart;
            diagnostics.step_world_time_ms_total += stepElapsedMs;
            recorder.pushSample("step_world", stepElapsedMs);

            const deltaStart = nowMs();
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
                notifyWarning(`delta sync skipped (${formatError(error)})`);
            }
            recorder.pushSample("delta_sync", nowMs() - deltaStart);

            if (changes.height && basePositions) {
                const geometryStart = nowMs();
                try {
                    build_render_positions({
                        base_positions: basePositions,
                        height_data: core.heightData,
                        surface_mode: profile?.surfaceMode ?? "globe",
                    });
                } catch (error) {
                    notifyWarning(`geometry update skipped (${formatError(error)})`);
                }
                recorder.pushSample("geometry_update", nowMs() - geometryStart);
            }

            if (changes.river) {
                const riverStart = nowMs();
                estimateRiverMaskUpdate(core.riverNext, core.riverFlux);
                recorder.pushSample("river_mask_update", nowMs() - riverStart);
            }

            recorder.pushSample("tick_total", nowMs() - tickTotalStart);
            const percent = Math.floor((tickIndex / totalTicks) * 100);
            const sampleLabel = shouldSampleBreakdown ? " | breakdown sample" : "";
            postProgress({
                done: tickIndex,
                total: totalTicks,
                percent,
                status: `Running ${tickIndex}/${totalTicks} ticks... (${percent}%)${sampleLabel}`,
            });
        }

        const tickEnd = Math.floor(controller.get_metrics(worldId)?.tick ?? tickStart);
        const wallTimeMs = nowMs() - wallStartedAt;
        const replayShareOfWall = wallTimeMs > 0
            ? diagnostics.replay_time_ms_total / wallTimeMs
            : 0;
        const replayShareOfStepWorld = diagnostics.step_world_time_ms_total > 0
            ? diagnostics.replay_time_ms_total / diagnostics.step_world_time_ms_total
            : 0;

        return {
            meta: {
                generated_at: new Date().toISOString(),
                user_agent: meta?.user_agent ?? "benchmark-runner",
                timezone: meta?.timezone ?? "unknown",
            },
            profile: {
                ...profile,
                tickStart,
                tickEnd,
            },
            totals: {
                wall_time_ms: roundMs(wallTimeMs),
                processed_ticks: totalTicks,
            },
            metrics: recorder.buildSummary(),
            diagnostics: {
                profile_attempt_count: diagnostics.profile_attempt_count,
                profile_success_count: diagnostics.profile_success_count,
                profile_fallback_count: diagnostics.profile_fallback_count,
                replay_ticks_total: diagnostics.replay_ticks_total,
                replay_time_ms_total: roundMs(diagnostics.replay_time_ms_total),
                step_world_time_ms_total: roundMs(diagnostics.step_world_time_ms_total),
                replay_time_share_of_wall: roundRatio(replayShareOfWall),
                replay_time_share_of_step_world: roundRatio(replayShareOfStepWorld),
                river_network_rebuild_count_total: diagnostics.river_network_rebuild_count_total,
                river_fallback_count_total: diagnostics.river_fallback_count_total,
            },
        };
    }

    return {
        runBenchmark,
    };
}

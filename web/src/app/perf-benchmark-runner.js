import { createTickPerfRecorder } from "./perf-benchmark.js";
import { createControllerState, rebuildControllerState } from "./perf-benchmark/controller-state.js";
import { buildDiagnosticsSummary, createDiagnostics, recordProfiledStepSuccess } from "./perf-benchmark/diagnostics.js";
import {
    defaultNowMs,
    formatError,
    getDeltaFieldKindsForProfile,
    pushRiverBreakdownSamples,
    pushStepBreakdownSamples,
    roundMs,
} from "./perf-benchmark/helpers.js";
import { applyWorldDeltaToCore, estimateRiverMaskUpdate } from "./perf-benchmark/world-core.js";

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
            profileEveryTick = false,
            skipGeometry = false,
            geometryUpdateMinChangedRatio = 0.0,
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
        const diagnostics = createDiagnostics();

        for (let i = 0; i < totalTicks; i += 1) {
            const tickTotalStart = nowMs();
            const tickIndex = i + 1;
            const shouldSampleBreakdown = profileEveryTick
                || tickIndex % normalizedSampleInterval === 0
                || tickIndex === totalTicks;
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
                    const profiled = typeof controller.exec_world_profiled_detail === "function"
                        ? controller.exec_world_profiled_detail(worldId, 1)
                        : controller.exec_world_profiled(worldId, 1);
                    pushStepBreakdownSamples(recorder, profiled);
                    pushRiverBreakdownSamples(recorder, profiled);
                    recordProfiledStepSuccess(diagnostics, profiled);
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
                        controller.exec_world(worldId, 1);
                        diagnostics.replay_time_ms_total += nowMs() - replayStart;
                    } catch (recoverError) {
                        throw new Error(
                            `step profiling recovery failed at tick ${tickIndex}: ${formatError(recoverError)} (profile error: ${formatError(error)})`,
                        );
                    }
                }
            } else {
                try {
                    controller.exec_world(worldId, 1);
                } catch (error) {
                    throw new Error(`exec_world failed at tick ${tickIndex}: ${formatError(error)}`);
                }
            }
            const stepElapsedMs = nowMs() - stepStart;
            diagnostics.exec_world_time_ms_total += stepElapsedMs;
            if (shouldSampleBreakdown) {
                diagnostics.exec_world_profiled_time_ms_total += stepElapsedMs;
            }
            recorder.pushSample("exec_world", stepElapsedMs);

            const deltaStart = nowMs();
            let changes = {
                height: false,
                heightChangedCount: 0,
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

            const changedRatio = core.heightData.length > 0
                ? changes.heightChangedCount / core.heightData.length
                : 0;
            const shouldRunGeometry = !skipGeometry
                && changes.height
                && basePositions
                && changedRatio >= geometryUpdateMinChangedRatio;

            if (shouldRunGeometry) {
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
            } else if (changes.height) {
                diagnostics.geometry_update_skipped_count += 1;
            }

            if (changes.river) {
                const riverStart = nowMs();
                estimateRiverMaskUpdate(core.riverNext, core.riverFlux);
                recorder.pushSample("river_mask_update", nowMs() - riverStart);
            }

            const tickTotalElapsed = nowMs() - tickTotalStart;
            diagnostics.tick_total_time_ms_total += tickTotalElapsed;
            recorder.pushSample("tick_total", tickTotalElapsed);
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
        const metrics = recorder.buildSummary();

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
            metrics,
            diagnostics: buildDiagnosticsSummary(diagnostics, totalTicks, wallTimeMs),
        };
    }

    return {
        runBenchmark,
    };
}

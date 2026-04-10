import {
    STEP_BREAKDOWN_SAMPLE_INTERVAL,
} from "./perf-step-breakdown";
import { type PerfProfile } from "./recorder";

interface PerfStatFields {
    tickP50: HTMLElement;
    tickP95: HTMLElement;
    stepMean: HTMLElement;
    deltaMean: HTMLElement;
    geomMean: HTMLElement;
    riverMean: HTMLElement;
}

interface PerfResult {
    metrics: {
        tick_total?: { p50?: number; p95?: number };
        exec_world?: { mean?: number };
        delta_sync?: { mean?: number };
        geometry_update?: { mean?: number };
        river_mask_update?: { mean?: number };
    };
}

function formatMs(value: unknown) {
    if (!Number.isFinite(value)) {
        return "-";
    }
    return `${Number(value).toFixed(3)} ms`;
}

function createPerfStatsRenderer(perfStatFields: PerfStatFields | null) {
    return function renderPerfStats(result: PerfResult | null) {
        if (!perfStatFields) {
            return;
        }
        const metrics = result?.metrics ?? {};
        perfStatFields.tickP50.textContent = formatMs(metrics.tick_total?.p50);
        perfStatFields.tickP95.textContent = formatMs(metrics.tick_total?.p95);
        perfStatFields.stepMean.textContent = formatMs(metrics.exec_world?.mean);
        perfStatFields.deltaMean.textContent = formatMs(metrics.delta_sync?.mean);
        perfStatFields.geomMean.textContent = formatMs(metrics.geometry_update?.mean);
        perfStatFields.riverMean.textContent = formatMs(metrics.river_mask_update?.mean);
    };
}

interface PerfControllerOptions {
    enabled: boolean;
    controls: {
        status: HTMLElement;
        progress: HTMLProgressElement;
        runButton: HTMLButtonElement;
        copyButton: HTMLButtonElement;
    };
    perfStatFields: PerfStatFields | null;
    workerUrl: string;
    terrainParams: Record<string, unknown>;
    level: number;
    createPerfProfile: (overrides?: Partial<PerfProfile>) => PerfProfile;
    createPerfConsoleTable: (result: any) => unknown;
    formatPerfSummaryLine: (result: any) => string;
    getRuntimeMeta: () => Record<string, unknown>;
    canRunBenchmark: () => boolean;
    setPlaybackRunning: (playing: boolean) => boolean;
    syncPlaybackUi: () => void;
}

interface WorkerMessage {
    type: "progress" | "done" | "error";
    runId: number;
    done?: number;
    total?: number;
    percent?: number;
    status?: string;
    result?: unknown;
    message?: string;
}

export function createPerfController(options: Partial<PerfControllerOptions> = {}) {
    const {
        enabled,
        controls,
        perfStatFields,
        workerUrl,
        terrainParams,
        level,
        createPerfProfile,
        createPerfConsoleTable,
        formatPerfSummaryLine,
        getRuntimeMeta,
        canRunBenchmark,
        setPlaybackRunning,
        syncPlaybackUi,
    } = options;

    if (!enabled || !controls) {
        return {
            initialize() {},
            getLastResult() {
                return null;
            },
            async copyResult() {},
            async runBenchmark() {},
        };
    }

    let lastResult: unknown = null;
    let isRunning = false;
    let worker: Worker | null = null;
    let runSeq = 0;
    const renderPerfStats = createPerfStatsRenderer(perfStatFields ?? null);

    const setStatus = (message: string) => {
        controls.status.textContent = message;
    };

    const setProgress = (value: number, max = 1) => {
        const normalizedMax = Math.max(1, Math.floor(max));
        const normalizedValue = Math.max(0, Math.min(normalizedMax, Math.floor(value)));
        controls.progress.max = normalizedMax;
        controls.progress.value = normalizedValue;
    };

    const setControlsDisabled = (isDisabled: boolean) => {
        controls.runButton.disabled = isDisabled;
        controls.copyButton.disabled = isDisabled || !lastResult;
    };

    const getWorker = () => {
        if (!worker) {
            worker = new Worker(workerUrl!, { type: "module" });
        }
        return worker;
    };

    const resetWorker = () => {
        if (!worker) {
            return;
        }
        worker.terminate();
        worker = null;
    };

    const runOnWorker = async (profile: PerfProfile) => {
        const currentWorker = getWorker();
        const runId = runSeq + 1;
        runSeq = runId;
        const tickCount = Math.max(1, Math.floor(Number(profile.tickCount ?? 1)));

        return await new Promise((resolve, reject) => {
            const handleMessage = (event: MessageEvent<WorkerMessage>) => {
                const message = event.data ?? {};
                if (message.runId !== runId) {
                    return;
                }
                if (message.type === "progress") {
                    const done = Math.max(0, Math.floor(message.done ?? 0));
                    const total = Math.max(1, Math.floor(message.total ?? tickCount));
                    const percent = Math.max(0, Math.min(100, Math.floor(message.percent ?? 0)));
                    const status = typeof message.status === "string"
                        ? message.status
                        : `Running ${done}/${total} ticks... (${percent}%)`;
                    setProgress(done, total);
                    setStatus(status);
                    return;
                }
                if (message.type === "done") {
                    cleanup();
                    resolve(message.result);
                    return;
                }
                if (message.type === "error") {
                    cleanup();
                    reject(new Error(message.message || "Worker performance run failed"));
                }
            };

            const handleError = (event: ErrorEvent) => {
                cleanup();
                reject(new Error(event?.message || "Worker crashed during performance run"));
            };

            const cleanup = () => {
                currentWorker.removeEventListener("message", handleMessage);
                currentWorker.removeEventListener("error", handleError);
            };

            currentWorker.addEventListener("message", handleMessage);
            currentWorker.addEventListener("error", handleError);
            currentWorker.postMessage({
                type: "run",
                runId,
                profile,
                level,
                terrainParams,
                sampleInterval: STEP_BREAKDOWN_SAMPLE_INTERVAL,
                meta: getRuntimeMeta?.() ?? {},
            });
        });
    };

    const copyResult = async () => {
        if (!lastResult) {
            setStatus("No result to copy.");
            return;
        }
        const payload = JSON.stringify(lastResult, null, 2);
        try {
            if (navigator.clipboard?.writeText) {
                await navigator.clipboard.writeText(payload);
                setStatus("Copied performance JSON.");
                return;
            }
        } catch (error) {
            console.warn("clipboard write failed", error);
        }
        console.log("[perf][json]", payload);
        setStatus("Clipboard unavailable. JSON logged to console.");
    };

    const runBenchmark = async () => {
        if (isRunning || !canRunBenchmark?.()) {
            return;
        }

        const profile = createPerfProfile?.();
        if (!profile) {
            setStatus("Failed to create performance profile.");
            return;
        }
        const tickCount = Math.max(1, Math.floor(Number(profile.tickCount ?? 1)));
        isRunning = true;
        setControlsDisabled(true);
        setStatus("Preparing performance profile...");
        const wasPlaying = setPlaybackRunning?.(false) ?? false;

        try {
            setStatus(`Running 0/${tickCount} ticks... (0%)`);
            setProgress(0, tickCount);
            let result;
            try {
                result = await runOnWorker(profile);
            } catch (error) {
                const errorText = String((error as Error)?.message ?? error);
                const looksLikeWasmTrap = /unreachable|wasm|worker crashed/i.test(errorText);
                if (!looksLikeWasmTrap) {
                    setStatus(`Performance run failed: ${errorText}`);
                    console.error(error);
                    return;
                }
                console.warn("[perf] worker trap detected. restarting worker and retrying once.", error);
                resetWorker();
                setStatus("Worker trapped. Restarting perf worker and retrying once...");
                try {
                    result = await runOnWorker(profile);
                } catch (retryError) {
                    setStatus(`Performance run failed: ${String((retryError as Error)?.message ?? retryError)}`);
                    console.error(retryError);
                    return;
                }
            }

            lastResult = result;
            renderPerfStats(result as PerfResult | null);
            const summaryLine = formatPerfSummaryLine?.(result) ?? "Done";
            setStatus(`Done: ${summaryLine}`);
            setProgress(tickCount, tickCount);
            console.group(`[perf] ${tickCount} tick performance run`);
            console.log("result", result);
            console.table(createPerfConsoleTable?.(result) ?? result);
            console.groupEnd();
        } finally {
            syncPlaybackUi?.();
            setPlaybackRunning?.(wasPlaying);
            isRunning = false;
            setControlsDisabled(false);
        }
    };

    const initialize = () => {
        setStatus("Idle");
        setProgress(0, 1);
        renderPerfStats(null);
        controls.copyButton.disabled = true;
    };

    return {
        initialize,
        getLastResult() {
            return lastResult;
        },
        copyResult,
        runBenchmark,
    };
}

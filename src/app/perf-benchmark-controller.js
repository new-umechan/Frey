import {
    STEP_BREAKDOWN_SAMPLE_INTERVAL,
} from "./perf-step-breakdown.js";

function formatMs(value) {
    if (!Number.isFinite(value)) {
        return "-";
    }
    return `${value.toFixed(3)} ms`;
}

function createPerfStatsRenderer(perfStatFields) {
    return function renderPerfStats(result) {
        if (!perfStatFields) {
            return;
        }
        const metrics = result?.metrics ?? {};
        perfStatFields.tickP50.textContent = formatMs(metrics.tick_total?.p50);
        perfStatFields.tickP95.textContent = formatMs(metrics.tick_total?.p95);
        perfStatFields.stepMean.textContent = formatMs(metrics.step_world?.mean);
        perfStatFields.deltaMean.textContent = formatMs(metrics.delta_sync?.mean);
        perfStatFields.geomMean.textContent = formatMs(metrics.geometry_update?.mean);
        perfStatFields.riverMean.textContent = formatMs(metrics.river_mask_update?.mean);
    };
}

export function createPerfBenchmarkController(options = {}) {
    const {
        enabled,
        controls,
        perfStatFields,
        workerUrl,
        terrainParams,
        level,
        createBenchmarkProfile,
        createBenchmarkConsoleTable,
        formatBenchmarkSummaryLine,
        getRuntimeMeta,
        canRunBenchmark,
        setPlaybackRunning,
        syncPlaybackUi,
    } = options;

    if (!enabled) {
        return {
            initialize() {},
            getLastResult() {
                return null;
            },
            async copyResult() {},
            async runBenchmark() {},
        };
    }

    let lastResult = null;
    let isRunning = false;
    let worker = null;
    let runSeq = 0;
    const renderPerfStats = createPerfStatsRenderer(perfStatFields);

    const setStatus = (message) => {
        controls.status.textContent = message;
    };

    const setProgress = (value, max = 1) => {
        const normalizedMax = Math.max(1, Math.floor(max));
        const normalizedValue = Math.max(0, Math.min(normalizedMax, Math.floor(value)));
        controls.progress.max = normalizedMax;
        controls.progress.value = normalizedValue;
    };

    const setControlsDisabled = (isDisabled) => {
        controls.runButton.disabled = isDisabled;
        controls.copyButton.disabled = isDisabled || !lastResult;
    };

    const getWorker = () => {
        if (!worker) {
            worker = new Worker(workerUrl, { type: "module" });
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

    const runOnWorker = async (profile) => {
        const currentWorker = getWorker();
        const runId = runSeq + 1;
        runSeq = runId;

        return await new Promise((resolve, reject) => {
            const handleMessage = (event) => {
                const message = event.data ?? {};
                if (message.runId !== runId) {
                    return;
                }
                if (message.type === "progress") {
                    const done = Math.max(0, Math.floor(message.done ?? 0));
                    const total = Math.max(1, Math.floor(message.total ?? profile.tickCount));
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
                    reject(new Error(message.message || "Worker benchmark failed"));
                }
            };

            const handleError = (event) => {
                cleanup();
                reject(new Error(event?.message || "Worker crashed during benchmark"));
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
                meta: getRuntimeMeta(),
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
                setStatus("Copied benchmark JSON.");
                return;
            }
        } catch (error) {
            console.warn("clipboard write failed", error);
        }
        console.log("[perf-bench][json]", payload);
        setStatus("Clipboard unavailable. JSON logged to console.");
    };

    const runBenchmark = async () => {
        if (isRunning || !canRunBenchmark()) {
            return;
        }

        const profile = createBenchmarkProfile();
        isRunning = true;
        setControlsDisabled(true);
        setStatus("Preparing benchmark profile...");
        const wasPlaying = setPlaybackRunning(false);

        try {
            setStatus(`Running 0/${profile.tickCount} ticks... (0%)`);
            setProgress(0, profile.tickCount);
            let result;
            try {
                result = await runOnWorker(profile);
            } catch (error) {
                const errorText = String(error?.message ?? error);
                const looksLikeWasmTrap = /unreachable|wasm|worker crashed/i.test(errorText);
                if (!looksLikeWasmTrap) {
                    setStatus(`Benchmark failed: ${errorText}`);
                    console.error(error);
                    return;
                }
                console.warn("[perf-bench] worker trap detected. restarting worker and retrying once.", error);
                resetWorker();
                setStatus("Worker trapped. Restarting benchmark worker and retrying once...");
                try {
                    result = await runOnWorker(profile);
                } catch (retryError) {
                    setStatus(`Benchmark failed: ${String(retryError?.message ?? retryError)}`);
                    console.error(retryError);
                    return;
                }
            }

            lastResult = result;
            renderPerfStats(result);
            const summaryLine = formatBenchmarkSummaryLine(result);
            setStatus(`Done: ${summaryLine}`);
            setProgress(profile.tickCount, profile.tickCount);
            console.group(`[perf-bench] ${profile.tickCount} tick benchmark`);
            console.log("result", result);
            console.table(createBenchmarkConsoleTable(result));
            console.groupEnd();
        } finally {
            syncPlaybackUi();
            setPlaybackRunning(wasPlaying);
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

import { createPerfBenchmarkController } from "../perf-benchmark-controller.js";

export function createPerfRuntime(options = {}) {
    const {
        isPerfEnabled,
        perfControls,
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

    const perfUiEnabled = isPerfEnabled && Boolean(perfControls);
    const perfBenchmarkController = createPerfBenchmarkController({
        enabled: perfUiEnabled,
        controls: perfControls,
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
    });

    perfBenchmarkController.initialize();
    return {
        perfUiEnabled,
        perfBenchmarkController,
    };
}

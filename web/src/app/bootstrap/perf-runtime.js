import { createPerfController } from "../perf/controller.js";

export function createPerfRuntime(options = {}) {
    const {
        isPerfEnabled,
        perfControls,
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

    const perfUiEnabled = isPerfEnabled && Boolean(perfControls);
    const perfBenchmarkController = createPerfController({
        enabled: perfUiEnabled,
        controls: perfControls,
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
    });

    perfBenchmarkController.initialize();
    return {
        perfUiEnabled,
        perfBenchmarkController,
    };
}

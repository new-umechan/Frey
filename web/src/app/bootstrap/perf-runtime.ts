import { createPerfController } from "../perf/controller";
import { type PerfControlsElements, type PerfStatFields } from "../../components/dom";
import { type PerfProfile } from "../perf/recorder";

export interface PerfRuntimeOptions {
    isPerfEnabled: boolean;
    perfControls: PerfControlsElements | null;
    perfStatFields: PerfStatFields | null;
    workerUrl: URL;
    terrainParams: Record<string, unknown>;
    level: number;
    createPerfProfile: (overrides?: Partial<PerfProfile>) => PerfProfile;
    createPerfConsoleTable: (result: any) => unknown;
    formatPerfSummaryLine: (result: any) => string;
    getRuntimeMeta: () => Record<string, unknown>;
    canRunBenchmark: () => boolean;
    setPlaybackRunning: (nextPlaying: boolean) => boolean;
    syncPlaybackUi: () => void;
}

export function createPerfRuntime(options: PerfRuntimeOptions) {
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
        controls: perfControls ?? undefined,
        perfStatFields,
        workerUrl: String(workerUrl),
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

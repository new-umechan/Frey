import { type EngineClient } from "../../engine/engine-client";
import { type FieldKind } from "./constants";
import { type TerrainRenderer } from "../../visualizers/terrain-renderer";
import { type WorldState } from "../../state/app-state";
import { type EraMetrics } from "../../state/era-presets";
import { type StatFields } from "../../../components/dom";
import { type TickPerfRecorder } from "../../perf/recorder";

export type TypedArray = Float32Array | Int32Array | Uint32Array;

export interface TectonicDebugBuffers {
    trench: TypedArray;
    arc: TypedArray;
    backarc: TypedArray;
    oceanOceanArc: TypedArray;
}

export interface CoreBuffers {
    [key: string]: TypedArray | TectonicDebugBuffers | undefined;
    heightData: TypedArray;
    lakeDepth: TypedArray;
    plateId: TypedArray;
    riverFlux: TypedArray;
    riverNext: TypedArray;
    mantleHeat: TypedArray;
    erosionRate: TypedArray;
    depositionRate: TypedArray;
    temperature: TypedArray;
    precipitation: TypedArray;
    evapotranspiration: TypedArray;
    aridity: TypedArray;
    runoff: TypedArray;
    icePressure: TypedArray;
    oceanTemperature: TypedArray;
    windU: TypedArray;
    windV: TypedArray;
    moistureFluxU: TypedArray;
    moistureFluxV: TypedArray;
    riverTransportCost: TypedArray;
    tectonicDebug?: TectonicDebugBuffers;
}

export interface SyncOptions {
    worldSimController: EngineClient;
    worldId: string;
    world: WorldState;
    currentSeed: string;
    currentSurfaceMode: string;
    terrainRenderer: TerrainRenderer;
    createEraMetrics: (era: string) => EraMetrics;
    buildEraMetricsFromRuntime: (era: string, metrics: any) => EraMetrics;
    setEraScale: (era: string) => void;
    setCurrentTerrainData: (data: CoreBuffers) => void;
    statFields: StatFields;
    level: number;
}

export interface SyncDeltaOptions {
    worldSimController: EngineClient;
    worldId: string;
    world: WorldState;
    core: CoreBuffers;
    currentSurfaceMode: string;
    terrainRenderer: TerrainRenderer;
    createEraMetrics: (era: string) => EraMetrics;
    buildEraMetricsFromRuntime: (era: string, metrics: any) => EraMetrics;
    setEraScale: (era: string) => void;
    refreshStats: boolean;
    refreshWorldStats: () => boolean;
    deltaFieldKinds: FieldKind[];
    perfRecorder?: TickPerfRecorder | null;
}

export interface SyncVisibleOptions {
    worldSimController: EngineClient;
    worldId: string;
    core: CoreBuffers;
    fieldKinds: FieldKind[];
}

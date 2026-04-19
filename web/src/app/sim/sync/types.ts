import { type EngineClient, type MetricsResult, type ViewDeltaResult } from "../../engine/engine-client";
import { type FieldKind } from "./constants";
import { type TerrainRenderer } from "../../visualizers/terrain-renderer";
import { type WorldState } from "../../state/app-state";
import { type EraMetrics } from "../../state/era-presets";
import { type StatFields } from "../../../components/dom";
import { type TickPerfRecorder } from "../../perf/recorder";
import { type WorldChangeset, type WorldDirtyCells, type WorldDeltaApplyResult } from "./constants";

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
    biome: TypedArray;
    riverTransportCost: TypedArray;
    cropAdoptionWheat: TypedArray;
    cropAdoptionRice: TypedArray;
    cropAdoptionMaize: TypedArray;
    cropAdoptionMillet: TypedArray;
    cropAdoptionPotato: TypedArray;
    cropAdoptionCassava: TypedArray;
    cropAdoptionSorghum: TypedArray;
    cropAdoptionYam: TypedArray;
    cropAvailableWheat: TypedArray;
    cropAvailableRice: TypedArray;
    cropAvailableMaize: TypedArray;
    cropAvailableMillet: TypedArray;
    cropAvailablePotato: TypedArray;
    cropAvailableCassava: TypedArray;
    cropAvailableSorghum: TypedArray;
    cropAvailableYam: TypedArray;
    livestockAdoptionCattle: TypedArray;
    livestockAdoptionHorse: TypedArray;
    livestockAdoptionSheep: TypedArray;
    livestockAdoptionPig: TypedArray;
    livestockAdoptionCamel: TypedArray;
    livestockAvailableCattle: TypedArray;
    livestockAvailableHorse: TypedArray;
    livestockAvailableSheep: TypedArray;
    livestockAvailablePig: TypedArray;
    livestockAvailableCamel: TypedArray;
    tectonicDebug?: TectonicDebugBuffers;
}

export interface SyncOptions {
    engineClient: EngineClient;
    worldId: string;
    world: WorldState;
    currentSeed: string;
    currentSurfaceMode: string;
    terrainRenderer: TerrainRenderer;
    createEraMetrics: (era: string) => EraMetrics;
    buildEraMetricsFromRuntime: (era: string, metrics: MetricsResult) => EraMetrics;
    setEraScale: (era: string) => void;
    setCurrentTerrainData: (data: CoreBuffers) => void;
    statFields: StatFields;
    level: number;
}

export interface SyncDeltaOptions {
    engineClient: EngineClient;
    worldId: string;
    world: WorldState;
    core: CoreBuffers;
    currentSurfaceMode: string;
    terrainRenderer: TerrainRenderer;
    createEraMetrics: (era: string) => EraMetrics;
    buildEraMetricsFromRuntime: (era: string, metrics: MetricsResult) => EraMetrics;
    setEraScale: (era: string) => void;
    refreshStats: boolean;
    refreshWorldStats: () => Promise<boolean>;
    deltaFieldKinds: FieldKind[];
    perfRecorder?: TickPerfRecorder | null;
}

export interface SyncVisibleOptions {
    engineClient: EngineClient;
    worldId: string;
    core: CoreBuffers;
    fieldKinds: FieldKind[];
}

export interface SyncWorldResult {
    eraMetrics: EraMetrics;
}

export interface SyncDeltaResult {
    changes: WorldChangeset;
    dirtyCells: WorldDirtyCells;
    eraMetrics: EraMetrics | null;
    statsRefreshed: boolean;
}

export type ViewDelta = ViewDeltaResult;

export type CoreDeltaApplyResult = WorldDeltaApplyResult;

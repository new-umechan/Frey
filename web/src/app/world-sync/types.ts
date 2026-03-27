import { type WorldSimController } from "../../interface/wasm.js";
import { type FieldKind, type WorldChangeset } from "./constants.js";

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
    lakeDepth?: TypedArray;
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
    oceanTemperature: TypedArray;
    windU: TypedArray;
    windV: TypedArray;
    moistureFluxU: TypedArray;
    moistureFluxV: TypedArray;
    riverTransportCost: TypedArray;
    tectonicDebug?: TectonicDebugBuffers;
}

export interface SyncOptions {
    worldSimController: WorldSimController;
    worldId: string;
    world: any;
    currentSeed: string;
    currentSurfaceMode: string;
    terrainRenderer: any;
    createEraMetrics: (era: string) => any;
    buildEraMetricsFromRuntime: (era: string, metrics: any) => any;
    setEraScale: (era: string) => void;
    setCurrentTerrainData: (data: CoreBuffers) => void;
    statFields: any;
    level: number;
}

export interface SyncDeltaOptions {
    worldSimController: WorldSimController;
    worldId: string;
    world: any;
    core: CoreBuffers;
    currentSurfaceMode: string;
    terrainRenderer: any;
    createEraMetrics: (era: string) => any;
    buildEraMetricsFromRuntime: (era: string, metrics: any) => any;
    setEraScale: (era: string) => void;
    refreshStats: boolean;
    refreshWorldStats: () => boolean;
    deltaFieldKinds: FieldKind[];
    perfRecorder?: any;
}

export interface SyncVisibleOptions {
    worldSimController: WorldSimController;
    worldId: string;
    core: CoreBuffers;
    fieldKinds: FieldKind[];
}

/**
 * Frey Project - Common Type Definitions
 * This file contains the interfaces used throughout the frontend.
 */

import type * as Wasm from "../../../generated/wasm/web/frey_wasm";

export interface AppState {
    tick: number;
    isRunning: boolean;
    config: SimulationConfig;
}

export interface SimulationConfig {
    seed: string;
    level: number;
}

// Re-export Wasm types for easier access
export { Wasm };

// Extend Global types if necessary (e.g. for window object)
declare global {
    interface Window {
        __FREY_APP__?: any;
    }
}

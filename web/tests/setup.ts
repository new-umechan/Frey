/**
 * Vitest global setup file
 * 
 * This file runs before all tests and provides:
 * - WASM module mocks for tests that don't need actual WASM execution
 * - Global test utilities
 */

import { vi } from "vitest";

// Mock WASM module imports for unit tests
// Tests that need actual WASM should import and initialize manually
vi.mock("../generated/wasm/web/frey_wasm", () => ({
    default: vi.fn().mockResolvedValue({}),
    WorldSimController: vi.fn().mockImplementation(() => ({
        world_id: vi.fn().mockReturnValue("mock-world-id"),
        getRuntimeParams: vi.fn().mockReturnValue({}),
        setRuntimeParams: vi.fn(),
        getTerrainParams: vi.fn().mockReturnValue({}),
        setTerrainParams: vi.fn(),
        getClimateParams: vi.fn().mockReturnValue({}),
        setClimateParams: vi.fn(),
        stepGeology: vi.fn(),
        stepClimate: vi.fn(),
        stepEcology: vi.fn(),
        stepCivilization: vi.fn(),
        dispose: vi.fn(),
    })),
    build_render_positions: vi.fn(),
    generate_geology: vi.fn(),
    generate_mesh: vi.fn(),
}));

// Suppress console warnings during tests (can be overridden per-test)
const originalWarn = console.warn;
console.warn = (...args) => {
    // Only show warnings that are explicitly tested
    if (process.env.DEBUG_TEST) {
        originalWarn(...args);
    }
};

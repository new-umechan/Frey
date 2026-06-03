import type { EngineClient } from "./engine-client";
import { HttpPrecomputedEngineClient } from "./http-precomputed-engine-client";

type ClosableEngineClient = EngineClient & { close?: () => void };

function readViteEnv(key: string): string {
    const env = (import.meta as unknown as { env?: Record<string, unknown> }).env;
    const value = env?.[key];
    return typeof value === "string" ? value : "";
}

export function getConfiguredEngineMode(): "http" | "wasm" {
    const explicitMode = readViteEnv("VITE_FREY_ENGINE").trim().toLowerCase();
    if (explicitMode === "http") {
        return "http";
    }
    if (readViteEnv("VITE_FREY_API_BASE").trim().length > 0) {
        return "http";
    }
    return "wasm";
}

export function getConfiguredApiBase(): string {
    return readViteEnv("VITE_FREY_API_BASE").trim() || "http://127.0.0.1:8787";
}

export async function prepareDefaultEngineRuntime(): Promise<void> {
    if (getConfiguredEngineMode() === "wasm") {
        const { initializeFreyWasm } = await import("../../transport/wasm/frey-wasm-module");
        await initializeFreyWasm();
    }
}

export async function createDefaultEngineClient(): Promise<ClosableEngineClient> {
    if (getConfiguredEngineMode() === "http") {
        return new HttpPrecomputedEngineClient(getConfiguredApiBase());
    }
    const { createEngineWorkerClient } = await import("./engine-worker-client");
    return createEngineWorkerClient();
}

export function closeEngineClient(client: ClosableEngineClient): void {
    client.close?.();
}

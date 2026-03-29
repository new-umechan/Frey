import { describe, it, expect } from "vitest";
import {
    formatRealYearsPerTick,
    ERA_SCALE_PRESETS,
    WORLD_SUBSYSTEM_KEYS,
    DEFAULT_TERRAIN_SEED,
    DEFAULT_VIEW_MODE,
    DEFAULT_SURFACE_MODE,
    DEFAULT_ERA_SCALE,
    PLATE_HOVER_POPUP_DELAY_MS,
    TERRAIN_HEIGHT_CLAMP,
} from "../../src/shared/constants";

describe("constants", () => {
    describe("default values", () => {
        it("should have correct default terrain seed", () => {
            expect(DEFAULT_TERRAIN_SEED).toBe("alpha");
        });

        it("should have correct default view mode", () => {
            expect(DEFAULT_VIEW_MODE).toBe("normal");
        });

        it("should have correct default surface mode", () => {
            expect(DEFAULT_SURFACE_MODE).toBe("globe");
        });

        it("should have correct default era scale", () => {
            expect(DEFAULT_ERA_SCALE).toBe("crust");
        });

        it("should have correct plate hover delay", () => {
            expect(PLATE_HOVER_POPUP_DELAY_MS).toBe(450);
        });

        it("should have correct terrain height clamp", () => {
            expect(TERRAIN_HEIGHT_CLAMP).toBe(1.2);
        });
    });

    describe("WORLD_SUBSYSTEM_KEYS", () => {
        it("should contain all subsystem keys", () => {
            expect(WORLD_SUBSYSTEM_KEYS).toEqual([
                "geology",
                "climate",
                "ecology",
                "civilization",
            ]);
        });

        it("should have length of 4", () => {
            expect(WORLD_SUBSYSTEM_KEYS.length).toBe(4);
        });
    });

    describe("formatRealYearsPerTick", () => {
        it("should return '-' for invalid values", () => {
            expect(formatRealYearsPerTick(NaN)).toBe("-");
            expect(formatRealYearsPerTick(Infinity)).toBe("-");
            expect(formatRealYearsPerTick(-Infinity)).toBe("-");
            expect(formatRealYearsPerTick(0)).toBe("-");
            expect(formatRealYearsPerTick(-100)).toBe("-");
        });

        it("should format years less than 10000", () => {
            expect(formatRealYearsPerTick(1)).toBe("1年");
            expect(formatRealYearsPerTick(5)).toBe("5年");
            expect(formatRealYearsPerTick(10)).toBe("10年");
            expect(formatRealYearsPerTick(9999)).toBe("9999年");
        });

        it("should format years in 万年 (10k-100M)", () => {
            expect(formatRealYearsPerTick(10000)).toBe("1万年");
            expect(formatRealYearsPerTick(50000)).toBe("5万年");
            expect(formatRealYearsPerTick(100000)).toBe("10万年");
            expect(formatRealYearsPerTick(500000)).toBe("50万年");
            expect(formatRealYearsPerTick(1000000)).toBe("100万年");
        });

        it("should format years in 億年 (100M+)", () => {
            expect(formatRealYearsPerTick(100000000)).toBe("1億年");
            expect(formatRealYearsPerTick(500000000)).toBe("5億年");
            expect(formatRealYearsPerTick(1000000000)).toBe("10億年");
            expect(formatRealYearsPerTick(5000000000)).toBe("50億年");
        });

        it("should handle decimal formatting correctly", () => {
            expect(formatRealYearsPerTick(15000)).toBe("1.5万年");
            expect(formatRealYearsPerTick(150000)).toBe("15万年");
            expect(formatRealYearsPerTick(150000000)).toBe("1.5億年");
            expect(formatRealYearsPerTick(1500000000)).toBe("15億年");
        });
    });

    describe("ERA_SCALE_PRESETS", () => {
        it("should have all era presets", () => {
            expect(ERA_SCALE_PRESETS.crust).toBeDefined();
            expect(ERA_SCALE_PRESETS.environment).toBeDefined();
            expect(ERA_SCALE_PRESETS.life).toBeDefined();
            expect(ERA_SCALE_PRESETS.civilization).toBeDefined();
            expect(ERA_SCALE_PRESETS.history).toBeDefined();
        });

        it("should have correct crust era config", () => {
            const crust = ERA_SCALE_PRESETS.crust;
            expect(crust.label).toBe("地殻形成期");
            expect(crust.tickLabel).toBe("500万年");
            expect(crust.runtimeTickMs).toBe(70);
            expect(crust.weights.geology).toBe(4.0);
            expect(crust.weights.climate).toBe(0.0);
            expect(crust.weights.ecology).toBe(0.0);
            expect(crust.weights.civilization).toBe(0.0);
        });

        it("should have correct environment era config", () => {
            const environment = ERA_SCALE_PRESETS.environment;
            expect(environment.label).toBe("環境形成期");
            expect(environment.tickLabel).toBe("100万年");
            expect(environment.runtimeTickMs).toBe(150);
            expect(environment.weights.geology).toBe(3.0);
            expect(environment.weights.climate).toBe(3.0);
            expect(environment.weights.ecology).toBe(1.0);
            expect(environment.weights.civilization).toBe(0.0);
        });

        it("should have correct life era config", () => {
            const life = ERA_SCALE_PRESETS.life;
            expect(life.label).toBe("先史期");
            expect(life.tickLabel).toBe("1000年");
            expect(life.runtimeTickMs).toBe(110);
            expect(life.weights.geology).toBe(2.0);
            expect(life.weights.climate).toBe(3.0);
            expect(life.weights.ecology).toBe(4.0);
            expect(life.weights.civilization).toBe(1.0);
        });

        it("should have correct civilization era config", () => {
            const civilization = ERA_SCALE_PRESETS.civilization;
            expect(civilization.label).toBe("文明成立期");
            expect(civilization.tickLabel).toBe("100年");
            expect(civilization.runtimeTickMs).toBe(90);
            expect(civilization.weights.geology).toBe(1.0);
            expect(civilization.weights.climate).toBe(2.0);
            expect(civilization.weights.ecology).toBe(2.0);
            expect(civilization.weights.civilization).toBe(4.0);
        });

        it("should have correct history era config", () => {
            const history = ERA_SCALE_PRESETS.history;
            expect(history.label).toBe("歴史展開期");
            expect(history.tickLabel).toBe("1年");
            expect(history.runtimeTickMs).toBe(70);
            expect(history.weights.geology).toBe(1.0);
            expect(history.weights.climate).toBe(1.0);
            expect(history.weights.ecology).toBe(1.0);
            expect(history.weights.civilization).toBe(4.0);
        });

        it("should have valid weights that sum to positive values", () => {
            Object.values(ERA_SCALE_PRESETS).forEach((preset) => {
                const totalWeight = Object.values(preset.weights).reduce(
                    (sum, weight) => sum + weight,
                    0,
                );
                expect(totalWeight).toBeGreaterThan(0);
            });
        });

        it("should have positive runtimeTickMs for all presets", () => {
            Object.values(ERA_SCALE_PRESETS).forEach((preset) => {
                expect(preset.runtimeTickMs).toBeGreaterThan(0);
            });
        });
    });
});

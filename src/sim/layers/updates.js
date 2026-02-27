import { LAYER_KIND, SUBSYSTEM_ACTIVITY_SIGNAL_GAIN } from "../../core/constants.js";
import { step_layers_bundle as wasmStepLayersBundle } from "../../interface/wasm.js";
import { recordSubsystemActivity } from "../runtime/activity.js";

export function createClimateLayer(cellCount) {
    return {
        temp: new Float32Array(cellCount),
        rain: new Float32Array(cellCount),
    };
}

export function createEcologyLayer(cellCount) {
    return {
        habitability: new Float32Array(cellCount),
        productivity: new Float32Array(cellCount),
    };
}

export function createCivilizationLayer(cellCount) {
    return {
        population: new Float32Array(cellCount),
        stateId: new Uint32Array(cellCount),
    };
}

export function getRequiredLayerKindsForEra(eraKey) {
    switch (eraKey) {
        case "history":
            return [LAYER_KIND.CLIMATE, LAYER_KIND.ECOLOGY, LAYER_KIND.CIVILIZATION];
        case "civilization":
            return [LAYER_KIND.CLIMATE, LAYER_KIND.ECOLOGY, LAYER_KIND.CIVILIZATION];
        case "life":
            return [LAYER_KIND.CLIMATE, LAYER_KIND.ECOLOGY];
        case "environment":
            return [LAYER_KIND.CLIMATE];
        case "crust":
        default:
            return [];
    }
}

export function ensureRequiredLayers(nextWorld) {
    if (!nextWorld.core?.heightData) {
        return;
    }
    const cellCount = nextWorld.core.heightData.length;
    const requiredKinds = getRequiredLayerKindsForEra(nextWorld.era);
    for (const layerKind of requiredKinds) {
        if (nextWorld.layers[layerKind]) {
            continue;
        }
        if (layerKind === LAYER_KIND.CLIMATE) {
            nextWorld.layers[layerKind] = createClimateLayer(cellCount);
            continue;
        }
        if (layerKind === LAYER_KIND.ECOLOGY) {
            nextWorld.layers[layerKind] = createEcologyLayer(cellCount);
            continue;
        }
        if (layerKind === LAYER_KIND.CIVILIZATION) {
            nextWorld.layers[layerKind] = createCivilizationLayer(cellCount);
        }
    }
}

export function stepLayersWithBudgets({
    world,
    worldState,
    currentTerrainData,
    basePositions,
    budgets,
}) {
    const climateLayer = world.layers[LAYER_KIND.CLIMATE] ?? null;
    const ecologyLayer = world.layers[LAYER_KIND.ECOLOGY] ?? null;
    const civilizationLayer = world.layers[LAYER_KIND.CIVILIZATION] ?? null;
    const heightData = currentTerrainData?.heightData;
    const riverFlux = currentTerrainData?.riverFlux;
    if (!heightData || !riverFlux) {
        return;
    }

    const cellCount = heightData.length;
    const basePositionsY = new Array(cellCount);
    for (let i = 0; i < cellCount; i += 1) {
        basePositionsY[i] = basePositions[i * 3 + 1] ?? 0;
    }

    try {
        const result = wasmStepLayersBundle({
            height_data: Array.from(heightData),
            river_flux: Array.from(riverFlux),
            base_positions_y: basePositionsY,
            climate_temp: climateLayer ? Array.from(climateLayer.temp) : null,
            climate_rain: climateLayer ? Array.from(climateLayer.rain) : null,
            ecology_habitability: ecologyLayer ? Array.from(ecologyLayer.habitability) : null,
            ecology_productivity: ecologyLayer ? Array.from(ecologyLayer.productivity) : null,
            civilization_population: civilizationLayer ? Array.from(civilizationLayer.population) : null,
            civilization_state_id: civilizationLayer ? Array.from(civilizationLayer.stateId) : null,
            climate_steps: Math.max(0, budgets?.climate ?? 0),
            ecology_steps: Math.max(0, budgets?.ecology ?? 0),
            civilization_steps: Math.max(0, budgets?.civilization ?? 0),
        });

        if (climateLayer && Array.isArray(result?.climate_temp) && Array.isArray(result?.climate_rain)) {
            climateLayer.temp.set(result.climate_temp);
            climateLayer.rain.set(result.climate_rain);
            recordSubsystemActivity(
                worldState,
                "climate",
                (Number(result.climate_delta_abs_sum) || 0) /
                    Math.max(1, cellCount * 2) * SUBSYSTEM_ACTIVITY_SIGNAL_GAIN.climate,
            );
        }

        if (
            ecologyLayer &&
            Array.isArray(result?.ecology_habitability) &&
            Array.isArray(result?.ecology_productivity)
        ) {
            ecologyLayer.habitability.set(result.ecology_habitability);
            ecologyLayer.productivity.set(result.ecology_productivity);
            recordSubsystemActivity(
                worldState,
                "ecology",
                (Number(result.ecology_delta_abs_sum) || 0) /
                    Math.max(1, cellCount * 2) * SUBSYSTEM_ACTIVITY_SIGNAL_GAIN.ecology,
            );
        }

        if (
            civilizationLayer &&
            Array.isArray(result?.civilization_population) &&
            Array.isArray(result?.civilization_state_id)
        ) {
            civilizationLayer.population.set(result.civilization_population);
            civilizationLayer.stateId.set(result.civilization_state_id);
            const populationSignal =
                (Number(result.civilization_population_delta_sum) || 0) / Math.max(1, cellCount) * 4;
            const politySignal =
                (Number(result.civilization_polity_change_count) || 0) / Math.max(1, cellCount);
            recordSubsystemActivity(
                worldState,
                "civilization",
                Math.max(populationSignal, politySignal * 6) * SUBSYSTEM_ACTIVITY_SIGNAL_GAIN.civilization,
            );
        }
    } catch (error) {
        console.warn("[layers] wasm step_layers_bundle failed", error);
    }
}

export function applyTerrainBundle({
    world,
    worldState,
    createEmptyLayers,
    terrainBundle,
}) {
    world.core = terrainBundle.core;
    world.layers = createEmptyLayers();

    worldState.erosionAutomatonState = terrainBundle.erosionAutomatonState;
    worldState.pendingRiverSteps = 0;
    worldState.terrainErosionDirty = false;
    worldState.terrainDynamics = terrainBundle.terrainDynamics;

    return world.core;
}

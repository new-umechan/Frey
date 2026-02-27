export async function runCrustGeneration({
    seed,
    token,
    getActiveToken,
    terrainParams,
    CrustTerrainAutomaton,
    setStatus,
    waitNextFrame,
}) {
    const crustTerrainAutomaton = new CrustTerrainAutomaton(seed, terrainParams);
    try {
        let stepCount = 0;
        while (!crustTerrainAutomaton.isDone()) {
            if (token !== getActiveToken()) {
                return null;
            }
            if (stepCount % 2 === 0) {
                setStatus(
                    `Generating terrain for "${seed}"... (${crustTerrainAutomaton.phaseName()})`,
                );
                await waitNextFrame();
                if (token !== getActiveToken()) {
                    return null;
                }
            }
            crustTerrainAutomaton.step(256);
            stepCount += 1;
        }
        return crustTerrainAutomaton.finish();
    } finally {
        crustTerrainAutomaton.free();
    }
}

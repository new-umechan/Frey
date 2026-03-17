import { DEBUG_SNAPSHOT_TOPK_LIMIT, LEVEL, TERRAIN_HEIGHT_CLAMP } from "../../core/constants.js";

export async function saveDebugSnapshotIfNeeded({
    isDev,
    tick,
    debugSnapshotTickSet,
    debugSnapshotSavedTicks,
    currentTerrainData,
    currentSeed,
    currentEraScale,
    world,
    worldState,
    prevHeightForSnapshot,
    setStatus,
}) {
    if (!isDev) {
        return;
    }
    if (!Number.isInteger(tick) || tick < 0) {
        return;
    }
    if (!debugSnapshotTickSet.has(tick)) {
        return;
    }
    if (debugSnapshotSavedTicks.has(tick)) {
        return;
    }
    if (!currentTerrainData) {
        return;
    }

    debugSnapshotSavedTicks.add(tick);
    const payload = buildDebugSnapshotPayload({
        tick,
        currentTerrainData,
        currentSeed,
        currentEraScale,
        world,
        worldState,
        prevHeightForSnapshot,
    });
    if (!payload) {
        return;
    }

    try {
        const response = await fetch("/__debug/snapshot", {
            method: "POST",
            headers: {
                "content-type": "application/json",
            },
            body: JSON.stringify(payload),
        });
        if (!response.ok) {
            throw new Error(`HTTP ${response.status}`);
        }
        const result = await response.json().catch(() => null);
        const fileLabel = typeof result?.file === "string" ? result.file : "debug/snapshots/latest.json";
        setStatus(`Snapshot saved at tick=${tick}: ${fileLabel}`);
    } catch (error) {
        console.warn("[debug-snapshot] failed to save", error);
        setStatus(`Snapshot save failed at tick=${tick}`);
    }
}

export function buildDebugSnapshotPayload({
    tick,
    currentTerrainData,
    currentSeed,
    currentEraScale,
    world,
    worldState,
    prevHeightForSnapshot,
}) {
    if (!currentTerrainData) {
        return null;
    }

    const heightData = currentTerrainData.heightData;
    const plateId = currentTerrainData.plateId;
    const riverFlux = currentTerrainData.riverFlux;
    if (!heightData || !plateId || !riverFlux) {
        return null;
    }

    const cellCount = Math.min(
        heightData.length,
        plateId.length,
        riverFlux.length,
    );
    if (cellCount <= 0) {
        return null;
    }

    let seaCount = 0;
    let maxHeight = -Infinity;
    let minHeight = Infinity;
    let sumHeight = 0;
    let sumRiverFlux = 0;
    let highlandCount = 0;
    let clampCount = 0;
    for (let i = 0; i < cellCount; i += 1) {
        const h = heightData[i];
        sumHeight += h;
        sumRiverFlux += riverFlux[i];
        if (h <= 0) {
            seaCount += 1;
        }
        if (h >= 0.45) {
            highlandCount += 1;
        }
        if (Math.abs(h) >= TERRAIN_HEIGHT_CLAMP - 1e-4) {
            clampCount += 1;
        }
        if (h > maxHeight) {
            maxHeight = h;
        }
        if (h < minHeight) {
            minHeight = h;
        }
    }

    const plateCount = currentTerrainData.plateInfo?.isOcean?.length ?? 0;
    const plateCellCounts = new Array(plateCount).fill(0);
    for (let i = 0; i < cellCount; i += 1) {
        const pid = plateId[i];
        if (Number.isInteger(pid) && pid >= 0 && pid < plateCount) {
            plateCellCounts[pid] += 1;
        }
    }

    const hasPrev = !!prevHeightForSnapshot && prevHeightForSnapshot.length >= cellCount;
    const topChanges = [];
    let deltaAbsSum = 0;
    let deltaAbsMax = 0;
    for (let i = 0; i < cellCount; i += 1) {
        const prev = hasPrev ? prevHeightForSnapshot[i] : heightData[i];
        const delta = heightData[i] - prev;
        const absDelta = Math.abs(delta);
        deltaAbsSum += absDelta;
        if (absDelta > deltaAbsMax) {
            deltaAbsMax = absDelta;
        }
        topChanges.push({
            i,
            p: plateId[i],
            h: Number(heightData[i].toFixed(5)),
            dh: Number(delta.toFixed(5)),
            rf: Number(riverFlux[i].toFixed(5)),
        });
    }
    topChanges.sort((a, b) => Math.abs(b.dh) - Math.abs(a.dh));
    const topKChanges = topChanges.slice(0, DEBUG_SNAPSHOT_TOPK_LIMIT);

    return {
        type: "terrain-debug-snapshot",
        version: 1,
        createdAt: new Date().toISOString(),
        tick,
        seed: currentSeed,
        era: currentEraScale,
        mesh: {
            vertexCount: cellCount,
            neighborEdgeCount: world.mesh?.nbrs?.length ?? worldState.erosionAutomatonState?.nbrs?.length ?? 0,
            level: LEVEL,
        },
        stats: {
            seaRatio: seaCount / cellCount,
            landRatio: 1 - seaCount / cellCount,
            targetLandRatio: Number.isFinite(currentTerrainData.targetLandRatio)
                ? currentTerrainData.targetLandRatio
                : null,
            highlandRatio: highlandCount / cellCount,
            minHeight,
            maxHeight,
            meanHeight: sumHeight / cellCount,
            meanRiverFlux: sumRiverFlux / cellCount,
            meanAbsHeightDelta: deltaAbsSum / cellCount,
            maxAbsHeightDelta: deltaAbsMax,
            clampRatio: clampCount / cellCount,
            plateCount,
        },
        plateStats: {
            cellCounts: plateCellCounts,
        },
        topKChanges,
    };
}

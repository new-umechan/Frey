import {
    DEFAULT_ERA_SCALE,
    PLATE_MOTION_REMAP_INTERVAL_BY_ERA,
    PLATE_MOTION_SPEED_BY_ERA,
    PLATE_REMAP_DOMINANCE_CAP,
    PLATE_REMAP_MAX_FRACTION,
    PLATE_REMAP_SWITCH_MARGIN,
} from "../../core/constants.js";
import { dotVec3, hash01, normalizeVec3 } from "../../core/math.js";

export function createPlateMotionState({ terrainData, basePositions, seed }) {
    if (!terrainData?.plateId || !terrainData?.plateInfo?.isOcean) {
        return null;
    }
    const plateId = terrainData.plateId;
    const plateCount = terrainData.plateInfo.isOcean.length;
    if (!Number.isInteger(plateCount) || plateCount <= 0) {
        return null;
    }

    const sumX = new Float64Array(plateCount);
    const sumY = new Float64Array(plateCount);
    const sumZ = new Float64Array(plateCount);
    const counts = new Uint32Array(plateCount);
    for (let i = 0; i < plateId.length; i += 1) {
        const pid = plateId[i];
        if (!Number.isInteger(pid) || pid < 0 || pid >= plateCount) {
            continue;
        }
        const base = i * 3;
        sumX[pid] += basePositions[base] ?? 0;
        sumY[pid] += basePositions[base + 1] ?? 0;
        sumZ[pid] += basePositions[base + 2] ?? 0;
        counts[pid] += 1;
    }

    const centroids = new Float32Array(plateCount * 3);
    const velocities = new Float32Array(plateCount * 3);
    for (let pid = 0; pid < plateCount; pid += 1) {
        let cx = sumX[pid];
        let cy = sumY[pid];
        let cz = sumZ[pid];
        if (counts[pid] <= 0) {
            const rx = hash01(seed, `plate-rx-${pid}`) * 2 - 1;
            const ry = hash01(seed, `plate-ry-${pid}`) * 2 - 1;
            const rz = hash01(seed, `plate-rz-${pid}`) * 2 - 1;
            [cx, cy, cz] = normalizeVec3(rx, ry, rz);
        } else {
            [cx, cy, cz] = normalizeVec3(cx, cy, cz);
        }

        centroids[pid * 3] = cx;
        centroids[pid * 3 + 1] = cy;
        centroids[pid * 3 + 2] = cz;

        let ax = hash01(seed, `axis-x-${pid}`) * 2 - 1;
        let ay = hash01(seed, `axis-y-${pid}`) * 2 - 1;
        let az = hash01(seed, `axis-z-${pid}`) * 2 - 1;
        [ax, ay, az] = normalizeVec3(ax, ay, az);
        const proj = dotVec3(ax, ay, az, cx, cy, cz);
        let tx = ax - cx * proj;
        let ty = ay - cy * proj;
        let tz = az - cz * proj;
        if (Math.hypot(tx, ty, tz) <= 1e-6) {
            const fallback = Math.abs(cy) < 0.9 ? [0, 1, 0] : [1, 0, 0];
            const projFallback = dotVec3(fallback[0], fallback[1], fallback[2], cx, cy, cz);
            tx = fallback[0] - cx * projFallback;
            ty = fallback[1] - cy * projFallback;
            tz = fallback[2] - cz * projFallback;
        }
        [tx, ty, tz] = normalizeVec3(tx, ty, tz);
        const isOcean = terrainData.plateInfo.isOcean[pid] > 0;
        const speedJitter = 0.8 + hash01(seed, `speed-${pid}`) * 0.6;
        const speedScale = speedJitter * (isOcean ? 1.15 : 0.9);
        velocities[pid * 3] = tx * speedScale;
        velocities[pid * 3 + 1] = ty * speedScale;
        velocities[pid * 3 + 2] = tz * speedScale;
    }

    return {
        centroids,
        velocities,
        remapCarry: 0,
    };
}

export function remapPlateIdsFromMotion({ plateMotionState, terrainData, basePositions }) {
    if (!plateMotionState || !terrainData?.plateId) {
        return 0;
    }
    const centroids = plateMotionState.centroids;
    const plateCount = centroids.length / 3;
    const plateId = terrainData.plateId;
    const vertexWeight = terrainData.vertexWeight;
    const cellCount = plateId.length;
    const currentCounts = new Uint32Array(plateCount);
    for (let i = 0; i < cellCount; i += 1) {
        const pid = plateId[i];
        if (Number.isInteger(pid) && pid >= 0 && pid < plateCount) {
            currentCounts[pid] += 1;
        }
    }
    const nextCounts = new Uint32Array(currentCounts);
    const maxCellsPerPlate = Math.max(1, Math.floor(cellCount * PLATE_REMAP_DOMINANCE_CAP));
    const minCellsPerPlate = Math.max(1, Math.floor(cellCount * 0.01));
    const maxChanges = Math.max(1, Math.floor(cellCount * PLATE_REMAP_MAX_FRACTION));
    const candidates = [];
    let changedCount = 0;
    for (let i = 0; i < cellCount; i += 1) {
        const base = i * 3;
        const vx = basePositions[base] ?? 0;
        const vy = basePositions[base + 1] ?? 0;
        const vz = basePositions[base + 2] ?? 1;
        const currentPid = plateId[i];
        let bestPid = 0;
        let bestDot = -Infinity;
        let secondDot = -Infinity;
        let currentDot = -Infinity;
        for (let pid = 0; pid < plateCount; pid += 1) {
            const c = pid * 3;
            const score = dotVec3(vx, vy, vz, centroids[c], centroids[c + 1], centroids[c + 2]);
            if (pid === currentPid) {
                currentDot = score;
            }
            if (score > bestDot) {
                secondDot = bestDot;
                bestDot = score;
                bestPid = pid;
                continue;
            }
            if (score > secondDot) {
                secondDot = score;
            }
        }
        const confidence = Math.min(1, Math.max(0, (bestDot - secondDot) * 8));
        if (
            currentPid !== bestPid &&
            Number.isFinite(currentDot) &&
            bestDot - currentDot > PLATE_REMAP_SWITCH_MARGIN
        ) {
            candidates.push({
                i,
                fromPid: currentPid,
                toPid: bestPid,
                gain: bestDot - currentDot,
            });
        }
        if (vertexWeight && i < vertexWeight.length) {
            vertexWeight[i] = confidence;
        }
    }

    candidates.sort((a, b) => b.gain - a.gain);
    for (let i = 0; i < candidates.length; i += 1) {
        if (changedCount >= maxChanges) {
            break;
        }
        const cand = candidates[i];
        if (cand.fromPid < 0 || cand.fromPid >= plateCount || cand.toPid < 0 || cand.toPid >= plateCount) {
            continue;
        }
        if (nextCounts[cand.fromPid] <= minCellsPerPlate) {
            continue;
        }
        if (nextCounts[cand.toPid] >= maxCellsPerPlate) {
            continue;
        }
        if (plateId[cand.i] !== cand.fromPid) {
            continue;
        }
        plateId[cand.i] = cand.toPid;
        nextCounts[cand.fromPid] -= 1;
        nextCounts[cand.toPid] += 1;
        changedCount += 1;
    }
    return changedCount;
}

export function updatePlateMotionStep({ plateMotionState, terrainData, basePositions, currentEraScale }) {
    if (!plateMotionState) {
        return 0;
    }
    const speed = PLATE_MOTION_SPEED_BY_ERA[currentEraScale] ?? PLATE_MOTION_SPEED_BY_ERA[DEFAULT_ERA_SCALE];
    const remapInterval =
        PLATE_MOTION_REMAP_INTERVAL_BY_ERA[currentEraScale] ??
        PLATE_MOTION_REMAP_INTERVAL_BY_ERA[DEFAULT_ERA_SCALE];
    if (!Number.isFinite(speed) || speed <= 0 || remapInterval <= 0) {
        return 0;
    }

    const centroids = plateMotionState.centroids;
    const velocities = plateMotionState.velocities;
    for (let pid = 0; pid < centroids.length / 3; pid += 1) {
        const c = pid * 3;
        const cx = centroids[c];
        const cy = centroids[c + 1];
        const cz = centroids[c + 2];
        const vx = velocities[c];
        const vy = velocities[c + 1];
        const vz = velocities[c + 2];
        let nx = cx + vx * speed;
        let ny = cy + vy * speed;
        let nz = cz + vz * speed;
        [nx, ny, nz] = normalizeVec3(nx, ny, nz);
        centroids[c] = nx;
        centroids[c + 1] = ny;
        centroids[c + 2] = nz;
    }

    plateMotionState.remapCarry += 1;
    if (plateMotionState.remapCarry < remapInterval) {
        return 0;
    }
    plateMotionState.remapCarry = 0;
    return remapPlateIdsFromMotion({
        plateMotionState,
        terrainData,
        basePositions,
    });
}

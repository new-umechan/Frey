import {
    LAND_RATIO_FLOOR_BY_ERA,
    LAND_RATIO_RECOVERY_BY_ERA,
    PLATE_MOTION_ACTIVITY_GAIN,
    SUBSYSTEM_ACTIVITY_SIGNAL_GAIN,
    TERRAIN_DYNAMICS_BY_ERA,
    TERRAIN_EARLY_OCEAN_GUARD_TICK,
    TERRAIN_HEIGHT_CLAMP,
    TERRAIN_OCEAN_DIFFUSION_SCALE,
    TERRAIN_OCEAN_MAX_DROP_EARLY,
    TERRAIN_OCEAN_MAX_DROP_LATE,
    TERRAIN_OCEAN_MAX_SUBSIDENCE,
    TERRAIN_STRESS_MEMORY_DECAY,
    TERRAIN_STRESS_MEMORY_GAIN,
    TERRAIN_UPLIFT_SATURATION_HARD,
    TERRAIN_UPLIFT_SATURATION_SOFT,
} from "../../core/constants.js";
import { lengthVec3 } from "../../core/math.js";
import { clamp01, recordSubsystemActivity, smoothstep } from "../runtime/activity.js";
import { createPlateMotionState, updatePlateMotionStep } from "./plate-motion.js";
import { apply_land_ratio_floor as wasmApplyLandRatioFloor } from "../../interface/wasm.js";

export function applyLandRatioFloor(heightData, plateId, plateIsOcean, targetLandRatio, currentEraScale) {
    if (!heightData || !plateId || !plateIsOcean || !Number.isFinite(targetLandRatio) || targetLandRatio <= 0) {
        return 0;
    }
    const cellCount = Math.min(heightData.length, plateId.length);
    if (cellCount <= 0) {
        return 0;
    }

    let landCount = 0;
    for (let i = 0; i < cellCount; i += 1) {
        if (heightData[i] > 0) {
            landCount += 1;
        }
    }
    const currentLandRatio = landCount / Math.max(1, cellCount);
    const floorScale = LAND_RATIO_FLOOR_BY_ERA[currentEraScale] ?? LAND_RATIO_FLOOR_BY_ERA.crust;
    const floorLandRatio = targetLandRatio * floorScale;
    const landDeficit = Math.max(0, floorLandRatio - currentLandRatio);
    if (landDeficit <= 0) {
        return 0;
    }

    const recoveryGain = LAND_RATIO_RECOVERY_BY_ERA[currentEraScale] ?? LAND_RATIO_RECOVERY_BY_ERA.crust;
    try {
        const result = wasmApplyLandRatioFloor({
            height_data: Array.from(heightData),
            plate_id: Array.from(plateId),
            plate_is_ocean: Array.from(plateIsOcean),
            target_land_ratio: targetLandRatio,
            floor_scale: floorScale,
            recovery_gain: recoveryGain,
            height_clamp: TERRAIN_HEIGHT_CLAMP,
        });
        const nextHeight = result?.height_data;
        const deltaAbs = result?.delta_abs;
        if (Array.isArray(nextHeight) && nextHeight.length === heightData.length) {
            for (let i = 0; i < heightData.length; i += 1) {
                heightData[i] = nextHeight[i];
            }
            if (Number.isFinite(deltaAbs)) {
                return deltaAbs;
            }
            return 0;
        }
    } catch (error) {
        console.warn("[terrain-core] wasm apply_land_ratio_floor failed, fallback to JS", error);
    }

    let deltaAbs = 0;
    for (let i = 0; i < cellCount; i += 1) {
        const pid = plateId[i];
        if (!Number.isInteger(pid) || pid < 0 || pid >= plateIsOcean.length || plateIsOcean[pid] > 0) {
            continue;
        }
        const h = heightData[i];
        if (h <= -0.08) {
            continue;
        }
        const coastalBoost = Math.max(0, 1 - Math.min(1, Math.abs(h) / 0.30));
        const uplift = landDeficit * recoveryGain * (0.30 + coastalBoost);
        if (uplift <= 0) {
            continue;
        }
        const raised = Math.min(TERRAIN_HEIGHT_CLAMP, h + uplift);
        const changed = raised - h;
        if (Math.abs(changed) < 1e-8) {
            continue;
        }
        heightData[i] = raised;
        deltaAbs += Math.abs(changed);
    }
    return deltaAbs;
}

export function syncTerrainHeightToErosionState(worldState, currentTerrainData) {
    const erosionState = worldState.erosionAutomatonState;
    const heightData = currentTerrainData?.heightData;
    if (!erosionState || !heightData) {
        return;
    }
    const stateHeight = erosionState.height;
    if (!stateHeight || stateHeight.length !== heightData.length) {
        return;
    }
    if (Array.isArray(stateHeight) || ArrayBuffer.isView(stateHeight)) {
        for (let i = 0; i < heightData.length; i += 1) {
            stateHeight[i] = heightData[i];
        }
        return;
    }
    erosionState.height = Array.from(heightData);
}

function ensureTerrainDynamicsState(worldState, cellCount, plateId, plateIsOcean, heightData) {
    const state = worldState.terrainDynamics;
    if (
        state &&
        state.oceanAgeNorm?.length === cellCount &&
        state.targetBuoyancy?.length === cellCount &&
        state.upliftMemory?.length === cellCount &&
        state.isOceanCell?.length === cellCount
    ) {
        return state;
    }

    const oceanAgeNorm = new Float32Array(cellCount);
    const targetBuoyancy = new Float32Array(cellCount);
    const upliftMemory = new Float32Array(cellCount);
    const isOceanCell = new Uint8Array(cellCount);

    for (let i = 0; i < cellCount; i += 1) {
        const pid = plateId[i];
        const byPlate =
            !!plateIsOcean &&
            Number.isInteger(pid) &&
            pid >= 0 &&
            pid < plateIsOcean.length &&
            plateIsOcean[pid] > 0;
        const ocean = byPlate || heightData[i] <= 0;
        if (!ocean) {
            continue;
        }
        isOceanCell[i] = 1;
        const seedAge = clamp01(Math.abs(heightData[i]) / 0.22);
        oceanAgeNorm[i] = seedAge;
        targetBuoyancy[i] = -0.03 - 0.20 * seedAge;
    }

    worldState.terrainDynamics = {
        oceanAgeNorm,
        targetBuoyancy,
        upliftMemory,
        isOceanCell,
    };
    return worldState.terrainDynamics;
}

export function runTerrainCoreStep({
    currentTerrainData,
    world,
    worldState,
    basePositions,
    currentEraScale,
    currentSeed,
    plateMotionState,
}) {
    const result = {
        plateMotionState,
        terrainDeltaDelta: 0,
        plateReassignDelta: 0,
        skipNoNeighborsDelta: 0,
        skipNoPlateMotionDelta: 0,
    };

    const heightData = currentTerrainData?.heightData;
    const plateId = currentTerrainData?.plateId;
    const plateIsOcean = currentTerrainData?.plateInfo?.isOcean;
    const riverFlux = currentTerrainData?.riverFlux;
    const targetLandRatio = currentTerrainData?.targetLandRatio;
    const erosionNbrOffsets = worldState.erosionAutomatonState?.nbr_offsets;
    const erosionNbrs = worldState.erosionAutomatonState?.nbrs;
    const nbrOffsets = world.mesh?.nbrOffsets ?? erosionNbrOffsets ?? null;
    const nbrs = world.mesh?.nbrs ?? erosionNbrs ?? null;
    if (!heightData || !plateId || !riverFlux) {
        return result;
    }
    if (!nbrOffsets || !nbrs) {
        result.skipNoNeighborsDelta += 1;
        return result;
    }
    const cellCount = heightData.length;
    if (
        cellCount <= 0 ||
        plateId.length < cellCount ||
        riverFlux.length < cellCount ||
        nbrOffsets.length !== cellCount + 1
    ) {
        return result;
    }

    const terrainDynamics = ensureTerrainDynamicsState(worldState, cellCount, plateId, plateIsOcean, heightData);

    if (!result.plateMotionState) {
        result.plateMotionState = createPlateMotionState({
            terrainData: currentTerrainData,
            basePositions,
            seed: currentSeed,
        });
        if (!result.plateMotionState) {
            result.skipNoPlateMotionDelta += 1;
        }
    }

    const movedVertices = updatePlateMotionStep({
        plateMotionState: result.plateMotionState,
        terrainData: currentTerrainData,
        basePositions,
        currentEraScale,
    });

    const dynamics = TERRAIN_DYNAMICS_BY_ERA[currentEraScale] ?? TERRAIN_DYNAMICS_BY_ERA.crust;
    const plateVelocities = result.plateMotionState?.velocities ?? null;
    const earlyOceanGuard = 1 - clamp01(world.tick / TERRAIN_EARLY_OCEAN_GUARD_TICK);
    const oceanStepDropCap =
        TERRAIN_OCEAN_MAX_DROP_EARLY +
        (1 - earlyOceanGuard) * (TERRAIN_OCEAN_MAX_DROP_LATE - TERRAIN_OCEAN_MAX_DROP_EARLY);
    const nextHeight = new Float32Array(heightData);
    let deltaAbsSum = 0;

    for (let i = 0; i < cellCount; i += 1) {
        const start = nbrOffsets[i] ?? 0;
        const end = nbrOffsets[i + 1] ?? start;
        if (end <= start) {
            continue;
        }

        const current = heightData[i];
        let nbrCount = 0;
        let nbrHeightSum = 0;
        let boundaryCount = 0;
        let shorelineEdgeCount = 0;
        let slopeAbsSum = 0;
        let convergentStrength = 0;
        let divergentStrength = 0;
        let shearStrength = 0;
        let intraplateRiftStrength = 0;
        const currentPlate = plateId[i];
        const vi = currentPlate * 3;
        const velIx =
            plateVelocities && vi >= 0 && vi + 2 < plateVelocities.length
                ? plateVelocities[vi]
                : 0;
        const velIy =
            plateVelocities && vi >= 0 && vi + 2 < plateVelocities.length
                ? plateVelocities[vi + 1]
                : 0;
        const velIz =
            plateVelocities && vi >= 0 && vi + 2 < plateVelocities.length
                ? plateVelocities[vi + 2]
                : 0;
        const base = i * 3;
        const px = basePositions[base] ?? 0;
        const py = basePositions[base + 1] ?? 0;
        const pz = basePositions[base + 2] ?? 0;

        for (let cursor = start; cursor < end; cursor += 1) {
            const n = nbrs[cursor];
            if (!Number.isInteger(n) || n < 0 || n >= cellCount) {
                continue;
            }
            nbrCount += 1;
            nbrHeightSum += heightData[n];
            slopeAbsSum += Math.abs(heightData[n] - current);
            if (plateId[n] !== currentPlate) {
                boundaryCount += 1;
            }
            const nb = n * 3;
            const nx = basePositions[nb] ?? 0;
            const ny = basePositions[nb + 1] ?? 0;
            const nz = basePositions[nb + 2] ?? 0;
            let ex = nx - px;
            let ey = ny - py;
            let ez = nz - pz;
            const elen = lengthVec3(ex, ey, ez);
            if (elen > 1e-8) {
                ex /= elen;
                ey /= elen;
                ez /= elen;
                const npid = plateId[n];
                const vn = npid * 3;
                const velNx =
                    plateVelocities && vn >= 0 && vn + 2 < plateVelocities.length
                        ? plateVelocities[vn]
                        : velIx;
                const velNy =
                    plateVelocities && vn >= 0 && vn + 2 < plateVelocities.length
                        ? plateVelocities[vn + 1]
                        : velIy;
                const velNz =
                    plateVelocities && vn >= 0 && vn + 2 < plateVelocities.length
                        ? plateVelocities[vn + 2]
                        : velIz;
                const relX = velNx - velIx;
                const relY = velNy - velIy;
                const relZ = velNz - velIz;
                const relNormal = relX * ex + relY * ey + relZ * ez;
                const relMag = lengthVec3(relX, relY, relZ);
                if (npid !== currentPlate) {
                    if (relNormal > 0) {
                        convergentStrength += relNormal;
                    } else {
                        divergentStrength += -relNormal;
                    }
                    shearStrength += Math.max(0, relMag - Math.abs(relNormal));
                } else if (relNormal < 0) {
                    intraplateRiftStrength += -relNormal;
                }
            }
            const isCurrentLand = current > 0;
            const isNeighborLand = heightData[n] > 0;
            if (isCurrentLand !== isNeighborLand) {
                shorelineEdgeCount += 1;
            }
        }
        if (nbrCount <= 0) {
            continue;
        }

        const meanNbrHeight = nbrHeightSum / nbrCount;
        const boundaryRatio = boundaryCount / nbrCount;
        const shorelineRatio = shorelineEdgeCount / nbrCount;
        const meanSlope = slopeAbsSum / nbrCount;
        const conv = convergentStrength / nbrCount;
        const div = divergentStrength / nbrCount;
        const shear = shearStrength / nbrCount;
        const intraRift = intraplateRiftStrength / nbrCount;
        const flux = Math.max(0, riverFlux[i]);
        const isOceanPlate =
            !!plateIsOcean &&
            currentPlate >= 0 &&
            currentPlate < plateIsOcean.length &&
            plateIsOcean[currentPlate] > 0;
        const isOceanCell = terrainDynamics.isOceanCell[i] > 0 || isOceanPlate || current <= 0;
        terrainDynamics.isOceanCell[i] = isOceanCell ? 1 : 0;

        const upliftCap = 1 - smoothstep(
            TERRAIN_UPLIFT_SATURATION_SOFT,
            TERRAIN_UPLIFT_SATURATION_HARD,
            Math.max(0, current),
        );
        const boundaryUplift = dynamics.uplift * (isOceanCell ? 0.40 : 1.0) * upliftCap * (0.45 * boundaryRatio + 0.55 * conv * 2.8);
        const backgroundSubsidence = dynamics.subsidence
            * (isOceanCell ? 0.16 + 0.28 * (1 - earlyOceanGuard) : 0.32)
            * (0.35 + 0.65 * div * 3.2);
        const stress = conv * 1.2 - div * 0.9 + shear * 0.35;
        terrainDynamics.upliftMemory[i] =
            terrainDynamics.upliftMemory[i] * TERRAIN_STRESS_MEMORY_DECAY +
            stress * TERRAIN_STRESS_MEMORY_GAIN;
        const memoryDelta =
            terrainDynamics.upliftMemory[i] * (isOceanCell ? 0.0022 : 0.0038) * upliftCap;

        const oceanFluvialGuard = 0.12 + 0.88 * (1 - earlyOceanGuard);
        const fluvialScale = isOceanPlate ? 0.03 * oceanFluvialGuard : 1.0;
        const estuarySuppression = 1 - shorelineRatio * 0.75;
        const fluvialErode =
            Math.log1p(flux) * dynamics.fluvial * fluvialScale * estuarySuppression * Math.max(0, current + 0.08);
        let marineBuoyancyDelta = 0;
        if (isOceanCell) {
            const prevAge = terrainDynamics.oceanAgeNorm[i];
            const ageGrow = boundaryRatio * 0.10 + (1 - boundaryRatio) * 0.022;
            const ageReset = shorelineRatio * 0.06;
            const ageNext = clamp01(prevAge + ageGrow - ageReset);
            terrainDynamics.oceanAgeNorm[i] = ageNext;
            const target = -0.03 - 0.20 * ageNext;
            terrainDynamics.targetBuoyancy[i] = target;
            const maxMarineSubsidence =
                TERRAIN_OCEAN_MAX_SUBSIDENCE * (0.12 + 0.88 * (1 - earlyOceanGuard));
            marineBuoyancyDelta =
                clamp01(Math.abs(target - current) / 0.3) *
                Math.max(
                    -maxMarineSubsidence,
                    Math.min(maxMarineSubsidence, (target - current) * 0.08),
                );
        }

        const diffusionScale = isOceanCell ? TERRAIN_OCEAN_DIFFUSION_SCALE : 1.0;
        const slopeGain = 0.45 + clamp01(meanSlope / 0.12) * 0.55;
        const diffusionDelta = (meanNbrHeight - current) * dynamics.diffusion * diffusionScale * slopeGain;
        const coastalBand = Math.max(0, 1 - Math.min(1, Math.abs(current) / 0.14));
        const coastlineDelta =
            (meanNbrHeight - current) * dynamics.coastline * shorelineRatio * coastalBand;
        const continentalRiftDelta =
            !isOceanCell
                ? -Math.min(0.0038, (intraRift * 0.8 + div * 0.6) * dynamics.subsidence * (1 - boundaryRatio))
                : 0;
        const highlandRelax = Math.max(0, current - TERRAIN_UPLIFT_SATURATION_SOFT);
        const isostaticDelta = -highlandRelax * highlandRelax * (isOceanCell ? 0.002 : 0.0035);
        const tectonicDelta = boundaryUplift - backgroundSubsidence + memoryDelta;
        const delta =
            diffusionDelta +
            tectonicDelta +
            coastlineDelta +
            continentalRiftDelta +
            marineBuoyancyDelta +
            isostaticDelta -
            fluvialErode;
        let next = Math.min(TERRAIN_HEIGHT_CLAMP, Math.max(-TERRAIN_HEIGHT_CLAMP, current + delta));
        if (isOceanCell) {
            const minNext = current - oceanStepDropCap;
            if (next < minNext) {
                next = minNext;
            }
        }
        const changed = next - current;
        if (Math.abs(changed) < 1e-8) {
            continue;
        }
        nextHeight[i] = next;
        deltaAbsSum += Math.abs(changed);
    }

    deltaAbsSum += applyLandRatioFloor(nextHeight, plateId, plateIsOcean, targetLandRatio, currentEraScale);

    if (deltaAbsSum <= 0) {
        if (movedVertices > 0) {
            result.plateReassignDelta += movedVertices;
            worldState.terrainCoreDirty = true;
            recordSubsystemActivity(
                worldState,
                "terrain",
                Math.min(1, movedVertices / Math.max(1, cellCount) * PLATE_MOTION_ACTIVITY_GAIN),
            );
        }
        return result;
    }

    currentTerrainData.heightData = nextHeight;
    syncTerrainHeightToErosionState(worldState, currentTerrainData);
    result.terrainDeltaDelta += deltaAbsSum;
    result.plateReassignDelta += movedVertices;
    worldState.terrainCoreDirty = true;
    const deformationSignal =
        deltaAbsSum / Math.max(1, cellCount) * SUBSYSTEM_ACTIVITY_SIGNAL_GAIN.terrain;
    const plateMotionSignal =
        movedVertices > 0
            ? Math.min(1, movedVertices / Math.max(1, cellCount) * PLATE_MOTION_ACTIVITY_GAIN)
            : 0;
    recordSubsystemActivity(worldState, "terrain", Math.max(deformationSignal, plateMotionSignal));

    return result;
}

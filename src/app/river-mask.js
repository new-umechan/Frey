import * as THREE from "three";

const RIVER_MASK_WIDTH = 2048;
const RIVER_MASK_HEIGHT = 1024;
const RIVER_DRAW_WIDTH_PX = 2.0;
const RIVER_MIN_FLUX = 0.10;

function lonLatToMapUv(x, y, z) {
    const invLen = 1 / Math.max(1e-6, Math.hypot(x, y, z));
    const nx = x * invLen;
    const ny = y * invLen;
    const nz = z * invLen;
    const u = Math.atan2(nz, nx) / (Math.PI * 2) + 0.5;
    const v = 0.5 - Math.asin(THREE.MathUtils.clamp(ny, -1, 1)) / Math.PI;
    return { u, v };
}

function wrapAwarePair(u0, u1) {
    let a = u0;
    let b = u1;
    if (Math.abs(b - a) <= 0.5) {
        return [a, b];
    }
    if (a < b) {
        a += 1;
    } else {
        b += 1;
    }
    return [a, b];
}

function drawWrappedSegment(ctx, x0, y0, x1, y1, width, canvasWidth) {
    ctx.lineWidth = width;
    ctx.beginPath();
    ctx.moveTo(x0, y0);
    ctx.lineTo(x1, y1);
    ctx.stroke();

    if (x0 > canvasWidth || x1 > canvasWidth) {
        ctx.beginPath();
        ctx.moveTo(x0 - canvasWidth, y0);
        ctx.lineTo(x1 - canvasWidth, y1);
        ctx.stroke();
    }
    if (x0 < 0 || x1 < 0) {
        ctx.beginPath();
        ctx.moveTo(x0 + canvasWidth, y0);
        ctx.lineTo(x1 + canvasWidth, y1);
        ctx.stroke();
    }
}

function createEmptyRiverMaskTexture() {
    const data = new Uint8Array([0, 0, 0, 255]);
    const texture = new THREE.DataTexture(data, 1, 1, THREE.RGBAFormat);
    texture.wrapS = THREE.RepeatWrapping;
    texture.wrapT = THREE.ClampToEdgeWrapping;
    texture.magFilter = THREE.LinearFilter;
    texture.minFilter = THREE.LinearFilter;
    texture.needsUpdate = true;
    return texture;
}

export function buildTerrainUvFromPositions(basePositions) {
    const vertexCount = basePositions.length / 3;
    const uv = new Float32Array(vertexCount * 2);
    for (let i = 0; i < vertexCount; i += 1) {
        const p = i * 3;
        const t = i * 2;
        const mapped = lonLatToMapUv(basePositions[p], basePositions[p + 1], basePositions[p + 2]);
        uv[t] = mapped.u;
        uv[t + 1] = mapped.v;
    }
    return uv;
}

export function buildRiverMaskTexture(basePositions, riverNext, riverFlux) {
    if (!basePositions?.length || !riverNext?.length || !riverFlux?.length) {
        return createEmptyRiverMaskTexture();
    }

    const canvas = document.createElement("canvas");
    canvas.width = RIVER_MASK_WIDTH;
    canvas.height = RIVER_MASK_HEIGHT;
    const ctx = canvas.getContext("2d");
    if (!ctx) {
        return createEmptyRiverMaskTexture();
    }

    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.imageSmoothingEnabled = false;
    ctx.strokeStyle = "#fff";
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    for (let i = 0; i < riverNext.length; i += 1) {
        const next = riverNext[i];
        if (next < 0 || next >= riverNext.length) {
            continue;
        }
        if (!Number.isFinite(riverFlux[i]) || riverFlux[i] < RIVER_MIN_FLUX) {
            continue;
        }

        const a = i * 3;
        const b = next * 3;
        const uv0 = lonLatToMapUv(basePositions[a], basePositions[a + 1], basePositions[a + 2]);
        const uv1 = lonLatToMapUv(basePositions[b], basePositions[b + 1], basePositions[b + 2]);
        const [u0, u1] = wrapAwarePair(uv0.u, uv1.u);

        drawWrappedSegment(
            ctx,
            u0 * canvas.width,
            uv0.v * canvas.height,
            u1 * canvas.width,
            uv1.v * canvas.height,
            RIVER_DRAW_WIDTH_PX,
            canvas.width,
        );
    }

    const texture = new THREE.CanvasTexture(canvas);
    texture.flipY = false;
    texture.wrapS = THREE.RepeatWrapping;
    texture.wrapT = THREE.ClampToEdgeWrapping;
    texture.magFilter = THREE.LinearFilter;
    texture.minFilter = THREE.LinearFilter;
    texture.needsUpdate = true;
    return texture;
}

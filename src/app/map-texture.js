import * as THREE from "three";

const MAP_TEX_WIDTH = 1024;
const MAP_TEX_HEIGHT = 512;
const MAP_BG = "#dfe5ec";

function lonLatToMapUv(x, y, z) {
    const invLen = 1 / Math.max(1e-6, Math.hypot(x, y, z));
    const nx = x * invLen;
    const ny = y * invLen;
    const nz = z * invLen;
    const u = Math.atan2(nz, nx) / (Math.PI * 2) + 0.5;
    const v = 0.5 - Math.asin(THREE.MathUtils.clamp(ny, -1, 1)) / Math.PI;
    return { u, v };
}

function wrapAwareU(a, b, c) {
    const values = [a, b, c];
    const min = Math.min(...values);
    const max = Math.max(...values);
    if (max - min <= 0.5) {
        return values;
    }
    return values.map((u) => (u < 0.5 ? u + 1 : u));
}

function drawTriangle(ctx, ax, ay, bx, by, cx, cy, fillStyle) {
    ctx.beginPath();
    ctx.moveTo(ax, ay);
    ctx.lineTo(bx, by);
    ctx.lineTo(cx, cy);
    ctx.closePath();
    ctx.fillStyle = fillStyle;
    ctx.fill();
    ctx.strokeStyle = fillStyle;
    ctx.lineJoin = "round";
    ctx.lineWidth = 1.25;
    ctx.stroke();
}

function triangleColor(vertexColors, ia, ib, ic) {
    const a3 = ia * 3;
    const b3 = ib * 3;
    const c3 = ic * 3;
    const r = Math.round(((vertexColors[a3] + vertexColors[b3] + vertexColors[c3]) / 3) * 255);
    const g = Math.round(
        ((vertexColors[a3 + 1] + vertexColors[b3 + 1] + vertexColors[c3 + 1]) / 3) * 255,
    );
    const b = Math.round(
        ((vertexColors[a3 + 2] + vertexColors[b3 + 2] + vertexColors[c3 + 2]) / 3) * 255,
    );
    return `rgb(${r}, ${g}, ${b})`;
}

function vertexMapUv(basePositions, index) {
    const i = index * 3;
    return lonLatToMapUv(basePositions[i], basePositions[i + 1], basePositions[i + 2]);
}

export function buildEquirectangularMapTexture(basePositions, indices, vertexColors) {
    const canvas = document.createElement("canvas");
    canvas.width = MAP_TEX_WIDTH;
    canvas.height = MAP_TEX_HEIGHT;

    const ctx = canvas.getContext("2d");
    if (!ctx) {
        return null;
    }

    ctx.fillStyle = MAP_BG;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.imageSmoothingEnabled = false;

    for (let i = 0; i < indices.length; i += 3) {
        const ia = indices[i];
        const ib = indices[i + 1];
        const ic = indices[i + 2];
        const aUv = vertexMapUv(basePositions, ia);
        const bUv = vertexMapUv(basePositions, ib);
        const cUv = vertexMapUv(basePositions, ic);
        const [ua, ub, uc] = wrapAwareU(aUv.u, bUv.u, cUv.u);
        const fillStyle = triangleColor(vertexColors, ia, ib, ic);

        const ax = ua * canvas.width;
        const bx = ub * canvas.width;
        const cx = uc * canvas.width;
        const ay = aUv.v * canvas.height;
        const by = bUv.v * canvas.height;
        const cy = cUv.v * canvas.height;

        drawTriangle(ctx, ax, ay, bx, by, cx, cy, fillStyle);
        if (ua > 1 || ub > 1 || uc > 1) {
            drawTriangle(ctx, ax - canvas.width, ay, bx - canvas.width, by, cx - canvas.width, cy, fillStyle);
        }
        if (ua < 0 || ub < 0 || uc < 0) {
            drawTriangle(ctx, ax + canvas.width, ay, bx + canvas.width, by, cx + canvas.width, cy, fillStyle);
        }
    }

    const texture = new THREE.CanvasTexture(canvas);
    texture.flipY = false;
    texture.wrapS = THREE.RepeatWrapping;
    texture.wrapT = THREE.ClampToEdgeWrapping;
    texture.needsUpdate = true;
    return texture;
}

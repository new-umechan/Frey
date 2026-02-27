export function fnv1a32(text) {
    let hash = 0x811c9dc5;
    for (let i = 0; i < text.length; i += 1) {
        hash ^= text.charCodeAt(i);
        hash = Math.imul(hash, 0x01000193);
    }
    return hash >>> 0;
}

export function hash01(seed, salt) {
    const h = fnv1a32(`${seed}:${salt}`);
    return (h & 0x00ffffff) / 0x01000000;
}

export function normalizeVec3(x, y, z) {
    const len = Math.hypot(x, y, z);
    if (!Number.isFinite(len) || len <= 1e-8) {
        return [0, 0, 1];
    }
    return [x / len, y / len, z / len];
}

export function dotVec3(ax, ay, az, bx, by, bz) {
    return ax * bx + ay * by + az * bz;
}

export function lengthVec3(x, y, z) {
    return Math.hypot(x, y, z);
}

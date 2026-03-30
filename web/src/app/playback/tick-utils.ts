export function sanitizeTick(rawTick: unknown): number | null {
    const tick = Math.floor(Number(rawTick));
    return Number.isFinite(tick) && tick >= 0 ? tick : null;
}

export function normalizeTicks(rawTicks: unknown[]): number[] {
    return Array.from(
        new Set(
            rawTicks
                .map((value) => sanitizeTick(value))
                .filter((value): value is number => value !== null),
        ),
    ).sort((a, b) => a - b);
}

export function resolveStepTick(ticks: number[], baseTick: unknown, direction: number, interval: unknown): number | null {
    if (!Array.isArray(ticks) || ticks.length === 0) {
        return null;
    }

    const normalizedBaseTick = sanitizeTick(baseTick);
    if (normalizedBaseTick === null) {
        return null;
    }

    const stepInterval = sanitizeTick(interval);
    const step = stepInterval !== null && stepInterval > 0 ? stepInterval : 1;
    const desiredTick = normalizedBaseTick + (direction * step);

    if (direction < 0) {
        if (desiredTick < ticks[0]) {
            return null;
        }
        for (let i = ticks.length - 1; i >= 0; i -= 1) {
            if (ticks[i] <= desiredTick) {
                return ticks[i];
            }
        }
        return null;
    }

    if (desiredTick > ticks[ticks.length - 1]) {
        return null;
    }
    for (let i = 0; i < ticks.length; i += 1) {
        if (ticks[i] >= desiredTick) {
            return ticks[i];
        }
    }
    return null;
}

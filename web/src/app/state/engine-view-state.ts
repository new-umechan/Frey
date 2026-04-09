export interface EngineViewState {
    tick: number;
    era: string;
    budgets: Record<string, number>;
    deltaRevision: number;
}

export function createInitialEngineViewState(): EngineViewState {
    return {
        tick: 0,
        era: "crust",
        budgets: {
            geology: 0,
            climate: 0,
            ecology: 0,
            civilization: 0,
        },
        deltaRevision: 0,
    };
}

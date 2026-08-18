import { type WorldSubsystemKey } from "../../shared/constants";

/**
 * 現在の era の計算予算(サブシステム別の重み)を共有する。
 * 重みが 0 のサブシステムはその時間スケールで計算されていない
 * (例: 1tick=500万年の地殻形成期では climate=0 → 気温・降水は未計算)。
 * runtime-store の setCurrentEraMetrics(全 era metrics 更新の中心)から更新する。
 */
type EraBudgets = Record<WorldSubsystemKey, number>;

let budgets: EraBudgets = { geology: 0, climate: 0, ecology: 0, civilization: 0 };
const listeners = new Set<() => void>();

function budgetsEqual(a: EraBudgets, b: EraBudgets): boolean {
    return (
        a.geology === b.geology &&
        a.climate === b.climate &&
        a.ecology === b.ecology &&
        a.civilization === b.civilization
    );
}

export function setEraBudgets(next: EraBudgets): void {
    // 予算が変わった時だけ通知する。高速再生中は毎 tick 呼ばれるため、
    // 変化なしで再描画するとレイヤー表示がちらついて崩れて見える。
    if (budgetsEqual(budgets, next)) {
        return;
    }
    budgets = next;
    for (const listener of listeners) {
        listener();
    }
}

export function getEraBudgets(): EraBudgets {
    return budgets;
}

/** 指標カテゴリ → 対応するサブシステム(予算キー)。 */
export function subsystemForMetricCategory(category: string): WorldSubsystemKey {
    switch (category) {
        case "terrain":
            return "geology";
        case "climate":
        case "hydrology":
        case "glaciology":
            return "climate";
        case "domesticates":
        case "ecology":
            return "ecology";
        default:
            return "civilization";
    }
}

/** その指標カテゴリが現在の時間スケールで計算されているか。 */
export function isMetricCategoryComputed(category: string): boolean {
    return budgets[subsystemForMetricCategory(category)] > 0;
}

export function onEraBudgetsChange(listener: () => void): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
}

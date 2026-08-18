import { formatRealYearsPerTick } from "../../shared/constants";

/**
 * 右上メタの「◯年前」表示。
 * tick = PRESENT_TICK を「現在」とみなし、そこから遡って何年前かを表示する。
 * 年数 = (PRESENT_TICK - 現在tick) × 1tickあたりの実年数。
 */
const PRESENT_TICK = 1600;

let yearsPerTick = 0;

/** 1tick あたりの実年数を更新する。0 や不正値は無視して直前の値を保つ
 *  (init 時の createEraMetrics(0) が runtime の実値を上書きしないように)。 */
export function setYearsPerTick(years: number): void {
    if (Number.isFinite(years) && years > 0) {
        yearsPerTick = years;
    }
}

export function formatYearsAgo(tick: number): string {
    const yearsAgo = Math.max(0, PRESENT_TICK - tick) * yearsPerTick;
    if (yearsAgo <= 0) {
        return "現在";
    }
    return `${formatRealYearsPerTick(yearsAgo)}前`;
}

export function renderYearsAgo(element: HTMLElement, tick: number): void {
    element.textContent = formatYearsAgo(tick);
}

/** 毎 tick の表示更新用(#status-era を直接更新)。 */
export function updateYearsAgoDisplay(tick: number): void {
    const element = document.getElementById("status-era");
    if (element instanceof HTMLElement) {
        renderYearsAgo(element, tick);
    }
}

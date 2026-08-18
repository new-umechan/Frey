import * as THREE from "three";
import { getMetricCategories, type CellMetricDef } from "../../app/visualizers/cell-metric";
import {
    getMetricRange,
    resolveOverlayMetricColor,
    supportsMetricOverlay,
} from "../../app/visualizers/metric-overlay-style";
import { isMetricCategoryComputed, onEraBudgetsChange } from "../../app/state/era-runtime";

/**
 * レイヤーパネル(Figma 357-95)を実データで駆動する。
 *
 * レイヤーをスタックとして持ち、各レイヤーに表示/非表示トグルを付ける。
 * 球面レンダラは一度に1指標しか出せないため、**非表示でないレイヤーのうち
 * 最上位**を球面に表示する。名前クリックで最前面へ、目アイコンで表示/非表示。
 * タイトルの ＋ で別パネル(追加パネル)を開き、そこからドラッグ&ドロップで追加。
 */
export interface LayerPanelController {
    refresh: () => void;
}

interface Layer {
    key: string;
    visible: boolean;
}

const DRAG_MIME = "application/x-frey-metric";
const REORDER_MIME = "application/x-frey-reorder";

export function createLayerPanelController(options: {
    getCurrentCellMetric: () => string;
    onCellMetricChange: (metric: string) => void;
    onViewModeChange: (mode: string) => void;
}): LayerPanelController {
    const { getCurrentCellMetric, onCellMetricChange, onViewModeChange } = options;

    const panel = document.getElementById("layer-panel");
    const bodyElement = panel?.querySelector<HTMLElement>(".v2-panel__body");
    const addButton = document.getElementById("layer-add-button");
    const pickerPanel = document.getElementById("layer-add-panel");
    const pickerBody = document.getElementById("layer-add-body");
    if (!bodyElement) {
        return { refresh: () => {} };
    }
    const body = bodyElement;

    const categories = getMetricCategories();
    const byKey = new Map<string, CellMetricDef>();
    for (const category of categories) {
        for (const metric of category.metrics) {
            byKey.set(metric.key, metric);
        }
    }

    // レイヤースタック(先頭 = 最上位)。
    const layers: Layer[] = [];
    const initial = getCurrentCellMetric();
    if (byKey.has(initial)) {
        layers.push({ key: initial, visible: true });
    }

    const colorScratch = new THREE.Color();

    function displayedKey(): string | null {
        return layers.find((layer) => layer.visible)?.key ?? null;
    }

    /** 最上位の表示レイヤーを球面へ反映する。表示レイヤーが無ければ通常表示へ。 */
    function applyDisplay(): void {
        const key = displayedKey();
        if (key) {
            onViewModeChange("metric");
            onCellMetricChange(key);
        } else {
            onViewModeChange("normal");
        }
    }

    function addLayer(metricKey: string): void {
        if (!byKey.has(metricKey)) {
            return;
        }
        const existing = layers.findIndex((layer) => layer.key === metricKey);
        if (existing >= 0) {
            // 既存なら最前面へ持ってきて表示に。
            const [layer] = layers.splice(existing, 1);
            layer.visible = true;
            layers.unshift(layer);
        } else {
            layers.unshift({ key: metricKey, visible: true });
        }
        applyDisplay();
        renderLayers();
    }

    function moveToTop(metricKey: string): void {
        const index = layers.findIndex((layer) => layer.key === metricKey);
        if (index < 0) {
            return;
        }
        const [layer] = layers.splice(index, 1);
        layer.visible = true;
        layers.unshift(layer);
        applyDisplay();
        renderLayers();
    }

    function toggleVisible(metricKey: string): void {
        const layer = layers.find((entry) => entry.key === metricKey);
        if (!layer) {
            return;
        }
        layer.visible = !layer.visible;
        applyDisplay();
        renderLayers();
    }

    // ドラッグ中のレイヤー(dragover では dataTransfer を読めないため保持する)。
    let draggingKey: string | null = null;

    function clearDropIndicators(): void {
        for (const el of body.querySelectorAll(".v2-layer-item")) {
            el.classList.remove("is-drop-above", "is-drop-below");
        }
    }

    /** カーソルの Y 位置から挿入先インデックスを求める(隙間も連続的に判定)。 */
    function insertIndexFromCursor(event: DragEvent): number {
        const items = body.querySelectorAll<HTMLElement>(".v2-layer-item");
        for (let i = 0; i < items.length; i += 1) {
            const rect = items[i].getBoundingClientRect();
            if (event.clientY < rect.top + rect.height / 2) {
                return i;
            }
        }
        return items.length;
    }

    /** その挿入先が、ドラッグ中レイヤーにとって「動かない位置」か。 */
    function isNoopInsert(insertIndex: number): boolean {
        const from = draggingKey ? layers.findIndex((l) => l.key === draggingKey) : -1;
        return from >= 0 && (insertIndex === from || insertIndex === from + 1);
    }

    /** 隙間ごとに1つだけ挿入線を出す(下段 item の上辺 / 末尾は下辺で表現)。 */
    function showDropIndicator(insertIndex: number): void {
        clearDropIndicators();
        const items = body.querySelectorAll<HTMLElement>(".v2-layer-item");
        if (items.length === 0) {
            return;
        }
        if (insertIndex >= items.length) {
            items[items.length - 1].classList.add("is-drop-below");
        } else {
            items[insertIndex].classList.add("is-drop-above");
        }
    }

    // Notion のブロックのように、ドラッグでレイヤーを並び替える。
    function reorderTo(draggedKey: string, insertIndex: number): void {
        const from = layers.findIndex((layer) => layer.key === draggedKey);
        if (from < 0) {
            return;
        }
        const [moved] = layers.splice(from, 1);
        // 削除で後ろ側の index がひとつ詰まる。
        let target = insertIndex > from ? insertIndex - 1 : insertIndex;
        target = Math.max(0, Math.min(layers.length, target));
        layers.splice(target, 0, moved);
        // 最上位の表示レイヤーが変わりうるので球面へ反映。
        applyDisplay();
        renderLayers();
    }

    function gradientCss(metricKey: string): string | null {
        if (!supportsMetricOverlay(metricKey)) {
            return null;
        }
        const range = getMetricRange(metricKey);
        if (!range) {
            return null;
        }
        const [min, max] = range;
        const stops: string[] = [];
        const steps = 6;
        for (let i = 0; i <= steps; i += 1) {
            const t = i / steps;
            resolveOverlayMetricColor(metricKey, min + t * (max - min), colorScratch);
            stops.push(`#${colorScratch.getHexString()} ${Math.round(t * 100)}%`);
        }
        return `linear-gradient(to right, ${stops.join(", ")})`;
    }

    function iconImg(src: string, size: number): HTMLImageElement {
        const img = document.createElement("img");
        img.src = src;
        img.width = size;
        img.height = size;
        img.alt = "";
        return img;
    }

    function buildLayerHead(metricKey: string): HTMLButtonElement {
        const def = byKey.get(metricKey);
        const head = document.createElement("button");
        head.type = "button";
        head.className = "v2-layer-item__head";
        head.append(iconImg("/icons/ic-layers.svg", 20));
        const name = document.createElement("span");
        name.className = "v2-layer-item__name";
        name.textContent = def?.label ?? metricKey;
        head.append(name);
        // 名前クリックで最前面へ(=球面に表示)。
        head.addEventListener("click", () => moveToTop(metricKey));
        return head;
    }

    function buildLegend(metricKey: string): HTMLElement | null {
        const def = byKey.get(metricKey);
        const gradient = gradientCss(metricKey);
        const range = getMetricRange(metricKey);
        if (!gradient || !range || !def) {
            return null;
        }
        const legend = document.createElement("div");
        legend.className = "v2-layer-legend";
        const bar = document.createElement("div");
        bar.className = "v2-legend-gradient";
        bar.style.background = gradient;
        legend.append(bar);
        const scale = document.createElement("div");
        scale.className = "v2-legend-scale";
        const [min, max] = range;
        for (const value of [min, (min + max) / 2, max]) {
            const span = document.createElement("span");
            span.textContent = def.formatter(value);
            scale.append(span);
        }
        legend.append(scale);
        return legend;
    }

    function buildVisibilityButton(layer: Layer): HTMLButtonElement {
        const def = byKey.get(layer.key);
        const button = document.createElement("button");
        button.type = "button";
        button.className = "v2-icon-button";
        const label = def?.label ?? layer.key;
        button.setAttribute("aria-label", layer.visible ? `${label}を非表示` : `${label}を表示`);
        button.append(
            iconImg(layer.visible ? "/icons/ic-visibility.svg" : "/icons/ic-visibility-off.svg", 20),
        );
        button.addEventListener("click", () => toggleVisible(layer.key));
        return button;
    }

    function buildLayerItem(layer: Layer, displayed: string | null): HTMLElement {
        const def = byKey.get(layer.key);
        const computed = def ? isMetricCategoryComputed(def.category) : true;
        const isDisplayed = layer.key === displayed;
        const item = document.createElement("div");
        item.className = "v2-layer-item";
        item.draggable = true;
        if (!layer.visible) {
            item.classList.add("v2-layer-item--hidden");
        }
        if (!computed) {
            item.classList.add("v2-layer-item--uncomputed");
        }

        const row = document.createElement("div");
        row.className = "v2-layer-item__row";
        const handle = document.createElement("span");
        handle.className = "v2-layer-drag-handle";
        handle.setAttribute("aria-hidden", "true");
        handle.textContent = "⠿";
        row.append(handle, buildLayerHead(layer.key), buildVisibilityButton(layer));
        item.append(row);

        // 並び替え(ドラッグ)。
        item.addEventListener("dragstart", (event) => {
            event.dataTransfer?.setData(REORDER_MIME, layer.key);
            if (event.dataTransfer) {
                event.dataTransfer.effectAllowed = "move";
            }
            draggingKey = layer.key;
            item.classList.add("is-dragging");
        });
        item.addEventListener("dragend", () => {
            draggingKey = null;
            item.classList.remove("is-dragging");
            clearDropIndicators();
        });
        // 並び替えの当たり判定はボディ全体で行う(隙間も死角にならないように)。

        // 球面に出ている(最上位表示)レイヤーだけ凡例 or 未計算注記を出す。
        if (isDisplayed) {
            item.classList.add("v2-layer-item--active");
            if (!computed) {
                item.append(buildUncomputedNote());
            } else {
                const legend = buildLegend(layer.key);
                if (legend) {
                    item.append(legend);
                }
            }
        }
        return item;
    }

    function buildUncomputedNote(): HTMLElement {
        const note = document.createElement("p");
        note.className = "v2-layer-note";
        note.textContent = "この時間スケールでは計算されていません";
        return note;
    }

    function renderLayers(): void {
        body.replaceChildren();
        if (layers.length === 0) {
            const hint = document.createElement("p");
            hint.className = "v2-layer-empty";
            hint.textContent = "＋ からレイヤーをドラッグして追加";
            body.append(hint);
            return;
        }
        const displayed = displayedKey();
        for (const layer of layers) {
            body.append(buildLayerItem(layer, displayed));
        }
    }

    function renderPicker(): void {
        if (!pickerBody) {
            return;
        }
        pickerBody.replaceChildren();
        const picker = document.createElement("div");
        picker.className = "v2-layer-picker";
        for (const category of categories) {
            if (category.metrics.length === 0) {
                continue;
            }
            const group = document.createElement("div");
            group.className = "v2-layer-picker__group";
            const heading = document.createElement("p");
            heading.className = "v2-layer-picker__heading";
            heading.textContent = category.label;
            group.append(heading);
            for (const metric of category.metrics) {
                const button = document.createElement("button");
                button.type = "button";
                button.className = "v2-layer-picker__item";
                button.draggable = true;
                button.dataset.metric = metric.key;
                if (layers.some((layer) => layer.key === metric.key)) {
                    button.classList.add("is-added");
                }
                if (!isMetricCategoryComputed(metric.category)) {
                    button.classList.add("is-uncomputed");
                    button.title = "この時間スケールでは計算されていません";
                }
                button.textContent = metric.label;
                button.addEventListener("dragstart", (event) => {
                    event.dataTransfer?.setData(DRAG_MIME, metric.key);
                    event.dataTransfer?.setData("text/plain", metric.key);
                    if (event.dataTransfer) {
                        event.dataTransfer.effectAllowed = "copy";
                    }
                });
                button.addEventListener("click", () => addLayer(metric.key));
                group.append(button);
            }
            picker.append(group);
        }
        pickerBody.append(picker);
    }

    function readDraggedMetric(event: DragEvent): string | null {
        const data =
            event.dataTransfer?.getData(DRAG_MIME) ||
            event.dataTransfer?.getData("text/plain") ||
            "";
        return byKey.has(data) ? data : null;
    }

    body.addEventListener("dragover", (event) => {
        const types = event.dataTransfer ? Array.from(event.dataTransfer.types) : [];
        if (types.includes(REORDER_MIME)) {
            // 並び替え: ボディ全体でカーソル位置から挿入位置を決める。
            event.preventDefault();
            event.dataTransfer!.dropEffect = "move";
            const insertIndex = insertIndexFromCursor(event);
            if (isNoopInsert(insertIndex)) {
                clearDropIndicators();
            } else {
                showDropIndicator(insertIndex);
            }
            return;
        }
        if (types.includes(DRAG_MIME)) {
            event.preventDefault();
            event.dataTransfer!.dropEffect = "copy";
            panel?.classList.add("is-drop-target");
        }
    });
    body.addEventListener("dragleave", () => {
        panel?.classList.remove("is-drop-target");
        clearDropIndicators();
    });
    body.addEventListener("drop", (event) => {
        const types = event.dataTransfer ? Array.from(event.dataTransfer.types) : [];
        if (types.includes(REORDER_MIME)) {
            event.preventDefault();
            const draggedKey = event.dataTransfer?.getData(REORDER_MIME) ?? "";
            const insertIndex = insertIndexFromCursor(event);
            clearDropIndicators();
            if (draggedKey && !isNoopInsert(insertIndex)) {
                reorderTo(draggedKey, insertIndex);
            }
            return;
        }
        const metricKey = readDraggedMetric(event);
        panel?.classList.remove("is-drop-target");
        if (metricKey) {
            event.preventDefault();
            addLayer(metricKey);
        }
    });

    addButton?.addEventListener("click", () => {
        if (pickerPanel) {
            pickerPanel.hidden = !pickerPanel.hidden;
        }
    });

    // era(時間スケール)が変わると計算されるサブシステムが変わるので、両方を再描画。
    onEraBudgetsChange(() => {
        renderLayers();
        renderPicker();
    });

    renderPicker();
    renderLayers();

    return { refresh: renderLayers };
}

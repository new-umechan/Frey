import { getMetricCategories, type CellMetricDef } from "../../app/visualizers/cell-metric";

function clamp(value: number, min: number, max: number): number {
    return Math.max(min, Math.min(max, value));
}

export interface ViewCuiController {
    moveViewCursor: (delta: number) => void;
    commitViewSelection: (index?: number) => void;
    backViewMenu: () => boolean;
    handleDigitSelect: (key: string) => boolean;
}

interface ViewMenuEntryBase {
    label: string;
}

interface ViewMenuModeEntry extends ViewMenuEntryBase {
    type: "mode";
    value: string;
}

interface ViewMenuNextEntry extends ViewMenuEntryBase {
    type: "next";
    next: string;
}

interface ViewMenuMetricEntry extends ViewMenuEntryBase {
    type: "metric";
    value: string;
}

type ViewMenuEntry = ViewMenuModeEntry | ViewMenuNextEntry | ViewMenuMetricEntry;

export function createViewCuiController(options: {
    viewModeInputs: HTMLInputElement[];
    getCurrentCellMetric: () => string;
    onViewModeChange: (mode: string) => void;
    onCellMetricChange: (metric: string) => void;
}): ViewCuiController {
    const {
        viewModeInputs,
        getCurrentCellMetric,
        onViewModeChange,
        onCellMetricChange,
    } = options;

    const viewCuiContext = document.getElementById("view-cui-context");
    const viewCuiOptions = document.getElementById("view-cui-options");
    const categories = getMetricCategories();

    let viewMenuKey = "root";
    let viewCursorIndex = 0;

    function getCheckedViewMode() {
        const checked = viewModeInputs.find((input) => input.checked);
        return checked?.value ?? "normal";
    }

    function getCheckedMetric() {
        return getCurrentCellMetric?.() ?? "height";
    }

    function appendCurrentSuffix(label: string, isCurrent: boolean) {
        return isCurrent ? `${label}（現在）` : label;
    }

    function getCategoryByMenuKey(menuKey: string) {
        return categories.find((category) => category.key === menuKey) ?? null;
    }

    function getViewMenuEntries(menuKey: string): ViewMenuEntry[] {
        const checkedMode = getCheckedViewMode();
        const checkedMetric = getCheckedMetric();
        if (menuKey === "root") {
            return [
                {
                    label: appendCurrentSuffix("1: 通常", checkedMode === "normal"),
                    value: "normal",
                    type: "mode",
                },
                ...categories.map((category, index) => ({
                    label: appendCurrentSuffix(
                        `${index + 2}: ${category.label}`,
                        checkedMode === "metric" &&
                            category.metrics.some((metric: CellMetricDef) => metric.key === checkedMetric)
                    ),
                    next: category.key,
                    type: "next" as const,
                })),
            ];
        }

        const category = getCategoryByMenuKey(menuKey);
        if (!category) {
            return [];
        }
        return category.metrics.map((metric: CellMetricDef, index: number) => ({
            label: appendCurrentSuffix(
                `${index + 1}: ${metric.label}`,
                checkedMode === "metric" && checkedMetric === metric.key
            ),
            value: metric.key,
            type: "metric" as const,
        }));
    }

    function getViewMenuContext(menuKey: string) {
        if (menuKey === "root") {
            return "";
        }
        const category = getCategoryByMenuKey(menuKey);
        if (!category) {
            return "";
        }
        return ` / ${category.label}`;
    }

    function getParentMenuIndex(menuKey: string) {
        if (menuKey === "root") {
            return 0;
        }
        const categoryIndex = categories.findIndex((category) => category.key === menuKey);
        return categoryIndex >= 0 ? categoryIndex + 1 : 0;
    }

    function syncViewCursorToSelection() {
        const entries = getViewMenuEntries(viewMenuKey);
        if (entries.length === 0) {
            viewCursorIndex = 0;
            return;
        }
        if (viewMenuKey === "root") {
            const checkedMode = getCheckedViewMode();
            if (checkedMode === "normal") {
                viewCursorIndex = 0;
                return;
            }
            const checkedMetric = getCheckedMetric();
            const selectedCategoryIndex = categories.findIndex((category) => {
                return category.metrics.some((metric: CellMetricDef) => metric.key === checkedMetric);
            });
            viewCursorIndex = selectedCategoryIndex >= 0 ? selectedCategoryIndex + 1 : 0;
            return;
        }
        const checkedMetric = getCheckedMetric();
        const metricIndex = entries.findIndex((entry) => "value" in entry && entry.value === checkedMetric);
        viewCursorIndex = metricIndex >= 0 ? metricIndex : 0;
    }

    function renderViewCui(syncCursor = false) {
        if (viewCuiContext instanceof HTMLElement) {
            viewCuiContext.textContent = getViewMenuContext(viewMenuKey);
        }
        if (!(viewCuiOptions instanceof HTMLElement)) {
            return;
        }
        if (syncCursor) {
            syncViewCursorToSelection();
        }

        const entries = getViewMenuEntries(viewMenuKey);
        if (entries.length === 0) {
            viewCursorIndex = 0;
            viewCuiOptions.replaceChildren();
            return;
        }
        viewCursorIndex = clamp(viewCursorIndex, 0, entries.length - 1);
        viewCuiOptions.replaceChildren();
        for (let i = 0; i < entries.length; i += 1) {
            const entry = entries[i];
            const button = document.createElement("button");
            button.type = "button";
            button.className = "view-cui-item";
            if (i === viewCursorIndex) {
                button.classList.add("is-cursor");
            }
            button.dataset.cuiOptionIndex = String(i);

            const cursor = document.createElement("span");
            cursor.className = "view-cursor";
            cursor.setAttribute("aria-hidden", "true");
            button.append(cursor);

            const text = document.createElement("span");
            text.textContent = entry.label;
            button.append(text);

            viewCuiOptions.append(button);
        }
    }

    function moveViewCursor(delta: number) {
        const entries = getViewMenuEntries(viewMenuKey);
        if (entries.length === 0) {
            return;
        }
        viewCursorIndex = clamp(viewCursorIndex + delta, 0, entries.length - 1);
        renderViewCui(false);
    }

    function commitViewSelection(index = viewCursorIndex) {
        const entries = getViewMenuEntries(viewMenuKey);
        if (index < 0 || index >= entries.length) {
            return;
        }
        const entry = entries[index];
        if (entry.type === "next" && "next" in entry) {
            viewMenuKey = entry.next;
            syncViewCursorToSelection();
            renderViewCui(false);
            return;
        }
        if (entry.type === "mode" && "value" in entry) {
            onViewModeChange(entry.value);
            renderViewCui(true);
            return;
        }
        if (entry.type === "metric" && "value" in entry) {
            onCellMetricChange(entry.value);
            onViewModeChange("metric");
            renderViewCui(true);
        }
    }

    function backViewMenu() {
        if (viewMenuKey === "root") {
            return false;
        }
        const previousMenuKey = viewMenuKey;
        viewMenuKey = "root";
        viewCursorIndex = getParentMenuIndex(previousMenuKey);
        renderViewCui(false);
        return true;
    }

    function handleDigitSelect(key: string) {
        if (!/^[1-9]$/.test(key)) {
            return false;
        }
        const index = Number(key) - 1;
        const entries = getViewMenuEntries(viewMenuKey);
        if (index < 0 || index >= entries.length) {
            return false;
        }
        viewCursorIndex = index;
        commitViewSelection(index);
        return true;
    }

    if (viewCuiOptions instanceof HTMLElement) {
        viewCuiOptions.addEventListener("click", (event) => {
            const target = event.target;
            if (!(target instanceof HTMLElement)) {
                return;
            }
            const button = target.closest("[data-cui-option-index]");
            if (!(button instanceof HTMLButtonElement)) {
                return;
            }
            const index = Number(button.dataset.cuiOptionIndex);
            if (!Number.isInteger(index)) {
                return;
            }
            viewCursorIndex = index;
            commitViewSelection(index);
        });
    }

    for (const input of viewModeInputs) {
        input.addEventListener("change", () => {
            if (!input.checked) {
                return;
            }
            onViewModeChange(input.value);
            renderViewCui(true);
        });
    }

    renderViewCui(true);

    return {
        moveViewCursor,
        commitViewSelection,
        backViewMenu,
        handleDigitSelect,
    };
}

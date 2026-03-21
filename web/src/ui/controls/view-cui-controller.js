function clamp(value, min, max) {
    return Math.max(min, Math.min(max, value));
}

export function createViewCuiController({
    viewModeInputs,
    getCurrentClimateMetric,
    onViewModeChange,
    onClimateMetricChange,
}) {
    const viewCuiContext = document.getElementById("view-cui-context");
    const viewCuiOptions = document.getElementById("view-cui-options");

    let viewMenuKey = "root";
    let viewCursorIndex = 0;

    function getCheckedViewMode() {
        const checked = viewModeInputs.find((input) => input.checked);
        return checked?.value ?? "normal";
    }

    function getCheckedClimateMetric() {
        const current = getCurrentClimateMetric?.();
        return current === "precipitation" ? "precipitation" : "temperature";
    }

    function appendCurrentSuffix(label, isCurrent) {
        return isCurrent ? `${label}（現在）` : label;
    }

    function getViewMenuEntries(menuKey) {
        const checkedMode = getCheckedViewMode();
        const checkedMetric = getCheckedClimateMetric();

        if (menuKey === "mode") {
            return [
                {
                    label: appendCurrentSuffix("1: プレート", checkedMode === "plates"),
                    value: "plates",
                    type: "mode",
                },
                {
                    label: appendCurrentSuffix("2: マントル", checkedMode === "mantle"),
                    value: "mantle",
                    type: "mode",
                },
            ];
        }

        if (menuKey === "climate") {
            return [
                {
                    label: appendCurrentSuffix(
                        "1: 気温",
                        checkedMode === "climate" && checkedMetric === "temperature",
                    ),
                    value: "temperature",
                    type: "climate",
                },
                {
                    label: appendCurrentSuffix(
                        "2: 降水量",
                        checkedMode === "climate" && checkedMetric === "precipitation",
                    ),
                    value: "precipitation",
                    type: "climate",
                },
            ];
        }

        return [
            {
                label: appendCurrentSuffix("1: 通常", checkedMode === "normal"),
                value: "normal",
                type: "mode",
            },
            {
                label: appendCurrentSuffix("2: 地形", checkedMode === "plates" || checkedMode === "mantle"),
                next: "mode",
                type: "next",
            },
            {
                label: appendCurrentSuffix("3: 気候", checkedMode === "climate"),
                next: "climate",
                type: "next",
            },
        ];
    }

    function getViewMenuContext(menuKey) {
        if (menuKey === "mode") {
            return " / 地形";
        }
        if (menuKey === "climate") {
            return " / 気候";
        }
        return "";
    }

    function getParentMenuIndex(menuKey) {
        if (menuKey === "mode") {
            return 1;
        }
        if (menuKey === "climate") {
            return 2;
        }
        return 0;
    }

    function syncViewCursorToSelection() {
        const entries = getViewMenuEntries(viewMenuKey);
        if (entries.length === 0) {
            viewCursorIndex = 0;
            return;
        }

        if (viewMenuKey === "mode") {
            const checkedMode = getCheckedViewMode();
            const modeIndex = entries.findIndex((entry) => entry.value === checkedMode);
            viewCursorIndex = modeIndex >= 0 ? modeIndex : 0;
            return;
        }

        if (viewMenuKey === "climate") {
            const checkedMetric = getCheckedClimateMetric();
            const metricIndex = entries.findIndex((entry) => entry.value === checkedMetric);
            viewCursorIndex = metricIndex >= 0 ? metricIndex : 0;
            return;
        }

        viewCursorIndex = clamp(viewCursorIndex, 0, entries.length - 1);
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

    function moveViewCursor(delta) {
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
        if (entry.type === "next" && entry.next) {
            if (entry.next === "climate") {
                onViewModeChange("climate");
            }
            viewMenuKey = entry.next;
            syncViewCursorToSelection();
            renderViewCui(false);
            return;
        }

        if (entry.type === "mode") {
            onViewModeChange(entry.value);
        } else if (entry.type === "climate") {
            onClimateMetricChange(entry.value);
        }
        renderViewCui(true);
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

    function handleDigitSelect(key) {
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

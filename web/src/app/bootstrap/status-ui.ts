export function isPerfFeatureEnabled() {
    const params = new URLSearchParams(window.location.search);
    return params.get("perf") === "1" || params.get("bench") === "1";
}

export function createStatusController(statusMessage: HTMLElement, statusRows: HTMLElement[]) {
    function showStatusError(message: string) {
        statusMessage.hidden = false;
        statusMessage.textContent = message;
        for (const row of statusRows) {
            row.hidden = true;
        }
    }

    function clearStatusError() {
        statusMessage.hidden = true;
        for (const row of statusRows) {
            row.hidden = false;
        }
    }

    return {
        setStatus(message: string) {
            const text = String(message ?? "");
            const lowered = text.toLowerCase();
            const isError = lowered.includes("failed") || lowered.includes("error");
            if (isError) {
                showStatusError(text);
                return;
            }
            clearStatusError();
        },
    };
}

export function setPerfPanelVisibility(perfPanel: HTMLElement | null, isPerfEnabled: boolean) {
    if (!perfPanel) {
        return;
    }
    perfPanel.hidden = !isPerfEnabled;
    perfPanel.setAttribute("aria-hidden", String(!isPerfEnabled));
}

export function bindPlaybackUiEvents({ playbackControls, eventLogList, onTogglePlay, onHistorySeek, onHistoryStepDirection, onEventLogJump }) {
    playbackControls.playToggleButton.addEventListener("click", onTogglePlay);
    playbackControls.historySeekSlider.addEventListener("input", () => {
        onHistorySeek(playbackControls.historySeekSlider.value);
    });
    playbackControls.historySeekSlider.addEventListener("change", () => {
        onHistorySeek(playbackControls.historySeekSlider.value);
    });
    playbackControls.seekBackwardButton.addEventListener("click", () => {
        onHistoryStepDirection(-1);
    });
    playbackControls.seekForwardButton.addEventListener("click", () => {
        onHistoryStepDirection(1);
    });

    eventLogList.addEventListener("click", (event) => {
        const target = event.target;
        if (!(target instanceof HTMLElement)) {
            return;
        }
        const entryButton = target.closest("[data-log-tick]");
        if (!(entryButton instanceof HTMLButtonElement)) {
            return;
        }
        const tickText = entryButton.dataset.logTick;
        if (!tickText) {
            return;
        }
        onEventLogJump(tickText);
    });
}

export function bindPerfEvents(perfEnabled, perfControls, onRunPerfBenchmark, onCopyPerfBenchmark) {
    if (!perfEnabled || !perfControls) {
        return;
    }
    perfControls.runButton.addEventListener("click", onRunPerfBenchmark);
    perfControls.copyButton.addEventListener("click", onCopyPerfBenchmark);
}

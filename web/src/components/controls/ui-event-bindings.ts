interface PlaybackUiEventsOptions {
    playbackControls: {
        playToggleButton: HTMLElement;
        historySeekSlider: HTMLInputElement;
        seekBackwardButton: HTMLElement;
        seekForwardButton: HTMLElement;
    };
    eventLogList: HTMLUListElement;
    onTogglePlay: () => void;
    onHistoryPrefetch: (indexText: string) => void;
    onHistorySeek: (indexText: string) => void;
    onHistoryStepDirection: (direction: number) => void;
    onEventLogJump: (tickText: string) => void;
}

export function bindPlaybackUiEvents({ playbackControls, eventLogList, onTogglePlay, onHistoryPrefetch, onHistorySeek, onHistoryStepDirection, onEventLogJump }: PlaybackUiEventsOptions) {
    playbackControls.playToggleButton.addEventListener("click", onTogglePlay);
    playbackControls.historySeekSlider.addEventListener("input", () => {
        onHistoryPrefetch(playbackControls.historySeekSlider.value);
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

    eventLogList.addEventListener("click", (event: Event) => {
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

interface PerfEventsOptions {
    perfControls: {
        runButton: HTMLElement;
        copyButton: HTMLElement;
    } | null;
}

export function bindPerfEvents(perfEnabled: boolean, perfControls: PerfEventsOptions["perfControls"] | null, onRunPerfBenchmark: () => void, onCopyPerfBenchmark: () => void) {
    if (!perfEnabled || !perfControls) {
        return;
    }
    perfControls.runButton.addEventListener("click", onRunPerfBenchmark);
    perfControls.copyButton.addEventListener("click", onCopyPerfBenchmark);
}

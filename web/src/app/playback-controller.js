import {
    createEmptyEventLogElement,
    createEventLogElement,
} from "./playback/event-log.js";
import { createPlaybackOverlayController } from "./playback/overlay-controller.js";
import {
    normalizeTicks,
    resolveStepTick,
    sanitizeTick,
} from "./playback/tick-utils.js";

const PLAYBACK_OVERLAY_IDLE_MS = 3600;
const PLAY_ICON = '<svg class="glyph-icon glyph-icon-play" viewBox="0 0 16 18" aria-hidden="true"><path d="M8.78626e-08 2.00059C-0.000104208 1.64868 0.0926453 1.30298 0.268884 0.998383C0.445122 0.693786 0.69861 0.441083 1.00375 0.265789C1.30889 0.090495 1.65488 -0.00118309 2.00679 1.15272e-05C2.3587 0.00120614 2.70406 0.0952313 3.008 0.272593L15.005 7.27059C15.3078 7.44627 15.5591 7.69834 15.7339 8.0016C15.9088 8.30486 16.0009 8.64869 16.0012 8.99873C16.0015 9.34877 15.91 9.69276 15.7357 9.99633C15.5614 10.2999 15.3105 10.5524 15.008 10.7286L3.008 17.7286C2.70406 17.906 2.3587 18 2.00679 18.0012C1.65488 18.0024 1.30889 17.9107 1.00375 17.7354C0.69861 17.5601 0.445122 17.3074 0.268884 17.0028C0.0926453 16.6982 -0.000104208 16.3525 8.78626e-08 16.0006V2.00059Z" /></svg>';
const PAUSE_ICON = '<svg class="glyph-icon glyph-icon-pause" viewBox="0 0 24 24" aria-hidden="true"><path d="M18 3H15C14.4477 3 14 3.44772 14 4V20C14 20.5523 14.4477 21 15 21H18C18.5523 21 19 20.5523 19 20V4C19 3.44772 18.5523 3 18 3Z" /><path d="M9 3H6C5.44772 3 5 3.44772 5 4V20C5 20.5523 5.44772 21 6 21H9C9.55228 21 10 20.5523 10 20V4C10 3.44772 9.55228 3 9 3Z" /></svg>';

export function createPlaybackController({
    playbackControls,
    eventLogList,
    playbackState,
    worldState,
    worldSimController,
    getActiveWorldId,
    getCurrentTerrainData,
    getWorldTick,
    syncWorldFromActiveController,
    stepWorldTick,
    setStatus,
}) {
    const overlayController = createPlaybackOverlayController({
        overlay: playbackControls.overlay,
        idleMs: PLAYBACK_OVERLAY_IDLE_MS,
    });

    function getAvailableTicks() {
        return Array.isArray(playbackState.availableTicks) ? playbackState.availableTicks : [];
    }

    function getPreviousHistoryTick(baseTick) {
        return resolveStepTick(getAvailableTicks(), baseTick, -1, playbackState.historyInterval);
    }

    function getNextHistoryTick(baseTick) {
        return resolveStepTick(getAvailableTicks(), baseTick, 1, playbackState.historyInterval);
    }

    function updateMaxTickLabel() {
        const ticks = getAvailableTicks();
        const maxTick = ticks.length > 0
            ? ticks[ticks.length - 1]
            : Math.max(0, sanitizeTick(getWorldTick()) ?? 0);
        playbackControls.maxTick.textContent = String(maxTick);
    }

    function updateSeekSliderFill() {
        const slider = playbackControls.historySeekSlider;
        const min = Number(slider.min);
        const max = Number(slider.max);
        const value = Number(slider.value);

        if (!Number.isFinite(min) || !Number.isFinite(max) || max <= min) {
            slider.style.setProperty("--seek-progress", "0%");
            return;
        }

        const ratio = Math.max(0, Math.min(1, (value - min) / (max - min)));
        slider.style.setProperty("--seek-progress", `${(ratio * 100).toFixed(2)}%`);
    }

    function renderHistorySeekSlider() {
        const slider = playbackControls.historySeekSlider;
        const ticks = getAvailableTicks();
        if (ticks.length === 0) {
            slider.min = "0";
            slider.max = "0";
            slider.value = "0";
            playbackControls.seekMinLabel.textContent = "-";
            playbackControls.seekMaxLabel.textContent = "-";
            updateSeekSliderFill();
            return;
        }

        const fallbackTick = ticks[ticks.length - 1];
        const selectedTick = ticks.includes(playbackState.selectedTick)
            ? playbackState.selectedTick
            : fallbackTick;
        playbackState.selectedTick = selectedTick;

        const selectedIndex = Math.max(0, ticks.indexOf(selectedTick));
        slider.min = "0";
        slider.max = String(ticks.length - 1);
        slider.value = String(selectedIndex);
        playbackControls.seekMinLabel.textContent = `t${ticks[0]}`;
        playbackControls.seekMaxLabel.textContent = `t${ticks[ticks.length - 1]}`;
        updateSeekSliderFill();
    }

    function syncSeekSliderWithWorldTick() {
        const ticks = getAvailableTicks();
        if (ticks.length === 0) {
            return;
        }

        const slider = playbackControls.historySeekSlider;
        const currentTick = getWorldTick();
        let nextIndex = 0;
        for (let i = ticks.length - 1; i >= 0; i -= 1) {
            if (ticks[i] <= currentTick) {
                nextIndex = i;
                break;
            }
        }

        playbackState.selectedTick = ticks[nextIndex];
        slider.value = String(nextIndex);
    }

    function renderEventLog() {
        eventLogList.replaceChildren();

        if (!Array.isArray(playbackState.eventLog) || playbackState.eventLog.length === 0) {
            eventLogList.append(createEmptyEventLogElement());
            return;
        }

        const availableTicks = getAvailableTicks();
        for (const entry of playbackState.eventLog) {
            const canJump = availableTicks.includes(entry.tick);
            eventLogList.append(createEventLogElement(entry, canJump));
        }
    }

    function syncPlaybackUi() {
        playbackControls.currentTick.textContent = String(getWorldTick());
        updateMaxTickLabel();
        playbackControls.playToggleButton.innerHTML = playbackState.isPlaying ? PAUSE_ICON : PLAY_ICON;
        playbackControls.playToggleButton.setAttribute("aria-label", playbackState.isPlaying ? "停止" : "再生");

        const hasWorld = Boolean(getActiveWorldId()) && Boolean(getCurrentTerrainData());
        const hasTicks = getAvailableTicks().length > 0;

        playbackControls.playToggleButton.disabled = !hasWorld;
        playbackControls.historySeekSlider.disabled = !hasWorld || !hasTicks;
        playbackControls.seekForwardButton.disabled = !hasWorld || getNextHistoryTick(getWorldTick()) === null;
        playbackControls.seekBackwardButton.disabled = !hasWorld || getPreviousHistoryTick(getWorldTick()) === null;

        if (hasWorld && hasTicks) {
            syncSeekSliderWithWorldTick();
        }
        updateSeekSliderFill();
    }

    function setPlaybackRunning(nextPlaying) {
        const normalized = Boolean(nextPlaying);
        if (playbackState.isPlaying === normalized) {
            return;
        }
        playbackState.isPlaying = normalized;
        worldState.isRunning = normalized;
        syncPlaybackUi();
    }

    function refreshHistoryTicks() {
        const activeWorldId = getActiveWorldId();
        if (!activeWorldId) {
            playbackState.availableTicks = [];
            playbackState.selectedTick = null;
            renderHistorySeekSlider();
            renderEventLog();
            updateMaxTickLabel();
            return;
        }

        const response = worldSimController.list_history_ticks(activeWorldId);
        const ticks = Array.isArray(response?.ticks) ? response.ticks : [];
        const normalized = normalizeTicks(ticks);
        playbackState.availableTicks = normalized;

        const interval = sanitizeTick(response?.interval);
        if (interval !== null && interval > 0) {
            playbackState.historyInterval = interval;
        }

        if (!normalized.includes(playbackState.selectedTick)) {
            const candidates = normalized.filter((tick) => tick <= getWorldTick());
            playbackState.selectedTick = candidates.length > 0
                ? candidates[candidates.length - 1]
                : normalized[normalized.length - 1] ?? null;
        }

        renderHistorySeekSlider();
        renderEventLog();
        updateMaxTickLabel();
    }

    function restoreWorldToTick(targetTick) {
        const activeWorldId = getActiveWorldId();
        if (!activeWorldId) {
            return;
        }

        const normalizedTick = sanitizeTick(targetTick);
        if (normalizedTick === null) {
            return;
        }

        try {
            worldSimController.restore_world_to_tick(activeWorldId, normalizedTick);
            setPlaybackRunning(false);
            syncWorldFromActiveController();
            playbackState.selectedTick = normalizedTick;
            renderHistorySeekSlider();
            syncPlaybackUi();
        } catch (error) {
            setStatus(`Restore failed: ${String(error)}`);
            console.error(error);
        }
    }

    function appendPlaybackEvent(type, label, detail = "", tick = getWorldTick()) {
        const safeTick = sanitizeTick(tick);
        if (safeTick === null) {
            return;
        }

        const loadedTicks = getAvailableTicks();
        const loadedMaxTick = loadedTicks.length > 0 ? loadedTicks[loadedTicks.length - 1] : null;
        if (loadedMaxTick !== null && safeTick < loadedMaxTick) {
            return;
        }

        const nextId = Math.max(1, Math.floor(Number(playbackState.nextLogId) || 1));
        playbackState.nextLogId = nextId + 1;
        playbackState.eventLog.push({
            id: nextId,
            type,
            tick: safeTick,
            label,
            detail: typeof detail === "string" ? detail : String(detail),
            createdAtMs: Date.now(),
        });

        if (playbackState.eventLog.length > 120) {
            playbackState.eventLog.shift();
        }
        renderEventLog();
    }

    function handleTogglePlay() {
        if (!getActiveWorldId()) {
            return;
        }
        setPlaybackRunning(!playbackState.isPlaying);
    }

    function handleStepForward() {
        if (playbackState.isPlaying || !getActiveWorldId()) {
            return;
        }
        stepWorldTick();
    }

    function withHistoryRestore(callback) {
        if (!getActiveWorldId()) {
            return;
        }
        setPlaybackRunning(false);
        callback();
    }

    function handleRewind() {
        withHistoryRestore(() => {
            const targetTick = getPreviousHistoryTick(getWorldTick());
            restoreWorldToTick(targetTick);
        });
    }

    function handleHistoryJump(tickText) {
        withHistoryRestore(() => {
            const targetTick = sanitizeTick(tickText);
            if (targetTick === null) {
                return;
            }
            restoreWorldToTick(targetTick);
        });
    }

    function handleHistorySeek(indexText) {
        withHistoryRestore(() => {
            const historyIndex = sanitizeTick(indexText);
            if (historyIndex === null) {
                return;
            }

            const ticks = getAvailableTicks();
            if (historyIndex >= ticks.length) {
                return;
            }

            const targetTick = ticks[historyIndex];
            if (targetTick === getWorldTick()) {
                return;
            }
            restoreWorldToTick(targetTick);
        });
    }

    function handleHistoryStepDirection(direction) {
        withHistoryRestore(() => {
            const normalizedDirection = direction >= 0 ? 1 : -1;
            const targetTick = normalizedDirection < 0
                ? getPreviousHistoryTick(getWorldTick())
                : getNextHistoryTick(getWorldTick());
            if (targetTick === null) {
                return;
            }
            restoreWorldToTick(targetTick);
        });
    }

    function shouldRefreshHistoryOnAdvance(previousTick, nextTick) {
        const safePrevTick = sanitizeTick(previousTick);
        const safeNextTick = sanitizeTick(nextTick);
        const interval = Math.max(1, sanitizeTick(playbackState.historyInterval) ?? 1);

        if (safeNextTick !== null && (safeNextTick % interval) === 0) {
            return true;
        }
        if (safePrevTick === null || safeNextTick === null || safeNextTick <= safePrevTick) {
            return false;
        }
        return Math.floor(safePrevTick / interval) < Math.floor(safeNextTick / interval);
    }

    function syncAfterWorldStep(stepInfo = {}) {
        const worldTick = getWorldTick();
        const previousTick = stepInfo?.previousTick;
        const nextTick = stepInfo?.nextTick ?? worldTick;
        if (shouldRefreshHistoryOnAdvance(previousTick, nextTick)) {
            refreshHistoryTicks();
        }
        syncPlaybackUi();
    }

    function syncAfterWorldSync() {
        refreshHistoryTicks();
        syncPlaybackUi();
    }

    return {
        appendPlaybackEvent,
        bindOverlayActivityEvents: overlayController.bindActivityEvents,
        handleHistoryJump,
        handleHistorySeek,
        handleHistoryStepDirection,
        handleRewind,
        handleStepForward,
        handleTogglePlay,
        notePlaybackOverlayActivity: overlayController.noteActivity,
        refreshHistoryTicks,
        setPlaybackRunning,
        syncAfterWorldStep,
        syncAfterWorldSync,
        syncPlaybackUi,
    };
}

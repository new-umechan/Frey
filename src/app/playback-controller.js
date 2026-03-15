const PLAYBACK_OVERLAY_IDLE_MS = 1800;

function sanitizeTick(rawTick) {
    const tick = Math.floor(Number(rawTick));
    return Number.isFinite(tick) && tick >= 0 ? tick : null;
}

function formatEventLogLine(entry) {
    const detail = entry.detail ? ` | ${entry.detail}` : "";
    return `[t=${entry.tick}] ${entry.label}${detail}`;
}

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
    let playbackOverlayHideTimerId = null;

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
        const ticks = Array.isArray(playbackState.availableTicks) ? playbackState.availableTicks : [];
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
        const slider = playbackControls.historySeekSlider;
        const ticks = Array.isArray(playbackState.availableTicks) ? playbackState.availableTicks : [];
        if (ticks.length === 0) {
            return;
        }

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

    function resolveHistoryStepTarget(baseTick, direction) {
        const ticks = Array.isArray(playbackState.availableTicks) ? playbackState.availableTicks : [];
        if (ticks.length === 0) {
            return null;
        }

        const normalizedBaseTick = sanitizeTick(baseTick);
        if (normalizedBaseTick === null) {
            return null;
        }

        const interval = sanitizeTick(playbackState.historyInterval);
        const step = interval !== null && interval > 0 ? interval : 1;
        const desiredTick = normalizedBaseTick + (direction * step);

        if (direction < 0) {
            if (desiredTick < ticks[0]) {
                return null;
            }
            for (let i = ticks.length - 1; i >= 0; i -= 1) {
                if (ticks[i] <= desiredTick) {
                    return ticks[i];
                }
            }
            return null;
        }

        if (desiredTick > ticks[ticks.length - 1]) {
            return null;
        }
        for (let i = 0; i < ticks.length; i += 1) {
            if (ticks[i] >= desiredTick) {
                return ticks[i];
            }
        }
        return null;
    }

    function getPreviousHistoryTick(baseTick) {
        return resolveHistoryStepTarget(baseTick, -1);
    }

    function getNextHistoryTick(baseTick) {
        return resolveHistoryStepTarget(baseTick, 1);
    }

    function renderEventLog() {
        eventLogList.replaceChildren();
        if (!Array.isArray(playbackState.eventLog) || playbackState.eventLog.length === 0) {
            const item = document.createElement("li");
            item.className = "event-log-item";
            const button = document.createElement("button");
            button.type = "button";
            button.className = "event-log-entry is-static";
            button.disabled = true;
            button.textContent = "イベントログはまだありません";
            item.append(button);
            eventLogList.append(item);
            return;
        }

        for (const entry of playbackState.eventLog) {
            const item = document.createElement("li");
            item.className = "event-log-item";
            const button = document.createElement("button");
            const canJump = playbackState.availableTicks.includes(entry.tick);
            button.type = "button";
            button.className = `event-log-entry${canJump ? "" : " is-static"}`;
            button.textContent = formatEventLogLine(entry);
            if (canJump) {
                button.dataset.logTick = String(entry.tick);
            } else {
                button.disabled = true;
            }
            const meta = document.createElement("span");
            meta.className = "event-log-meta";
            meta.textContent = `#${entry.id}`;
            button.append(document.createTextNode("\n"));
            button.append(meta);
            item.append(button);
            eventLogList.append(item);
        }
    }

    function syncPlaybackUi() {
        playbackControls.currentTick.textContent = String(getWorldTick());
        playbackControls.playToggleButton.textContent = playbackState.isPlaying ? "⏸" : "▶";
        playbackControls.playToggleButton.setAttribute(
            "aria-label",
            playbackState.isPlaying ? "停止" : "再生",
        );
        const hasWorld = Boolean(getActiveWorldId()) && Boolean(getCurrentTerrainData());
        playbackControls.playToggleButton.disabled = !hasWorld;
        playbackControls.historySeekSlider.disabled = (
            !hasWorld
            || playbackState.availableTicks.length === 0
        );
        playbackControls.seekForwardButton.disabled = (
            !hasWorld
            || getNextHistoryTick(getWorldTick()) === null
        );
        playbackControls.seekBackwardButton.disabled = (
            !hasWorld
            || getPreviousHistoryTick(getWorldTick()) === null
        );
        if (hasWorld && playbackState.availableTicks.length > 0) {
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
            return;
        }
        const response = worldSimController.list_history_ticks(activeWorldId);
        const ticks = Array.isArray(response?.ticks) ? response.ticks : [];
        const normalizedTicks = Array.from(
            new Set(
                ticks
                    .map((value) => sanitizeTick(value))
                    .filter((value) => value !== null),
            ),
        ).sort((a, b) => a - b);
        playbackState.availableTicks = normalizedTicks;
        const interval = sanitizeTick(response?.interval);
        if (interval !== null && interval > 0) {
            playbackState.historyInterval = interval;
        }
        if (!normalizedTicks.includes(playbackState.selectedTick)) {
            const candidates = normalizedTicks.filter((tick) => tick <= getWorldTick());
            playbackState.selectedTick = candidates.length > 0
                ? candidates[candidates.length - 1]
                : normalizedTicks[normalizedTicks.length - 1] ?? null;
        }
        renderHistorySeekSlider();
        renderEventLog();
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
        const nextId = Math.max(1, Math.floor(Number(playbackState.nextLogId) || 1));
        playbackState.nextLogId = nextId + 1;
        playbackState.eventLog.unshift({
            id: nextId,
            type,
            tick: safeTick,
            label,
            detail: typeof detail === "string" ? detail : String(detail),
            createdAtMs: Date.now(),
        });
        if (playbackState.eventLog.length > 120) {
            playbackState.eventLog.length = 120;
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

    function handleRewind() {
        if (!getActiveWorldId()) {
            return;
        }
        setPlaybackRunning(false);
        const targetTick = getPreviousHistoryTick(getWorldTick());
        restoreWorldToTick(targetTick);
    }

    function handleHistoryJump(tickText) {
        if (!getActiveWorldId()) {
            return;
        }
        setPlaybackRunning(false);
        const targetTick = sanitizeTick(tickText);
        if (targetTick === null) {
            return;
        }
        restoreWorldToTick(targetTick);
    }

    function handleHistorySeek(indexText) {
        if (!getActiveWorldId()) {
            return;
        }
        setPlaybackRunning(false);
        const historyIndex = sanitizeTick(indexText);
        if (historyIndex === null) {
            return;
        }
        const ticks = playbackState.availableTicks;
        if (!Array.isArray(ticks) || historyIndex >= ticks.length) {
            return;
        }
        const targetTick = ticks[historyIndex];
        if (targetTick === getWorldTick()) {
            return;
        }
        restoreWorldToTick(targetTick);
    }

    function handleHistoryStepDirection(direction) {
        if (!getActiveWorldId()) {
            return;
        }
        setPlaybackRunning(false);
        const normalizedDirection = direction >= 0 ? 1 : -1;
        const targetTick = normalizedDirection < 0
            ? getPreviousHistoryTick(getWorldTick())
            : getNextHistoryTick(getWorldTick());
        if (targetTick === null) {
            return;
        }
        restoreWorldToTick(targetTick);
    }

    function clearPlaybackOverlayHideTimer() {
        if (playbackOverlayHideTimerId !== null) {
            window.clearTimeout(playbackOverlayHideTimerId);
            playbackOverlayHideTimerId = null;
        }
    }

    function isPlaybackOverlayStickyVisible() {
        const active = document.activeElement;
        return playbackControls.overlay.matches(":hover")
            || (active instanceof HTMLElement && playbackControls.overlay.contains(active));
    }

    function showPlaybackOverlay() {
        playbackControls.overlay.classList.remove("is-idle-hidden");
    }

    function schedulePlaybackOverlayAutoHide() {
        clearPlaybackOverlayHideTimer();
        playbackOverlayHideTimerId = window.setTimeout(() => {
            if (isPlaybackOverlayStickyVisible()) {
                schedulePlaybackOverlayAutoHide();
                return;
            }
            playbackControls.overlay.classList.add("is-idle-hidden");
        }, PLAYBACK_OVERLAY_IDLE_MS);
    }

    function notePlaybackOverlayActivity() {
        showPlaybackOverlay();
        schedulePlaybackOverlayAutoHide();
    }

    function bindOverlayActivityEvents(viewportPanel) {
        viewportPanel.addEventListener("pointermove", notePlaybackOverlayActivity);
        viewportPanel.addEventListener("pointerenter", notePlaybackOverlayActivity);
        viewportPanel.addEventListener("wheel", notePlaybackOverlayActivity, { passive: true });
        viewportPanel.addEventListener("touchstart", notePlaybackOverlayActivity, { passive: true });
        playbackControls.overlay.addEventListener("pointerenter", notePlaybackOverlayActivity);
        playbackControls.overlay.addEventListener("pointermove", notePlaybackOverlayActivity);
        playbackControls.overlay.addEventListener("focusin", notePlaybackOverlayActivity);
        playbackControls.overlay.addEventListener("pointerleave", schedulePlaybackOverlayAutoHide);
        playbackControls.overlay.addEventListener("focusout", schedulePlaybackOverlayAutoHide);
        document.addEventListener("keydown", (event) => {
            if (event.code === "Space" || event.key === "ArrowLeft" || event.key === "ArrowRight") {
                notePlaybackOverlayActivity();
            }
        });
    }

    function syncAfterWorldStep() {
        const worldTick = getWorldTick();
        if ((worldTick % playbackState.historyInterval) === 0) {
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
        bindOverlayActivityEvents,
        handleHistoryJump,
        handleHistorySeek,
        handleHistoryStepDirection,
        handleRewind,
        handleStepForward,
        handleTogglePlay,
        notePlaybackOverlayActivity,
        refreshHistoryTicks,
        setPlaybackRunning,
        syncAfterWorldStep,
        syncAfterWorldSync,
        syncPlaybackUi,
    };
}

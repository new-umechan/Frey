import {
    isHelpToggleKey,
    isInteractiveTarget,
} from "./keyboard-guards";

interface KeyboardHandlerOptions {
    controlHelp: {
        toggleControlHelp: () => void;
        closeControlHelp: () => void;
        isOpen: () => boolean;
    };
    viewCui: {
        moveViewCursor: (direction: number) => void;
        commitViewSelection: () => void;
        backViewMenu: () => boolean;
        handleDigitSelect: (key: string) => boolean;
    };
    seedInput: HTMLInputElement;
    getDebugEnabled: () => boolean;
    getCurrentSurfaceMode: () => string;
    onToggleDebug: (enabled: boolean) => void;
    onToggleSurface: (mode: string) => void;
    onTogglePlay: () => void;
    onStepForward: () => void;
    onRewind: () => void;
    onHistoryStepDirection: (direction: number) => void;
}

export function createGlobalKeyboardHandler({
    controlHelp,
    viewCui,
    seedInput,
    getDebugEnabled,
    getCurrentSurfaceMode,
    onToggleDebug,
    onToggleSurface,
    onTogglePlay,
    onStepForward,
    onRewind,
    onHistoryStepDirection,
}: KeyboardHandlerOptions) {
    return function onDocumentKeyDown(event: KeyboardEvent) {
        if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) {
            return;
        }

        if (isInteractiveTarget(event.target)) {
            return;
        }

        const lowerKey = event.key.toLowerCase();

        if (isHelpToggleKey(event)) {
            event.preventDefault();
            controlHelp.toggleControlHelp();
            return;
        }

        if (controlHelp.isOpen()) {
            if (event.key === "Escape") {
                event.preventDefault();
                controlHelp.closeControlHelp();
            }
            return;
        }

        if (event.key === "ArrowUp" || lowerKey === "k") {
            event.preventDefault();
            viewCui.moveViewCursor(-1);
            return;
        }

        if (event.key === "ArrowDown" || lowerKey === "j") {
            event.preventDefault();
            viewCui.moveViewCursor(1);
            return;
        }

        if (event.key === "Enter" || lowerKey === "l") {
            event.preventDefault();
            viewCui.commitViewSelection();
            return;
        }

        if ((event.key === "Escape" || lowerKey === "h") && viewCui.backViewMenu()) {
            event.preventDefault();
            return;
        }

        if (viewCui.handleDigitSelect(event.key)) {
            event.preventDefault();
            return;
        }

        if (lowerKey === "t" || lowerKey === "s") {
            event.preventDefault();
            seedInput.focus();
            seedInput.select();
            return;
        }

        if (lowerKey === "d") {
            event.preventDefault();
            onToggleDebug(!getDebugEnabled());
            return;
        }

        if (lowerKey === "v") {
            event.preventDefault();
            onToggleSurface(getCurrentSurfaceMode() === "globe" ? "map" : "globe");
            return;
        }

        if (event.code === "Space") {
            event.preventDefault();
            onTogglePlay();
            return;
        }

        if (event.key === ".") {
            event.preventDefault();
            onStepForward();
            return;
        }

        if (event.key === ",") {
            event.preventDefault();
            onRewind();
            return;
        }

        if (event.key === "ArrowLeft") {
            event.preventDefault();
            onHistoryStepDirection(-1);
            return;
        }

        if (event.key === "ArrowRight") {
            event.preventDefault();
            onHistoryStepDirection(1);
        }
    };
}

export function isHelpToggleKey(event: KeyboardEvent) {
    return event.key === "?" || (event.code === "Slash" && event.shiftKey);
}

export function isInteractiveTarget(target: EventTarget | null) {
    return target instanceof HTMLElement && (
        target.isContentEditable
        || (target instanceof HTMLInputElement && target.type !== "range")
        || target instanceof HTMLTextAreaElement
        || target instanceof HTMLSelectElement
    );
}

export function isHelpToggleKey(event) {
    return event.key === "?" || (event.code === "Slash" && event.shiftKey);
}

export function isInteractiveTarget(target) {
    return target instanceof HTMLElement && (
        target.isContentEditable
        || (target instanceof HTMLInputElement && target.type !== "range")
        || target instanceof HTMLTextAreaElement
        || target instanceof HTMLSelectElement
    );
}

interface EventLogEntry {
    type: string;
    tick: number;
    label: string;
    detail?: string;
}

function formatEventLogLine(entry: EventLogEntry) {
    const detail = entry.detail ? ` ${entry.detail}` : "";
    return `${entry.label}${detail}`;
}

function hasFailureKeyword(entry: EventLogEntry | null | undefined) {
    const text = `${entry?.label ?? ""} ${entry?.detail ?? ""}`;
    return /(fail|error|失敗 | エラー)/i.test(text);
}

function resolveEventTone(entry: EventLogEntry) {
    const type = entry?.type ?? "";
    if (type === "era-changed" || type === "error" || type === "fatal" || hasFailureKeyword(entry)) {
        return "important";
    }
    if (type === "world-generated" || type === "info" || type === "debug") {
        return "muted";
    }
    return "normal";
}

export function createEmptyEventLogElement() {
    const item = document.createElement("li");
    item.className = "event-log-item";

    const button = document.createElement("button");
    button.type = "button";
    button.className = "event-log-entry is-static is-muted";
    button.disabled = true;

    const tick = document.createElement("span");
    tick.className = "event-log-tick";
    tick.textContent = "--";

    const text = document.createElement("span");
    text.className = "event-log-text";
    text.textContent = "イベントログはまだありません";

    button.append(tick, text);
    item.append(button);
    return item;
}

export function createEventLogElement(entry: EventLogEntry, canJump: boolean) {
    const item = document.createElement("li");
    item.className = "event-log-item";

    const button = document.createElement("button");
    button.type = "button";
    button.className = `event-log-entry is-${resolveEventTone(entry)}${canJump ? "" : " is-static"}`;

    const tick = document.createElement("span");
    tick.className = "event-log-tick";
    tick.textContent = `t=${entry.tick}`;

    const text = document.createElement("span");
    text.className = "event-log-text";
    text.textContent = formatEventLogLine(entry);

    button.append(tick, text);
    if (canJump) {
        button.dataset.logTick = String(entry.tick);
    } else {
        button.disabled = true;
    }

    item.append(button);
    return item;
}

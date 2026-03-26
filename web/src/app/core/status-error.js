export function formatStatusError(phase, error) {
    const phaseText = String(phase ?? "Operation");
    if (error instanceof Error) {
        return `${phaseText} failed: ${error.message}`;
    }
    return `${phaseText} failed: ${String(error)}`;
}

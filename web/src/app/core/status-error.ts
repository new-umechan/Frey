export function formatStatusError(phase: string | undefined, error: unknown): string {
    const phaseText = String(phase ?? "Operation");
    if (error instanceof Error) {
        return `${phaseText} failed: ${error.message}`;
    }
    return `${phaseText} failed: ${String(error)}`;
}

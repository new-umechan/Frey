import { createApp } from "./app/app.js";
import { formatStatusError } from "./app/core/status-error.js";

/**
 * @param {Error} error
 */
function showInitializationError(error) {
    const statusMessage = document.getElementById("status-message");
    const statusEra = document.getElementById("status-era");
    const statusTick = document.getElementById("era-scale-tick-label");

    if (statusMessage instanceof HTMLElement) {
        statusMessage.hidden = false;
        statusMessage.textContent = formatStatusError("Initialization", error);
    }
    if (statusEra instanceof HTMLElement) {
        statusEra.hidden = true;
    }
    if (statusTick instanceof HTMLElement) {
        statusTick.hidden = true;
    }
}

async function main() {
    /** @type {Awaited<ReturnType<typeof createApp>>} */
    const app = await createApp();

    /**
     * @param {number} nowMs
     */
    function frame(nowMs) {
        app.tick(nowMs);
        requestAnimationFrame(frame);
    }

    requestAnimationFrame(frame);
}

main().catch((error) => {
    showInitializationError(error);
    console.error(error);
});

import { createApp } from "./app/app.js";
import { formatStatusError } from "./app/status-error.js";

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
    const app = await createApp();

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

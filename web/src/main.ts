import { createApp } from "./app/app";
import { setupBackgroundMusic } from "./app/bootstrap/background-music";
import { setupMeta } from "./app/ui/meta";
import { setupV2Panels } from "./app/ui/v2-panels";
import { formatStatusError } from "./app/state/status-error";

function showInitializationError(error: unknown) {
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
    setupBackgroundMusic();
    setupV2Panels();
    setupMeta();
    const app = await createApp();

    function frame(nowMs: number) {
        app.tick(nowMs);
        requestAnimationFrame(frame);
    }

    requestAnimationFrame(frame);
}

main().catch((error) => {
    showInitializationError(error);
    console.error(error);
});

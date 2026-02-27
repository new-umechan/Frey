import { createApp } from "./app/app.js";

async function main() {
    const app = await createApp();

    function frame(nowMs) {
        app.tick(nowMs);
        requestAnimationFrame(frame);
    }

    requestAnimationFrame(frame);
}

main().catch((error) => {
    const statusMessage = document.getElementById("status-message");
    if (statusMessage instanceof HTMLElement) {
        statusMessage.textContent = `Initialization failed: ${String(error)}`;
    }
    console.error(error);
});

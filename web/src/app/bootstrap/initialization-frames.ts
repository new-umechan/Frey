function renderOnNextAnimationFrame(renderFrame) {
    return new Promise((resolve) => {
        window.requestAnimationFrame(() => {
            renderFrame();
            resolve(undefined);
        });
    });
}

export async function renderInitializationFrames(renderFrame, frameCount = 2) {
    for (let i = 0; i < frameCount; i += 1) {
        await renderOnNextAnimationFrame(renderFrame);
    }
}

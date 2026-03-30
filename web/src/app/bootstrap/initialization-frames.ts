function renderOnNextAnimationFrame(renderFrame: () => void): Promise<void> {
    return new Promise((resolve) => {
        window.requestAnimationFrame(() => {
            renderFrame();
            resolve(undefined);
        });
    });
}

export async function renderInitializationFrames(renderFrame: () => void, frameCount = 2): Promise<void> {
    for (let i = 0; i < frameCount; i += 1) {
        await renderOnNextAnimationFrame(renderFrame);
    }
}

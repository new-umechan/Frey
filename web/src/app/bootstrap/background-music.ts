const BGM_VOLUME = 0.2;
const BGM_SOURCE = "/bgm.mp3";

export function setupBackgroundMusic() {
    const bgm = new Audio(BGM_SOURCE);
    bgm.loop = true;
    bgm.preload = "auto";
    bgm.volume = BGM_VOLUME;

    const startPlayback = () => {
        void bgm
            .play()
            .then(() => {
                window.removeEventListener("pointerdown", startPlayback);
                window.removeEventListener("keydown", startPlayback);
            })
            .catch((error: unknown) => {
                console.info("BGM playback is waiting for user interaction.", error);
            });
    };

    startPlayback();
    window.addEventListener("pointerdown", startPlayback);
    window.addEventListener("keydown", startPlayback);
}

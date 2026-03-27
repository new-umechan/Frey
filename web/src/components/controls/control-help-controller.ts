export function createControlHelpController(controlHelpModal, controlHelpCloseButton) {
    function openControlHelp() {
        if (!controlHelpModal) {
            return;
        }
        controlHelpModal.hidden = false;
    }

    function closeControlHelp() {
        if (!controlHelpModal) {
            return;
        }
        controlHelpModal.hidden = true;
    }

    function toggleControlHelp() {
        if (!controlHelpModal) {
            return;
        }
        if (controlHelpModal.hidden) {
            openControlHelp();
            return;
        }
        closeControlHelp();
    }

    if (controlHelpCloseButton) {
        controlHelpCloseButton.addEventListener("click", closeControlHelp);
    }

    if (controlHelpModal) {
        controlHelpModal.addEventListener("click", (event) => {
            const target = event.target;
            if (!(target instanceof HTMLElement)) {
                return;
            }
            if (target.dataset.controlHelpClose !== undefined) {
                closeControlHelp();
            }
        });
    }

    return {
        closeControlHelp,
        toggleControlHelp,
        isOpen() {
            return Boolean(controlHelpModal && !controlHelpModal.hidden);
        },
    };
}

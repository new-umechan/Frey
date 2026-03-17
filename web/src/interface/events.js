export function bindEvent(target, type, listener, options) {
    target.addEventListener(type, listener, options);
    return () => {
        target.removeEventListener(type, listener, options);
    };
}

export function bindEvents(bindings) {
    const disposers = bindings.map((binding) =>
        bindEvent(binding.target, binding.type, binding.listener, binding.options),
    );
    return () => {
        for (const dispose of disposers) {
            dispose();
        }
    };
}

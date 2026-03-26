import importPlugin from "eslint-plugin-import";

const browserGlobals = {
    window: "readonly",
    document: "readonly",
    navigator: "readonly",
    performance: "readonly",
    requestAnimationFrame: "readonly",
    cancelAnimationFrame: "readonly",
    HTMLElement: "readonly",
    Worker: "readonly",
    URL: "readonly",
    Intl: "readonly",
    self: "readonly",
    console: "readonly",
};

export default [
    {
        files: ["web/src/**/*.js"],
        languageOptions: {
            ecmaVersion: "latest",
            sourceType: "module",
            globals: browserGlobals,
        },
        plugins: {
            import: importPlugin,
        },
        rules: {
            "no-unused-vars": [
                "error",
                {
                    argsIgnorePattern: "^_",
                    varsIgnorePattern: "^_",
                    caughtErrorsIgnorePattern: "^_",
                },
            ],
        },
    },
    {
        files: ["web/src/app/**/*.js", "web/src/gfx/**/*.js", "web/src/ui/**/*.js"],
        plugins: {
            import: importPlugin,
        },
        rules: {
            "import/no-unused-modules": [
                "warn",
                {
                    unusedExports: true,
                    missingExports: false,
                },
            ],
        },
    },
];

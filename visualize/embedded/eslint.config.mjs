import globals from "globals";
import pluginJs from "@eslint/js";
import tseslint from "typescript-eslint";
import pluginVue from "eslint-plugin-vue";


/** @type {import('eslint').Linter.Config[]} */
export default [
    { files: ["**/*.{js,ts,vue}"] },
    { ignores: ['**/dist/*', 'tsconfig.json'] },
    { languageOptions: { globals: globals.browser } },
    pluginJs.configs.recommended,
    ...tseslint.configs.recommended,
    ...pluginVue.configs["flat/essential"],
    {
        files: ["**/*.vue", "**/*.ts"],
        languageOptions: { parserOptions: { parser: tseslint.parser } },
        rules: {
            "@typescript-eslint/no-explicit-any": "off",
            "vue/multi-word-component-names": "off",
        },
    },
];
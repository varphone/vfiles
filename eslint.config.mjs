import js from "@eslint/js";
import globals from "globals";
import tsParser from "@typescript-eslint/parser";
import tsPlugin from "@typescript-eslint/eslint-plugin";
import vue from "eslint-plugin-vue";
import vueParser from "vue-eslint-parser";
import prettier from "eslint-config-prettier";

const commonIgnores = [
  "node_modules/",
  "dist/",
  "client/dist/",
  "release/",
  "coverage/",
  "data/",
  "data.git/",
  ".bun/",
  ".vfiles_uploads/",
  ".vfiles_download_cache/",
];

/** @type {import('eslint').Linter.FlatConfig[]} */
export default [
  { ignores: commonIgnores },

  js.configs.recommended,

  // Base language options
  {
    files: ["**/*.{js,cjs,mjs,ts,tsx,vue}"],
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "module",
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
    rules: {
      // Keep lint usable on existing codebase
      "no-unused-vars": "off",
      "no-undef": "off",
      "no-constant-condition": "off",
      "no-unsafe-finally": "off",
    },
  },

  // TypeScript
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
    },
    plugins: {
      "@typescript-eslint": tsPlugin,
    },
    rules: {
      ...tsPlugin.configs["flat/recommended"][0].rules,
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/no-unused-vars": [
        "warn",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
        },
      ],
    },
  },

  // Vue (Vue SFCs)
  ...vue.configs["flat/recommended"],
  {
    files: ["**/*.vue"],
    languageOptions: {
      parser: vueParser,
      parserOptions: {
        parser: tsParser,
        ecmaVersion: "latest",
        sourceType: "module",
        extraFileExtensions: [".vue"],
      },
    },
    plugins: {
      vue,
      "@typescript-eslint": tsPlugin,
    },
    rules: {
      "vue/multi-word-component-names": "off",
      "vue/attributes-order": "off",
      "vue/attribute-hyphenation": "off",
      "vue/no-v-html": "off",
    },
  },

  // Disable formatting-related rules (let Prettier handle formatting)
  prettier,
];

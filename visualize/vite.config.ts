import { fileURLToPath, URL } from 'node:url'
import { resolve } from 'path'

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import cssInjectedByJsPlugin from 'vite-plugin-css-injected-by-js'
import { compress_js, PluginConfig } from './compress_js'

// https://vitejs.dev/config/
export default defineConfig({
    plugins: [vue(), cssInjectedByJsPlugin(), compress_js(new PluginConfig('hyperion-visual.js', 'hyperion-visual.js.b64'))],
    resolve: {
        alias: {
            '@': fileURLToPath(new URL('./src', import.meta.url)),
        },
    },
    build: {
        chunkSizeWarningLimit: 1500, // 1.5MB chunk limit to remove warning: we will use compression anyway
        rollupOptions: {
            input: {
                main: resolve(__dirname, 'index.html'),
            },
            output: {
                entryFileNames: `hyperion-visual.js`,
                chunkFileNames: `assets/[name].js`,
                assetFileNames: `assets/[name].[ext]`,
                // disable chunks to ensure a single js file
                manualChunks: undefined,
            },
        },
        cssCodeSplit: false,
    },
})

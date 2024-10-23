import { fileURLToPath, URL } from 'node:url'
import { resolve } from 'path'

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import cssInjectedByJsPlugin from 'vite-plugin-css-injected-by-js'
import { compress_js } from './compress_js'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    vue(), cssInjectedByJsPlugin(), compress_js({ js_filename: 'hyperion-visual.js' })
  ],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url))
    }
  },
  build: {
    chunkSizeWarningLimit: 1000,  // 1MB chunk limit
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

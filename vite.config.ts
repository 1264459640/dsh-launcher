import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import VueI18nPlugin from '@intlify/unplugin-vue-i18n/vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    vue(),
    // Discovers every JSON locale file under src/locales, pre-compiles the
    // messages at build time and hot-reloads them during development.
    VueI18nPlugin({
      // Normalize to posix separators: picomatch patterns never match
      // backslashes on Windows otherwise.
      include: [fileURLToPath(new URL('./src/locales/**', import.meta.url)).replace(/\\/g, '/')],
      strictMessage: false,
      escapeHtml: false,
    }),
  ],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  // Tauri expects a fixed port in development.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Rust build output and editor/atomic-save temp dirs must not be
      // watched (EBUSY on Windows when cargo locks the dll).
      ignored: ['**/src-tauri/**', '**/.*/**', '**/.*.tmpdir/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: 'es2021',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
})

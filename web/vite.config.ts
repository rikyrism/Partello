/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// In development the Rust server runs on :8080 and Vite proxies /api (HTTP + WebSocket)
// so the browser sees a single origin, exactly like production.
const backend = process.env.PARTELLO_BACKEND ?? 'http://localhost:8080'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      '/api': { target: backend, ws: true },
      '/healthz': backend,
      '/readyz': backend,
    },
  },
  build: {
    target: 'es2022',
    sourcemap: true,
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    css: false,
  },
})

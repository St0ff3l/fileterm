import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  base: './',
  plugins: [react()],
  build: {
    // Monaco's TypeScript worker is a lazy-loaded, self-contained language service.
    // Its expected size is not representative of the initial renderer bundle.
    chunkSizeWarningLimit: 7_000
  },
  server: {
    port: 5188,
    strictPort: true
  }
})

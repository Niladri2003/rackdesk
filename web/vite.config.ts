import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// In development, Vite serves the UI on :5173 and forwards API calls to webdeskd.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:7171',
      '/healthz': 'http://127.0.0.1:7171',
    },
  },
})

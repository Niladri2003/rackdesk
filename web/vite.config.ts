import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// In development, Vite serves the UI on :5173 and forwards API calls to rackdeskd.
export default defineConfig({
  plugins: [react()],
  build: {
    // Small files are normally put inside the CSS or JS as "data:" text. The
    // server's content security policy allows fonts and images from our own
    // address only, so such a font would be blocked. Keep every asset a file.
    assetsInlineLimit: 0,
  },
  server: {
    proxy: {
      // `ws` passes WebSocket connections on too. The terminal needs it.
      '/api': { target: 'http://127.0.0.1:7171', ws: true },
      '/healthz': 'http://127.0.0.1:7171',
    },
  },
})

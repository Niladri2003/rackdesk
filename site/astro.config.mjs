import { defineConfig } from 'astro/config'

// The site is built to plain files in dist/. The security header in
// netlify.toml allows scripts and styles from files only, so nothing may be
// written into the HTML itself: check-output.mjs fails the build if it is.
export default defineConfig({
  output: 'static',
  devToolbar: { enabled: false },
  build: {
    inlineStylesheets: 'never',
  },
  vite: {
    build: {
      // 0 keeps small scripts and files as files, not as text in the HTML.
      assetsInlineLimit: 0,
    },
  },
})

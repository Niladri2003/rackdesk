// Fails the build if a page in dist/ holds an inline script, an inline style
// or a style attribute. The Content-Security-Policy header in netlify.toml
// blocks all 3, and a blocked script or style fails without a visible error.
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

const pages = readdirSync('dist', { recursive: true }).filter((name) => name.endsWith('.html'))
const rules = [
  [/<script(?![^>]*\ssrc=)[^>]*>/i, 'an inline script'],
  [/<style[\s>]/i, 'an inline style element'],
  [/<[^>]+\sstyle=/i, 'a style attribute'],
  [/\son[a-z]+=["']/i, 'an inline event handler'],
]
let failed = false
for (const page of pages) {
  const html = readFileSync(join('dist', page), 'utf8')
  for (const [pattern, what] of rules) {
    if (pattern.test(html)) {
      console.error(`dist/${page} has ${what}`)
      failed = true
    }
  }
}
if (failed) process.exit(1)
console.log(`checked ${pages.length} pages: no inline script or style`)

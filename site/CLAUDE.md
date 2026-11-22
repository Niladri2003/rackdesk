# Landing page guide

Read the root `CLAUDE.md` first. Its writing rules apply here too: the hyphen is the only dash, simple English, digits for numbers. This file covers the landing page in `site/`.

The page is an Astro project. The build writes plain files to `site/dist`, and Netlify publishes that folder. The page has no server code.

## Commands
```sh
npm install        # once
npm run dev        # dev server on 127.0.0.1:8100
npm run build      # build into dist/, then check the output (check-output.mjs)
npm run preview    # serve dist/ on 127.0.0.1:8100. Stop it with: npx astro preview stop
```
`make site` and `make site-build` in the repo root run the same commands. Run `npm run build` after every change: it holds the only automatic check.

## Stack
- **Installed:** Astro and `sharp` (Astro uses it to convert pictures).
- **Not used, on purpose:** a UI framework (React, Vue), Tailwind CSS, an animation library, analytics. The page is HTML, 1 stylesheet and 1 script.
- Check with the user before adding a package.

## Where things are
| Path | What it holds |
|---|---|
| `src/pages/` | `index.astro` (the page) and `404.astro` |
| `src/layouts/Base.astro` | The `<head>` and the coloured light behind the page |
| `src/components/` | 1 file per section: `Hero`, `Stats`, `Apps`, `Security`, `Planned`, `Install`. Also `Header`, `Footer`, `Mark` (the logo) and `InstallCommand` |
| `src/site.ts` | The repository URL and the install command |
| `src/styles/site.css` | All styles. Colours are CSS variables on `:root` |
| `src/scripts/main.js` | All script: copy buttons, the root program diagram, the start sequence |
| `src/assets/shots/` | The 9 screenshots of RackDesk, as JPEG source files |
| `public/` | Fonts and icons. Astro copies them unchanged |
| `check-output.mjs` | Fails the build if a built page breaks the CSP |
| `../netlify.toml` | Build settings, security headers and cache times |

## Security rules
The Content-Security-Policy header in `netlify.toml` allows scripts, styles, fonts and pictures from the site's own address only. A blocked script or style fails without a visible error.
- **No inline script, no inline `<style>`, no `style` attribute, no `onclick` attribute.** `check-output.mjs` fails the build if a built page has one. In Astro this means: no `is:inline`, no `<style>` block in a component, no `style={...}`, no `define:vars`. Styles go in `site.css`.
- **A script may set a style through the DOM** (`element.style.setProperty`). The CSP allows that, and `main.js` does it.
- **Nothing loads from another address.** No CDN, no remote font, no analytics, no embedded video.
- **Keep `build.inlineStylesheets: 'never'` and `assetsInlineLimit: 0`** in `astro.config.mjs`. Without them Astro writes small styles and scripts into the HTML.
- A new kind of content (for example a form or a frame) needs a change to the CSP in `netlify.toml`. Ask the user first.

## Content rules
- **The page says only what is true.** An app that is not built is in the "Planned" section and marked as planned. The 4 numbers match `docs/ROADMAP.md`.
- **The install command and the repository URL are written in `src/site.ts` only.** Components import them. The command works once a release is published on GitHub.
- **The terminal of the start sequence shows the real output of the installer.** The 3 "ok" lines and the last line come from `packaging/install.sh`. The account block is the text `rackdeskd` prints (`print_password` in `crates/rackdeskd/src/main.rs`). Change the page when either changes.
- **The pictures of RackDesk are real screenshots.** Do not draw the UI by hand.

## The start sequence
A drawn pointer clicks Copy, the command is pasted in a terminal, the browser opens, and the pointer signs in and opens Files, the Terminal and search. Each click shows the next screenshot.
- The script reads the screenshot addresses from `data-frames` on `#view`. `Hero.astro` writes them at build time, as WebP in 2 widths.
- It runs once, and again from the button under the picture. The same button is "Skip" while it runs.
- It does not run with reduced motion, below 701 px width, or without script. In those cases the last screenshot (`term-1`) shows. Check all 3 after a change.
- It scrolls the page once, to bring the browser into view, and only if the visitor is still at the top.

### When the screenshots change
The sequence depends on the exact layout of the screenshots. After retaking them, update:
| What | Where |
|---|---|
| Where the pointer clicks, as parts of the width and height | `SPOT` in `src/scripts/main.js` |
| The command that is typed | `COMMAND` in `src/scripts/main.js` |
| The place and size of the typed command (it must sit on the prompt line of `term-0`) | `.typed` in `src/styles/site.css` |
| The names and order of the screenshots | `NAMES` and `LAST` in `src/components/Hero.astro`, and the `show(...)` calls in `main.js` |

How the current screenshots were made. No script for this is saved in the repo:
- A separate `rackdeskd` with an empty data folder, a sample folder as `--files-root` (with `etc/nginx` in it) and `--terminal direct`.
- Playwright, viewport 1440 x 810, device scale 1.5, dark colour scheme, JPEG quality 86.
- The browser showed the address `127.0.0.1:7171`.
- The shell prompt was set to `admin@server:~$` first, so that no real user name or host name is in a picture. Check every new screenshot for private data before it is committed.

## Quality bar
- Works at 390 px width with no sideways scroll.
- `prefers-reduced-motion` stops every animation. Add each new animation to the reduced-motion block at the end of `site.css`.
- Every picture that carries meaning has `alt` text. A picture that only decorates has `aria-hidden="true"`.
- Visible keyboard focus on every link and button.
- A new picture goes in `src/assets/` and through `getImage` or `<Image>`, so the build converts it. Only fonts and icons go in `public/`.

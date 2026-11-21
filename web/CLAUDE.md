# Web UI guide

Read the root `CLAUDE.md` first. This file covers the React app in `web/`.

**Status:** the desktop shell, Settings, the file browser (read and write), the file preview, the text editor and the terminal are built. The look follows the accepted design ("Shelf" direction: a rail on the left edge).

## Commands
```sh
npm run dev      # :5173, proxies /api and /healthz to rackdeskd on 127.0.0.1:7171
npm run lint     # oxlint
npm run build    # tsc -b (app, config and tests), then vite build into dist/
npm run e2e      # Playwright and axe. Needs a built UI and `cargo build -p rackdeskd`
```
`make e2e` from the repo root builds both and runs the tests. Run it after any UI change.

## Stack
- **Installed:** React 19, TypeScript, Vite, oxlint, Radix UI (`radix-ui`), Lucide icons (`lucide-react`), TanStack Virtual (`@tanstack/react-virtual`), CodeMirror 6 (loaded on demand, in `apps/editor/codemirror.ts`), xterm.js (loaded on demand, in `apps/terminal/Terminal.tsx`). Tests: Playwright, `@axe-core/playwright`.
- **Not used, on purpose:** Tailwind CSS and Motion. Styles are plain CSS with tokens, and animations are CSS transitions (ADR 0005).
- **Planned for later apps:** dnd-kit (drag and drop).
- Check with the user before adding anything outside this list.

## Where things are
| Path | What it holds |
|---|---|
| `src/i18n/en.ts` | Every text the UI shows. No component holds text of its own |
| `src/theme/tokens.ts` | Colour tokens for light and dark, per accent and wallpaper |
| `src/ui/Logo.tsx` | The logo as components: `LogoMark`, `LogoTile`, `Lockup`. The source files are in `assets/logo/` |
| `src/styles/*.css` | All styles. `base.css` has the shared controls (`.btn`, `.input`, `.icon-btn`) |
| `src/shell/wm.ts` | The window manager: a pure reducer, with no DOM |
| `src/shell/` | Desktop, Window, Rail (taskbar), Launcher (search), Switcher, toasts, connection banner |
| `src/shell/search.ts` | The search providers |
| `src/apps/registry.ts` | The list of apps |
| `src/apps/<name>/` | One folder per app |
| `src/auth/` | Sign-in and first-password pages |
| `e2e/` | Browser tests |

## Talking to the server
- `src/api/generated/*.ts` is written by `cargo test -p rd-api`. **Never edit it by hand.** To change a type, change the Rust type in `crates/rd-api`.
- All requests go through `src/api/client.ts`. No `fetch` calls in components.
- Errors are `ApiFailure` with a stable `code`. Branch on `code`, never on the message text. `errorText(e)` from `src/i18n` gives the text to show.
- A request that fails with `unauthorized` means the session ended: call `shell.sessionEnded()`.
- Requests are same-origin with `credentials: 'same-origin'`. The session is an `HttpOnly` cookie, so JavaScript never sees or stores a token. Don't put secrets in `localStorage`.

## Security rules for the UI
- The CSP is `default-src 'self'` with no inline scripts. So: no CDN scripts, no remote fonts or images, no `eval`, no inline `<script>`. Fonts and icons are bundled.
- Never render a user's file as HTML or SVG in our origin. Previews are text, an `<img>`/`<video>` of a download URL, or a sandboxed frame. No `dangerouslySetInnerHTML` with file content or server strings.
- File names and paths are untrusted text. Render them as text and handle very long names.
- File content is untrusted too. `fileUrl(path)` gives a download. `fileUrl(path, true)` asks the server to show it in the page, and the server agrees only for image, video, audio and PDF types. Text is read with `readFileStart` and drawn as text nodes (`apps/preview/highlight.ts` returns pieces of text, not HTML).
- Data from `localStorage` is untrusted. Check every field when reading it, as `parseSaved` in `wm.ts` does.
- Hiding or disabling a control is for usability. The server enforces the rule.

## Adding an app
1. Add its id to `AppId` in `src/shell/wm.ts`.
2. Add its name and description to `apps` in `src/i18n/en.ts`.
3. Create `src/apps/<name>/` with a component that takes `AppProps`.
4. Add an entry to `APPS` in `src/apps/registry.ts`, and to `RAIL_APPS` if it belongs on the taskbar.
5. Add a search provider to `PROVIDERS` in `src/shell/search.ts`, or to `REMOTE_PROVIDERS` if it asks the server.
6. Add a browser test, and add the app's states to the accessibility test in `e2e/desktop.spec.ts`.

## Structure rules
- **Windows are generic.** `Window.tsx` draws the title bar and holds one content kind: `builtin` (a React component), `web` (a frame) or `stream` (video). An app never draws its own title bar.
- **Apps are self-contained.** An app gets `useShell()` to open other apps and to sign out. Apps don't import each other's components.
- **A window keeps its app mounted while minimised**, so the app keeps its state. State that must survive a reload goes in `params` (strings only) through `setParams`.
- **An app gets `params`, not its window.** It never receives the position or size, so a move or resize does not redraw it. `Window`, `WindowBody` and `Rail` are wrapped in `memo`: keep their props stable (no inline objects or functions), or the redraw test in `e2e/desktop.spec.ts` fails.
- **Tokens are the only source of visual values.** No colour, radius or duration is written in a component. Use the CSS variables.
- **Text comes from `t`.** `import { t } from '../i18n'`, then `t.window.close`. Text with a value in it is a function in `en.ts`.
- **Questions go through `shell/dialog.ts`.** `choose()` asks for 1 of a few answers and `askText()` for 1 line of text. Both return a promise. `showPanel()` shows a dialog whose content the caller draws, as the Properties dialog does. Don't use `window.confirm` or `window.prompt`, and don't open a Radix dialog of your own: the shell makes the desktop behind its dialog inert.
- **A change to files is announced.** After a write, call `folderChanged(dir)` from `apps/files/changes.ts`. Open file browser windows that show `dir` load it again. Nothing watches the server.
- **Uploads go through `shell/transfers.ts`** (`startUploads`). It sends chunks, resumes, and shows the panel.
- **A window with unsaved work sets a close check** with `setCloseGuard` from `AppProps`, as the editor does.
- **Two files must not differ only in letter case** (`transfers.ts` and `Transfers.tsx`). macOS treats them as 1 file.
- **An app must not take the focus late.** If it sets the focus after loading something, it first checks that its window is still the focused one (`data-focused` on `.win`), as the terminal does.
- **Nothing polls in the background.** Live data is requested only while the window showing it is open, and stops when it closes.
- **A file that exports a component exports nothing else.** Constants and helpers go in a `.ts` file next to it. The linter checks this.

## Quality bar
- Light and dark themes are both first-class. Check every screen in both.
- Everything works from the keyboard, with visible focus rings and screen-reader labels. WCAG 2.2 AA contrast. The axe test must pass for every new state.
- Icons that only decorate get `aria-hidden="true"`. A button with only an icon gets an `aria-label`.
- Respect `prefers-reduced-motion`. `base.css` turns off all transitions for it.
- Responsive: full windowing at 1024 px and wider, every window maximised below that. The phone layout is not built.
- Long lists are virtualised and loaded in pages. See `apps/files/useListing.ts` and the list in `Files.tsx`. A folder with 100,000 entries must scroll smoothly.
- A container has `role="listbox"` only while it holds options. An empty, loading or failed state is a message, not a list.
- Every data view has its empty, loading, error and disconnected states.

## Style
- Function components and hooks. Types over `any`; `tsc` must pass with no errors.
- `erasableSyntaxOnly` is on: no `enum`, no constructor parameter properties.
- Keep the first-load bundle small: load heavy apps (terminal, editor, previews) with dynamic `import()` when their window opens.

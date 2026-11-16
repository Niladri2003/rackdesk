# Web UI guide

Read the root `CLAUDE.md` first. This file covers the React app in `web/`.

**Status:** `src/App.tsx` holds plain sign-in and change-password pages and a placeholder for the signed-in state. The real desktop shell is built in M2 from the Claude Design handoff (`docs/design/CLAUDE-DESIGN-PROMPT.md`). Don't invent a visual design before the handoff arrives.

## Commands
```sh
npm run dev      # :5173, proxies /api and /healthz to rackdeskd on 127.0.0.1:7171
npm run lint     # oxlint
npm run build    # tsc -b, then vite build into dist/ (embedded into the binary)
```
There's no test runner yet. Playwright and axe checks are added in M2.

## Stack
- **Installed now:** React 19, TypeScript, Vite, oxlint.
- **Planned for M2** (named in the design brief; install them when M2 starts, not before): Tailwind CSS, Radix UI primitives, Lucide icons, Motion.
- **Planned for later apps:** TanStack Virtual (file lists), xterm.js (terminal), CodeMirror (editor), dnd-kit (drag and drop).
- Check with the user before adding anything outside this list.

## Talking to the server
- `src/api/generated/*.ts` is written by `cargo test -p rd-api`. **Never edit it by hand.** To change a type, change the Rust type in `crates/rd-api`.
- All requests go through `src/api/client.ts`. No `fetch` calls in components.
- Errors are `ApiFailure` with a stable `code`. Branch on `code`, never on the message text. The text shown for each code is in `src/messages.ts`.
- A request that fails with `unauthorized` means the session ended: show the sign-in page.
- Requests are same-origin with `credentials: 'same-origin'`. The session is an `HttpOnly` cookie, so JavaScript never sees or stores a token. Don't put secrets in `localStorage`.

## Security rules for the UI
- The CSP is `default-src 'self'` with no inline scripts. So: no CDN scripts, no remote fonts or images, no `eval`, no inline `<script>`. Fonts and icons are bundled.
- Never render a user's file as HTML or SVG in our origin. Previews are text, an `<img>`/`<video>` of a download URL, or a sandboxed frame. No `dangerouslySetInnerHTML` with file content or server strings.
- File names and paths are untrusted text. Render them as text and handle very long names.
- Hiding or disabling a control is for usability. The server enforces the rule.

## Structure (from M2)
- **Windows are generic.** One window component provides the chrome and holds one content type: `builtin` (a React component), `web` (an iframe) or `stream` (WebRTC). An app never draws its own title bar.
- **Apps are self-contained.** Each app lives in its own folder and registers a manifest, its window content, and its search provider (results and actions for the launcher's one search).
- **Apps open each other** through shared actions ("open in Files", "open terminal here"), not by importing each other's components.
- **Design tokens** (colour, spacing, radius, motion) are the only source of visual values. No hard-coded colours in components.
- **Nothing polls in the background.** Live data is requested only while the window showing it is open, and stops when it closes.

## Quality bar
- Light and dark themes are both first-class. Check every screen in both.
- Everything works from the keyboard, with visible focus rings and screen-reader labels. WCAG 2.2 AA contrast.
- Respect `prefers-reduced-motion`.
- Responsive: full windowing at 1024 px and wider, maximised windows on tablet, one app at a time on phone.
- Long lists are virtualised. A folder with 100,000 entries must scroll smoothly.
- Every data view has its empty, loading, error and disconnected states.

## Style
- Function components and hooks. Types over `any`; `tsc` must pass with no errors.
- Keep the first-load bundle small: load heavy apps (terminal, editor, previews) with dynamic `import()` when their window opens.

# ADR 0001: Rust (axum) server, React + TypeScript UI, one binary

**Status:** accepted (M0)

## Context
The agent runs on small servers (512 MB VPS, Raspberry Pi) and has root-adjacent power, so it needs low memory, memory safety and careful file handling. The UI has to feel like a desktop: windows, drag and drop, virtualised lists, terminal, editor.

## Decision
- **Server:** Rust with tokio + axum. ~5–10 MB RAM idle, no GC, and the same language as `~/projects/sandbox`, so the low-level crates and knowledge are shared (rustix, cap-std/pathrs, the privileged-helper split).
- **UI:** React + TypeScript + Vite. It has the biggest ecosystem for desktop-style widgets (dnd-kit, TanStack Virtual, xterm.js, CodeMirror, Radix) and is the most in-demand frontend skill. Frontend weight costs only the first download, never server RAM.
- **One binary:** the built UI is embedded with rust-embed. Debug builds read `web/dist` from disk.
- **One contract:** API types live in Rust (`wd-api`) and are exported to TypeScript with ts-rs.

## Alternatives
- **Go:** faster to learn and ship (File Browser, CasaOS and 1Panel use it). Rejected so the learning stays in one systems language.
- **Svelte 5:** lighter and pleasant to write, but a smaller ecosystem for desktop UIs.

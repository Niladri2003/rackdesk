# ADR 0002: All file access through cap-std

**Status:** accepted (M0)

## Context
Path traversal and symlink escapes are the classic file-manager bugs. Checking strings (`starts_with(root)`) is racy and misses symlinks.

## Decision
`wd-fs::Root` wraps a `cap_std::fs::Dir`. Every path is resolved *beneath* that directory. On Linux this uses `openat2(RESOLVE_BENEATH)`; on other platforms cap-std resolves component by component. `clean_path` adds early, readable rejections (`..`, NUL, backslash, length).

## Consequences
- Works on macOS too, so development doesn't need a Linux VM.
- Symlinks that stay inside the root still work. Symlinks pointing outside fail. They're listed as `symlink` but can't be opened.
- Privileged access outside the root goes through `webdesk-helper` (M5), never by widening the root.
- Alternative considered: `pathrs` (Linux-only, used by runc). Revisit for the helper in M5.

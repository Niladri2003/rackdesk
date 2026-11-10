# Prompt for Claude Design

Copy everything below the line into Claude Design. Bring the result back here as a design handoff (export or code), and we'll build it in `web/`.

---

Design the web UI for **Webdesk** (working name). It's a browser-based desktop for running a Linux server. A user installs one small program on their server, opens it in a browser, signs in, and gets a desktop environment: windows, a taskbar and an app launcher. From there they manage the server visually instead of over SSH. The first and most important app is a **file browser**.

## Who uses it
Developers, indie hackers and small teams running their own VPS or home server. They're technical and keyboard-friendly, but tired of SSH for everyday tasks. Mostly on a laptop. Sometimes on a tablet or phone to check something quickly.

## How it should feel
- A real, polished desktop OS: fast, calm and precise. Not an admin dashboard, and not a template.
- **An original visual identity.** Don't copy macOS, Windows, ChromeOS or Synology DSM. Familiar patterns (windows, taskbar, context menus) are fine.
- Dense but breathable, with content first. Use depth (shadow, blur) sparingly. Crisp typography, with monospace for paths, sizes and permissions.
- Light and dark themes, both first-class. The default follows the system setting. The accent colour can be changed.
- Quick, purposeful motion: 150–200 ms for open, minimise and maximise. Respect reduced-motion settings.

## v1 screens (design in full)

### 1. Sign-in and first run
- A sign-in card showing the server's hostname.
- A first sign-in step titled "Set a new password". The initial password was generated at install time and printed in the server's terminal. Explain that in one calm line.
- States: wrong password, and too many attempts ("try again in 30 s").

### 2. Desktop shell
- A subtle, original wallpaper. The user can change it.
- A taskbar or dock with open apps, a launcher button and a clock.
- A status area with the server name, a mini CPU/RAM indicator, notifications, and a user menu (Settings, Sign out).
- An app launcher with search.
- Window chrome: title, icon, minimise, maximise and close buttons, and resize handles. Focused and unfocused states.
- Snapping to halves of the screen, overlapping windows, and a keyboard window switcher.
- A banner for "Connection lost, reconnecting…".

### 3. File browser (the most detail)
- **Layout:**
  - **Sidebar:** Home, Root `/`, Favourites, Recent, disks with usage bars, Trash.
  - **Toolbar:** back, forward and up; a breadcrumb path you can click to edit as text; search; a list/grid toggle; sort; New folder; Upload.
  - **Main pane,** with an optional details or preview panel.
- **List view** columns: name, size, modified, type, permissions (`rwxr-xr-x`), owner.
- **Grid view,** with image thumbnails.
- **Selection:** single, shift/cmd multi-select, rubber-band, and an action bar while files are selected.
- **Right-click menu:** Open, Open with, Download, Rename, Copy, Cut, Paste, Duplicate, Compress, Extract, Copy path, Move to Trash, Properties.
- **Drag and drop:** from the user's own computer into a window (to upload), and between folders (to move), with clear hover feedback.
- **Transfers panel:** many files at once, each with progress, speed, pause/resume, retry and cancel, plus completed and failed states.
- **Preview:** images, video, PDF, and code or text with syntax highlighting.
- **Text editor mode:** save, an unsaved-changes dot, and a "file changed on disk" warning.
- **Properties dialog:** size, full path, timestamps, and a permissions editor (owner/group/other checkboxes plus the octal value).
- **Conflict dialog:** when a file exists, offer Replace, Keep both or Skip, with "apply to all".
- **Confirmation** for permanent delete.
- **States to design:**
  - an empty folder
  - loading (skeleton)
  - permission denied
  - a folder with 100,000 files (fast, virtualised, with the count shown)
  - very long file names
  - a path that doesn't exist
  - the server disconnected

### 4. Settings app
A full app in its own window, with a sidebar of sections, a search box that finds any setting, and a clear saved or unsaved state. Settings that need a restart say so. Dangerous changes ask for the password again.

**v1 sections (design in full):**
- **Account:** username; change password (current, new, confirm, with a strength meter and a minimum-12-characters hint); sign out.
- **Sessions:** where you're signed in (browser, IP, last active, "this device"), with Sign out for each and "Sign out everywhere else".
- **Appearance:** theme (light, dark, system); accent colour; wallpaper (built-in choices plus upload); density (comfortable, compact); text size; reduce motion.
- **Files:** show hidden files; default view (list or grid); default sort; confirm before moving to Trash; empty Trash after N days; the folder shown as "/" (read-only in v1, with a note on how to change it).
- **Access and security:** listening address and port, HTTPS certificate status (self-signed or custom, with expiry date), and failed sign-in attempts. Read-only in v1, with "edit the config file to change this" hints.
- **About:** hostname, OS and kernel, CPU and RAM, uptime, Webdesk version, a "check for updates" button, licences.

**Later sections (light sketches only):** two-factor authentication and passkeys, users and roles, notifications, apps (installed apps, their permissions and resource limits), backup and restore of settings, and the audit log.

### 5. Notifications
Toasts for success, error and long-running progress, plus a notification list.

## Future screens (light sketches only, to keep the system consistent)
- **App store:** categories and app cards with requirement badges ("2 GB RAM", "GPU recommended"). Apps this server can't run are greyed out with the reason. Show install progress and an Installed tab.
- **Terminal window.**
- **System monitor:** CPU, memory, disk, network and processes.
- **A web app** shown inside a window.
- **A streamed remote desktop app** inside a window, with connecting, latency and lost-connection states.

## Windows are generic
Every window has the same chrome and holds one of three things: a built-in app, an embedded web app, or a streamed remote app. Design the chrome, loading state and error state so they work for all three.

## Responsive
- **Desktop (≥ 1024 px):** full windowing.
- **Tablet:** windows open maximised, with a quick app switcher.
- **Phone:** no floating windows. One app full-screen at a time, with a bottom app switcher. The file browser uses list view, and a bottom action sheet replaces the right-click menu.

## Accessibility
- WCAG 2.2 AA contrast in both themes. Never rely on colour alone.
- Everything works from the keyboard:
  - arrow keys move through files, and Enter opens
  - F2 renames, and Delete moves to Trash
  - Cmd/Ctrl+C, X, V and A
  - Esc closes menus
- Visible focus rings and screen-reader labels.
- Hit targets of at least 24 px, or 44 px on touch.

## Design system deliverables
- **Tokens:** colour (neutrals, accent, success, warning, danger, for both themes), type scale, spacing, radius, elevation, layers (desktop, windows, menus, toasts, dialogs), and motion durations and easings.
- **Components:** button, icon button, input, search field, segmented control, checkbox, switch, menu and context menu, dialog, tooltip, toast, progress bar, breadcrumb, list row, grid tile, sidebar item, tabs, badge, window chrome, taskbar item, launcher.
- **Icons:** a consistent line style compatible with Lucide, plus file-type icons (folder, image, video, audio, archive, code, text, PDF, executable, symlink, unknown).

## Build constraints (so the design can be built as-is)
- React + TypeScript, Tailwind CSS, Radix UI primitives (menus, dialogs, tooltips), Lucide icons, and Motion for animation.
- One open-source UI font (for example Inter or Geist) plus a monospace (for example JetBrains Mono or Geist Mono).
- Use realistic Linux data in mockups: paths like `/var/log/nginx` and `/home/dev/projects`, owners like `www-data`, real file sizes and permissions.

## What to deliver
1. A design system page (tokens and components).
2. Full mockups of the v1 screens in light and dark.
3. A clickable prototype of this flow:
   1. sign in
   2. desktop
   3. open the file browser and go into a folder
   4. select files and right-click
   5. drag-and-drop upload with progress
   6. preview an image
   7. edit and save a text file
4. Short notes on interactions that static frames can't show.

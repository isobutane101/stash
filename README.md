# Stash

A native macOS **menu-bar** app that is a clipboard manager, a notebook, **and** a to-do
app. Stash watches the system clipboard in the background and automatically saves whatever
you copy — text, links, and images — into a searchable, taggable, pinnable, folder-able
collection. You can also jot notes, attach/drag-drop files, build multiple to-do lists with
due dates and subtasks, switch themes, copy items back with one click, and it keeps itself
up to date.

Built with **Tauri v2** + a **Rust** backend and a vanilla HTML/CSS/JS UI in an editorial
paper-and-oxblood theme.

---

## ⬇️ Download & run (macOS)

1. Download **`Stash_universal.dmg`** from the **[latest release](../../releases/latest)**.
2. Open the `.dmg` and drag **Stash** into your **Applications** folder.
3. **First launch:** right-click **Stash** in Applications → **Open** → **Open**.
   (This one-time step is needed because the app isn't paid-Apple-Developer signed.)

   If macOS says Stash is *“damaged”* or won’t open, run this once in **Terminal**, then open again:
   ```bash
   xattr -cr /Applications/Stash.app
   ```
4. Stash lives in your **menu bar** as a small **backpack** icon — click it to show/hide the
   window. (There is no dock icon.)

Works on both Apple Silicon and Intel Macs (universal binary).

### Updates

Stash updates itself. On launch it quietly checks for a newer version, and you can also
trigger it from the menu-bar icon → **Check for Updates…**. If one is available, Stash
downloads the **signed** update, verifies it, installs in place, and relaunches — no
re-downloading the `.dmg`. Updates are served from this repo's
[GitHub Releases](../../releases) and verified against a public key embedded in the app, so
only releases signed with the project's private key will install.

---

## Features

### Capture
- **Background clipboard auto-capture** — a Rust thread polls the clipboard, hashes contents
  to dedupe, and saves new items automatically. No paste needed.
- **Manual capture** — paste (⌘V), type a note, drag files in, or attach files; ⌘↵ to save.
- **Four item types** — notes (text), links, images, and files, each rendered to suit.
- **Copy-back without duplicates** — copying an item from the app (text *or* the actual image)
  updates the watcher's last-seen hash so it isn't re-captured.

### Organize
- **Folders** — create folders and **drag any card onto a folder** in the sidebar to file it.
- **Tags** — add multiple tags per item; click a sidebar tag to filter the whole collection.
- **Pin** — keep important items at the top.
- **To-do lists** — create as many lists as you want, each with its own tasks.

### Find & sort
- **Unified search** — searches text, links, file names, tags, and folders across everything,
  regardless of the current view.
- **Sort toggle** — flip between **Newest** and **Oldest** (chronological); remembered across
  launches. Pinned items stay on top.
- **Library filters** — All items, Pinned, Notes, Links, Images, Files, plus per-folder and
  per-tag views.
- **Masonry layout** — a deterministic shortest-column grid that rebalances as previews load.

### Links & media
- **Link previews** — links fetch an `og:image` thumbnail and page title when you're online,
  so saved links look like cards instead of raw URLs.
- **Custom monogram favicons** — a generated SVG tile in the app's palette (initial + color
  picked per-site), so links always have a clean icon, online or off.
- **Image lightbox** — click an image to enlarge it, then **Copy** (to clipboard), **Download**,
  **Export as PNG / JPEG**, or **Delete**.

### To-do lists (Google-Tasks style)
- **Due dates & time** with color-coded chips (Today / Tomorrow / weekday / date; **red when
  overdue**) and quick **Today / Tomorrow / Next week** buttons.
- **Notes** and **subtasks** per task; checking off a parent checks its subtasks.
- **Auto-sort** — incomplete first, then soonest due — plus **Clear done** and remaining-count
  badges in the sidebar.

### Look & feel
- **Themes** — **Paper** (warm oxblood/cream), **Mono** (black & white), and **Party** (bright
  multi-color). Persisted across launches; even re-tints the link monograms.

### System
- **Menu-bar (tray) app** — left-click the backpack icon to toggle the window; right-click for
  Open / Check for Updates / Quit. No dock icon (macOS accessory activation policy).
- **Auto-updates** — checks on launch (and on demand); downloads, signature-verifies, installs
  in place, and relaunches. See [Updates](#updates).
- **Local SQLite persistence** — items, folders, and to-do lists live in `stash.db` (WAL mode);
  image/file blobs are written to disk under `blobs/` (paths, not base64, in the DB). Your data
  never leaves your machine, except link-preview fetches to the sites you save.
- **Keyboard shortcuts** — `/` to focus search, `⌘V` paste-to-save, `⌘↵` save, `Esc` to close
  dialogs.

## Project layout

```
stash/
├─ src/                       # frontend (no bundler; served directly)
│  ├─ index.html              # UI markup
│  ├─ styles.css              # styles
│  └─ main.js                 # data layer → Tauri commands + clipboard-captured listener
├─ src-tauri/
│  ├─ src/
│  │  ├─ lib.rs               # app setup, state, plugins, tray, watcher spawn
│  │  ├─ clipboard_watch.rs   # background polling thread (arboard + sha2)
│  │  ├─ db.rs                # SQLite schema + queries (rusqlite, bundled)
│  │  └─ commands.rs          # #[tauri::command] handlers (incl. fetch_link_preview)
│  ├─ icons/                  # app + menu-bar (tray.png) icons
│  ├─ tauri.conf.json
│  └─ capabilities/default.json
└─ package.json
```

## Build from source

Prerequisites: Rust 1.77+ (rustup), Node 18+, Xcode command line tools.

> A Homebrew `rust` may shadow rustup on `PATH` — ensure `~/.cargo/bin` comes first, or
> prefix commands with `PATH="$HOME/.cargo/bin:$PATH"`.

```bash
# run in development
PATH="$HOME/.cargo/bin:$PATH" npm install
PATH="$HOME/.cargo/bin:$PATH" npm run tauri dev

# build a universal distributable (.app + .dmg under src-tauri/target/.../bundle)
PATH="$HOME/.cargo/bin:$PATH" npm run tauri build -- --target universal-apple-darwin
```

### Cutting a release (maintainer)

To ship an update so installed copies upgrade themselves:

1. Bump `"version"` in `src-tauri/tauri.conf.json`.
2. Run `scripts/release.sh "what changed"`.

That builds a signed universal app, generates the updater manifest (`latest.json`), and
publishes a GitHub Release. Signing uses the private key at `~/.tauri/stash.key` — **keep it
safe and never commit it**; losing it means you can no longer push updates to existing installs.

## Implementation notes

- **rusqlite (bundled SQLite)** is used directly in Rust commands, so all DB logic lives in
  one place alongside the watcher and blob handling.
- Clipboard read/write uses **`arboard`** in Rust (watcher + `copy_to_clipboard`); save dialogs
  and link-opening are Rust commands (`download_item`, `open_url`).
- Link previews are fetched server-side in Rust (`reqwest` + rustls) to avoid CORS, parsing
  `og:image` / `og:title` / favicon from the page.
- The menu-bar icon is a monochrome template (`src-tauri/icons/tray.png`) so macOS tints it
  for light/dark menu bars.

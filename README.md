# Stash

A native macOS **menu-bar** app that is a clipboard manager *and* a notebook. Stash watches
the system clipboard in the background and automatically saves whatever you copy — text,
links, and images — into a searchable, taggable, pinnable, folder-able collection. You can
also jot notes, attach/drag-drop files, copy items back with one click, and download saved
blobs.

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

---

## Features

- **Background clipboard auto-capture** — a Rust thread polls the clipboard, hashes contents
  to dedupe, and saves new items automatically. No paste needed.
- **Link previews** — when you’re online, links fetch a thumbnail (`og:image`), page title,
  and favicon so your saved links look like cards, not raw URLs.
- **Menu-bar (tray) app** — left-click the backpack icon to toggle the window; right-click for
  Open / Quit. No dock icon (macOS accessory activation policy).
- **SQLite persistence** — items and folders live in `stash.db`; image/file blobs are written
  to disk under `blobs/` (paths, not base64, are stored in the DB).
- **Notes, links, images, files** — manual capture, pin, tag, folder-sort, and unified search
  across the whole collection.
- **Copy-back without duplicates** — copying an item from the app updates the watcher’s
  last-seen hash so it isn’t re-captured.

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

## Implementation notes

- **rusqlite (bundled SQLite)** is used directly in Rust commands, so all DB logic lives in
  one place alongside the watcher and blob handling.
- Clipboard read/write uses **`arboard`** in Rust (watcher + `copy_to_clipboard`); save dialogs
  and link-opening are Rust commands (`download_item`, `open_url`).
- Link previews are fetched server-side in Rust (`reqwest` + rustls) to avoid CORS, parsing
  `og:image` / `og:title` / favicon from the page.
- The menu-bar icon is a monochrome template (`src-tauri/icons/tray.png`) so macOS tints it
  for light/dark menu bars.

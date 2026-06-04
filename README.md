# Stash

Stash is a macOS menu-bar app for clipboard history, notes, links, images, files, and to-do
lists. It runs in the background and saves what you copy so you can find it again later.
Everything is stored locally in a SQLite database.

It's built with Tauri v2: a Rust backend with a plain HTML/CSS/JS frontend (the UI has no
build step).

## Download

1. Download `Stash_universal.dmg` from the [latest release](../../releases/latest).
2. Open it and drag Stash into your Applications folder.
3. The first time, right-click Stash and choose Open, then Open again. Gatekeeper warns
   because the app isn't signed with a paid Apple Developer account. If macOS says the app is
   "damaged", run this once and try again:
   ```bash
   xattr -cr /Applications/Stash.app
   ```

It's a universal binary, so it runs on both Apple Silicon and Intel Macs. Stash sits in the
menu bar (the backpack icon) and has no dock icon. Click the icon to show or hide the window.

## Updates

Stash checks for a newer version when it starts, and you can also check from the menu-bar icon
(Check for Updates). If there's one, it downloads the update, checks the signature against a
public key built into the app, installs it, and restarts, so you don't have to re-download the
dmg. Updates are published as GitHub Releases in this repo.

## Source

The whole app is in this repo. The frontend is in `src/` (`index.html`, `styles.css`,
`main.js`) and the Rust backend is in `src-tauri/src/`. See [Project layout](#project-layout)
for what each file does, or [Build from source](#build-from-source) to run it yourself.

## Features

**Capturing things**
- Watches the clipboard and saves new text, links, and images on its own. It hashes contents
  so the same thing isn't saved twice, and it skips its own copies.
- You can also paste (⌘V), type a note, or drag and drop / attach files. ⌘↵ saves.

**Keeping it organized**
- Folders. Drag any card onto a folder in the sidebar to move it there.
- Tags. Add as many as you want per item, and click a tag to filter everything by it.
- Pin items to keep them at the top.
- To-do lists (below).

**Finding things**
- Search runs across text, link URLs, file names, tags, and folder names, over the whole
  collection rather than just the current view.
- Sort by newest or oldest with the toggle in the header. Pinned items stay on top.
- Filter by type in the sidebar: All, Pinned, Notes, Links, Images, Files, plus per-folder and
  per-tag views.
- The grid is a masonry layout that rebalances as link previews load in.

**Links and images**
- Links pull an `og:image` thumbnail and the page title when you're online.
- Every link gets a generated SVG favicon (the site's initial on a colored tile), so it has an
  icon even when offline.
- Click an image to open it full size. From there you can copy it, download it, export it as
  PNG or JPEG, or delete it.

**To-do lists**
- Make as many lists as you want.
- Tasks can have a due date and time. Overdue dates show in red, and there are quick buttons
  for Today, Tomorrow, and Next week.
- Tasks can have notes and subtasks. Checking off a task checks off its subtasks.
- Incomplete tasks sort first, then by soonest due date. "Clear done" removes the finished
  ones, and the sidebar shows how many are left.

**Themes**
- Three themes: Paper (the warm default), Mono (black and white), and Party (bright colors).
  Your choice is remembered between launches.

**Under the hood**
- Menu-bar app. Left-click toggles the window; right-click has Open / Check for Updates / Quit.
- Stored locally in SQLite (WAL mode). Image and file blobs are written to disk and the
  database keeps the path, not the bytes. Nothing is uploaded anywhere, apart from fetching
  link previews from the sites you save.
- Shortcuts: `/` focuses search, ⌘V pastes and saves, ⌘↵ saves, Esc closes dialogs.

## Project layout

```
stash/
├─ src/                       # frontend (no bundler; served directly)
│  ├─ index.html              # UI markup
│  ├─ styles.css              # styles and the three themes
│  └─ main.js                 # UI logic + calls into the Rust commands
├─ src-tauri/
│  ├─ src/
│  │  ├─ lib.rs               # app setup, state, plugins, tray, updater, watcher
│  │  ├─ clipboard_watch.rs   # background clipboard polling thread (arboard + sha2)
│  │  ├─ db.rs                # SQLite schema + queries (rusqlite, bundled)
│  │  └─ commands.rs          # the #[tauri::command] handlers
│  ├─ icons/                  # app icon + menu-bar template (tray.png)
│  ├─ tauri.conf.json
│  └─ capabilities/default.json
├─ scripts/release.sh         # build, sign, and publish a release
└─ package.json
```

## Build from source

You'll need Rust 1.77+ (via rustup), Node 18+, and the Xcode command line tools.

If you have a Homebrew `rust` installed, it can shadow rustup on `PATH`. Put `~/.cargo/bin`
first, or prefix the commands with `PATH="$HOME/.cargo/bin:$PATH"`.

```bash
# run in development
npm install
npm run tauri dev

# build a universal .app + .dmg (output under src-tauri/target/.../bundle)
npm run tauri build -- --target universal-apple-darwin
```

## Releasing (maintainer)

To ship an update that installed copies pick up on their own:

1. Bump `"version"` in `src-tauri/tauri.conf.json`.
2. Run `scripts/release.sh "what changed"`.

It builds a signed universal app, writes the updater manifest (`latest.json`), and publishes a
GitHub Release. Signing uses the private key at `~/.tauri/stash.key`. Keep that key safe and
don't commit it; if you lose it you can't push updates to existing installs.

## How it works

- SQLite is accessed directly with rusqlite (bundled), so all the database code sits in one
  place next to the clipboard watcher and blob handling.
- Clipboard read/write goes through `arboard` in Rust. Save dialogs and opening links are also
  Rust commands (`download_item`, `open_url`).
- Link previews are fetched in Rust with `reqwest` (rustls) to avoid CORS, pulling `og:image`
  and the title out of the page. Favicons are generated client-side as SVG.
- The menu-bar icon is a monochrome template PNG, so macOS tints it for light and dark menu
  bars. Icons are rendered from SVG with `rsvg-convert` to keep the transparency.

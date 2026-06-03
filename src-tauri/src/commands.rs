// Tauri command handlers exposed to the frontend.
use crate::db::{self, Item, NewItem};
use crate::{clipboard_watch, AppState};
use base64::Engine;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

fn blobs_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("blobs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn ext_for(name: &Option<String>, mime: &Option<String>) -> String {
    if let Some(n) = name {
        if let Some(dot) = n.rfind('.') {
            let ext = &n[dot + 1..];
            if !ext.is_empty() && ext.len() <= 8 {
                return ext.to_lowercase();
            }
        }
    }
    match mime.as_deref().unwrap_or("") {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "application/pdf" => "pdf",
        "application/zip" => "zip",
        m if m.starts_with("text/") => "txt",
        _ => "bin",
    }
    .to_string()
}

/// Decode a `data:` URL, write the bytes to the blobs dir, return (abs path, byte length).
fn save_data_url(app: &AppHandle, data_url: &str, id: &str, ext: &str) -> Result<(String, usize), String> {
    let comma = data_url.find(',').ok_or("malformed data URL")?;
    let b64 = &data_url[comma + 1..];
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.as_bytes())
        .map_err(|e| e.to_string())?;
    let path = blobs_dir(app)?.join(format!("{id}.{ext}"));
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok((path.to_string_lossy().to_string(), bytes.len()))
}

#[tauri::command]
pub fn add_item(app: AppHandle, state: State<AppState>, item: NewItem) -> Result<Item, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = chrono::Utc::now().timestamp_millis();
    let mut content = item.content;
    let mut size = item.size;

    // Inline blobs (from paste/drag-drop) arrive as data URLs → write to disk, store the path.
    if content.starts_with("data:") {
        let ext = ext_for(&item.name, &item.mime);
        let (path, len) = save_data_url(&app, &content, &id, &ext)?;
        content = path;
        if size.is_none() {
            size = Some(len as i64);
        }
    }

    let it = Item {
        id,
        kind: item.kind,
        content,
        name: item.name,
        mime: item.mime,
        size,
        pinned: item.pinned.unwrap_or(false),
        folder: item.folder,
        tags: item.tags.unwrap_or_default(),
        ts,
    };
    let conn = state.db.lock().unwrap();
    db::insert(&conn, &it).map_err(|e| e.to_string())?;
    Ok(it)
}

#[tauri::command]
pub fn list_items(state: State<AppState>) -> Result<Vec<Item>, String> {
    let conn = state.db.lock().unwrap();
    db::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_item(
    state: State<AppState>,
    id: String,
    pinned: Option<bool>,
    tags: Option<Vec<String>>,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::update(&conn, &id, pinned, tags).map_err(|e| e.to_string())
}

/// Move an item into a folder (drag-and-drop), or out of any folder when `folder` is null.
#[tauri::command]
pub fn set_item_folder(state: State<AppState>, id: String, folder: Option<String>) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_folder(&conn, &id, folder.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_item(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    if let Some((kind, content)) = db::kind_content(&conn, &id).map_err(|e| e.to_string())? {
        db::delete(&conn, &id).map_err(|e| e.to_string())?;
        if kind == "image" || kind == "file" {
            let _ = std::fs::remove_file(&content);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn list_folders(state: State<AppState>) -> Result<Vec<String>, String> {
    let conn = state.db.lock().unwrap();
    db::list_folders(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_folder(state: State<AppState>, name: String) -> Result<(), String> {
    let ts = chrono::Utc::now().timestamp_millis();
    let conn = state.db.lock().unwrap();
    db::add_folder(&conn, &name, ts).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_folder(state: State<AppState>, name: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::delete_folder(&conn, &name).map_err(|e| e.to_string())
}

/// Write text to the system clipboard and record its hash so the background
/// watcher recognises our own write and does not re-capture it.
#[tauri::command]
pub fn copy_to_clipboard(state: State<AppState>, text: String) -> Result<(), String> {
    let h = clipboard_watch::hash(text.trim());
    let mut clip = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clip.set_text(text).map_err(|e| e.to_string())?;
    *state.last_hash.lock().unwrap() = Some(h);
    Ok(())
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Show a native save dialog and copy the item's blob to the chosen location.
/// Returns true if the user saved, false if they cancelled.
#[tauri::command]
pub fn download_item(app: AppHandle, state: State<AppState>, id: String) -> Result<bool, String> {
    let (name, src) = {
        let conn = state.db.lock().unwrap();
        match db::name_content(&conn, &id).map_err(|e| e.to_string())? {
            Some(v) => v,
            None => return Ok(false),
        }
    };
    use tauri_plugin_dialog::DialogExt;
    let mut builder = app.dialog().file();
    if let Some(n) = name {
        builder = builder.set_file_name(n);
    }
    match builder.blocking_save_file() {
        Some(dest) => {
            let dest_path = dest.into_path().map_err(|e| e.to_string())?;
            std::fs::copy(&src, &dest_path).map_err(|e| e.to_string())?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[derive(serde::Serialize, Default)]
pub struct LinkPreview {
    pub image: Option<String>,
    pub title: Option<String>,
    pub icon: Option<String>,
    pub host: Option<String>,
}

/// Pull the value of an HTML attribute (e.g. `content`) out of a single tag string.
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(name) {
        let i = from + rel;
        // Make sure this is the attribute name (preceded by space) and followed by '='.
        let before_ok = i == 0 || lower.as_bytes()[i - 1].is_ascii_whitespace();
        let after = lower[i + name.len()..].trim_start();
        if before_ok && after.starts_with('=') {
            let rest = lower[i + name.len()..].trim_start()[1..].trim_start();
            let q = rest.chars().next()?;
            let val_start_lower = lower.len() - rest.len() + 1;
            if q == '"' || q == '\'' {
                if let Some(end) = lower[val_start_lower..].find(q) {
                    return Some(tag[val_start_lower..val_start_lower + end].trim().to_string());
                }
            }
        }
        from = i + name.len();
    }
    None
}

/// Find a `<meta>` whose property/name matches `key` and return its `content`.
fn meta_content(html: &str, key: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<meta") {
        let start = from + rel;
        let end = lower[start..].find('>').map(|e| start + e + 1).unwrap_or(html.len());
        let tag = &html[start..end];
        let prop = attr(tag, "property").or_else(|| attr(tag, "name"));
        if prop.as_deref().map(|p| p.eq_ignore_ascii_case(key)).unwrap_or(false) {
            if let Some(c) = attr(tag, "content") {
                if !c.is_empty() {
                    return Some(c);
                }
            }
        }
        from = end;
    }
    None
}

fn first_match<'a>(html: &str, keys: &[&'a str]) -> Option<String> {
    keys.iter().find_map(|k| meta_content(html, k))
}

/// Locate a favicon href from <link rel="icon"> tags.
fn find_icon(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<link") {
        let start = from + rel;
        let end = lower[start..].find('>').map(|e| start + e + 1).unwrap_or(html.len());
        let tag = &html[start..end];
        if let Some(r) = attr(tag, "rel") {
            let r = r.to_lowercase();
            if r.contains("icon") {
                if let Some(href) = attr(tag, "href") {
                    return Some(href);
                }
            }
        }
        from = end;
    }
    None
}

/// Fetch a URL and extract a preview (og:image, title, favicon). Best-effort; never panics.
#[tauri::command]
pub async fn fetch_link_preview(url: String) -> Result<LinkPreview, String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&url)
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Accept-Encoding", "identity") // avoid needing decompression features
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let base = resp.url().clone();
    let host = base.host_str().map(|h| h.trim_start_matches("www.").to_string());

    // Only parse HTML; cap the body so a huge page can't hang us.
    let body = resp.text().await.unwrap_or_default();
    let body = if body.len() > 600_000 { &body[..600_000] } else { &body[..] };

    let resolve = |maybe: Option<String>| -> Option<String> {
        maybe.and_then(|h| base.join(&h).ok().map(|u| u.to_string()))
    };

    let image = resolve(first_match(body, &["og:image:secure_url", "og:image", "twitter:image", "twitter:image:src"]));
    let title = first_match(body, &["og:title", "twitter:title"]).or_else(|| {
        let l = body.to_lowercase();
        let s = l.find("<title")?;
        let s = l[s..].find('>')? + s + 1;
        let e = l[s..].find("</title>")? + s;
        Some(body[s..e].trim().to_string()).filter(|t| !t.is_empty())
    });
    let icon = resolve(find_icon(body)).or_else(|| base.join("/favicon.ico").ok().map(|u| u.to_string()));

    Ok(LinkPreview { image, title, icon, host })
}

fn item_path(state: &State<AppState>, id: &str) -> Result<(Option<String>, String), String> {
    let conn = state.db.lock().unwrap();
    db::name_content(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "item not found".to_string())
}

/// Copy an image item's pixels onto the system clipboard (so it can be pasted as an image).
#[tauri::command]
pub fn copy_image_to_clipboard(state: State<AppState>, id: String) -> Result<(), String> {
    let (_, path) = item_path(&state, &id)?;
    let img = image::open(&path).map_err(|e| e.to_string())?.to_rgba8();
    let (w, h) = img.dimensions();
    let bytes = img.into_raw();
    // Record the hash so the background watcher recognises our own write and won't re-capture it.
    *state.last_hash.lock().unwrap() = Some(clipboard_watch::hash_bytes(&bytes));
    let mut clip = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clip.set_image(arboard::ImageData {
        width: w as usize,
        height: h as usize,
        bytes: std::borrow::Cow::Owned(bytes),
    })
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Re-encode an image item to the requested format and save it via a native dialog.
/// Returns true if saved, false if cancelled.
#[tauri::command]
pub fn export_image_as(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    format: String,
) -> Result<bool, String> {
    let (name, path) = item_path(&state, &id)?;
    let img = image::open(&path).map_err(|e| e.to_string())?;
    let (fmt, ext) = match format.to_lowercase().as_str() {
        "png" => (image::ImageFormat::Png, "png"),
        "jpeg" | "jpg" => (image::ImageFormat::Jpeg, "jpg"),
        "gif" => (image::ImageFormat::Gif, "gif"),
        "bmp" => (image::ImageFormat::Bmp, "bmp"),
        other => return Err(format!("unsupported format: {other}")),
    };
    let base = name.unwrap_or_else(|| "image".into());
    let stem = base.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or(base);
    let default_name = format!("{stem}.{ext}");

    use tauri_plugin_dialog::DialogExt;
    match app.dialog().file().set_file_name(default_name).blocking_save_file() {
        Some(dest) => {
            let dest_path = dest.into_path().map_err(|e| e.to_string())?;
            // JPEG/BMP have no alpha channel — flatten to RGB first.
            let res = if matches!(fmt, image::ImageFormat::Jpeg | image::ImageFormat::Bmp) {
                img.to_rgb8().save_with_format(&dest_path, fmt)
            } else {
                img.save_with_format(&dest_path, fmt)
            };
            res.map_err(|e| e.to_string())?;
            Ok(true)
        }
        None => Ok(false),
    }
}

// ---------------- to-do lists ----------------

#[tauri::command]
pub fn list_todo_lists(state: State<AppState>) -> Result<Vec<db::TodoList>, String> {
    let conn = state.db.lock().unwrap();
    db::list_todo_lists(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_todo_list(state: State<AppState>, name: String) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = chrono::Utc::now().timestamp_millis();
    let conn = state.db.lock().unwrap();
    db::add_todo_list(&conn, &id, &name, ts).map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub fn rename_todo_list(state: State<AppState>, id: String, name: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::rename_todo_list(&conn, &id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_todo_list(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::delete_todo_list(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_todos(state: State<AppState>, list_id: String) -> Result<Vec<db::Todo>, String> {
    let conn = state.db.lock().unwrap();
    db::list_todos(&conn, &list_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_todo(state: State<AppState>, list_id: String, text: String) -> Result<(), String> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = chrono::Utc::now().timestamp_millis();
    let conn = state.db.lock().unwrap();
    db::add_todo(&conn, &id, &list_id, &text, ts).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_todo_done(state: State<AppState>, id: String, done: bool) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::set_todo_done(&conn, &id, done).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_todo(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::delete_todo(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_completed(state: State<AppState>, list_id: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::clear_completed(&conn, &list_id).map_err(|e| e.to_string())
}

/// Export the entire collection (items + folders) as a JSON value.
#[tauri::command]
pub fn export_data(state: State<AppState>) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().unwrap();
    let items = db::list(&conn).map_err(|e| e.to_string())?;
    let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "items": items, "folders": folders }))
}

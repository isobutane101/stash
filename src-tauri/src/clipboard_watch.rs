// Background clipboard watcher. Polls the system clipboard on a Rust thread and
// emits a `clipboard-captured` event whenever the contents change. Dedupe + self-write
// suppression are done via a shared SHA-256 hash of the last-seen content.
use arboard::Clipboard;
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};

/// Shared with the `copy_to_clipboard` command so the app's own writes are not re-captured.
pub type LastHash = Arc<Mutex<Option<String>>>;

#[derive(Clone, serde::Serialize)]
pub struct CapturedItem {
    #[serde(rename = "type")]
    pub kind: String, // "text" | "link" | "image"
    pub content: String, // text/URL, or an absolute blob path for images
    pub name: Option<String>,
    pub mime: Option<String>,
    pub size: Option<i64>,
}

pub fn spawn(app: AppHandle, last_hash: LastHash) {
    thread::spawn(move || {
        let mut clip = match Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("clipboard init failed: {e}");
                return;
            }
        };
        loop {
            // ---- TEXT (and links) ----
            if let Ok(text) = clip.get_text() {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    let h = hash(&trimmed);
                    let changed = {
                        let mut last = last_hash.lock().unwrap();
                        if last.as_deref() != Some(&h) {
                            *last = Some(h.clone());
                            true
                        } else {
                            false
                        }
                    };
                    if changed {
                        let kind = if is_url(&trimmed) { "link" } else { "text" };
                        let item = CapturedItem {
                            kind: kind.into(),
                            content: trimmed,
                            name: None,
                            mime: None,
                            size: None,
                        };
                        let _ = app.emit("clipboard-captured", item);
                    }
                }
            }
            // ---- IMAGE ----
            else if let Ok(img) = clip.get_image() {
                let h = hash_bytes(&img.bytes);
                let changed = {
                    let mut last = last_hash.lock().unwrap();
                    if last.as_deref() != Some(&h) {
                        *last = Some(h.clone());
                        true
                    } else {
                        false
                    }
                };
                if changed {
                    if let Some((path, size)) = save_clipboard_image(&app, &img) {
                        let item = CapturedItem {
                            kind: "image".into(),
                            content: path,
                            name: Some("clipboard-image.png".into()),
                            mime: Some("image/png".into()),
                            size: Some(size as i64),
                        };
                        let _ = app.emit("clipboard-captured", item);
                    }
                }
            }
            thread::sleep(Duration::from_millis(600));
        }
    });
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

pub fn hash(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    to_hex(&h.finalize())
}

fn hash_bytes(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    to_hex(&h.finalize())
}

fn is_url(s: &str) -> bool {
    let s = s.trim();
    (s.starts_with("http://") || s.starts_with("https://")) && !s.contains(char::is_whitespace)
}

/// Encode the RGBA clipboard image to PNG under app_data_dir/blobs and return (path, byte len).
fn save_clipboard_image(app: &AppHandle, img: &arboard::ImageData) -> Option<(String, usize)> {
    let dir = app.path().app_data_dir().ok()?.join("blobs");
    std::fs::create_dir_all(&dir).ok()?;
    let id = uuid::Uuid::new_v4().to_string();
    let path = dir.join(format!("{id}.png"));
    let buf = image::RgbaImage::from_raw(
        img.width as u32,
        img.height as u32,
        img.bytes.clone().into_owned(),
    )?;
    buf.save(&path).ok()?;
    let size = std::fs::metadata(&path).ok()?.len() as usize;
    Some((path.to_string_lossy().to_string(), size))
}

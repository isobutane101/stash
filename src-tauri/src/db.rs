// SQLite schema + queries for Stash. Blob bytes live on disk; the DB stores paths.
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Item {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub content: String,
    pub name: Option<String>,
    pub mime: Option<String>,
    pub size: Option<i64>,
    pub pinned: bool,
    pub folder: Option<String>,
    pub tags: Vec<String>,
    pub ts: i64,
}

/// Shape accepted from the frontend / watcher when creating an item.
#[derive(Deserialize, Debug)]
pub struct NewItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub content: String,
    pub name: Option<String>,
    pub mime: Option<String>,
    pub size: Option<i64>,
    pub pinned: Option<bool>,
    pub folder: Option<String>,
    pub tags: Option<Vec<String>>,
}

pub fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS items (
            id        TEXT PRIMARY KEY,
            kind      TEXT NOT NULL,
            content   TEXT NOT NULL,
            name      TEXT,
            mime      TEXT,
            size      INTEGER,
            pinned    INTEGER NOT NULL DEFAULT 0,
            folder    TEXT,
            tags      TEXT NOT NULL DEFAULT '[]',
            created   INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS folders (
            name    TEXT PRIMARY KEY,
            created INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_items_created ON items(created DESC);
        CREATE INDEX IF NOT EXISTS idx_items_kind    ON items(kind);",
    )
}

fn row_to_item(row: &rusqlite::Row) -> Result<Item> {
    let tags_json: String = row.get("tags")?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    Ok(Item {
        id: row.get("id")?,
        kind: row.get("kind")?,
        content: row.get("content")?,
        name: row.get("name")?,
        mime: row.get("mime")?,
        size: row.get("size")?,
        pinned: row.get::<_, i64>("pinned")? != 0,
        folder: row.get("folder")?,
        tags,
        ts: row.get("created")?,
    })
}

pub fn insert(conn: &Connection, it: &Item) -> Result<()> {
    let tags_json = serde_json::to_string(&it.tags).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO items (id, kind, content, name, mime, size, pinned, folder, tags, created)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            it.id, it.kind, it.content, it.name, it.mime, it.size,
            it.pinned as i64, it.folder, tags_json, it.ts
        ],
    )?;
    Ok(())
}

pub fn list(conn: &Connection) -> Result<Vec<Item>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, content, name, mime, size, pinned, folder, tags, created
         FROM items ORDER BY pinned DESC, created DESC",
    )?;
    let rows = stmt.query_map([], row_to_item)?;
    rows.collect()
}

/// Update pin state and/or tags. `None` fields are left unchanged.
pub fn update(
    conn: &Connection,
    id: &str,
    pinned: Option<bool>,
    tags: Option<Vec<String>>,
) -> Result<()> {
    if let Some(p) = pinned {
        conn.execute("UPDATE items SET pinned = ?1 WHERE id = ?2", params![p as i64, id])?;
    }
    if let Some(t) = tags {
        let tags_json = serde_json::to_string(&t).unwrap_or_else(|_| "[]".into());
        conn.execute("UPDATE items SET tags = ?1 WHERE id = ?2", params![tags_json, id])?;
    }
    Ok(())
}

/// Return (kind, content) for an item, used to clean up blobs on delete.
pub fn kind_content(conn: &Connection, id: &str) -> Result<Option<(String, String)>> {
    let mut stmt = conn.prepare("SELECT kind, content FROM items WHERE id = ?1")?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        Ok(Some((row.get(0)?, row.get(1)?)))
    } else {
        Ok(None)
    }
}

/// Return (name, content) for an item, used when downloading a blob to disk.
pub fn name_content(conn: &Connection, id: &str) -> Result<Option<(Option<String>, String)>> {
    let mut stmt = conn.prepare("SELECT name, content FROM items WHERE id = ?1")?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        Ok(Some((row.get(0)?, row.get(1)?)))
    } else {
        Ok(None)
    }
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM items WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_folders(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM folders ORDER BY created ASC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect()
}

pub fn add_folder(conn: &Connection, name: &str, ts: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO folders (name, created) VALUES (?1, ?2)",
        params![name, ts],
    )?;
    Ok(())
}

pub fn delete_folder(conn: &Connection, name: &str) -> Result<()> {
    conn.execute("DELETE FROM folders WHERE name = ?1", params![name])?;
    conn.execute("UPDATE items SET folder = NULL WHERE folder = ?1", params![name])?;
    Ok(())
}

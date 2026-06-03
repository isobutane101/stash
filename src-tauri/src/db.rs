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
        CREATE TABLE IF NOT EXISTS todo_lists (
            id      TEXT PRIMARY KEY,
            name    TEXT NOT NULL,
            created INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS todos (
            id       TEXT PRIMARY KEY,
            list_id  TEXT NOT NULL,
            text     TEXT NOT NULL,
            done     INTEGER NOT NULL DEFAULT 0,
            created  INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_items_created ON items(created DESC);
        CREATE INDEX IF NOT EXISTS idx_items_kind    ON items(kind);
        CREATE INDEX IF NOT EXISTS idx_todos_list    ON todos(list_id);",
    )
}

/// Move an item into a folder (`Some(name)`) or out of any folder (`None`).
pub fn set_folder(conn: &Connection, id: &str, folder: Option<&str>) -> Result<()> {
    conn.execute("UPDATE items SET folder = ?1 WHERE id = ?2", params![folder, id])?;
    Ok(())
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

// ---------------- to-do lists ----------------

#[derive(Serialize)]
pub struct TodoList {
    pub id: String,
    pub name: String,
    pub total: i64,
    pub done: i64,
}

#[derive(Serialize)]
pub struct Todo {
    pub id: String,
    pub list_id: String,
    pub text: String,
    pub done: bool,
    pub ts: i64,
}

pub fn list_todo_lists(conn: &Connection) -> Result<Vec<TodoList>> {
    let mut stmt = conn.prepare(
        "SELECT l.id, l.name,
                (SELECT COUNT(*) FROM todos t WHERE t.list_id = l.id) AS total,
                (SELECT COUNT(*) FROM todos t WHERE t.list_id = l.id AND t.done = 1) AS done
         FROM todo_lists l ORDER BY l.created ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(TodoList {
            id: r.get(0)?,
            name: r.get(1)?,
            total: r.get(2)?,
            done: r.get(3)?,
        })
    })?;
    rows.collect()
}

pub fn add_todo_list(conn: &Connection, id: &str, name: &str, ts: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO todo_lists (id, name, created) VALUES (?1, ?2, ?3)",
        params![id, name, ts],
    )?;
    Ok(())
}

pub fn rename_todo_list(conn: &Connection, id: &str, name: &str) -> Result<()> {
    conn.execute("UPDATE todo_lists SET name = ?1 WHERE id = ?2", params![name, id])?;
    Ok(())
}

pub fn delete_todo_list(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM todos WHERE list_id = ?1", params![id])?;
    conn.execute("DELETE FROM todo_lists WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_todos(conn: &Connection, list_id: &str) -> Result<Vec<Todo>> {
    let mut stmt = conn.prepare(
        "SELECT id, list_id, text, done, created FROM todos
         WHERE list_id = ?1 ORDER BY done ASC, created ASC",
    )?;
    let rows = stmt.query_map(params![list_id], |r| {
        Ok(Todo {
            id: r.get(0)?,
            list_id: r.get(1)?,
            text: r.get(2)?,
            done: r.get::<_, i64>(3)? != 0,
            ts: r.get(4)?,
        })
    })?;
    rows.collect()
}

pub fn add_todo(conn: &Connection, id: &str, list_id: &str, text: &str, ts: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO todos (id, list_id, text, done, created) VALUES (?1, ?2, ?3, 0, ?4)",
        params![id, list_id, text, ts],
    )?;
    Ok(())
}

pub fn set_todo_done(conn: &Connection, id: &str, done: bool) -> Result<()> {
    conn.execute("UPDATE todos SET done = ?1 WHERE id = ?2", params![done as i64, id])?;
    Ok(())
}

pub fn delete_todo(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM todos WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn clear_completed(conn: &Connection, list_id: &str) -> Result<()> {
    conn.execute("DELETE FROM todos WHERE list_id = ?1 AND done = 1", params![list_id])?;
    Ok(())
}

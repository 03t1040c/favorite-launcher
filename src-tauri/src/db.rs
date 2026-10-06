use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
#[cfg(windows)]
use std::iter;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use chrono::{DateTime, Local, TimeZone, Utc};
use once_cell::sync::Lazy;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Manager;
#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::{BOOL, RPC_E_CHANGED_MODE};
#[cfg(windows)]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IBindCtx, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
#[cfg(windows)]
use windows::Win32::UI::Shell::{
    FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName, FOF_NOCONFIRMATION,
    FOF_NOCONFIRMMKDIR, FOF_NOERRORUI, FOF_SILENT,
};

use crate::APP_HANDLE;

const DEFAULT_QUICK_KEYWORDS: [&str; 10] = [
    "Quick1", "Quick2", "Quick3", "Quick4", "Quick5", "Quick6", "Quick7", "Quick8", "Quick9",
    "Quick10",
];

const LEGACY_DEFAULT_QUICK_KEYWORDS: [&str; 10] = [
    "機種A", "機種B", "機種C", "機種D", "機種E", "機種F", "機種G", "機種H", "機種I", "機種J",
];

static EDGE_IMPORT_STATUS: Lazy<Mutex<EdgeImportStatus>> = Lazy::new(|| {
    Mutex::new(EdgeImportStatus {
        running: false,
        message: "まだEdge履歴を更新していません。".to_string(),
        last_outcome: None,
        last_updated_at: None,
        revision: 0,
        debug_log: Vec::new(),
    })
});
static FAVICON_CACHE: Lazy<Mutex<HashMap<String, Option<String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static FILE_ICON_CACHE: Lazy<Mutex<HashMap<String, Option<String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRecord {
    pub id: i64,
    pub service: String,
    pub title: String,
    pub site: Option<String>,
    pub url: String,
    pub first_seen: Option<String>,
    pub last_access: Option<String>,
    pub access_count: i64,
    pub is_favorite: i64,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub display_count: i64,
    pub quick_keywords: Vec<String>,
    pub blocked_words: Vec<String>,
    pub site_names: Vec<String>,
    #[serde(default = "default_true")]
    pub show_edge_favorites: bool,
    #[serde(default = "default_search_mode")]
    pub search_mode: String,
    #[serde(default = "default_window_size")]
    pub window_size: String,
    #[serde(default)]
    pub index_folders: Vec<String>,
    #[serde(default = "default_favorite_pane_count")]
    pub favorite_pane_count: i64,
    #[serde(default = "default_left_pane_percent")]
    pub left_pane_percent: i64,
    #[serde(default = "default_favorite_density")]
    pub favorite_density: String,
    #[serde(default = "default_favorite_tab_limit")]
    pub favorite_tab_limit: i64,
    #[serde(default = "default_heading_color")]
    pub heading_default_color: String,
    #[serde(default = "default_heading_palette_size")]
    pub heading_palette_size: i64,
    #[serde(default = "default_edge_sync_interval")]
    pub edge_sync_interval_minutes: i64,
    #[serde(default = "default_edge_sync_interval_seconds")]
    pub edge_sync_interval_seconds: i64,
    #[serde(default = "default_folder_sync_interval_seconds")]
    pub folder_sync_interval_seconds: i64,
}

fn default_search_mode() -> String {
    "web".to_string()
}
fn default_true() -> bool {
    true
}
fn default_window_size() -> String {
    "1100x760".to_string()
}
fn default_favorite_pane_count() -> i64 {
    5
}
fn default_left_pane_percent() -> i64 {
    50
}
fn default_favorite_density() -> String {
    "standard".to_string()
}
fn default_favorite_tab_limit() -> i64 {
    5
}
fn default_heading_color() -> String {
    "#dbeafe".to_string()
}
fn default_heading_palette_size() -> i64 {
    32
}
fn default_edge_sync_interval() -> i64 {
    5
}
fn default_edge_sync_interval_seconds() -> i64 {
    10
}
fn default_folder_sync_interval_seconds() -> i64 {
    1800
}

const DEFAULT_BLOCKED_WORDS: [&str; 4] = ["すべてのドキュメント", "不要語2", "不要語3", "不要語4"];

const DEFAULT_SITE_NAMES: [&str; 4] = [
    "IJS開発設計部 上林部門 機種フォルダ（商業産業系）",
    "サイト名2",
    "サイト名3",
    "サイト名4",
];

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            display_count: 0,
            quick_keywords: DEFAULT_QUICK_KEYWORDS
                .iter()
                .map(|value| value.to_string())
                .collect(),
            blocked_words: DEFAULT_BLOCKED_WORDS
                .iter()
                .map(|value| value.to_string())
                .collect(),
            site_names: DEFAULT_SITE_NAMES
                .iter()
                .map(|value| value.to_string())
                .collect(),
            show_edge_favorites: true,
            search_mode: default_search_mode(),
            window_size: default_window_size(),
            index_folders: Vec::new(),
            favorite_pane_count: 5,
            left_pane_percent: 50,
            favorite_density: default_favorite_density(),
            favorite_tab_limit: default_favorite_tab_limit(),
            heading_default_color: default_heading_color(),
            heading_palette_size: default_heading_palette_size(),
            edge_sync_interval_minutes: default_edge_sync_interval(),
            edge_sync_interval_seconds: default_edge_sync_interval_seconds(),
            folder_sync_interval_seconds: default_folder_sync_interval_seconds(),
        }
    }
}

impl AppSettings {
    fn sanitized(self) -> Self {
        let quick_keywords = sanitize_lines(self.quick_keywords, 50);
        let blocked_words = sanitize_lines(self.blocked_words, 200);
        let site_names = sanitize_lines(self.site_names, 100);

        Self {
            display_count: self.display_count.clamp(0, 20),
            quick_keywords: if quick_keywords.is_empty() {
                DEFAULT_QUICK_KEYWORDS
                    .iter()
                    .map(|value| value.to_string())
                    .collect()
            } else {
                quick_keywords
            },
            blocked_words,
            site_names,
            // Legacy DB column is retained for compatibility; the old UI option no longer exists.
            show_edge_favorites: true,
            search_mode: if self.search_mode == "folder" {
                "folder".to_string()
            } else {
                "web".to_string()
            },
            window_size: sanitize_window_size(&self.window_size),
            index_folders: sanitize_lines(self.index_folders, 10),
            favorite_pane_count: self.favorite_pane_count.clamp(1, 10),
            left_pane_percent: self.left_pane_percent.clamp(15, 75),
            favorite_density: match self.favorite_density.as_str() {
                "compact" => "compact",
                "comfortable" => "comfortable",
                _ => "standard",
            }
            .to_string(),
            favorite_tab_limit: self.favorite_tab_limit.clamp(1, 20),
            heading_default_color: if valid_color(&self.heading_default_color) {
                self.heading_default_color.to_ascii_lowercase()
            } else {
                default_heading_color()
            },
            heading_palette_size: match self.heading_palette_size {
                64 => 64,
                128 => 128,
                _ => 32,
            },
            edge_sync_interval_minutes: self.edge_sync_interval_minutes.clamp(1, 60),
            edge_sync_interval_seconds: self.edge_sync_interval_seconds.clamp(10, 3600),
            folder_sync_interval_seconds: self.folder_sync_interval_seconds.clamp(30, 86400),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteItem {
    pub id: i64,
    pub kind: String,
    pub history_id: Option<i64>,
    pub label: String,
    pub service: Option<String>,
    pub target: Option<String>,
    pub position: i64,
    pub pane: i64,
    pub color: String,
    pub opened_count: i64,
    pub last_opened_at: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteTab {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub position: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteColumn {
    pub id: i64,
    pub tab_id: i64,
    pub name: String,
    pub color: String,
    pub position: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppLogEntry {
    pub id: i64,
    pub timestamp: String,
    pub category: String,
    pub level: String,
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionConnectionStatus {
    pub connected: bool,
    pub last_contact: Option<String>,
}

pub fn extension_connection_status() -> Result<ExtensionConnectionStatus, Box<dyn Error>> {
    let path = extension_data_dir()?.join("edge-extension-last-contact.txt");
    if !path.exists() {
        return Ok(ExtensionConnectionStatus {
            connected: false,
            last_contact: None,
        });
    }
    let modified = fs::metadata(path)?.modified()?;
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    Ok(ExtensionConnectionStatus {
        connected: age < Duration::from_secs(120),
        last_contact: Some(
            DateTime::<Local>::from(modified)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        ),
    })
}

fn sanitize_window_size(value: &str) -> String {
    let Some((w, h)) = value.split_once('x') else {
        return default_window_size();
    };
    let (Ok(w), Ok(h)) = (w.parse::<i64>(), h.parse::<i64>()) else {
        return default_window_size();
    };
    format!("{}x{}", w.clamp(1100, 3840), h.clamp(760, 2160))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchRecord {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub parent: String,
    pub is_directory: bool,
    pub modified_at: Option<String>,
    pub is_favorite: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsResponse {
    pub settings: AppSettings,
    pub is_default: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EdgeImportDebugEntry {
    pub timestamp: String,
    pub stage: String,
    pub level: String,
    pub message: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EdgeImportStatus {
    pub running: bool,
    pub message: String,
    pub last_outcome: Option<String>,
    pub last_updated_at: Option<String>,
    pub revision: u64,
    pub debug_log: Vec<EdgeImportDebugEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeImportSummary {
    pub imported_url_count: usize,
    pub imported_profile_count: usize,
    pub fallback_used: bool,
    pub restarted_edge: bool,
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeRecord {
    pub title: String,
    pub url: String,
    pub last_visit: Option<String>,
}

#[derive(Clone)]
struct EdgeHistoryRow {
    title: String,
    url: String,
    last_visit_time: i64,
    visit_count: i64,
}

struct ImportedEdgeHistory {
    records: Vec<(String, EdgeHistoryRow)>,
    profile_count: usize,
}

struct EdgeProfileCandidate {
    profile_name: String,
    profile_dir: PathBuf,
    history_path: PathBuf,
}

struct UserDataMoveGuard {
    original_path: PathBuf,
    moved_path: PathBuf,
    temp_root: PathBuf,
    active: bool,
}

impl UserDataMoveGuard {
    fn moved_user_data_path(&self) -> &Path {
        &self.moved_path
    }

    fn restore(&mut self) -> Result<(), Box<dyn Error>> {
        if !self.active {
            return Ok(());
        }

        append_edge_import_debug(
            "User Data一時移動",
            "info",
            format!(
                "User Dataを元の場所へ復元します: {} -> {}",
                self.moved_path.display(),
                self.original_path.display()
            ),
        );

        if self.original_path.exists() {
            if self.original_path.read_dir()?.next().is_none() {
                fs::remove_dir(&self.original_path)?;
            } else {
                return Err(format!(
                    "Edge User Data restore failed because the original path already exists: {}",
                    self.original_path.display()
                )
                .into());
            }
        }

        move_directory_explorer_style(&self.moved_path, &self.original_path)?;
        self.active = false;

        if let Err(error) = fs::remove_dir_all(&self.temp_root) {
            eprintln!(
                "Failed to clean up temporary Edge directory {}: {}",
                self.temp_root.display(),
                error
            );
        }

        append_edge_import_debug(
            "User Data一時移動",
            "success",
            "User Dataの復元が完了しました。",
        );
        Ok(())
    }
}

impl Drop for UserDataMoveGuard {
    fn drop(&mut self) {
        if self.active {
            if let Err(error) = self.restore() {
                eprintln!(
                    "Failed to restore Edge User Data from {} to {}: {}",
                    self.moved_path.display(),
                    self.original_path.display(),
                    error
                );
            }
        }
    }
}

fn db_path() -> Result<PathBuf, Box<dyn Error>> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("FAVORITE_LAUNCHER_TEST_DIR") {
        let dir = PathBuf::from(path); fs::create_dir_all(&dir)?;
        return Ok(dir.join("search_launcher.db"));
    }
    let handle_opt = APP_HANDLE.lock().ok().and_then(|handle| handle.clone());

    if let Some(handle) = handle_opt {
        let data_dir = handle
            .path()
            .app_local_data_dir()
            .map_err(|error| format!("Failed to get app local data dir: {}", error))?;

        fs::create_dir_all(&data_dir)?;
        Ok(data_dir.join("search_launcher.db"))
    } else {
        Err("AppHandle not initialized".into())
    }
}

pub fn native_database_path() -> Result<PathBuf, Box<dyn Error>> { db_path() }

pub fn init_and_seed_db() -> Result<PathBuf, Box<dyn Error>> {
    let new_db_path = db_path()?;
    let old_db_path = PathBuf::from("search_launcher.db");

    if old_db_path.exists() && !new_db_path.exists() {
        println!(
            "Migrating database from {:?} to {:?}",
            old_db_path, new_db_path
        );
        fs::copy(&old_db_path, &new_db_path)?;
        println!("Database migration completed successfully");
    }

    if let Some(parent) = new_db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(&new_db_path)?;
    conn.execute_batch(
        "BEGIN;
        CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            service TEXT NOT NULL,
            title TEXT NOT NULL,
            site TEXT,
            url TEXT NOT NULL,
            first_seen TEXT,
            last_access TEXT,
            access_count INTEGER DEFAULT 0,
            is_favorite INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS models (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            display_order INTEGER DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            display_count INTEGER NOT NULL,
            quick_keywords TEXT NOT NULL,
            blocked_words TEXT NOT NULL,
            site_names TEXT NOT NULL,
            show_edge_favorites INTEGER NOT NULL DEFAULT 1,
            search_mode TEXT NOT NULL DEFAULT 'web',
            window_size TEXT NOT NULL DEFAULT '1100x760',
            index_folders TEXT NOT NULL DEFAULT '[]',
            favorite_pane_count INTEGER NOT NULL DEFAULT 1,
            left_pane_percent INTEGER NOT NULL DEFAULT 50,
            favorite_density TEXT NOT NULL DEFAULT 'standard',
            favorite_tab_limit INTEGER NOT NULL DEFAULT 5,
            edge_sync_interval_minutes INTEGER NOT NULL DEFAULT 5,
            edge_sync_interval_seconds INTEGER NOT NULL DEFAULT 30,
            folder_sync_interval_seconds INTEGER NOT NULL DEFAULT 1800
        );

        CREATE TABLE IF NOT EXISTS edge_history_source (
            profile_name TEXT NOT NULL,
            url TEXT NOT NULL,
            title TEXT NOT NULL,
            last_visit_time INTEGER NOT NULL,
            visit_count INTEGER NOT NULL,
            last_visit_text TEXT,
            PRIMARY KEY (profile_name, url)
        );

        CREATE TABLE IF NOT EXISTS app_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS favorite_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL CHECK (kind IN ('link', 'heading')),
            history_id INTEGER,
            label TEXT NOT NULL DEFAULT '',
            position INTEGER NOT NULL DEFAULT 0,
            pane INTEGER NOT NULL DEFAULT 0,
            color TEXT NOT NULL DEFAULT '#e8eef7',
            target TEXT,
            service TEXT,
            FOREIGN KEY(history_id) REFERENCES history(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS file_index (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            path TEXT NOT NULL UNIQUE,
            parent TEXT NOT NULL,
            is_directory INTEGER NOT NULL DEFAULT 0,
            modified_at TEXT
            ,scan_token TEXT
        );

        CREATE TABLE IF NOT EXISTS app_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            category TEXT NOT NULL,
            level TEXT NOT NULL,
            message TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS web_icon_cache (
            page_url TEXT PRIMARY KEY,
            icon_data TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS favorite_tabs (
            id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, color TEXT NOT NULL DEFAULT '#e2e8f0', position INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS favorite_columns (
            id INTEGER PRIMARY KEY AUTOINCREMENT, tab_id INTEGER NOT NULL, name TEXT NOT NULL, color TEXT NOT NULL DEFAULT '#f8fafc', position INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(tab_id) REFERENCES favorite_tabs(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_history_url ON history(url);
        CREATE INDEX IF NOT EXISTS idx_edge_history_source_url ON edge_history_source(url);
        CREATE INDEX IF NOT EXISTS idx_file_index_name ON file_index(name);
        COMMIT;",
    )?;

    let has_is_favorite = conn
        .prepare("PRAGMA table_info(history)")
        .and_then(|mut stmt| {
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            for column_name in rows {
                if column_name? == "is_favorite" {
                    return Ok(true);
                }
            }
            Ok(false)
        })
        .unwrap_or(false);

    if !has_is_favorite {
        conn.execute(
            "ALTER TABLE history ADD COLUMN is_favorite INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }

    let has_show_edge_favorites = conn
        .prepare("PRAGMA table_info(settings)")
        .and_then(|mut stmt| {
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            for column_name in rows {
                if column_name? == "show_edge_favorites" {
                    return Ok(true);
                }
            }
            Ok(false)
        })
        .unwrap_or(false);
    if !has_show_edge_favorites {
        conn.execute(
            "ALTER TABLE settings ADD COLUMN show_edge_favorites INTEGER NOT NULL DEFAULT 1",
            [],
        )?;
    }

    for (column, definition) in [
        ("search_mode", "TEXT NOT NULL DEFAULT 'web'"),
        ("window_size", "TEXT NOT NULL DEFAULT '1100x760'"),
        ("index_folders", "TEXT NOT NULL DEFAULT '[]'"),
        ("favorite_pane_count", "INTEGER NOT NULL DEFAULT 1"),
        ("left_pane_percent", "INTEGER NOT NULL DEFAULT 50"),
        ("favorite_density", "TEXT NOT NULL DEFAULT 'standard'"),
        ("favorite_tab_limit", "INTEGER NOT NULL DEFAULT 5"),
        ("edge_sync_interval_minutes", "INTEGER NOT NULL DEFAULT 5"),
        ("edge_sync_interval_seconds", "INTEGER NOT NULL DEFAULT 30"),
        (
            "folder_sync_interval_seconds",
            "INTEGER NOT NULL DEFAULT 1800",
        ),
    ] {
        let exists = conn
            .prepare("PRAGMA table_info(settings)")
            .and_then(|mut stmt| {
                let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
                for name in rows {
                    if name? == column {
                        return Ok(true);
                    }
                }
                Ok(false)
            })
            .unwrap_or(false);
        if !exists {
            conn.execute(
                &format!("ALTER TABLE settings ADD COLUMN {} {}", column, definition),
                [],
            )?;
        }
    }
    for (column, definition) in [
        ("pane", "INTEGER NOT NULL DEFAULT 0"),
        ("color", "TEXT NOT NULL DEFAULT '#e8eef7'"),
    ] {
        let exists = conn
            .prepare("PRAGMA table_info(favorite_items)")
            .and_then(|mut stmt| {
                let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
                for name in rows {
                    if name? == column {
                        return Ok(true);
                    }
                }
                Ok(false)
            })
            .unwrap_or(false);
        if !exists {
            conn.execute(
                &format!(
                    "ALTER TABLE favorite_items ADD COLUMN {} {}",
                    column, definition
                ),
                [],
            )?;
        }
    }
    for (column, definition) in [("target", "TEXT"), ("service", "TEXT")] {
        let exists = conn
            .prepare("PRAGMA table_info(favorite_items)")
            .and_then(|mut stmt| {
                let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
                for name in rows {
                    if name? == column {
                        return Ok(true);
                    }
                }
                Ok(false)
            })
            .unwrap_or(false);
        if !exists {
            conn.execute(
                &format!(
                    "ALTER TABLE favorite_items ADD COLUMN {} {}",
                    column, definition
                ),
                [],
            )?;
        }
    }
    for (column, definition) in [
        ("opened_count", "INTEGER NOT NULL DEFAULT 0"),
        ("last_opened_at", "TEXT"),
        ("deleted_at", "TEXT"),
    ] {
        let exists = conn
            .prepare("PRAGMA table_info(favorite_items)")
            .and_then(|mut stmt| {
                let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
                for name in rows {
                    if name? == column {
                        return Ok(true);
                    }
                }
                Ok(false)
            })
            .unwrap_or(false);
        if !exists {
            conn.execute(
                &format!(
                    "ALTER TABLE favorite_items ADD COLUMN {} {}",
                    column, definition
                ),
                [],
            )?;
        }
    }
    conn.execute("INSERT OR IGNORE INTO favorite_tabs(id, name, color, position) VALUES(1, 'お気に入り', '#dbeafe', 0)", [])?;
    let schema_version: i64 = conn.query_row("SELECT COALESCE((SELECT CAST(value AS INTEGER) FROM app_meta WHERE key='schema_version'),0)", [], |r| r.get(0))?;
    if schema_version < 2 {
        conn.execute("UPDATE favorite_items SET label=COALESCE(NULLIF(label,''),(SELECT title FROM history WHERE id=favorite_items.history_id),label),target=COALESCE(target,(SELECT url FROM history WHERE id=favorite_items.history_id)),service=COALESCE(service,(SELECT service FROM history WHERE id=favorite_items.history_id)),history_id=NULL WHERE history_id IS NOT NULL", [])?;
        conn.execute("INSERT INTO app_meta(key,value) VALUES('schema_version','2') ON CONFLICT(key) DO UPDATE SET value='2'", [])?;
    }
    // Convert the legacy pane numbers only once. After favorite_columns exists, `pane`
    // contains column IDs; treating its maximum as a count would recreate deleted IDs
    // and add many empty columns on every install/startup.
    let column_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM favorite_columns", [], |r| r.get(0))?;
    if column_count == 0 {
        let existing_max: i64 = conn
            .query_row(
                "SELECT MAX(COALESCE(pane, 0)) FROM favorite_items",
                [],
                |r| r.get::<_, Option<i64>>(0),
            )
            .unwrap_or(Some(1))
            .unwrap_or(1)
            .max(1);
        for id in 1..=existing_max {
            conn.execute("INSERT OR IGNORE INTO favorite_columns(id, tab_id, name, position) VALUES(?1, 1, ?2, ?3)", params![id, format!("列{}", id), id - 1])?;
        }
    }
    let has_scan_token = conn
        .prepare("PRAGMA table_info(file_index)")
        .and_then(|mut stmt| {
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            for name in rows {
                if name? == "scan_token" {
                    return Ok(true);
                }
            }
            Ok(false)
        })
        .unwrap_or(false);
    if !has_scan_token {
        conn.execute("ALTER TABLE file_index ADD COLUMN scan_token TEXT", [])?;
    }

    conn.execute(
        "INSERT INTO favorite_items (kind, history_id, label, target, service, position)
         SELECT 'link', NULL, title, url, service, (SELECT COALESCE(MAX(position), -1) + 1 FROM favorite_items)
         FROM history h
         WHERE h.is_favorite = 1
           AND NOT EXISTS (SELECT 1 FROM favorite_items f WHERE f.kind='link' AND lower(f.target)=lower(h.url) AND f.deleted_at IS NULL)",
        [],
    )?;

    ensure_default_settings(&conn)?;
    conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_favorite_target_active ON favorite_items(lower(target)) WHERE deleted_at IS NULL;
        CREATE INDEX IF NOT EXISTS idx_favorite_file_active ON favorite_items(target) WHERE deleted_at IS NULL;
        CREATE INDEX IF NOT EXISTS idx_history_latest_url ON history(url,COALESCE(last_access,'') DESC,id DESC);")?;
    migrate_legacy_default_quick_keywords(&conn)?;
    seed_models(&conn)?;
    cleanup_legacy_seeded_history_once(&conn)?;
    normalize_history_services(&conn)?;

    Ok(new_db_path)
}

pub fn get_settings() -> Result<AppSettingsResponse, Box<dyn Error>> {
    let path = db_path()?;
    let conn = Connection::open(path)?;
    let settings = read_settings(&conn)?.unwrap_or_default();
    let defaults = AppSettings::default();
    let is_default = settings.quick_keywords == defaults.quick_keywords
        && settings.blocked_words == defaults.blocked_words
        && settings.site_names == defaults.site_names
        && settings.display_count == defaults.display_count
        && settings.show_edge_favorites == defaults.show_edge_favorites
        && settings.search_mode == defaults.search_mode
        && settings.window_size == defaults.window_size
        && settings.index_folders == defaults.index_folders
        && settings.favorite_pane_count == defaults.favorite_pane_count
        && settings.left_pane_percent == defaults.left_pane_percent
        && settings.favorite_density == defaults.favorite_density
        && settings.favorite_tab_limit == defaults.favorite_tab_limit
        && settings.heading_default_color == defaults.heading_default_color
        && settings.heading_palette_size == defaults.heading_palette_size
        && settings.edge_sync_interval_minutes == defaults.edge_sync_interval_minutes
        && settings.edge_sync_interval_seconds == defaults.edge_sync_interval_seconds
        && settings.folder_sync_interval_seconds == defaults.folder_sync_interval_seconds;

    Ok(AppSettingsResponse {
        settings,
        is_default,
    })
}

pub fn save_settings(settings: AppSettings) -> Result<AppSettings, Box<dyn Error>> {
    let path = db_path()?;
    let conn = Connection::open(path)?;
    let sanitized = settings.sanitized();

    conn.execute(
        "INSERT INTO settings (id, display_count, quick_keywords, blocked_words, site_names, show_edge_favorites, search_mode, window_size, index_folders, favorite_pane_count, left_pane_percent, favorite_density, favorite_tab_limit, edge_sync_interval_minutes, edge_sync_interval_seconds, folder_sync_interval_seconds)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET
             display_count = excluded.display_count,
             quick_keywords = excluded.quick_keywords,
             blocked_words = excluded.blocked_words,
             site_names = excluded.site_names,
             show_edge_favorites = excluded.show_edge_favorites,
             search_mode = excluded.search_mode,
             window_size = excluded.window_size,
             index_folders = excluded.index_folders,
             favorite_pane_count = excluded.favorite_pane_count,
             left_pane_percent = excluded.left_pane_percent,
             favorite_density = excluded.favorite_density,
             favorite_tab_limit = excluded.favorite_tab_limit,
             edge_sync_interval_minutes = excluded.edge_sync_interval_minutes,
             edge_sync_interval_seconds = excluded.edge_sync_interval_seconds,
             folder_sync_interval_seconds = excluded.folder_sync_interval_seconds",
        params![
            sanitized.display_count,
            serde_json::to_string(&sanitized.quick_keywords)?,
            serde_json::to_string(&sanitized.blocked_words)?,
            serde_json::to_string(&sanitized.site_names)?,
            if sanitized.show_edge_favorites { 1 } else { 0 },
            sanitized.search_mode,
            sanitized.window_size,
            serde_json::to_string(&sanitized.index_folders)?,
            sanitized.favorite_pane_count,
            sanitized.left_pane_percent,
            sanitized.favorite_density,
            sanitized.favorite_tab_limit,
            sanitized.edge_sync_interval_minutes,
            sanitized.edge_sync_interval_seconds,
            sanitized.folder_sync_interval_seconds,
        ],
    )?;
    conn.execute(
        "INSERT INTO app_meta(key,value) VALUES('heading_default_color',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![sanitized.heading_default_color],
    )?;
    conn.execute(
        "INSERT INTO app_meta(key,value) VALUES('heading_palette_size',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![sanitized.heading_palette_size.to_string()],
    )?;
    if let Ok(dir) = extension_data_dir() {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(
            dir.join("edge-extension-config.json"),
            serde_json::to_vec(
                &serde_json::json!({"syncIntervalSeconds":sanitized.edge_sync_interval_seconds}),
            )?,
        );
    }

    Ok(sanitized)
}

pub fn clear_edge_history() -> Result<(), Box<dyn Error>> {
    let path = db_path()?;
    let mut conn = Connection::open(path)?;
    let tx = conn.transaction()?;
    // Favorites must be independent of the disposable history cache. Materialize
    // their display data before detaching and deleting the source history rows.
    tx.execute(
        "UPDATE favorite_items
         SET label=COALESCE(NULLIF(label,''),(SELECT title FROM history WHERE id=favorite_items.history_id),''),
             target=COALESCE(target,(SELECT url FROM history WHERE id=favorite_items.history_id)),
             service=COALESCE(service,(SELECT service FROM history WHERE id=favorite_items.history_id)),
             history_id=NULL
         WHERE history_id IS NOT NULL",
        [],
    )?;
    tx.execute("DELETE FROM history", [])?;
    tx.execute("DELETE FROM edge_history_source", [])?;
    tx.commit()?;
    request_extension_full_sync()?;
    Ok(())
}

pub fn request_extension_full_sync() -> Result<(), Box<dyn Error>> {
    let dir = extension_data_dir()?;
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("edge-extension-full-sync.request"), b"1")?;
    append_app_log(
        "Edge拡張連携",
        "info",
        "全件受信を要求しました。拡張機能の次回通信後に取り込まれます。",
    )?;
    Ok(())
}

pub fn get_edge_import_status() -> EdgeImportStatus {
    EDGE_IMPORT_STATUS
        .lock()
        .map(|state| state.clone())
        .unwrap_or_else(|_| EdgeImportStatus {
            running: false,
            message: "Edge履歴更新状態の取得に失敗しました。".to_string(),
            last_outcome: Some("failure".to_string()),
            last_updated_at: Some(now_string()),
            revision: 0,
            debug_log: Vec::new(),
        })
}

pub fn refresh_edge_history() -> Result<EdgeImportSummary, Box<dyn Error>> {
    refresh_edge_history_internal("手動")
}

fn refresh_edge_history_internal(trigger: &str) -> Result<EdgeImportSummary, Box<dyn Error>> {
    begin_edge_import(trigger)?;

    let import_result = perform_edge_history_refresh(trigger);
    match import_result {
        Ok(summary) => {
            finish_edge_import(Some("success"), summary.message.clone());
            Ok(summary)
        }
        Err(error) => {
            let message = format!("Edge履歴更新に失敗しました: {}", error);
            finish_edge_import(Some("failure"), message);
            Err(error)
        }
    }
}

fn begin_edge_import(trigger: &str) -> Result<(), Box<dyn Error>> {
    let mut state = EDGE_IMPORT_STATUS
        .lock()
        .map_err(|_| "Failed to acquire Edge import state lock")?;

    if state.running {
        return Err("Edge履歴更新はすでに実行中です。".into());
    }

    state.running = true;
    state.message = format!("{}のEdge履歴を更新しています…", trigger);
    state.last_updated_at = Some(now_string());
    state.debug_log.clear();
    drop(state);
    append_edge_import_debug(
        "開始",
        "info",
        format!("{}のEdge履歴更新を開始しました。", trigger),
    );
    Ok(())
}

fn finish_edge_import(outcome: Option<&str>, message: String) {
    if let Ok(mut state) = EDGE_IMPORT_STATUS.lock() {
        state.running = false;
        state.message = message;
        state.last_outcome = outcome.map(|value| value.to_string());
        state.last_updated_at = Some(now_string());
        state.revision = state.revision.saturating_add(1);
    }
}

fn update_edge_import_message(message: impl Into<String>) {
    if let Ok(mut state) = EDGE_IMPORT_STATUS.lock() {
        state.message = message.into();
        state.last_updated_at = Some(now_string());
    }
}

fn append_edge_import_debug(
    stage: impl Into<String>,
    level: impl Into<String>,
    message: impl Into<String>,
) {
    let stage = stage.into();
    let level = level.into();
    let message = message.into();

    eprintln!("[edge-import][{}][{}] {}", stage, level, message);
    let _ = append_app_log(&format!("Edge履歴/{}", stage), &level, &message);

    if let Ok(mut state) = EDGE_IMPORT_STATUS.lock() {
        state.debug_log.push(EdgeImportDebugEntry {
            timestamp: now_string(),
            stage,
            level,
            message,
        });
        if state.debug_log.len() > 200 {
            let overflow = state.debug_log.len() - 200;
            state.debug_log.drain(0..overflow);
        }
        state.last_updated_at = Some(now_string());
    }
}

fn perform_edge_history_refresh(trigger: &str) -> Result<EdgeImportSummary, Box<dyn Error>> {
    let mut summary = import_edge_history_with_user_selected_file()?;
    summary.restarted_edge = false;
    summary.fallback_used = false;
    summary.message = format!("{}。{}", summary.message, trigger_summary(trigger));
    append_edge_import_debug("完了", "success", summary.message.clone());
    Ok(summary)
}

fn import_edge_history_with_user_selected_file() -> Result<EdgeImportSummary, Box<dyn Error>> {
    append_edge_import_debug(
        "User Dataアクセス",
        "info",
        "Edge User Dataフォルダを解決します。",
    );
    let user_data_dir = match edge_user_data_dir() {
        Ok(path) => {
            append_edge_import_debug(
                "User Dataアクセス",
                "success",
                format!("Edge User Dataを確認しました: {}", path.display()),
            );
            path
        }
        Err(error) => {
            append_edge_import_debug(
                "User Dataアクセス",
                "error",
                format!("Edge User Dataアクセス失敗: {}", error),
            );
            return Err(error);
        }
    };

    update_edge_import_message("Edgeプロファイルを確認しています…".to_string());
    let profiles = enumerate_edge_profiles(&user_data_dir)?;
    append_edge_import_debug(
        "Profile列挙",
        "success",
        format!(
            "対象プロファイルを{}件検出しました: {}",
            profiles.len(),
            profiles.join(", ")
        ),
    );
    let candidate = choose_candidate_profile(&user_data_dir, &profiles)?;
    append_edge_import_debug(
        "Profile候補決定",
        "info",
        format!("候補Historyパス: {}", candidate.history_path.display()),
    );

    update_edge_import_message("候補Historyを直接読み取っています…".to_string());
    let (rows, profile_name) = match read_edge_history_rows_from_path(&candidate.history_path) {
        Ok(rows) => {
            append_edge_import_debug(
                "History直接読込",
                "success",
                format!(
                    "候補Historyの直接読込に成功しました: {}",
                    candidate.history_path.display()
                ),
            );
            (rows, candidate.profile_name.clone())
        }
        Err(error) => {
            append_edge_import_debug(
                "History直接読込",
                "error",
                format!("候補Historyの直接読込に失敗しました: {}", error),
            );
            update_edge_import_message(
                "直接読込に失敗したため、ExplorerでHistoryを選択してください…".to_string(),
            );
            open_profile_folder_in_explorer(&candidate.profile_dir)?;

            update_edge_import_message("Historyファイルを選択してください…".to_string());
            let selected_history_path = pick_history_file(&candidate.profile_dir)?;
            append_edge_import_debug(
                "History選択",
                "success",
                format!(
                    "ユーザーが選択したHistory: {}",
                    selected_history_path.display()
                ),
            );
            let rows = read_edge_history_rows_from_path(&selected_history_path).map_err(|selected_error| {
                format!(
                    "{}\n回避策:\n1) Edgeを完全終了してから再試行\n2) Historyを別の場所へコピーしてから、そのコピーを選択",
                    selected_error
                )
            })?;
            let profile_name = selected_history_path
                .parent()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().to_string())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| candidate.profile_name.clone());
            (rows, profile_name)
        }
    };

    let records = rows
        .into_iter()
        .filter(|row| !row.url.trim().is_empty())
        .map(|row| (profile_name.clone(), row))
        .collect::<Vec<_>>();

    let url_count = persist_imported_edge_history(records)?;
    Ok(EdgeImportSummary {
        imported_url_count: url_count,
        imported_profile_count: 1,
        fallback_used: false,
        restarted_edge: false,
        message: format!(
            "Edge履歴を更新しました。{}件のURLを取り込みました（選択元: {}）",
            url_count, profile_name
        ),
    })
}

fn collect_edge_history_from_user_data(base: &Path) -> Result<ImportedEdgeHistory, Box<dyn Error>> {
    append_edge_import_debug(
        "Profile列挙",
        "info",
        format!("Edgeプロファイル列挙を開始します: {}", base.display()),
    );
    let profiles = enumerate_edge_profiles(base)?;
    append_edge_import_debug(
        "Profile列挙",
        "success",
        format!(
            "対象プロファイルを{}件検出しました: {}",
            profiles.len(),
            profiles.join(", ")
        ),
    );
    let mut records = Vec::new();

    for profile in profiles.iter() {
        update_edge_import_message(format!("Edge履歴を読込中: {}", profile));
        let history_path = base.join(profile).join("History");
        append_edge_import_debug(
            "History存在確認",
            "info",
            format!(
                "Historyファイルを確認します: {} ({})",
                profile,
                history_path.display()
            ),
        );
        if !history_path.exists() {
            append_edge_import_debug(
                "History存在確認",
                "error",
                format!("Historyファイルが存在しません: {}", history_path.display()),
            );
            continue;
        }
        append_edge_import_debug(
            "History存在確認",
            "success",
            format!("Historyファイルを確認しました: {}", history_path.display()),
        );

        let rows = read_edge_history_rows_from_path(&history_path)?;
        for row in rows {
            if row.url.trim().is_empty() {
                continue;
            }
            records.push((profile.clone(), row));
        }
    }

    Ok(ImportedEdgeHistory {
        records,
        profile_count: profiles.len(),
    })
}

fn enumerate_edge_profiles(base: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut profiles = Vec::new();
    let mut seen = HashSet::new();

    let local_state_path = base.join("Local State");
    if local_state_path.exists() {
        append_edge_import_debug(
            "Profile列挙",
            "info",
            format!("Local Stateを読み込みます: {}", local_state_path.display()),
        );
        let content = match fs::read_to_string(&local_state_path) {
            Ok(content) => content,
            Err(error) => {
                append_edge_import_debug(
                    "Profile列挙",
                    "error",
                    format!("Local State読込失敗: {}", error),
                );
                return Err(error.into());
            }
        };
        let parsed: Value = match serde_json::from_str(&content) {
            Ok(parsed) => parsed,
            Err(error) => {
                append_edge_import_debug(
                    "Profile列挙",
                    "error",
                    format!("Local State JSON解析失敗: {}", error),
                );
                return Err(error.into());
            }
        };

        if let Some(info_cache) = parsed
            .get("profile")
            .and_then(|profile| profile.get("info_cache"))
            .and_then(|value| value.as_object())
        {
            for (profile_name, metadata) in info_cache {
                if should_include_profile(profile_name, metadata)
                    && base.join(profile_name).join("History").exists()
                    && seen.insert(profile_name.clone())
                {
                    profiles.push(profile_name.clone());
                }
            }
        }
    } else {
        append_edge_import_debug(
            "Profile列挙",
            "info",
            format!(
                "Local Stateが見つからないためディレクトリ走査へフォールバックします: {}",
                local_state_path.display()
            ),
        );
    }

    append_edge_import_debug(
        "Profile列挙",
        "info",
        format!("User Data配下を走査します: {}", base.display()),
    );
    let entries = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(error) => {
            append_edge_import_debug(
                "User Dataアクセス",
                "error",
                format!("User Dataディレクトリ走査失敗: {}", error),
            );
            return Err(error.into());
        }
    };
    for entry in entries {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }

        let profile_name = entry.file_name().to_string_lossy().to_string();
        if is_profile_directory_name(&profile_name)
            && profile_name != "Guest Profile"
            && entry.path().join("History").exists()
            && seen.insert(profile_name.clone())
        {
            profiles.push(profile_name);
        }
    }

    if profiles.is_empty() {
        append_edge_import_debug(
            "Profile列挙",
            "error",
            "対象プロファイルが見つかりませんでした。",
        );
        return Err("Edgeの対象プロファイルが見つかりませんでした。".into());
    }

    profiles.sort();
    Ok(profiles)
}

fn choose_candidate_profile(
    base: &Path,
    profiles: &[String],
) -> Result<EdgeProfileCandidate, Box<dyn Error>> {
    if profiles.is_empty() {
        return Err("Edgeの対象プロファイルが見つかりませんでした。".into());
    }

    if profiles.len() == 1 {
        let profile_name = profiles[0].clone();
        let profile_dir = base.join(&profile_name);
        let history_path = profile_dir.join("History");
        append_edge_import_debug(
            "Profile候補決定",
            "success",
            format!("プロファイルが1件のため候補を確定: {}", profile_name),
        );
        return Ok(EdgeProfileCandidate {
            profile_name,
            profile_dir,
            history_path,
        });
    }

    let mut best: Option<(String, PathBuf, PathBuf, SystemTime)> = None;
    for profile_name in profiles {
        let profile_dir = base.join(profile_name);
        let history_path = profile_dir.join("History");
        let modified = fs::metadata(&history_path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        append_edge_import_debug(
            "Profile候補決定",
            "info",
            format!(
                "候補評価: {} / History更新時刻={}",
                profile_name,
                DateTime::<Local>::from(modified).format("%Y-%m-%d %H:%M:%S")
            ),
        );
        match &best {
            Some((_, _, _, current_best_time)) if modified <= *current_best_time => {}
            _ => {
                best = Some((profile_name.clone(), profile_dir, history_path, modified));
            }
        }
    }

    let (profile_name, profile_dir, history_path, modified) =
        best.ok_or("候補プロファイルを決定できませんでした。")?;
    append_edge_import_debug(
        "Profile候補決定",
        "success",
        format!(
            "最新History候補を選択: {} (更新時刻: {})",
            profile_name,
            DateTime::<Local>::from(modified).format("%Y-%m-%d %H:%M:%S")
        ),
    );
    Ok(EdgeProfileCandidate {
        profile_name,
        profile_dir,
        history_path,
    })
}

fn open_profile_folder_in_explorer(profile_dir: &Path) -> Result<(), Box<dyn Error>> {
    append_edge_import_debug(
        "Explorer起動",
        "info",
        format!(
            "候補プロファイルフォルダを開きます: {}",
            profile_dir.display()
        ),
    );
    Command::new("explorer.exe").arg(profile_dir).spawn()?;
    append_edge_import_debug("Explorer起動", "success", "Windows Explorerを開きました。");
    Ok(())
}

fn pick_history_file(initial_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    append_edge_import_debug(
        "History選択",
        "info",
        format!(
            "Historyファイル選択ダイアログを表示します (初期フォルダ: {})",
            initial_dir.display()
        ),
    );
    let selected = rfd::FileDialog::new()
        .set_title("EdgeのHistoryファイルを選択してください")
        .set_directory(initial_dir)
        .pick_file()
        .ok_or("Historyファイルが選択されませんでした。")?;

    if !selected.exists() {
        return Err(format!("選択したファイルが見つかりません: {}", selected.display()).into());
    }

    Ok(selected)
}

fn should_include_profile(profile_name: &str, metadata: &Value) -> bool {
    if !is_profile_directory_name(profile_name) {
        return false;
    }

    if profile_name == "Guest Profile" {
        return false;
    }

    if metadata
        .get("name")
        .and_then(|value| value.as_str())
        .map(|value| value.eq_ignore_ascii_case("Guest"))
        .unwrap_or(false)
    {
        return false;
    }

    if metadata
        .get("is_ephemeral")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return false;
    }

    true
}

fn is_profile_directory_name(name: &str) -> bool {
    name == "Default" || name.starts_with("Profile ")
}

fn read_edge_history_rows_from_path(path: &Path) -> Result<Vec<EdgeHistoryRow>, Box<dyn Error>> {
    append_edge_import_debug(
        "Historyコピー",
        "info",
        format!("Historyファイルを直接読み取ります: {}", path.display()),
    );
    append_edge_import_debug(
        "SQLite read-only open",
        "info",
        format!("Historyをread-onlyで開きます: {}", path.display()),
    );
    let connection =
        match Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(connection) => {
                append_edge_import_debug(
                    "SQLite read-only open",
                    "success",
                    "SQLite read-only openに成功しました。",
                );
                connection
            }
            Err(error) => {
                append_edge_import_debug(
                    "SQLite read-only open",
                    "error",
                    format!("SQLite read-only openに失敗しました: {}", error),
                );
                return Err(error.into());
            }
        };

    let mut stmt = connection.prepare(
        "SELECT title, url, last_visit_time, visit_count
         FROM urls
         ORDER BY last_visit_time DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        let title: String = row.get(0)?;
        let url: String = row.get(1)?;
        let last_visit_time: i64 = row.get::<_, Option<i64>>(2)?.unwrap_or(0);
        let visit_count: i64 = row.get::<_, Option<i64>>(3)?.unwrap_or(0);

        Ok(EdgeHistoryRow {
            title,
            url,
            last_visit_time,
            visit_count,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    append_edge_import_debug(
        "SQLite read-only open",
        "success",
        format!("History SQLiteから{}件を読み込みました。", results.len()),
    );

    Ok(results)
}

fn copy_edge_history_snapshot(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let snapshot_dir = std::env::temp_dir().join(format!(
        "search-launcher-edge-{}-{}",
        std::process::id(),
        unique_stamp()
    ));
    fs::create_dir_all(&snapshot_dir)?;

    let history_copy = snapshot_dir.join("History");
    copy_file_with_os_error_detail(path, &history_copy, "History")?;

    let wal_path = path.with_file_name("History-wal");
    if wal_path.exists() {
        let _ = copy_file_with_os_error_detail(
            &wal_path,
            &snapshot_dir.join("History-wal"),
            "History-wal",
        );
    }

    let shm_path = path.with_file_name("History-shm");
    if shm_path.exists() {
        let _ = copy_file_with_os_error_detail(
            &shm_path,
            &snapshot_dir.join("History-shm"),
            "History-shm",
        );
    }

    Ok(snapshot_dir)
}

/// Copy a file and enrich any IO error with the Windows OS error code for diagnosis.
fn copy_file_with_os_error_detail(
    src: &Path,
    dst: &Path,
    label: &str,
) -> Result<(), Box<dyn Error>> {
    match fs::copy(src, dst) {
        Ok(_) => Ok(()),
        Err(error) => {
            let os_code = error.raw_os_error().unwrap_or(-1);
            Err(format!(
                "{}のコピー失敗: {} (OS error code: {}; src: {}; dst: {})",
                label,
                error,
                os_code,
                src.display(),
                dst.display()
            )
            .into())
        }
    }
}

#[cfg(windows)]
struct ComInitGuard {
    should_uninitialize: bool,
}

#[cfg(windows)]
impl ComInitGuard {
    fn initialize_sta() -> Result<Self, Box<dyn Error>> {
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        match result {
            Ok(()) => Ok(Self {
                should_uninitialize: true,
            }),
            Err(error) if error.code() == RPC_E_CHANGED_MODE => Ok(Self {
                should_uninitialize: false,
            }),
            Err(error) => Err(format!("COM初期化失敗: {}", error).into()),
        }
    }
}

#[cfg(windows)]
impl Drop for ComInitGuard {
    fn drop(&mut self) {
        if self.should_uninitialize {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(windows)]
fn wide_null(value: &Path) -> Vec<u16> {
    value
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect()
}

/// Move a directory using Explorer-compatible shell APIs on Windows.
///
/// `std::fs::rename` and PowerShell `Rename-Item` both map to MoveFileEx-style behavior,
/// which can fail with AccessDenied on Edge User Data in some environments.
/// Explorer move uses shell file operation APIs, so we mirror that path here.
#[cfg(windows)]
fn move_directory_explorer_style(src: &Path, dst: &Path) -> Result<(), Box<dyn Error>> {
    let destination_parent = dst
        .parent()
        .ok_or("移動先の親フォルダを解決できませんでした。")?;
    fs::create_dir_all(destination_parent)?;

    let destination_name = dst
        .file_name()
        .ok_or("移動先フォルダ名を解決できませんでした。")?;

    let _com_guard = ComInitGuard::initialize_sta()?;

    let source_w = wide_null(src);
    let destination_parent_w = wide_null(destination_parent);
    let destination_name_w: Vec<u16> = destination_name
        .encode_wide()
        .chain(iter::once(0))
        .collect();

    unsafe {
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
        operation.SetOperationFlags(
            FOF_NOCONFIRMATION | FOF_NOCONFIRMMKDIR | FOF_NOERRORUI | FOF_SILENT,
        )?;

        let source_item: IShellItem =
            SHCreateItemFromParsingName(PCWSTR(source_w.as_ptr()), Option::<&IBindCtx>::None)?;
        let destination_parent_item: IShellItem = SHCreateItemFromParsingName(
            PCWSTR(destination_parent_w.as_ptr()),
            Option::<&IBindCtx>::None,
        )?;

        operation.MoveItem(
            &source_item,
            &destination_parent_item,
            PCWSTR(destination_name_w.as_ptr()),
            None,
        )?;
        operation.PerformOperations()?;

        let aborted: BOOL = operation.GetAnyOperationsAborted()?;
        if aborted.as_bool() {
            return Err("IFileOperationの移動処理が中断されました。".into());
        }
    }

    if src.exists() || !dst.exists() {
        return Err(format!(
            "IFileOperation移動後の状態が不正です (src_exists={}, dst_exists={})",
            src.exists(),
            dst.exists()
        )
        .into());
    }

    Ok(())
}

#[cfg(not(windows))]
fn move_directory_explorer_style(src: &Path, dst: &Path) -> Result<(), Box<dyn Error>> {
    fs::rename(src, dst)?;
    Ok(())
}

fn move_edge_user_data_to_temp(user_data_dir: &Path) -> Result<UserDataMoveGuard, Box<dyn Error>> {
    if !user_data_dir.exists() {
        return Err(format!(
            "Edge User Data directory was not found: {}",
            user_data_dir.display()
        )
        .into());
    }

    let parent = user_data_dir
        .parent()
        .ok_or("Failed to resolve Edge User Data parent directory")?;
    let temp_root = parent.join(format!("SearchLauncherEdgeTemp-{}", unique_stamp()));
    let moved_path = temp_root.join("User Data");

    append_edge_import_debug(
        "User Data一時移動",
        "info",
        format!(
            "User Dataを一時移動します (IFileOperation): {} -> {}",
            user_data_dir.display(),
            moved_path.display()
        ),
    );
    append_edge_import_debug(
        "User Data一時移動",
        "info",
        format!("移動先親フォルダを作成します: {}", temp_root.display()),
    );
    if let Err(error) = fs::create_dir_all(&temp_root) {
        append_edge_import_debug(
            "User Data一時移動",
            "error",
            format!(
                "移動先親フォルダ作成失敗: {} (OS error: {})",
                error,
                error.raw_os_error().unwrap_or(-1)
            ),
        );
        return Err(error.into());
    }

    match move_directory_explorer_style(user_data_dir, &moved_path) {
        Ok(()) => {
            append_edge_import_debug(
                "User Data一時移動",
                "success",
                "User Dataの一時移動に成功しました。",
            );
        }
        Err(move_error) => {
            append_edge_import_debug(
                "User Data一時移動",
                "error",
                format!(
                    "IFileOperation移動失敗: {} (src: {}; dst: {})",
                    move_error,
                    user_data_dir.display(),
                    moved_path.display()
                ),
            );
            // Clean up the empty temp_root we just created since rename failed
            let _ = fs::remove_dir_all(&temp_root);
            // Run a targeted diagnostic scan to identify which file/directory blocks the move
            diagnose_user_data_move_failure(user_data_dir);
            return Err(format!("User Data移動失敗: {}", move_error).into());
        }
    }

    Ok(UserDataMoveGuard {
        original_path: user_data_dir.to_path_buf(),
        moved_path,
        temp_root,
        active: true,
    })
}

/// Diagnostic function called when User Data move fails.
///
/// SAFE: Does NOT move, rename, or modify any files or directories.
/// Only reads metadata and enumerates the directory tree to identify
/// likely causes of the rename failure.
///
/// Background (from research):
/// - Explorer style move (IFileOperation) can still fail if handles in the subtree
///   are opened with an exclusive share mode (no FILE_SHARE_DELETE).
/// - Known culprits for Edge User Data: Default\lockfile, WAL database files,
///   Windows Search Indexer (SearchIndexer.exe), Windows Defender real-time protection,
///   MicrosoftEdgeUpdate.exe, and orphaned Edge crash handler subprocesses.
/// - This diagnostic intentionally avoids move attempts and only gathers metadata.
fn diagnose_user_data_move_failure(user_data_dir: &Path) {
    append_edge_import_debug(
        "User Data診断",
        "info",
        concat!(
            "User Data rename失敗の診断を開始します。",
            "実データは移動しません。metadata確認とディレクトリ列挙のみ実施します。",
        ),
    );
    append_edge_import_debug(
        "User Data診断",
        "info",
        concat!(
            "参考: 現在はIFileOperationでUser Data移動を試行しています。",
            "それでも失敗する場合は、サブツリー内の排他ハンドルが原因の可能性が高いです。",
        ),
    );

    // 1. User Data directory itself
    diagnose_entry_metadata(user_data_dir, "User Data");

    // 2. Enumerate and log top-level entries with metadata
    let entries = match fs::read_dir(user_data_dir) {
        Ok(entries) => entries,
        Err(error) => {
            append_edge_import_debug(
                "User Data診断",
                "error",
                format!(
                    "User Dataディレクトリ読取り失敗: {} (OS error: {})",
                    error,
                    error.raw_os_error().unwrap_or(-1)
                ),
            );
            return;
        }
    };

    let mut entry_count = 0usize;
    let mut readonly_entries: Vec<String> = Vec::new();

    for entry_result in entries {
        let entry = match entry_result {
            Ok(e) => e,
            Err(error) => {
                append_edge_import_debug(
                    "User Data診断",
                    "error",
                    format!(
                        "エントリ読取りエラー: {} (OS error: {})",
                        error,
                        error.raw_os_error().unwrap_or(-1)
                    ),
                );
                continue;
            }
        };

        let entry_path = entry.path();
        let entry_name = entry.file_name().to_string_lossy().to_string();
        entry_count += 1;

        match fs::metadata(&entry_path) {
            Ok(meta) => {
                let readonly = meta.permissions().readonly();
                let kind = if meta.is_dir() { "dir" } else { "file" };
                let len = meta.len();

                if readonly {
                    readonly_entries.push(entry_name.clone());
                }

                // Log known sensitive files with extra context
                let note = if entry_name == "lockfile" || entry_name == "SingletonLock" {
                    " [※ Chromiumシングルトンロックファイル]"
                } else if entry_name.ends_with(".lock") {
                    " [※ lockファイル]"
                } else if entry_name == "Default" || entry_name.starts_with("Profile ") {
                    " [プロファイルディレクトリ]"
                } else {
                    ""
                };

                append_edge_import_debug(
                    "User Data診断",
                    "info",
                    format!(
                        "  {}: {} readonly={} len={}{}",
                        entry_name, kind, readonly, len, note
                    ),
                );

                // For profile directories, enumerate one level deeper to find lockfile etc.
                if meta.is_dir() && (entry_name == "Default" || entry_name.starts_with("Profile "))
                {
                    diagnose_profile_directory(&entry_path, &entry_name);
                }
            }
            Err(error) => {
                append_edge_import_debug(
                    "User Data診断",
                    "error",
                    format!(
                        "  {}: metadata取得失敗: {} (OS error: {})",
                        entry_name,
                        error,
                        error.raw_os_error().unwrap_or(-1)
                    ),
                );
            }
        }
    }

    // Summary
    if readonly_entries.is_empty() {
        append_edge_import_debug(
            "User Data診断",
            "info",
            format!(
                "診断完了: {}件のエントリを確認。readonlyエントリなし。",
                entry_count
            ),
        );
    } else {
        append_edge_import_debug(
            "User Data診断",
            "info",
            format!(
                "診断完了: {}件のエントリを確認。readonlyエントリ{}件: {}",
                entry_count,
                readonly_entries.len(),
                readonly_entries.join(", ")
            ),
        );
    }

    append_edge_import_debug(
        "User Data診断",
        "info",
        concat!(
            "rename失敗の主な原因として疑われるもの: ",
            "SearchIndexer.exe (Windows検索インデックス)、",
            "Windows Defenderリアルタイム保護、",
            "MicrosoftEdgeUpdate.exe、",
            "Edge crashpad-handlerサブプロセス、",
            "Default\\lockfileの排他ロック。",
            "これらは各エントリのファイルを排他的ハンドルで保持するため、",
            "ディレクトリ全体のrenameをブロックします。",
        ),
    );
}

/// Log metadata for a single path entry without modifying it.
fn diagnose_entry_metadata(path: &Path, label: &str) {
    match fs::metadata(path) {
        Ok(meta) => {
            append_edge_import_debug(
                "User Data診断",
                "info",
                format!(
                    "{} metadata: readonly={}, is_dir={}, len={}",
                    label,
                    meta.permissions().readonly(),
                    meta.is_dir(),
                    meta.len()
                ),
            );
        }
        Err(error) => {
            append_edge_import_debug(
                "User Data診断",
                "error",
                format!(
                    "{} metadata取得失敗: {} (OS error: {})",
                    label,
                    error,
                    error.raw_os_error().unwrap_or(-1)
                ),
            );
        }
    }
}

/// Enumerate a profile directory one level deep to surface lockfile and WAL files.
fn diagnose_profile_directory(profile_path: &Path, profile_name: &str) {
    let entries = match fs::read_dir(profile_path) {
        Ok(entries) => entries,
        Err(error) => {
            append_edge_import_debug(
                "User Data診断",
                "error",
                format!(
                    "  [{}/] 読取り失敗: {} (OS error: {})",
                    profile_name,
                    error,
                    error.raw_os_error().unwrap_or(-1)
                ),
            );
            return;
        }
    };

    for entry_result in entries {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let name = entry.file_name().to_string_lossy().to_string();

        // Only surface files that are known lock or WAL candidates
        let is_notable = name == "lockfile"
            || name == "SingletonLock"
            || name.ends_with(".lock")
            || name.ends_with("-wal")
            || name.ends_with("-shm")
            || name == "History"
            || name == "Cookies"
            || name == "Network"
            || name == "Cache";

        if !is_notable {
            continue;
        }

        let entry_path = entry.path();
        match fs::metadata(&entry_path) {
            Ok(meta) => {
                let kind = if meta.is_dir() { "dir" } else { "file" };
                let note = if name == "lockfile" {
                    " [※ Chromiumシングルトンロック — Edge終了後も保持されることがある]"
                } else if name.ends_with("-wal") || name.ends_with("-shm") {
                    " [※ SQLite WAL/SHM — 強制終了後に残ることがある]"
                } else {
                    ""
                };
                append_edge_import_debug(
                    "User Data診断",
                    "info",
                    format!(
                        "    [{}/{}] {}: readonly={} len={}{}",
                        profile_name,
                        name,
                        kind,
                        meta.permissions().readonly(),
                        meta.len(),
                        note
                    ),
                );
            }
            Err(error) => {
                append_edge_import_debug(
                    "User Data診断",
                    "error",
                    format!(
                        "    [{}/{}] metadata取得失敗: {} (OS error: {})",
                        profile_name,
                        name,
                        error,
                        error.raw_os_error().unwrap_or(-1)
                    ),
                );
            }
        }
    }
}

fn persist_imported_edge_history(
    records: Vec<(String, EdgeHistoryRow)>,
) -> Result<usize, Box<dyn Error>> {
    let path = db_path()?;
    let mut conn = Connection::open(path)?;
    let transaction = conn.transaction()?;

    let profiles: HashSet<String> = records
        .iter()
        .map(|(profile_name, _)| profile_name.clone())
        .collect();

    for profile_name in profiles.iter() {
        transaction.execute(
            "DELETE FROM edge_history_source WHERE profile_name = ?1",
            params![profile_name],
        )?;
    }

    {
        let mut insert_stmt = transaction.prepare(
            "INSERT INTO edge_history_source (
                profile_name,
                url,
                title,
                last_visit_time,
                visit_count,
                last_visit_text
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;

        for (profile_name, row) in records.iter() {
            let title = if row.title.trim().is_empty() {
                row.url.clone()
            } else {
                row.title.clone()
            };
            let last_visit_text = webkit_time_to_local_string(row.last_visit_time);

            insert_stmt.execute(params![
                profile_name,
                row.url,
                title,
                row.last_visit_time,
                row.visit_count,
                last_visit_text,
            ])?;
        }
    }

    let mut imported_url_count = 0usize;
    update_edge_import_message("検索DBへ反映しています…".to_string());
    {
        let mut aggregate_stmt = transaction.prepare(
            "SELECT
                source.url,
                COALESCE(
                    (
                        SELECT latest.title
                        FROM edge_history_source latest
                        WHERE latest.url = source.url
                        ORDER BY latest.last_visit_time DESC
                        LIMIT 1
                    ),
                    source.url
                ) AS title,
                MAX(source.last_visit_time) AS max_last_visit_time,
                COALESCE(
                    (
                        SELECT latest.last_visit_text
                        FROM edge_history_source latest
                        WHERE latest.url = source.url
                        ORDER BY latest.last_visit_time DESC
                        LIMIT 1
                    ),
                    NULL
                ) AS last_visit_text,
                SUM(source.visit_count) AS total_visit_count
             FROM edge_history_source source
             GROUP BY source.url",
        )?;

        let mut aggregated_rows = aggregate_stmt.query([])?;
        while let Some(row) = aggregated_rows.next()? {
            let url: String = row.get(0)?;
            let title: String = row.get(1)?;
            let last_visit_time: i64 = row.get::<_, Option<i64>>(2)?.unwrap_or(0);
            let last_access: Option<String> = row.get(3)?;
            let access_count: i64 = row.get::<_, Option<i64>>(4)?.unwrap_or(0);
            let site = extract_site_from_url(&url);
            let service = classify_service_from_url(&url);
            let title_to_store = if title.trim().is_empty() {
                url.clone()
            } else {
                title
            };
            let first_seen = last_access.clone();

            let updated = transaction.execute(
                "UPDATE history
                 SET service = ?1,
                     title = ?2,
                     site = ?3,
                     last_access = ?4,
                     access_count = ?5,
                     first_seen = COALESCE(first_seen, ?6)
                 WHERE url = ?7",
                params![
                    service,
                    title_to_store,
                    site,
                    last_access,
                    access_count,
                    first_seen,
                    url,
                ],
            )?;

            if updated == 0 {
                transaction.execute(
                    "INSERT INTO history (
                        service,
                        title,
                        site,
                        url,
                        first_seen,
                        last_access,
                        access_count,
                        is_favorite
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
                    params![
                        service,
                        title_to_store,
                        site,
                        url,
                        first_seen,
                        last_access,
                        access_count,
                    ],
                )?;
            }

            if last_visit_time > 0 {
                imported_url_count += 1;
            }
        }
    }

    transaction.commit()?;
    Ok(imported_url_count)
}

fn edge_user_data_dir() -> Result<PathBuf, Box<dyn Error>> {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map_err(|_| "LOCALAPPDATA environment variable is not available")?;
    let path = PathBuf::from(local_app_data)
        .join("Microsoft")
        .join("Edge")
        .join("User Data");

    if !path.exists() {
        return Err(format!("Edge User Data directory was not found: {}", path.display()).into());
    }

    Ok(path)
}

/// Only msedge.exe is killed. /T kills the entire process tree (child processes including
/// crashpad-handler, renderer processes, etc.).
/// msedgewebview2.exe is excluded — Tauri itself runs on WebView2.
/// MicrosoftEdgeUpdate.exe is excluded — it does not hold User Data locks and cannot be
/// reliably terminated without elevated privileges.
const EDGE_PROCESS_NAMES: &[&str] = &["msedge.exe"];

fn is_any_edge_process_running() -> Result<bool, Box<dyn Error>> {
    let output = Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    for name in EDGE_PROCESS_NAMES {
        if stdout.contains(&name.to_ascii_lowercase()) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns true only if msedge.exe (the main browser window process) is running.
/// Used to decide whether Edge should be restarted after import.
fn is_edge_running() -> Result<bool, Box<dyn Error>> {
    let output = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq msedge.exe"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.to_ascii_lowercase().contains("msedge.exe"))
}

fn stop_edge() -> Result<(), Box<dyn Error>> {
    // Kill all Edge-family processes. /T kills child trees too.
    // Run taskkill twice: first pass terminates known processes,
    // second pass catches any survivors that were spawned during shutdown.
    for pass in 1..=2u32 {
        for name in EDGE_PROCESS_NAMES {
            let output = Command::new("taskkill")
                .args(["/IM", name, "/T", "/F"])
                .output()?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{}{}", stdout, stderr).to_ascii_lowercase();
            // Ignore "not found" — the process simply wasn't running.
            if !output.status.success()
                && !combined.contains("not found")
                && !combined.contains("指定されたプロセス")
                && !combined.contains("見つかりません")
            {
                return Err(format!(
                    "Failed to stop {} (pass {}): {}",
                    name,
                    pass,
                    combined.trim()
                )
                .into());
            }
        }
        thread::sleep(Duration::from_millis(500));
    }

    // Wait until every Edge-family process has fully exited (up to 15 s).
    for i in 0..30 {
        if !is_any_edge_process_running()? {
            // Extra pause so the kernel releases all file handles before we touch User Data.
            let wait_ms = 1500u64;
            eprintln!(
                "[stop_edge] プロセス消滅確認 ({}回目チェック)。{}ms待機中…",
                i + 1,
                wait_ms
            );
            thread::sleep(Duration::from_millis(wait_ms));
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }

    Err("Edgeを完全終了できませんでした。".into())
}

fn restart_edge() -> Result<(), Box<dyn Error>> {
    let edge_executable = resolve_edge_executable()?;
    Command::new(edge_executable).spawn()?;
    Ok(())
}

fn resolve_edge_executable() -> Result<PathBuf, Box<dyn Error>> {
    let candidates = [
        std::env::var("LOCALAPPDATA").ok().map(|base| {
            PathBuf::from(base)
                .join("Microsoft")
                .join("Edge")
                .join("Application")
                .join("msedge.exe")
        }),
        std::env::var("ProgramFiles(x86)").ok().map(|base| {
            PathBuf::from(base)
                .join("Microsoft")
                .join("Edge")
                .join("Application")
                .join("msedge.exe")
        }),
        std::env::var("ProgramFiles").ok().map(|base| {
            PathBuf::from(base)
                .join("Microsoft")
                .join("Edge")
                .join("Application")
                .join("msedge.exe")
        }),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("Edge executable was not found.".into())
}

fn should_fallback_to_move(error: &(dyn Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(item) = current {
        if let Some(io_error) = item.downcast_ref::<std::io::Error>() {
            if io_error.kind() == std::io::ErrorKind::PermissionDenied
                || io_error.raw_os_error() == Some(5)
            {
                return true;
            }
        }

        let message = item.to_string().to_ascii_lowercase();
        if message.contains("access is denied")
            || message.contains("permission denied")
            || message.contains("アクセスが拒否")
            || message.contains("code 5")
            || message.contains("os error 5")
            || message.contains("error 5)")
        {
            return true;
        }

        current = item.source();
    }

    false
}

fn ensure_default_settings(conn: &Connection) -> Result<(), Box<dyn Error>> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))?;
    if count == 0 {
        let defaults = AppSettings::default();
        conn.execute(
            "INSERT INTO settings (id, display_count, quick_keywords, blocked_words, site_names, show_edge_favorites, search_mode, window_size, index_folders, favorite_pane_count, left_pane_percent, favorite_density, favorite_tab_limit, edge_sync_interval_minutes, edge_sync_interval_seconds, folder_sync_interval_seconds)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                defaults.display_count,
                serde_json::to_string(&defaults.quick_keywords)?,
                serde_json::to_string(&defaults.blocked_words)?,
                serde_json::to_string(&defaults.site_names)?,
                if defaults.show_edge_favorites { 1 } else { 0 },
                defaults.search_mode,
                defaults.window_size,
                serde_json::to_string(&defaults.index_folders)?,
                defaults.favorite_pane_count,
                defaults.left_pane_percent,
                defaults.favorite_density,
                defaults.favorite_tab_limit,
                defaults.edge_sync_interval_minutes,
                defaults.edge_sync_interval_seconds,
                defaults.folder_sync_interval_seconds,
            ],
        )?;
    }

    Ok(())
}

fn read_settings(conn: &Connection) -> Result<Option<AppSettings>, Box<dyn Error>> {
    let row = conn
        .query_row(
            "SELECT display_count, quick_keywords, blocked_words, site_names, show_edge_favorites, search_mode, window_size, index_folders, favorite_pane_count, left_pane_percent, favorite_density, favorite_tab_limit, edge_sync_interval_minutes, edge_sync_interval_seconds, folder_sync_interval_seconds
             FROM settings
             WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, i64>(13)?,
                    row.get::<_, i64>(14)?,
                ))
            },
        )
        .optional()?;

    if let Some((
        display_count,
        quick_keywords_raw,
        blocked_words_raw,
        site_names_raw,
        show_edge_favorites,
        search_mode,
        window_size,
        index_folders_raw,
        favorite_pane_count,
        left_pane_percent,
        favorite_density,
        favorite_tab_limit,
        edge_sync_interval_minutes,
        edge_sync_interval_seconds,
        folder_sync_interval_seconds,
    )) = row
    {
        let mut settings = AppSettings {
            display_count,
            quick_keywords: serde_json::from_str(&quick_keywords_raw)?,
            blocked_words: serde_json::from_str(&blocked_words_raw)?,
            site_names: serde_json::from_str(&site_names_raw)?,
            show_edge_favorites: show_edge_favorites != 0,
            search_mode,
            window_size,
            index_folders: serde_json::from_str(&index_folders_raw).unwrap_or_default(),
            favorite_pane_count,
            left_pane_percent,
            favorite_density,
            favorite_tab_limit,
            heading_default_color: default_heading_color(),
            heading_palette_size: default_heading_palette_size(),
            edge_sync_interval_minutes,
            edge_sync_interval_seconds,
            folder_sync_interval_seconds,
        };
        settings.heading_default_color = conn
            .query_row(
                "SELECT value FROM app_meta WHERE key='heading_default_color'",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(default_heading_color);
        settings.heading_palette_size = conn
            .query_row(
                "SELECT value FROM app_meta WHERE key='heading_palette_size'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(default_heading_palette_size);
        let settings = settings.sanitized();

        Ok(Some(settings))
    } else {
        Ok(None)
    }
}

fn seed_models(conn: &Connection) -> Result<(), Box<dyn Error>> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM models", [], |row| row.get(0))
        .unwrap_or(0);
    if count > 0 {
        return Ok(());
    }

    for (index, name) in DEFAULT_QUICK_KEYWORDS.iter().enumerate() {
        conn.execute(
            "INSERT INTO models (name, display_order) VALUES (?1, ?2)",
            params![name, index as i64],
        )?;
    }

    Ok(())
}

fn cleanup_legacy_seeded_history_once(conn: &Connection) -> Result<(), Box<dyn Error>> {
    let cleaned: Option<String> = conn
        .query_row(
            "SELECT value FROM app_meta WHERE key = 'legacy_dummy_history_cleaned'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if cleaned.is_some() {
        return Ok(());
    }

    let deleted = conn.execute(
        "DELETE FROM history WHERE service IN ('spo', 'asana', 'notion')",
        [],
    )?;
    conn.execute(
        "INSERT INTO app_meta (key, value) VALUES ('legacy_dummy_history_cleaned', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![now_string()],
    )?;
    println!("Removed legacy dummy history rows: {}", deleted);
    Ok(())
}

fn normalize_history_services(conn: &Connection) -> Result<(), Box<dyn Error>> {
    conn.execute(
        "UPDATE history
         SET service = CASE
             WHEN lower(url) LIKE '%app.asana.com/%' THEN 'asana'
             WHEN lower(url) LIKE '%notion.so%'
               OR lower(url) LIKE '%notion.site%'
               OR lower(url) LIKE '%notion.com%'
               OR lower(url) LIKE '%app.notion.com%'
             THEN 'notion'
             WHEN lower(url) LIKE '%sharepoint.com%' THEN 'spo'
             ELSE 'edge'
         END",
        [],
    )?;
    Ok(())
}

fn migrate_legacy_default_quick_keywords(conn: &Connection) -> Result<(), Box<dyn Error>> {
    let settings = read_settings(conn)?;
    let Some(current) = settings else {
        return Ok(());
    };
    let legacy: Vec<String> = LEGACY_DEFAULT_QUICK_KEYWORDS
        .iter()
        .map(|value| value.to_string())
        .collect();
    if current.quick_keywords != legacy {
        return Ok(());
    }

    let migrated = AppSettings {
        quick_keywords: DEFAULT_QUICK_KEYWORDS
            .iter()
            .map(|value| value.to_string())
            .collect(),
        ..current
    };
    conn.execute(
        "UPDATE settings
         SET quick_keywords = ?1
         WHERE id = 1",
        params![serde_json::to_string(&migrated.quick_keywords)?],
    )?;
    Ok(())
}

pub fn fetch_history() -> Result<Vec<SearchRecord>, Box<dyn Error>> {
    let path = db_path()?;
    let conn = Connection::open(path)?;
    let mut stmt = conn.prepare(
        "SELECT id, service, title, site, url, first_seen, last_access, access_count, EXISTS(SELECT 1 FROM favorite_items f WHERE f.kind='link' AND f.deleted_at IS NULL AND lower(f.target)=lower(h.url))
         FROM history h
         WHERE h.id = (SELECT h2.id FROM history h2 WHERE h2.url = h.url ORDER BY COALESCE(h2.last_access, '') DESC, h2.id DESC LIMIT 1)
         ORDER BY last_access DESC, access_count DESC LIMIT 200",
    )?;

    let rows = stmt.query_map([], map_search_record)?;
    collect_search_rows_tolerant(rows)
}

pub fn search_history(query: &str) -> Result<Vec<SearchRecord>, Box<dyn Error>> {
    let path = db_path()?;
    let conn = Connection::open(path)?;
    let normalized = query.trim().to_lowercase();
    if normalized.is_empty() {
        return fetch_history();
    }

    let terms: Vec<String> = normalized
        .split_whitespace()
        .map(|term| term.to_string())
        .collect();
    let mut where_clauses = Vec::new();
    let mut params_vec: Vec<String> = Vec::new();

    for term in terms.iter() {
        where_clauses
            .push("(lower(title) LIKE ? OR lower(url) LIKE ? OR lower(coalesce(site, '')) LIKE ?)");
        let pattern = format!("%{}%", term.replace('%', "\\%"));
        params_vec.push(pattern.clone());
        params_vec.push(pattern.clone());
        params_vec.push(pattern);
    }

    let sql = format!(
        "SELECT id, service, title, site, url, first_seen, last_access, access_count, EXISTS(SELECT 1 FROM favorite_items f WHERE f.kind='link' AND f.deleted_at IS NULL AND lower(f.target)=lower(history.url))
         FROM history
         WHERE {} AND id = (SELECT h2.id FROM history h2 WHERE h2.url = history.url ORDER BY COALESCE(h2.last_access, '') DESC, h2.id DESC LIMIT 1)
         ORDER BY CASE WHEN lower(title)=? THEN 0 WHEN lower(title) LIKE ? THEN 1 WHEN lower(url) LIKE ? THEN 2 ELSE 3 END,
                  CASE WHEN EXISTS(SELECT 1 FROM favorite_items f WHERE f.kind='link' AND f.deleted_at IS NULL AND lower(f.target)=lower(history.url)) THEN 0 ELSE 1 END,
                  last_access DESC, access_count DESC LIMIT 200",
        where_clauses.join(" AND ")
    );
    params_vec.push(normalized.clone());
    params_vec.push(format!("{}%", normalized.replace('%', "\\%")));
    params_vec.push(format!("%{}%", normalized.replace('%', "\\%")));

    let mut stmt = conn.prepare(&sql)?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec
        .iter()
        .map(|value| value as &dyn rusqlite::ToSql)
        .collect();
    let rows = stmt.query_map(params_refs.as_slice(), map_search_record)?;
    collect_search_rows_tolerant(rows)
}

pub fn toggle_favorite(id: i64) -> Result<bool, Box<dyn Error>> {
    let path = db_path()?;
    let mut conn = Connection::open(path)?;
    let tx = conn.transaction()?;
    let (title, url, service): (String, String, String) = tx
        .query_row(
            "SELECT title,url,service FROM history WHERE id=?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| "Record not found")?;
    let existing:Option<i64>=tx.query_row("SELECT id FROM favorite_items WHERE kind='link' AND deleted_at IS NULL AND lower(target)=lower(?1) LIMIT 1",params![url],|r|r.get(0)).optional()?;
    if let Some(favorite_id) = existing {
        tx.execute(
            "UPDATE favorite_items SET deleted_at=?1 WHERE id=?2",
            params![now_string(), favorite_id],
        )?;
        tx.execute(
            "UPDATE history SET is_favorite=0 WHERE lower(url)=lower(?1)",
            params![url],
        )?;
        tx.commit()?;
        return Ok(false);
    }
    let title = clean_favorite_title(&title,&blocked_words_for_connection(&tx)?);
    tx.execute("INSERT INTO favorite_items(kind,history_id,label,target,service,pane,position) SELECT 'link',NULL,?1,?2,?3,0,COALESCE(MAX(position),-1)+1 FROM favorite_items WHERE pane=0",params![title,url,service])?;
    tx.execute(
        "UPDATE history SET is_favorite=1 WHERE lower(url)=lower(?1)",
        params![url],
    )?;
    tx.commit()?;
    Ok(true)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionHistoryItem {
    title: Option<String>,
    url: String,
    last_visit_time: Option<f64>,
    visit_count: Option<i64>,
}
#[derive(Deserialize)]
struct ExtensionHistoryMessage {
    #[serde(default)]
    records: Vec<ExtensionHistoryItem>,
    #[serde(default)]
    favorite: Option<ExtensionHistoryItem>,
}

fn extension_queue_path() -> Result<PathBuf, Box<dyn Error>> {
    Ok(extension_data_dir()?.join("edge-extension-queue.jsonl"))
}

fn extension_data_dir() -> Result<PathBuf, Box<dyn Error>> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("FAVORITE_LAUNCHER_TEST_DIR") { return Ok(PathBuf::from(path)); }
    Ok(PathBuf::from(std::env::var("LOCALAPPDATA")?).join("search-launcher-app"))
}

pub fn import_extension_history_queue() -> Result<usize, Box<dyn Error>> {
    static IMPORT_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
    let _guard = IMPORT_LOCK.lock().map_err(|_| "History import lock failed")?;
    let queue = extension_queue_path()?;
    let processing = queue.with_extension("processing");
    if !processing.exists() && (!queue.exists() || fs::rename(&queue, &processing).is_err()) {
        return Ok(0);
    }
    let content = fs::read_to_string(&processing)?;
    let mut items = Vec::new();
    let mut favorites = Vec::new();
    for line in content.lines().filter(|v| !v.trim().is_empty()) {
        if let Ok(message) = serde_json::from_str::<ExtensionHistoryMessage>(line) {
            items.extend(message.records);
            if let Some(favorite) = message.favorite { favorites.push(favorite); }
        }
    }
    if items.is_empty() && favorites.is_empty() {
        let _ = fs::remove_file(&processing);
        return Ok(0);
    }
    let mut conn = Connection::open(db_path()?)?;
    conn.busy_timeout(Duration::from_secs(3))?;
    let tx = conn.transaction()?;
    let mut count = 0;
    let recent_pane: i64 = 0; // All new registrations go to the search-side inbox.
    let valid_pane = recent_pane == 0 || tx.query_row("SELECT EXISTS(SELECT 1 FROM favorite_columns WHERE id=?1)",params![recent_pane],|r|r.get::<_,bool>(0))?;
    let pane = if valid_pane {recent_pane} else {0};
    for item in favorites {
        if !(item.url.starts_with("https://") || item.url.starts_with("http://")) { continue; }
        let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM favorite_items WHERE deleted_at IS NULL AND lower(target)=lower(?1))",params![item.url],|r|r.get(0))?;
        if !exists {
            let title = clean_favorite_title(&item.title.unwrap_or_else(||item.url.clone()),&blocked_words_for_connection(&tx)?);
            tx.execute("INSERT INTO favorite_items(kind,label,target,service,pane,position) SELECT 'link',?1,?2,?3,?4,COALESCE(MAX(position),-1)+1 FROM favorite_items WHERE pane=?4",params![title,item.url,classify_service_from_url(&item.url),pane])?;
            count += 1;
        }
    }
    for item in items {
        if item.url.trim().is_empty() {
            continue;
        }
        let millis = item.last_visit_time.unwrap_or(0.0) as i64;
        let last = Local
            .timestamp_millis_opt(millis)
            .single()
            .map(|v| v.format("%Y-%m-%d %H:%M:%S").to_string());
        let title = item
            .title
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| item.url.clone());
        let service = classify_service_from_url(&item.url);
        let site = extract_site_from_url(&item.url);
        let updated=tx.execute("UPDATE history SET service=?1,title=?2,site=?3,last_access=COALESCE(?4,last_access),access_count=MAX(access_count,?5),first_seen=COALESCE(first_seen,?4) WHERE url=?6",params![service,title,site,last,item.visit_count.unwrap_or(0),item.url])?;
        if updated == 0 {
            tx.execute("INSERT INTO history(service,title,site,url,first_seen,last_access,access_count,is_favorite) VALUES(?1,?2,?3,?4,?5,?5,?6,0)",params![service,title,site,item.url,last,item.visit_count.unwrap_or(0)])?;
        }
        count += 1;
    }
    tx.commit()?;
    let _ = fs::remove_file(&processing);
    append_app_log(
        "Edge拡張連携",
        "success",
        &format!("拡張機能から履歴{}件を取り込みました。", count),
    )?;
    Ok(count)
}

pub fn append_app_log(category: &str, level: &str, message: &str) -> Result<(), Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    conn.busy_timeout(Duration::from_secs(3))?;
    conn.execute(
        "INSERT INTO app_logs(timestamp, category, level, message) VALUES(?1, ?2, ?3, ?4)",
        params![now_string(), category, level, message],
    )?;
    conn.execute(
        "DELETE FROM app_logs WHERE id NOT IN (SELECT id FROM app_logs ORDER BY id DESC LIMIT 500)",
        [],
    )?;
    Ok(())
}

pub fn list_app_logs() -> Result<Vec<AppLogEntry>, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, category, level, message FROM app_logs ORDER BY id DESC LIMIT 500",
    )?;
    let entries = stmt
        .query_map([], |row| {
            Ok(AppLogEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                category: row.get(2)?,
                level: row.get(3)?,
                message: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

pub fn clear_app_logs() -> Result<(), Box<dyn Error>> {
    Connection::open(db_path()?)?.execute("DELETE FROM app_logs", [])?;
    Ok(())
}

fn vacuum_database_to(destination: &Path) -> Result<(), Box<dyn Error>> {
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    let conn = Connection::open(db_path()?)?;
    let quoted = destination.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{}';", quoted))?;
    Ok(())
}

pub fn create_backup() -> Result<Option<String>, Box<dyn Error>> {
    let name = format!(
        "search-launcher-backup-{}.db",
        Local::now().format("%Y%m%d-%H%M%S")
    );
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(&name)
        .add_filter("Search Launcher backup", &["db"])
        .save_file()
    else {
        return Ok(None);
    };
    vacuum_database_to(&path)?;
    Ok(Some(path.to_string_lossy().to_string()))
}

pub fn restore_backup() -> Result<Option<String>, Box<dyn Error>> {
    let Some(source) = rfd::FileDialog::new()
        .add_filter("Search Launcher backup", &["db"])
        .pick_file()
    else {
        return Ok(None);
    };
    let check = Connection::open(&source)?
        .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))?;
    if check != "ok" {
        return Err("バックアップファイルが破損しています。".into());
    }
    let current = db_path()?;
    let safety = current.with_extension(format!(
        "before-restore-{}.db",
        Local::now().format("%Y%m%d-%H%M%S")
    ));
    vacuum_database_to(&safety)?;
    fs::copy(&source, &current)?;
    init_and_seed_db()?;
    Ok(Some(safety.to_string_lossy().to_string()))
}

pub fn create_automatic_backup() -> Result<(), Box<dyn Error>> {
    let dir = db_path()?
        .parent()
        .ok_or("DB保存先がありません")?
        .join("backups");
    fs::create_dir_all(&dir)?;
    let today = dir.join(format!("auto-{}.db", Local::now().format("%Y%m%d")));
    if !today.exists() {
        vacuum_database_to(&today)?;
    }
    let mut files = fs::read_dir(&dir)?
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|v| v.to_str()) == Some("db") {
                Some(p)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    files.sort();
    while files.len() > 7 {
        let path = files.remove(0);
        let _ = fs::remove_file(path);
    }
    Ok(())
}

pub fn diagnostics_text() -> Result<String, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let schema: String = conn.query_row(
        "SELECT COALESCE((SELECT value FROM app_meta WHERE key='schema_version'),'0')",
        [],
        |r| r.get(0),
    )?;
    let favorites: i64 = conn.query_row(
        "SELECT COUNT(*) FROM favorite_items WHERE kind='link' AND deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let tabs: i64 = conn.query_row("SELECT COUNT(*) FROM favorite_tabs", [], |r| r.get(0))?;
    let columns: i64 = conn.query_row("SELECT COUNT(*) FROM favorite_columns", [], |r| r.get(0))?;
    let history: i64 = conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))?;
    Ok(format!("Search Launcher 0.2.2\nDB schema: {}\nFavorites: {}\nTabs: {}\nColumns: {}\nWeb history: {}\nDB: {}",schema,favorites,tabs,columns,history,db_path()?.display()))
}

pub fn list_favorite_items() -> Result<Vec<FavoriteItem>, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let mut stmt = conn.prepare(
        "SELECT f.id, f.kind, NULL,
                f.label, f.service, f.target, f.position, f.pane, f.color,
                COALESCE(f.opened_count, 0), f.last_opened_at, f.deleted_at
         FROM favorite_items f
         WHERE f.deleted_at IS NULL AND (f.kind = 'heading' OR f.target IS NOT NULL)
         ORDER BY f.pane, f.position, f.id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(FavoriteItem {
            id: row.get(0)?,
            kind: row.get(1)?,
            history_id: row.get(2)?,
            label: row.get(3)?,
            service: row.get(4)?,
            target: row.get(5)?,
            position: row.get(6)?,
            pane: row.get(7)?,
            color: row.get(8)?,
            opened_count: row.get(9)?,
            last_opened_at: row.get(10)?,
            deleted_at: row.get(11)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn add_favorite_heading(label: &str, pane: i64, color: &str) -> Result<i64, Box<dyn Error>> {
    let color = if valid_color(color) { color } else { "#dbeafe" };
    let conn = Connection::open(db_path()?)?;
    conn.execute(
        "INSERT INTO favorite_items (kind, label, pane, position, color) SELECT 'heading', ?1, ?2, COALESCE(MAX(position), -1) + 1, ?3 FROM favorite_items WHERE pane = ?2",
        params![if label.trim().is_empty() { "新しい見出し" } else { label.trim() }, pane.max(0), color],
    )?;
    Ok(conn.last_insert_rowid())
}

fn blocked_words_for_connection(conn: &Connection) -> Result<Vec<String>,Box<dyn Error>> {
    let raw: Option<String> = conn.query_row("SELECT blocked_words FROM settings WHERE id=1",[],|r|r.get(0)).optional()?;
    Ok(raw.map(|value|serde_json::from_str(&value)).transpose()?.unwrap_or_default())
}
fn clean_favorite_title(title: &str, words: &[String]) -> String {
    let mut value=title.to_string();
    for word in words.iter().filter(|word|!word.is_empty()) {
        if let Ok(pattern)=regex::RegexBuilder::new(&regex::escape(word)).case_insensitive(true).build() {value=pattern.replace_all(&value,"").into_owned();}
    }
    static SEPARATORS: Lazy<regex::Regex> = Lazy::new(||regex::Regex::new(r"[\s\-_:–—]{2,}").unwrap());
    value=SEPARATORS.replace_all(&value," ").into_owned();
    value=value.trim_matches(|ch:char|ch.is_whitespace() || "-_:–—".contains(ch)).to_string();
    if value.is_empty() {"名称未設定".to_string()} else {value}
}
fn migrate_stored_titles(conn: &mut Connection) -> Result<(),Box<dyn Error>> {
    let tx=conn.transaction()?;
    let (raw,sites):(String,String)=tx.query_row("SELECT blocked_words,site_names FROM settings WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let mut words:Vec<String>=serde_json::from_str(&raw)?;
    for site in serde_json::from_str::<Vec<String>>(&sites)? {if !words.iter().any(|word|word.eq_ignore_ascii_case(&site)){words.push(site);}}
    tx.execute("UPDATE settings SET blocked_words=?1,site_names='[]' WHERE id=1",params![serde_json::to_string(&words)?])?;
    let items:Vec<(i64,String)>={let mut stmt=tx.prepare("SELECT id,label FROM favorite_items WHERE kind='link'")?;let rows=stmt.query_map([],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<Result<_,_>>()?;rows};
    for (id,title) in items {let cleaned=clean_favorite_title(&title,&words);if cleaned!=title{tx.execute("INSERT OR IGNORE INTO app_meta(key,value) VALUES(?1,?2)",params![format!("original_favorite_title_{id}"),title])?;tx.execute("UPDATE favorite_items SET label=?1 WHERE id=?2",params![cleaned,id])?;}}
    tx.execute("INSERT INTO app_meta(key,value) VALUES('stored_clean_titles','1')",[])?;
    tx.commit()?;Ok(())
}
pub fn add_manual_favorite(label: &str, target: &str, pane: i64) -> Result<i64, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    add_manual_favorite_in_connection(&conn,label,target,pane)
}
fn add_manual_favorite_in_connection(conn: &Connection, label: &str, target: &str, pane: i64) -> Result<i64, Box<dyn Error>> {
    let target = target.trim();
    if target.is_empty() {
        return Err("URLまたはファイルパスを入力してください。".into());
    }
    let duplicate: Option<i64> = conn.query_row("SELECT id FROM favorite_items WHERE lower(target)=lower(?1) AND deleted_at IS NULL LIMIT 1", params![target], |r| r.get(0)).optional()?;
    if duplicate.is_some() {
        return Err("そのリンクはすでにお気に入り登録済みです。".into());
    }
    let service = if target.starts_with("http://") || target.starts_with("https://") {
        "edge"
    } else {
        "folder"
    };
    let fallback = Path::new(target)
        .file_name()
        .and_then(|v| v.to_str())
        .filter(|v| !v.is_empty())
        .unwrap_or(target);
    let title = clean_favorite_title(if label.trim().is_empty(){fallback}else{label.trim()},&blocked_words_for_connection(conn)?);
    conn.execute("INSERT INTO favorite_items(kind,label,target,service,pane,position) SELECT 'link',?1,?2,?3,?4,COALESCE(MAX(position),-1)+1 FROM favorite_items WHERE pane=?4", params![title,target,service,pane.max(0)])?;
    Ok(conn.last_insert_rowid())
}

pub fn set_favorite_color(id: i64, color: &str) -> Result<(), Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let allowed = color.len() == 7
        && color.starts_with('#')
        && color.chars().skip(1).all(|c| c.is_ascii_hexdigit());
    if !allowed {
        return Err("Invalid color".into());
    }
    conn.execute(
        "UPDATE favorite_items SET color = ?1 WHERE id = ?2 AND kind = 'heading'",
        params![color, id],
    )?;
    Ok(())
}

pub fn set_all_heading_colors(color: &str) -> Result<(), Box<dyn Error>> {
    if !valid_color(color) {
        return Err("Invalid color".into());
    }
    Connection::open(db_path()?)?.execute(
        "UPDATE favorite_items SET color=?1 WHERE kind='heading' AND deleted_at IS NULL",
        params![color],
    )?;
    Connection::open(db_path()?)?.execute("UPDATE favorite_columns SET color=?1",params![color])?;
    Ok(())
}

pub fn toggle_file_favorite(path: &str, name: &str) -> Result<bool, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let existing: Option<i64> = conn.query_row("SELECT id FROM favorite_items WHERE target = ?1 AND service = 'folder' AND deleted_at IS NULL", params![path], |r| r.get(0)).optional()?;
    if let Some(id) = existing {
        delete_favorite_item(id)?;
        return Ok(false);
    }
    let name = clean_favorite_title(&name,&blocked_words_for_connection(&conn)?);
    conn.execute("INSERT INTO favorite_items (kind, label, target, service, pane, position) SELECT 'link', ?1, ?2, 'folder', 0, COALESCE(MAX(position), -1) + 1 FROM favorite_items WHERE pane = 0", params![name, path])?;
    Ok(true)
}

pub fn rename_favorite_item(id: i64, label: &str) -> Result<(), Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    conn.execute(
        "UPDATE favorite_items SET label = ?1 WHERE id = ?2",
        params![label.trim(), id],
    )?;
    Ok(())
}

pub fn delete_favorite_item(id: i64) -> Result<(), Box<dyn Error>> {
    let mut conn = Connection::open(db_path()?)?;
    let tx = conn.transaction()?;
    let target: Option<String> = tx
        .query_row(
            "SELECT target FROM favorite_items WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    tx.execute(
        "UPDATE favorite_items SET deleted_at = ?1 WHERE id = ?2",
        params![now_string(), id],
    )?;
    if let Some(target) = target {
        tx.execute(
            "UPDATE history SET is_favorite=0 WHERE lower(url)=lower(?1)",
            params![target],
        )?;
    }
    tx.execute("DELETE FROM favorite_items WHERE deleted_at IS NOT NULL AND datetime(deleted_at) < datetime('now', '-30 days')", [])?;
    tx.execute("DELETE FROM favorite_items WHERE id IN (SELECT id FROM favorite_items WHERE deleted_at IS NOT NULL ORDER BY datetime(deleted_at) DESC, id DESC LIMIT -1 OFFSET 30)", [])?;
    tx.commit()?;
    Ok(())
}

pub fn place_favorite_items(pane: i64, ids: &[i64]) -> Result<(), Box<dyn Error>> {
    let mut conn = Connection::open(db_path()?)?;
    let tx = conn.transaction()?;
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE favorite_items SET pane = ?1, position = ?2 WHERE id = ?3",
            params![pane.max(0), position as i64, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn move_board_item(id: i64, pane: i64, before_id: Option<i64>) -> Result<(), Box<dyn Error>> {
    let mut conn = Connection::open(db_path()?)?;
    conn.busy_timeout(Duration::from_secs(3))?;
    move_board_item_in_connection(&mut conn,id,pane,before_id)
}

fn move_board_item_in_connection(conn: &mut Connection, id: i64, pane: i64, before_id: Option<i64>) -> Result<(), Box<dyn Error>> {
    let tx = conn.transaction()?;
    if pane != 0 && !tx.query_row("SELECT EXISTS(SELECT 1 FROM favorite_columns WHERE id=?1)",params![pane],|r|r.get::<_,bool>(0))? { return Err("移動先のグループが見つかりません".into()); }
    let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM favorite_items WHERE id=?1 AND deleted_at IS NULL)",params![id],|r|r.get(0))?;
    if !exists {return Err("お気に入りが見つかりません".into());}
    let mut ids: Vec<i64> = {
        let mut stmt = tx.prepare("SELECT id FROM favorite_items WHERE pane=?1 AND id<>?2 AND deleted_at IS NULL ORDER BY position,id")?;
        let result = stmt.query_map(params![pane,id],|r|r.get(0))?.collect::<Result<Vec<_>,_>>()?; result
    };
    let position = before_id.and_then(|before|ids.iter().position(|value|*value==before)).unwrap_or(ids.len());
    ids.insert(position,id);
    for (position,id) in ids.iter().enumerate() { tx.execute("UPDATE favorite_items SET pane=?1,position=?2 WHERE id=?3 AND deleted_at IS NULL",params![pane,position as i64,id])?; }
    tx.commit()?; Ok(())
}

pub fn list_favorite_tabs() -> Result<Vec<FavoriteTab>, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let mut stmt =
        conn.prepare("SELECT id, name, color, position FROM favorite_tabs ORDER BY position, id")?;
    let result = stmt
        .query_map([], |r| {
            Ok(FavoriteTab {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                position: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}

pub fn place_favorite_tabs(ids: &[i64]) -> Result<(), Box<dyn Error>> {
    let mut conn = Connection::open(db_path()?)?;
    let tx = conn.transaction()?;
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE favorite_tabs SET position=?1 WHERE id=?2",
            params![position as i64, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn list_favorite_columns() -> Result<Vec<FavoriteColumn>, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let mut stmt = conn.prepare("SELECT id, tab_id, name, color, position FROM favorite_columns ORDER BY tab_id, position, id")?;
    let result = stmt
        .query_map([], |r| {
            Ok(FavoriteColumn {
                id: r.get(0)?,
                tab_id: r.get(1)?,
                name: r.get(2)?,
                color: r.get(3)?,
                position: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}

// Headings become real containers once; links retain their IDs and usage history.
pub fn prepare_favorite_board() -> Result<String, Box<dyn Error>> {
    static MIGRATION_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
    let _guard = MIGRATION_LOCK.lock().map_err(|_| "Board migration lock failed")?;
    let mut conn = Connection::open(db_path()?)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    let migrated: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM app_meta WHERE key='board_migrated')", [], |r| r.get(0))?;
    if !migrated {
        let backup = db_path()?.with_extension(format!("before-board-{}.db", Local::now().format("%Y%m%d%H%M%S")));
        conn.execute("VACUUM INTO ?1", params![backup.to_string_lossy().to_string()])?;
        migrate_board_containers(&mut conn)?;
    }
    let cleaned: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM app_meta WHERE key='stored_clean_titles')",[],|r|r.get(0))?;
    if !cleaned {
        let backup = db_path()?.with_extension(format!("before-title-cleanup-{}.db",Local::now().format("%Y%m%d%H%M%S")));
        conn.execute("VACUUM INTO ?1",params![backup.to_string_lossy().to_string()])?;
        migrate_stored_titles(&mut conn)?;
    }
    Ok(conn.query_row("SELECT value FROM app_meta WHERE key='board_layout'",[],|r|r.get(0)).optional()?.unwrap_or_else(|| "{}".into()))
}

fn migrate_board_containers(conn: &mut Connection) -> Result<(), Box<dyn Error>> {
    if conn.query_row("SELECT EXISTS(SELECT 1 FROM app_meta WHERE key='board_migrated')",[],|r|r.get::<_,bool>(0))? {return Ok(());}
    let columns: Vec<(i64,i64,String)> = {
        let mut stmt = conn.prepare("SELECT id,tab_id,name FROM favorite_columns ORDER BY tab_id,position,id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect::<Result<Vec<_>,_>>()?;
        rows
    };
    let tx = conn.transaction()?;
    tx.execute("UPDATE favorite_tabs SET name='ホーム' WHERE id=1 AND name='お気に入り'",[])?;
    tx.execute("UPDATE settings SET left_pane_percent=35 WHERE left_pane_percent=50",[])?;
    let mut positions = HashMap::<i64,i64>::new();
    let mut original_columns = HashMap::<i64,i64>::new();
    let mut group_layout = serde_json::Map::new();
    for (column,tab,name) in columns {
        let slot = original_columns.entry(tab).or_default();
        let original_slot = *slot;
        *slot += 1;
        group_layout.insert(column.to_string(),serde_json::json!({"column":original_slot}));
        let items: Vec<(i64,String,String,String)> = {
            let mut stmt = tx.prepare("SELECT id,kind,label,color FROM favorite_items WHERE pane=?1 AND deleted_at IS NULL ORDER BY position,id")?;
            let rows = stmt.query_map(params![column], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?.collect::<Result<Vec<_>,_>>()?;
            rows
        };
        let position = positions.entry(tab).or_default();
        tx.execute("UPDATE favorite_columns SET position=?1,name=?2 WHERE id=?3", params![*position,if name.starts_with("列") {"未分類"} else {&name},column])?;
        *position += 1;
        let mut current = column;
        let mut count = 0i64;
        let mut named = false;
        for (id,kind,label,color) in items {
            if kind == "heading" {
                if count == 0 && current == column && !named {
                    tx.execute("UPDATE favorite_columns SET name=?1,color=?2 WHERE id=?3",params![label,color,current])?;
                } else {
                    tx.execute("INSERT INTO favorite_columns(tab_id,name,color,position) VALUES(?1,?2,?3,?4)",params![tab,label,color,*position])?;
                    current = tx.last_insert_rowid();
                    group_layout.insert(current.to_string(),serde_json::json!({"column":original_slot}));
                    *position += 1;
                }
                tx.execute("UPDATE favorite_items SET deleted_at=?1 WHERE id=?2",params![now_string(),id])?;
                named = true;
                count = 0;
            } else {
                tx.execute("UPDATE favorite_items SET pane=?1,position=?2 WHERE id=?3",params![current,count,id])?;
                count += 1;
            }
        }
    }
    tx.execute("INSERT OR REPLACE INTO app_meta(key,value) VALUES('board_layout',?1)",params![serde_json::json!({"groups":group_layout}).to_string()])?;
    tx.execute("INSERT OR REPLACE INTO app_meta(key,value) VALUES('board_migrated','1')", [])?;
    tx.commit()?;
    Ok(())
}

pub fn save_board_layout(value: &str) -> Result<(), Box<dyn Error>> {
    let parsed: serde_json::Value = serde_json::from_str(value)?;
    if !parsed.is_object() || value.len() > 100_000 { return Err("Invalid board layout".into()); }
    Connection::open(db_path()?)?.execute("INSERT INTO app_meta(key,value) VALUES('board_layout',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![value])?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiPreferences {
    pub show_favorite_search: bool,
    pub reset_tab_after_link: bool,
}
impl Default for UiPreferences {
    fn default() -> Self { Self { show_favorite_search: true, reset_tab_after_link: false } }
}
pub fn get_ui_preferences() -> Result<UiPreferences, Box<dyn Error>> {
    let value: Option<String> = Connection::open(db_path()?)?.query_row("SELECT value FROM app_meta WHERE key='ui_preferences'",[],|r|r.get(0)).optional()?;
    Ok(match value { Some(value) => serde_json::from_str(&value)?, None => UiPreferences::default() })
}
pub fn save_ui_preferences(value: UiPreferences) -> Result<(), Box<dyn Error>> {
    Connection::open(db_path()?)?.execute("INSERT INTO app_meta(key,value) VALUES('ui_preferences',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![serde_json::to_string(&value)?])?;
    Ok(())
}
pub fn open_edge_extensions() -> Result<(), Box<dyn Error>> {
    let executable = resolve_edge_executable()?;
    let mut command = Command::new(executable);
    command.arg("edge://extensions/");
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn()?;
    Ok(())
}

fn valid_color(color: &str) -> bool {
    color.len() == 7
        && color.starts_with('#')
        && color.chars().skip(1).all(|c| c.is_ascii_hexdigit())
}

pub fn add_favorite_tab(limit: i64) -> Result<i64, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM favorite_tabs", [], |r| r.get(0))?;
    if count >= limit.clamp(1, 20) {
        return Err(format!("タブは{}個までです。", limit.clamp(1, 20)).into());
    }
    conn.execute(
        "INSERT INTO favorite_tabs(name, color, position) VALUES(?1, '#e2e8f0', ?2)",
        params![format!("タブ{}", count + 1), count],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO favorite_columns(tab_id,name,position) VALUES(?1,'列1',0)",
        params![id],
    )?;
    Ok(id)
}

pub fn update_favorite_tab(id: i64, name: &str, color: &str) -> Result<(), Box<dyn Error>> {
    if !valid_color(color) {
        return Err("Invalid color".into());
    }
    Connection::open(db_path()?)?.execute(
        "UPDATE favorite_tabs SET name=?1,color=?2 WHERE id=?3",
        params![
            if name.trim().is_empty() {
                "お気に入り"
            } else {
                name.trim()
            },
            color,
            id
        ],
    )?;
    Ok(())
}

pub fn delete_favorite_tab(id: i64) -> Result<i64, Box<dyn Error>> {
    let mut conn = Connection::open(db_path()?)?;
    let tx = conn.transaction()?;
    let tab_count: i64 = tx.query_row("SELECT COUNT(*) FROM favorite_tabs", [], |r| r.get(0))?;
    if tab_count <= 1 {
        return Err("最後のタブは削除できません。".into());
    }
    let target_id: i64 = tx.query_row(
        "SELECT id FROM favorite_tabs WHERE id<>?1 ORDER BY CASE WHEN position < (SELECT position FROM favorite_tabs WHERE id=?1) THEN 0 ELSE 1 END, ABS(position-(SELECT position FROM favorite_tabs WHERE id=?1)), position LIMIT 1",
        params![id],
        |r| r.get(0),
    )?;
    let start: i64 = tx.query_row(
        "SELECT COALESCE(MAX(position),-1)+1 FROM favorite_columns WHERE tab_id=?1",
        params![target_id],
        |r| r.get(0),
    )?;
    let mut stmt =
        tx.prepare("SELECT id FROM favorite_columns WHERE tab_id=?1 ORDER BY position,id")?;
    let column_ids = stmt
        .query_map(params![id], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for (offset, column_id) in column_ids.iter().enumerate() {
        tx.execute(
            "UPDATE favorite_columns SET tab_id=?1,position=?2 WHERE id=?3",
            params![target_id, start + offset as i64, column_id],
        )?;
    }
    let deleted = tx.execute("DELETE FROM favorite_tabs WHERE id=?1", params![id])?;
    if deleted == 0 {
        return Err("削除するタブが見つかりません。".into());
    }
    tx.commit()?;
    Ok(target_id)
}

pub fn add_favorite_column(tab_id: i64, _limit: i64) -> Result<i64, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM favorite_columns WHERE tab_id=?1",
        params![tab_id],
        |r| r.get(0),
    )?;
    if count >= 100 {
        return Err("このタブには100グループまで追加できます。".into());
    }
    conn.execute(
        "INSERT INTO favorite_columns(tab_id,name,position) VALUES(?1,?2,?3)",
        params![tab_id, format!("列{}", count + 1), count],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_favorite_column(id: i64, name: &str, color: &str) -> Result<(), Box<dyn Error>> {
    if !valid_color(color) {
        return Err("Invalid color".into());
    }
    Connection::open(db_path()?)?.execute(
        "UPDATE favorite_columns SET name=?1,color=?2 WHERE id=?3",
        params![
            if name.trim().is_empty() {
                "お気に入り"
            } else {
                name.trim()
            },
            color,
            id
        ],
    )?;
    Ok(())
}

pub fn move_favorite_column(id: i64, tab_id: i64, _limit: i64) -> Result<(), Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM favorite_columns WHERE tab_id=?1 AND id<>?2",
        params![tab_id, id],
        |r| r.get(0),
    )?;
    if count >= 100 {
        return Err("移動先のタブは100グループに達しています。".into());
    }
    conn.execute(
        "UPDATE favorite_columns SET tab_id=?1,position=?2 WHERE id=?3",
        params![tab_id, count, id],
    )?;
    Ok(())
}
pub fn place_favorite_columns(tab_id: i64, ids: &[i64]) -> Result<(), Box<dyn Error>> {
    let mut conn = Connection::open(db_path()?)?;
    let tx = conn.transaction()?;
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE favorite_columns SET tab_id=?1,position=?2 WHERE id=?3",
            params![tab_id, position as i64, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}
pub fn delete_favorite_group(id: i64) -> Result<usize,Box<dyn Error>> {
    let mut conn=Connection::open(db_path()?)?;
    delete_favorite_group_in_connection(&mut conn,id)
}
fn delete_favorite_group_in_connection(conn: &mut Connection,id:i64) -> Result<usize,Box<dyn Error>> {
    let tx=conn.transaction()?;
    let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM favorite_columns WHERE id=?1)",params![id],|r|r.get(0))?;
    if !exists{return Err("グループが見つかりません。".into());}
    tx.execute("UPDATE history SET is_favorite=0 WHERE lower(url) IN (SELECT lower(target) FROM favorite_items WHERE pane=?1 AND deleted_at IS NULL)",params![id])?;
    let count=tx.execute("UPDATE favorite_items SET deleted_at=?1 WHERE pane=?2 AND deleted_at IS NULL",params![now_string(),id])?;
    tx.execute("DELETE FROM favorite_columns WHERE id=?1",params![id])?;
    tx.commit()?;
    Ok(count)
}
pub fn delete_favorite_column(id: i64, target_pane: i64) -> Result<(), Box<dyn Error>> {
    let mut conn = Connection::open(db_path()?)?;
    let tx = conn.transaction()?;
    let start: i64 = tx.query_row(
        "SELECT COALESCE(MAX(position),-1)+1 FROM favorite_items WHERE pane=?1",
        params![target_pane],
        |r| r.get(0),
    )?;
    let mut stmt = tx.prepare(
        "SELECT id FROM favorite_items WHERE pane=?1 AND deleted_at IS NULL ORDER BY position,id",
    )?;
    let ids = stmt
        .query_map(params![id], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for (offset, item_id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE favorite_items SET pane=?1,position=?2 WHERE id=?3",
            params![target_pane, start + offset as i64, item_id],
        )?;
    }
    tx.execute("DELETE FROM favorite_columns WHERE id=?1", params![id])?;
    tx.commit()?;
    Ok(())
}

pub fn record_favorite_open(id: i64) -> Result<(), Box<dyn Error>> {
    Connection::open(db_path()?)?.execute("UPDATE favorite_items SET opened_count=COALESCE(opened_count,0)+1,last_opened_at=?1 WHERE id=?2",params![now_string(),id])?;
    Ok(())
}

pub fn list_deleted_favorites() -> Result<Vec<FavoriteItem>, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let mut stmt=conn.prepare("SELECT f.id,f.kind,NULL,f.label,f.service,f.target,f.position,f.pane,f.color,COALESCE(f.opened_count,0),f.last_opened_at,f.deleted_at FROM favorite_items f WHERE f.deleted_at IS NOT NULL AND f.kind='link' AND f.target IS NOT NULL ORDER BY datetime(f.deleted_at) DESC,f.id DESC LIMIT 30")?;
    let result = stmt
        .query_map([], |row| {
            Ok(FavoriteItem {
                id: row.get(0)?,
                kind: row.get(1)?,
                history_id: row.get(2)?,
                label: row.get(3)?,
                service: row.get(4)?,
                target: row.get(5)?,
                position: row.get(6)?,
                pane: row.get(7)?,
                color: row.get(8)?,
                opened_count: row.get(9)?,
                last_opened_at: row.get(10)?,
                deleted_at: row.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}
pub fn restore_deleted_favorite(id: i64) -> Result<(), Box<dyn Error>> {
    let mut conn = Connection::open(db_path()?)?;
    let tx = conn.transaction()?;
    let target: Option<String> = tx.query_row(
        "SELECT target FROM favorite_items WHERE id=?1 AND deleted_at IS NOT NULL",
        params![id],
        |r| r.get(0),
    )?;
    if let Some(target) = &target {
        let duplicate:Option<i64>=tx.query_row("SELECT id FROM favorite_items WHERE deleted_at IS NULL AND kind='link' AND lower(target)=lower(?1) LIMIT 1",params![target],|r|r.get(0)).optional()?;
        if duplicate.is_some() {
            return Err("同じリンクがすでにお気に入りにあります。".into());
        }
    }
    tx.execute(
        "UPDATE favorite_items SET deleted_at=NULL WHERE id=?1",
        params![id],
    )?;
    if let Some(target) = target {
        tx.execute(
            "UPDATE history SET is_favorite=1 WHERE lower(url)=lower(?1)",
            params![target],
        )?;
    }
    tx.commit()?;
    Ok(())
}
pub fn permanently_delete_favorite(id: i64) -> Result<(), Box<dyn Error>> {
    Connection::open(db_path()?)?.execute(
        "DELETE FROM favorite_items WHERE id=?1 AND deleted_at IS NOT NULL",
        params![id],
    )?;
    Ok(())
}

pub fn save_window_size(size: &str) -> Result<(), Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    conn.execute(
        "UPDATE settings SET window_size = ?1 WHERE id = 1",
        params![sanitize_window_size(size)],
    )?;
    Ok(())
}

pub fn get_edge_favicon(url: &str) -> Result<Option<String>, Box<dyn Error>> {
    if let Ok(cache) = FAVICON_CACHE.lock() {
        if let Some(value) = cache.get(url) {
            return Ok(value.clone());
        }
    }
    let app_db = db_path()?;
    if let Some(icon) = Connection::open(&app_db)?
        .query_row(
            "SELECT icon_data FROM web_icon_cache WHERE page_url=?1",
            params![url],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        if let Ok(mut cache) = FAVICON_CACHE.lock() {
            cache.insert(url.to_string(), Some(icon.clone()));
        }
        return Ok(Some(icon));
    }
    let mut found = None;
    let origin_pattern = url
        .split_once("://")
        .and_then(|(scheme, rest)| {
            rest.split('/')
                .next()
                .map(|host| format!("{}://{}%", scheme, host))
        })
        .unwrap_or_else(|| format!("{}%", url));
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let root = PathBuf::from(local)
            .join("Microsoft")
            .join("Edge")
            .join("User Data");
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name != "Default" && !name.starts_with("Profile ") {
                    continue;
                }
                let db = entry.path().join("Favicons");
                if !db.exists() {
                    continue;
                }
                let uri = format!(
                    "file:{}?mode=ro&immutable=1",
                    db.to_string_lossy().replace('\\', "/")
                );
                let Ok(conn) = Connection::open_with_flags(
                    uri,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                        | rusqlite::OpenFlags::SQLITE_OPEN_URI,
                ) else {
                    continue;
                };
                let data:Option<Vec<u8>>=conn.query_row(
                    "SELECT b.image_data FROM icon_mapping m JOIN favicon_bitmaps b ON b.icon_id=m.icon_id WHERE (lower(m.page_url)=lower(?1) OR lower(m.page_url) LIKE lower(?2)) AND length(b.image_data)>0 ORDER BY CASE WHEN lower(m.page_url)=lower(?1) THEN 0 ELSE 1 END,b.width DESC,b.last_updated DESC LIMIT 1",
                    params![url, origin_pattern],|r|r.get(0)).optional().unwrap_or(None);
                if let Some(bytes) = data {
                    let mime = if bytes.starts_with(&[0, 0, 1, 0]) {
                        "image/x-icon"
                    } else if bytes.starts_with(b"GIF") {
                        "image/gif"
                    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
                        "image/jpeg"
                    } else {
                        "image/png"
                    };
                    found = Some(format!(
                        "data:{};base64,{}",
                        mime,
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    ));
                    break;
                }
            }
        }
    }
    if let Some(icon) = &found {
        let conn = Connection::open(app_db)?;
        conn.execute(
            "INSERT INTO web_icon_cache(page_url,icon_data,updated_at) VALUES(?1,?2,?3) ON CONFLICT(page_url) DO UPDATE SET icon_data=excluded.icon_data,updated_at=excluded.updated_at",
            params![url, icon, now_string()],
        )?;
        if let Ok(mut cache) = FAVICON_CACHE.lock() {
            cache.insert(url.to_string(), Some(icon.clone()));
        }
    }
    Ok(found)
}

pub fn get_file_icon(path: &str) -> Result<Option<String>, Box<dyn Error>> {
    let p = Path::new(path);
    // Shell icons can vary per directory, executable and shortcut, not just extension.
    let key = p.to_string_lossy().to_ascii_lowercase();
    if let Ok(cache) = FILE_ICON_CACHE.lock() {
        if let Some(value) = cache.get(&key) {
            return Ok(value.clone());
        }
    }
    #[cfg(target_os = "windows")]
    let value = windows_file_icon_data_url(path)?;
    #[cfg(not(target_os = "windows"))]
    let value = None;
    if let Ok(mut cache) = FILE_ICON_CACHE.lock() {
        cache.insert(key, value.clone());
    }
    Ok(value)
}

pub fn get_favorite_text_styles() -> Result<serde_json::Value, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    let mut stmt = conn.prepare("SELECT key,value FROM app_meta WHERE key LIKE 'favorite_text_style_%'")?;
    let mut styles = serde_json::Map::new();
    for row in stmt.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))? {
        let (key,value) = row?;
        if let Ok(value) = serde_json::from_str(&value) { styles.insert(key.trim_start_matches("favorite_text_style_").to_string(),value); }
    }
    Ok(serde_json::Value::Object(styles))
}

pub fn set_favorite_text_style(id: i64, color: &str, bold: bool) -> Result<(), Box<dyn Error>> {
    if !color.is_empty() && !(color.len()==7 && color.starts_with('#') && color[1..].chars().all(|c|c.is_ascii_hexdigit())) { return Err("Invalid text color".into()); }
    Connection::open(db_path()?)?.execute("INSERT INTO app_meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("favorite_text_style_{id}"),serde_json::json!({"color":color,"bold":bold}).to_string()])?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_file_icon_data_url(path: &str) -> Result<Option<String>, Box<dyn Error>> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON};
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};
    let wide: Vec<u16> = std::ffi::OsStr::new(path)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut info = SHFILEINFOW::default();
    let result = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            Default::default(),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_SMALLICON,
        )
    };
    if result == 0 || info.hIcon.0 == 0 {
        return Ok(None);
    }
    let mut icon = ICONINFO::default();
    if !unsafe { GetIconInfo(info.hIcon, &mut icon) }.as_bool() {
        unsafe { DestroyIcon(info.hIcon) };
        return Ok(None);
    }
    let mut bitmap = BITMAP::default();
    unsafe {
        GetObjectW(
            icon.hbmColor,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut _ as *mut _),
        );
    }
    let width = bitmap.bmWidth.max(1) as u32;
    let height = bitmap.bmHeight.max(1) as u32;
    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width as i32,
        biHeight: -(height as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0 as u32,
        ..Default::default()
    };
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let dc = unsafe { GetDC(HWND(0)) };
    let lines = unsafe {
        GetDIBits(
            dc,
            icon.hbmColor,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    unsafe {
        ReleaseDC(HWND(0), dc);
        DeleteObject(icon.hbmColor);
        DeleteObject(icon.hbmMask);
        DestroyIcon(info.hIcon);
    }
    if lines == 0 {
        return Ok(None);
    }
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        if pixel[3] == 0 {
            pixel[3] = 255;
        }
    }
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&pixels)?;
    }
    Ok(Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png_bytes)
    )))
}

struct PendingIndexEntry {
    name: String,
    parent: String,
    is_directory: bool,
    modified_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexUpdateSummary {
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
}

fn collect_index_entries(root: &Path, output: &mut HashMap<String, PendingIndexEntry>) -> bool {
    let mut stack = vec![root.to_path_buf()];
    let mut complete = true;
    while let Some(directory) = stack.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(v) => v,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().map(|kind|kind.is_symlink()).unwrap_or(false) {continue;}
            let metadata = match entry.metadata() {
                Ok(v) => v,
                Err(_) => {
                    complete = false;
                    continue;
                }
            };
            let is_directory = metadata.is_dir();
            let name = entry.file_name().to_string_lossy().to_string();
            // Generated trees are not useful user documents and can contain millions of files.
            if is_directory && matches!(name.to_lowercase().as_str(), ".git" | "node_modules" | "target" | "$recycle.bin" | "system volume information" | ".cache") { continue; }
            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|v| v.duration_since(UNIX_EPOCH).ok())
                .map(|v| v.as_secs().to_string());
            output.insert(
                path.to_string_lossy().to_string(),
                PendingIndexEntry {
                    name,
                    parent: directory.to_string_lossy().to_string(),
                    is_directory,
                    modified_at,
                },
            );
            if is_directory {
                stack.push(path);
            }
        }
    }
    complete
}

pub fn rebuild_file_index(folders: &[String]) -> Result<IndexUpdateSummary, Box<dyn Error>> {
    let mut pending = HashMap::new();
    let mut valid_roots = Vec::new();
    let mut incomplete_roots = Vec::new();
    for folder in folders.iter().take(10) {
        let path = Path::new(folder);
        if path.is_dir() {
            let root = path.to_string_lossy().to_string();
            if !collect_index_entries(path, &mut pending) {
                incomplete_roots.push(root.clone());
            }
            valid_roots.push(root);
        }
    }
    if valid_roots.is_empty() {
        return Err("有効なインデックス対象フォルダがありません。".into());
    }
    let scanned = pending.len();
    let mut conn = Connection::open(db_path()?)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    let mut updates: Vec<(i64, PendingIndexEntry)> = Vec::new();
    let mut deletions: Vec<i64> = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, name, path, parent, is_directory, modified_at FROM file_index")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)? != 0,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        for row in rows {
            let (id, old_name, path, old_parent, old_is_directory, old_modified_at) = row?;
            let normalized_path = path.to_ascii_lowercase();
            let in_current_roots = valid_roots.iter().any(|root| {
                let normalized_root = root.trim_end_matches(['\\', '/']).to_ascii_lowercase();
                normalized_path == normalized_root
                    || normalized_path.starts_with(&format!("{}\\", normalized_root))
                    || normalized_path.starts_with(&format!("{}/", normalized_root))
            });
            if !in_current_roots {
                continue;
            }
            let in_incomplete_root = incomplete_roots.iter().any(|root| {
                let root = root.trim_end_matches(['\\', '/']).to_ascii_lowercase();
                normalized_path == root
                    || normalized_path.starts_with(&format!("{}\\", root))
                    || normalized_path.starts_with(&format!("{}/", root))
            });
            match pending.remove(&path) {
                Some(item)
                    if item.name == old_name
                        && item.parent == old_parent
                        && item.is_directory == old_is_directory
                        && item.modified_at == old_modified_at => {}
                Some(item) => updates.push((id, item)),
                None if !in_incomplete_root => deletions.push(id),
                None => {}
            }
        }
    }
    let added = pending.len();
    let updated = updates.len();
    let deleted = deletions.len();
    let tx = conn.transaction()?;
    {
        let mut insert_stmt = tx.prepare("INSERT INTO file_index (name, path, parent, is_directory, modified_at) VALUES (?1, ?2, ?3, ?4, ?5)")?;
        for (path, item) in pending {
            insert_stmt.execute(params![
                item.name,
                path,
                item.parent,
                if item.is_directory { 1 } else { 0 },
                item.modified_at
            ])?;
        }
        let mut update_stmt = tx.prepare(
            "UPDATE file_index SET name=?1, parent=?2, is_directory=?3, modified_at=?4 WHERE id=?5",
        )?;
        for (id, item) in updates {
            update_stmt.execute(params![
                item.name,
                item.parent,
                if item.is_directory { 1 } else { 0 },
                item.modified_at,
                id
            ])?;
        }
        let mut delete_stmt = tx.prepare("DELETE FROM file_index WHERE id=?1")?;
        for id in deletions {
            delete_stmt.execute(params![id])?;
        }
    }
    if !valid_roots.is_empty() {
        tx.execute("INSERT INTO app_meta(key,value) VALUES('file_index_initialized',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![now_string()])?;
    }
    tx.commit()?;
    Ok(IndexUpdateSummary {
        scanned,
        added,
        updated,
        deleted,
    })
}

pub fn is_file_index_initialized() -> Result<bool, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    Ok(conn
        .query_row(
            "SELECT value FROM app_meta WHERE key='file_index_initialized'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub fn search_file_index(query: &str, filter: &str) -> Result<Vec<FileSearchRecord>, Box<dyn Error>> {
    let conn = Connection::open(db_path()?)?;
    search_file_rows(&conn,query,filter)
}

fn search_file_rows(conn: &Connection, query: &str, filter: &str) -> Result<Vec<FileSearchRecord>, Box<dyn Error>> {
    let terms: Vec<_> = query
        .to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect();
    let mut clauses = Vec::new();
    let mut values = Vec::new();
    for term in terms {
        clauses.push("(lower(i.name) LIKE ? OR lower(i.path) LIKE ?)");
        let p = format!("%{}%", term);
        values.push(p.clone());
        values.push(p);
    }
    let where_sql = if clauses.is_empty() {
        "1=1".to_string()
    } else {
        clauses.join(" AND ")
    };
    let mut type_clause = match filter {
        "folders" => "i.is_directory=1",
        "excel" => "(lower(i.name) LIKE '%.xlsx' OR lower(i.name) LIKE '%.xls' OR lower(i.name) LIKE '%.xlsm' OR lower(i.name) LIKE '%.csv')",
        "pdf" => "lower(i.name) LIKE '%.pdf'",
        "all" => "1=1",
        _ => "(i.is_directory=1 OR lower(i.name) LIKE '%.pdf' OR lower(i.name) LIKE '%.doc%' OR lower(i.name) LIKE '%.xls%' OR lower(i.name) LIKE '%.ppt%' OR lower(i.name) LIKE '%.csv' OR lower(i.name) LIKE '%.txt')",
    }.to_string();
    if let Some(custom) = filter.strip_prefix("custom:") {
        let clauses:Vec<_> = custom.split(',').take(20).filter(|ext|!ext.is_empty() && ext.len()<=12 && ext.bytes().all(|c|c.is_ascii_alphanumeric())).map(|ext|format!("lower(i.name) LIKE '%.{}'",ext.to_ascii_lowercase())).collect();
        type_clause = if clauses.is_empty() {"0=1".into()} else {format!("({})",clauses.join(" OR "))};
    }
    let order = if query.trim().is_empty() {
        "i.is_directory ASC, CAST(i.modified_at AS INTEGER) DESC, i.name COLLATE NOCASE"
    } else {
        values.push(query.to_lowercase());
        values.push(format!("{}%",query.to_lowercase()));
        values.push(format!("%{}%",query.to_lowercase()));
        "CASE WHEN lower(i.name)=? THEN 0 WHEN lower(i.name) LIKE ? THEN 1 WHEN lower(i.name) LIKE ? THEN 2 ELSE 3 END, i.is_directory DESC, i.name COLLATE NOCASE"
    };
    let sql = format!("SELECT i.id,i.name,i.path,i.parent,i.is_directory,i.modified_at, EXISTS(SELECT 1 FROM favorite_items f WHERE f.target=i.path AND f.deleted_at IS NULL AND f.service='folder') FROM file_index i WHERE ({where_sql}) AND {type_clause} ORDER BY {order} LIMIT 200");
    let refs: Vec<&dyn rusqlite::ToSql> =
        values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), |r| {
        Ok(FileSearchRecord {
            id: r.get(0)?,
            name: r.get(1)?,
            path: r.get(2)?,
            parent: r.get(3)?,
            is_directory: r.get::<_, i64>(4)? != 0,
            modified_at: r.get(5)?,
            is_favorite: r.get::<_, i64>(6)? != 0,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn get_file_filter() -> Result<String,Box<dyn Error>> {
    Ok(Connection::open(db_path()?)?.query_row("SELECT value FROM app_meta WHERE key='file_filter'",[],|r|r.get(0)).optional()?.unwrap_or_else(||"documents".into()))
}

pub fn save_file_filter(value: &str) -> Result<(),Box<dyn Error>> {
    if value.len()>300 {return Err("ファイル種類の指定が長すぎます".into());}
    Connection::open(db_path()?)?.execute("INSERT INTO app_meta(key,value) VALUES('file_filter',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![value])?;
    Ok(())
}

pub fn get_favorites(service: &str) -> Result<Vec<SearchRecord>, Box<dyn Error>> {
    let path = db_path()?;
    let conn = Connection::open(path)?;
    let mut stmt = conn.prepare(
        "SELECT id, service, title, site, url, first_seen, last_access, access_count, is_favorite
         FROM history
         WHERE is_favorite = 1 AND service = ?1
         ORDER BY last_access DESC",
    )?;

    let rows = stmt.query_map(params![service], map_search_record)?;
    collect_search_rows_tolerant(rows)
}

pub fn read_edge_history() -> Result<Vec<EdgeRecord>, Box<dyn Error>> {
    let user_data_dir = edge_user_data_dir()?;
    let profiles = enumerate_edge_profiles(&user_data_dir)?;
    let first_profile = profiles
        .first()
        .ok_or("Edgeの対象プロファイルが見つかりませんでした。")?;
    read_edge_history_from_path(&user_data_dir.join(first_profile).join("History"))
}

pub fn read_edge_history_from_path(path: &Path) -> Result<Vec<EdgeRecord>, Box<dyn Error>> {
    let rows = read_edge_history_rows_from_path(path)?;
    Ok(rows
        .into_iter()
        .map(|row| EdgeRecord {
            title: if row.title.trim().is_empty() {
                row.url.clone()
            } else {
                row.title
            },
            url: row.url,
            last_visit: webkit_time_to_local_string(row.last_visit_time),
        })
        .collect())
}

pub fn read_edge_history_stub() -> Vec<EdgeRecord> {
    vec![
        EdgeRecord {
            title: "Example Page - Waveform".to_string(),
            url: "https://example.com/waveform".to_string(),
            last_visit: Some("2026-08-10 12:00:00".to_string()),
        },
        EdgeRecord {
            title: "Asana: Task - Evaluation".to_string(),
            url: "https://app.asana.com/0/123/456".to_string(),
            last_visit: Some("2026-08-09 09:30:00".to_string()),
        },
        EdgeRecord {
            title: "Notion - Project Design".to_string(),
            url: "https://www.notion.so/project-design".to_string(),
            last_visit: Some("2026-08-08 15:20:00".to_string()),
        },
    ]
}

fn map_search_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchRecord> {
    Ok(SearchRecord {
        id: row.get(0)?,
        service: row.get(1)?,
        title: row.get(2)?,
        site: row.get(3).optional()?,
        url: row.get(4)?,
        first_seen: row.get(5).optional()?,
        last_access: row.get(6).optional()?,
        access_count: row.get(7)?,
        is_favorite: row.get(8)?,
    })
}

fn collect_search_rows<F>(
    rows: rusqlite::MappedRows<'_, F>,
) -> Result<Vec<SearchRecord>, Box<dyn Error>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<SearchRecord>,
{
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

fn collect_search_rows_tolerant<F>(
    rows: rusqlite::MappedRows<'_, F>,
) -> Result<Vec<SearchRecord>, Box<dyn Error>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<SearchRecord>,
{
    let mut results = Vec::new();
    let mut skipped = 0usize;
    for row in rows {
        match row {
            Ok(item) => results.push(item),
            Err(error) => {
                skipped += 1;
                eprintln!("Skipped malformed history row: {}", error);
            }
        }
    }
    if skipped > 0 {
        eprintln!("History loading skipped {} malformed rows", skipped);
    }
    Ok(results)
}

fn sanitize_lines(values: Vec<String>, limit: usize) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .take(limit)
        .collect()
}

fn sanitize_keyword(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else if character.is_whitespace() {
                '-'
            } else {
                '-'
            }
        })
        .collect()
}

fn extract_site_from_url(url: &str) -> Option<String> {
    let (_, remainder) = url.split_once("://").unwrap_or(("", url));
    let host = remainder.split('/').next().unwrap_or("").trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn classify_service_from_url(url: &str) -> &'static str {
    let lowered = url.to_ascii_lowercase();
    if lowered.contains("asana.com") {
        "asana"
    } else if lowered.contains("notion.so")
        || lowered.contains("notion.site")
        || lowered.contains("notion.com")
        || lowered.contains("app.notion.com")
    {
        "notion"
    } else if lowered.contains("sharepoint.com") {
        "spo"
    } else {
        "edge"
    }
}

fn webkit_time_to_local_string(value: i64) -> Option<String> {
    if value <= 0 {
        return None;
    }

    let microseconds = value as i128;
    let seconds = (microseconds / 1_000_000) - 11_644_473_600i128;
    let remaining_microseconds = (microseconds % 1_000_000) as u32;
    let seconds_i64 = seconds as i64;

    DateTime::<Utc>::from_timestamp(seconds_i64, remaining_microseconds * 1000).map(|utc_time| {
        utc_time
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    })
}

fn now_string() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn unique_stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod board_tests {
    use super::*;

    fn fixture() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE app_meta(key TEXT PRIMARY KEY,value TEXT);
            CREATE TABLE settings(id INTEGER PRIMARY KEY,left_pane_percent INTEGER,blocked_words TEXT,site_names TEXT);
            INSERT INTO settings VALUES(1,50,'[]','[]');
            CREATE TABLE favorite_tabs(id INTEGER PRIMARY KEY,name TEXT);
            CREATE TABLE favorite_columns(id INTEGER PRIMARY KEY,tab_id INTEGER,name TEXT,color TEXT,position INTEGER);
            CREATE TABLE favorite_items(id INTEGER PRIMARY KEY,kind TEXT,label TEXT,color TEXT,pane INTEGER,position INTEGER,deleted_at TEXT,target TEXT,service TEXT);
            CREATE TABLE file_index(id INTEGER PRIMARY KEY,name TEXT,path TEXT,parent TEXT,is_directory INTEGER,modified_at TEXT);
            INSERT INTO favorite_tabs VALUES(1,'お気に入り');
            INSERT INTO favorite_columns VALUES(1,1,'列1','#eeeeee',0);
            INSERT INTO favorite_items VALUES(10,'link','先頭リンク','#fff',1,0,NULL,'https://first.test','edge');
            INSERT INTO favorite_items VALUES(11,'heading','案件A','#dbeafe',1,1,NULL,NULL,NULL);
            INSERT INTO favorite_items VALUES(12,'link','案件Aリンク','#fff',1,2,NULL,'https://a.test','edge');
            INSERT INTO favorite_items VALUES(13,'heading','空グループ','#ffeedd',1,3,NULL,NULL,NULL);
            INSERT INTO favorite_items VALUES(14,'heading','案件B','#eeffee',1,4,NULL,NULL,NULL);
            INSERT INTO favorite_items VALUES(15,'link','案件Bリンク','#fff',1,5,NULL,'https://b.test','edge');
            INSERT INTO favorite_items VALUES(16,'link','削除済','#fff',1,6,'old','https://deleted.test','edge');").unwrap();
        conn
    }

    #[test]
    fn deleting_group_deletes_all_contents_without_moving_or_affecting_other_groups() {
        let mut conn=fixture();
        conn.execute_batch("CREATE TABLE history(url TEXT,is_favorite INTEGER); INSERT INTO history VALUES('https://first.test',1),('https://other.test',1); INSERT INTO favorite_columns VALUES(2,1,'別グループ','#fff',1); INSERT INTO favorite_items VALUES(20,'link','別リンク','#fff',2,0,NULL,'https://other.test','edge');").unwrap();
        assert_eq!(delete_favorite_group_in_connection(&mut conn,1).unwrap(),6);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM favorite_items WHERE pane=1 AND deleted_at IS NULL",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM favorite_items WHERE pane=0",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM favorite_items WHERE id=20 AND deleted_at IS NULL",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(conn.query_row("SELECT is_favorite FROM history WHERE url='https://first.test'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(conn.query_row("SELECT is_favorite FROM history WHERE url='https://other.test'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert!(delete_favorite_group_in_connection(&mut conn,1).is_err());
    }
    #[test]
    fn titles_are_stored_clean_and_old_sites_migrate_without_losing_targets() {
        let mut conn=fixture();
        conn.execute("UPDATE settings SET blocked_words='[\"不要\"]',site_names='[\"Notion\"]'",[]).unwrap();
        let id=add_manual_favorite_in_connection(&conn,"不要 - 工程表 - NOTION","https://clean.test",0).unwrap();
        assert_eq!(conn.query_row("SELECT label FROM favorite_items WHERE id=?1",params![id],|r|r.get::<_,String>(0)).unwrap(),"工程表 NOTION");
        migrate_stored_titles(&mut conn).unwrap();
        assert_eq!(conn.query_row("SELECT label FROM favorite_items WHERE id=?1",params![id],|r|r.get::<_,String>(0)).unwrap(),"工程表");
        assert_eq!(conn.query_row("SELECT target FROM favorite_items WHERE id=?1",params![id],|r|r.get::<_,String>(0)).unwrap(),"https://clean.test");
        assert_eq!(clean_favorite_title("不要",&["不要".into()]),"名称未設定");
    }
    #[test]
    fn manual_favorites_stay_in_search_inbox_and_preferences_roundtrip() {
        let conn = fixture();
        let id = add_manual_favorite_in_connection(&conn,"手動リンク","https://manual.test",0).unwrap();
        assert_eq!(conn.query_row("SELECT pane FROM favorite_items WHERE id=?1",params![id],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert!(add_manual_favorite_in_connection(&conn,"重複","https://manual.test",0).is_err());
        let preferences = UiPreferences {show_favorite_search:false,reset_tab_after_link:true};
        let loaded: UiPreferences = serde_json::from_str(&serde_json::to_string(&preferences).unwrap()).unwrap();
        assert!(!loaded.show_favorite_search && loaded.reset_tab_after_link);
        let defaults: UiPreferences = serde_json::from_str("{}").unwrap();
        assert!(defaults.show_favorite_search && !defaults.reset_tab_after_link);
    }
    #[test]
    fn headings_become_containers_without_losing_links_and_migration_is_idempotent() {
        let mut conn = fixture();
        migrate_board_containers(&mut conn).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM favorite_columns",[],|r|r.get(0)).unwrap();
        assert_eq!(count,4);
        for (id,name) in [(10,"未分類"),(12,"案件A"),(15,"案件B")] {
            let group: String = conn.query_row("SELECT c.name FROM favorite_items i JOIN favorite_columns c ON c.id=i.pane WHERE i.id=?1 AND i.deleted_at IS NULL",params![id],|r|r.get(0)).unwrap();
            assert_eq!(group,name);
        }
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM favorite_items WHERE kind='link' AND deleted_at IS NULL",[],|r|r.get::<_,i64>(0)).unwrap(),3);
        let layout: String = conn.query_row("SELECT value FROM app_meta WHERE key='board_layout'",[],|r|r.get(0)).unwrap();
        let layout: serde_json::Value = serde_json::from_str(&layout).unwrap();
        assert_eq!(layout["groups"].as_object().unwrap().len(),4);
        assert!(layout["groups"].as_object().unwrap().values().all(|group| group["column"] == 0));
        migrate_board_containers(&mut conn).unwrap();
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM favorite_columns",[],|r|r.get::<_,i64>(0)).unwrap(),count);
    }

    #[test]
    fn document_filter_hides_logs_and_filename_matches_rank_above_parent_matches() {
        let conn=fixture();
        conn.execute_batch("INSERT INTO file_index VALUES(1,'trace.log','C:/案件A/trace.log','C:/案件A',0,NULL);
            INSERT INTO file_index VALUES(2,'案件A.xlsx','C:/docs/案件A.xlsx','C:/docs',0,NULL);
            INSERT INTO file_index VALUES(3,'other.pdf','C:/案件A/other.pdf','C:/案件A',0,NULL);").unwrap();
        let documents=search_file_rows(&conn,"案件A","documents").unwrap();
        assert_eq!(documents.iter().map(|r|r.id).collect::<Vec<_>>(),vec![2,3]);
        assert_eq!(search_file_rows(&conn,"案件A","all").unwrap().len(),3);
        assert_eq!(search_file_rows(&conn,"","excel").unwrap()[0].id,2);
        assert_eq!(search_file_rows(&conn,"","pdf").unwrap()[0].id,3);
        conn.execute("INSERT INTO file_index VALUES(4,'drawing.dwg','C:/docs/drawing.dwg','C:/docs',0,NULL)",[]).unwrap();
        assert_eq!(search_file_rows(&conn,"","custom:dwg,msg").unwrap()[0].id,4);
        assert!(search_file_rows(&conn,"","custom:dwg';DROP TABLE file_index;--").unwrap().is_empty());
    }

    #[test]
    fn moving_links_is_atomic_and_never_revives_deleted_items() {
        let mut conn = fixture();
        migrate_board_containers(&mut conn).unwrap();
        move_board_item_in_connection(&mut conn,12,1,Some(10)).unwrap();
        let order:Vec<i64> = conn.prepare("SELECT id FROM favorite_items WHERE pane=1 AND deleted_at IS NULL ORDER BY position").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
        assert_eq!(order,vec![12,10]);
        assert!(move_board_item_in_connection(&mut conn,12,999,None).is_err());
        assert_eq!(conn.query_row("SELECT pane FROM favorite_items WHERE id=12",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert!(move_board_item_in_connection(&mut conn,16,1,None).is_err());
    }
}

fn trigger_summary(trigger: &str) -> &'static str {
    match trigger {
        "起動時" => "起動時更新を完了しました",
        "手動" => "手動更新を完了しました",
        _ => "更新を完了しました",
    }
}

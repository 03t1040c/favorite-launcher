mod db;

use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_global_shortcut::Builder as GlobalShortcutBuilder;

// Global storage for app handle to access paths
static APP_HANDLE: Lazy<Mutex<Option<tauri::AppHandle>>> = Lazy::new(|| Mutex::new(None));
static APP_EXITING: AtomicBool = AtomicBool::new(false);
static FILE_INDEX_RUNNING: AtomicBool = AtomicBool::new(false);

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn init_db() -> Result<String, String> {
    db::init_and_seed_db()
        .map(|path| format!("Initialized database at {}", path.display()))
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn fetch_history() -> Result<Vec<db::SearchRecord>, String> {
    tauri::async_runtime::spawn_blocking(|| db::fetch_history().map_err(|e|e.to_string())).await.map_err(|e|e.to_string())?
}

#[tauri::command]
fn get_settings() -> Result<db::AppSettingsResponse, String> {
    db::get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_settings(settings: db::AppSettings) -> Result<db::AppSettings, String> {
    db::save_settings(settings).map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_history(query: String) -> Result<Vec<db::SearchRecord>, String> {
    tauri::async_runtime::spawn_blocking(move || db::search_history(&query).map_err(|e|e.to_string())).await.map_err(|e|e.to_string())?
}

#[tauri::command]
async fn toggle_favorite(id: i64) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || db::toggle_favorite(id).map_err(|e|e.to_string())).await.map_err(|e|e.to_string())?
}

#[tauri::command]
fn get_favorites(service: String) -> Result<Vec<db::SearchRecord>, String> {
    db::get_favorites(&service).map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_favorite_items() -> Result<Vec<db::FavoriteItem>, String> {
    tauri::async_runtime::spawn_blocking(|| db::list_favorite_items().map_err(|e|e.to_string())).await.map_err(|e|e.to_string())?
}

#[tauri::command]
fn list_favorite_tabs() -> Result<Vec<db::FavoriteTab>, String> {
    db::list_favorite_tabs().map_err(|e| e.to_string())
}
#[tauri::command]
fn place_favorite_tabs(ids: Vec<i64>) -> Result<(), String> {
    db::place_favorite_tabs(&ids).map_err(|e| e.to_string())
}
#[tauri::command]
fn list_favorite_columns() -> Result<Vec<db::FavoriteColumn>, String> {
    db::list_favorite_columns().map_err(|e| e.to_string())
}
#[tauri::command]
async fn prepare_favorite_board() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| db::prepare_favorite_board().map_err(|e| e.to_string())).await.map_err(|e|e.to_string())?
}
#[tauri::command]
fn save_board_layout(value: String) -> Result<(), String> {
    db::save_board_layout(&value).map_err(|e|e.to_string())
}
#[tauri::command]
fn get_ui_preferences() -> Result<db::UiPreferences, String> {
    db::get_ui_preferences().map_err(|e|e.to_string())
}
#[tauri::command]
fn save_ui_preferences(preferences: db::UiPreferences) -> Result<(), String> {
    db::save_ui_preferences(preferences).map_err(|e|e.to_string())
}
#[tauri::command]
async fn move_board_item(id: i64, pane: i64, before_id: Option<i64>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || db::move_board_item(id,pane,before_id).map_err(|e|e.to_string())).await.map_err(|e|e.to_string())?
}
#[tauri::command]
fn open_edge_extensions() -> Result<(), String> {
    db::open_edge_extensions().map_err(|e|e.to_string())
}
#[tauri::command]
fn get_file_filter() -> Result<String, String> { db::get_file_filter().map_err(|e|e.to_string()) }
#[tauri::command]
fn save_file_filter(value: String) -> Result<(), String> { db::save_file_filter(&value).map_err(|e|e.to_string()) }
#[tauri::command]
fn configure_store_extension(id: String) -> Result<(), String> {
    if id.len()!=32 || !id.bytes().all(|v|(b'a'..=b'p').contains(&v)) { return Err("拡張機能IDはa〜pの32文字で入力してください。".into()); }
    let dir = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").map_err(|e|e.to_string())?).join("search-launcher-app");
    std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    std::fs::write(dir.join("store-extension-id.txt"),id).map_err(|e|e.to_string())?;
    ensure_edge_native_history_host()
}
#[tauri::command]
fn add_favorite_tab(limit: i64) -> Result<i64, String> {
    db::add_favorite_tab(limit).map_err(|e| e.to_string())
}
#[tauri::command]
fn update_favorite_tab(id: i64, name: String, color: String) -> Result<(), String> {
    db::update_favorite_tab(id, &name, &color).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_favorite_tab(id: i64) -> Result<i64, String> {
    db::delete_favorite_tab(id).map_err(|e| e.to_string())
}
#[tauri::command]
fn add_favorite_column(tab_id: i64, limit: i64) -> Result<i64, String> {
    db::add_favorite_column(tab_id, limit).map_err(|e| e.to_string())
}
#[tauri::command]
fn update_favorite_column(id: i64, name: String, color: String) -> Result<(), String> {
    db::update_favorite_column(id, &name, &color).map_err(|e| e.to_string())
}
#[tauri::command]
fn move_favorite_column(id: i64, tab_id: i64, limit: i64) -> Result<(), String> {
    db::move_favorite_column(id, tab_id, limit).map_err(|e| e.to_string())
}
#[tauri::command]
fn place_favorite_columns(tab_id: i64, ids: Vec<i64>) -> Result<(), String> {
    db::place_favorite_columns(tab_id, &ids).map_err(|e| e.to_string())
}
#[tauri::command]
fn delete_favorite_column(id: i64, target_pane: i64) -> Result<(), String> {
    db::delete_favorite_column(id, target_pane).map_err(|e| e.to_string())
}
#[tauri::command]
fn delete_favorite_group(id:i64) -> Result<usize,String> {
    db::delete_favorite_group(id).map_err(|e|e.to_string())
}
#[tauri::command]
async fn record_favorite_open(id: i64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || db::record_favorite_open(id).map_err(|e|e.to_string())).await.map_err(|e|e.to_string())?
}
#[tauri::command]
fn list_deleted_favorites() -> Result<Vec<db::FavoriteItem>, String> {
    db::list_deleted_favorites().map_err(|e| e.to_string())
}
#[tauri::command]
fn restore_deleted_favorite(id: i64) -> Result<(), String> {
    db::restore_deleted_favorite(id).map_err(|e| e.to_string())
}
#[tauri::command]
fn permanently_delete_favorite(id: i64) -> Result<(), String> {
    db::permanently_delete_favorite(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn add_favorite_heading(label: String, pane: i64, color: String) -> Result<i64, String> {
    db::add_favorite_heading(&label, pane, &color).map_err(|e| e.to_string())
}
#[tauri::command]
fn add_manual_favorite(label: String, target: String, pane: i64) -> Result<i64, String> {
    db::add_manual_favorite(&label, &target, pane).map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_favorite_item(id: i64, label: String) -> Result<(), String> {
    db::rename_favorite_item(id, &label).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_favorite_item(id: i64) -> Result<(), String> {
    db::delete_favorite_item(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn place_favorite_items(pane: i64, ids: Vec<i64>) -> Result<(), String> {
    db::place_favorite_items(pane, &ids).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_favorite_color(id: i64, color: String) -> Result<(), String> {
    db::set_favorite_color(id, &color).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_favorite_text_styles() -> Result<serde_json::Value, String> {
    db::get_favorite_text_styles().map_err(|e|e.to_string())
}

#[tauri::command]
fn set_favorite_text_style(id: i64, color: String, bold: bool) -> Result<(), String> {
    db::set_favorite_text_style(id, &color, bold).map_err(|e|e.to_string())
}

#[tauri::command]
fn set_all_heading_colors(color: String) -> Result<(), String> {
    db::set_all_heading_colors(&color).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_window_size(size: String) -> Result<(), String> {
    db::save_window_size(&size).map_err(|e| e.to_string())
}

#[tauri::command]
fn hide_main_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn center_on_active_monitor(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut cursor = POINT::default();
    if !unsafe { GetCursorPos(&mut cursor) }.as_bool() {
        return window.center().map_err(|error| error.to_string());
    }
    let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return window.center().map_err(|error| error.to_string());
    }
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let x = info.rcWork.left + ((info.rcWork.right - info.rcWork.left) - size.width as i32) / 2;
    let y = info.rcWork.top + ((info.rcWork.bottom - info.rcWork.top) - size.height as i32) / 2;
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_edge_favicon(url: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        db::get_edge_favicon(&url).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn get_file_icon(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        db::get_file_icon(&path).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn toggle_file_favorite(path: String, name: String) -> Result<bool, String> {
    db::toggle_file_favorite(&path, &name).map_err(|e| e.to_string())
}

#[tauri::command]
fn is_file_index_initialized() -> Result<bool, String> {
    db::is_file_index_initialized().map_err(|e| e.to_string())
}

#[tauri::command]
async fn rebuild_file_index(folders: Vec<String>) -> Result<db::IndexUpdateSummary, String> {
    if FILE_INDEX_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("インデックス作成はすでに実行中です。".to_string());
    }
    let _ = db::append_app_log("インデックス", "info", "インデックス作成開始");
    let started = std::time::Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        db::rebuild_file_index(&folders).map_err(|error| error.to_string())
    })
    .await;
    FILE_INDEX_RUNNING.store(false, Ordering::SeqCst);
    match result {
        Ok(Ok(summary)) => {
            let message = format!(
                "インデックス更新完了（確認{}件、追加{}件、変更{}件、削除{}件、{:.1}秒）",
                summary.scanned,
                summary.added,
                summary.updated,
                summary.deleted,
                started.elapsed().as_secs_f64()
            );
            let _ = db::append_app_log("インデックス", "success", &message);
            Ok(summary)
        }
        Ok(Err(error)) => {
            let _ = db::append_app_log(
                "インデックス",
                "error",
                &format!("インデックス作成失敗: {}", error),
            );
            Err(error)
        }
        Err(error) => {
            let message = format!("バックグラウンド処理失敗: {}", error);
            let _ = db::append_app_log("インデックス", "error", &message);
            Err(message)
        }
    }
}

#[tauri::command]
fn get_app_logs() -> Result<Vec<db::AppLogEntry>, String> {
    db::list_app_logs().map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_app_logs() -> Result<(), String> {
    db::clear_app_logs().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_backup() -> Result<Option<String>, String> {
    db::create_backup().map_err(|e| e.to_string())
}
#[tauri::command]
fn restore_backup() -> Result<Option<String>, String> {
    db::restore_backup().map_err(|e| e.to_string())
}
#[tauri::command]
fn diagnostics_text() -> Result<String, String> {
    db::diagnostics_text().map_err(|e| e.to_string())
}
#[tauri::command]
fn extension_connection_status() -> Result<db::ExtensionConnectionStatus, String> {
    db::extension_connection_status().map_err(|e| e.to_string())
}

#[tauri::command]
async fn import_extension_history() -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(|| {
        db::import_extension_history_queue().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(target_os = "windows")]
fn ensure_edge_native_history_host() -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let text = exe.to_string_lossy().to_ascii_lowercase();
    if text.contains("\\target\\debug\\") || text.contains("\\target\\release\\") {
        return Ok(());
    }
    let local = std::env::var("LOCALAPPDATA").map_err(|e| e.to_string())?;
    let dir = std::path::PathBuf::from(local).join("search-launcher-app");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let manifest = dir.join("edge-native-history-host.json");
    if let Ok(path) = db::native_database_path() { std::fs::write(dir.join("edge-native-db-path.txt"),path.to_string_lossy().as_bytes()).map_err(|e|e.to_string())?; }
    let mut origins = vec!["chrome-extension://oihhdgjihfmmomlacmemmokcihfembhh/".to_string()];
    if let Ok(id) = std::fs::read_to_string(dir.join("store-extension-id.txt")) {
        let id = id.trim();
        if id.len()==32 && id.bytes().all(|v|(b'a'..=b'p').contains(&v)) { origins.push(format!("chrome-extension://{id}/")); }
    }
    let json = serde_json::json!({"name":"com.t10.search_launcher_history","description":"Search Launcher Edge history bridge","path":exe.to_string_lossy(),"type":"stdio","allowed_origins":origins});
    std::fs::write(&manifest, serde_json::to_vec_pretty(&json).unwrap())
        .map_err(|e| e.to_string())?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(
            "Software\\Microsoft\\Edge\\NativeMessagingHosts\\com.t10.search_launcher_history",
        )
        .map_err(|e| e.to_string())?;
    key.set_value("", &manifest.to_string_lossy().to_string())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn search_file_index(query: String, filter: Option<String>) -> Result<Vec<db::FileSearchRecord>, String> {
    tauri::async_runtime::spawn_blocking(move || db::search_file_index(&query, filter.as_deref().unwrap_or("documents")).map_err(|e|e.to_string())).await.map_err(|e|e.to_string())?
}

#[tauri::command]
async fn open_local_path(window: tauri::WebviewWindow, path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open::that(path).map_err(|e|e.to_string())).await.map_err(|e|e.to_string())??;
    window.hide().map_err(|e|e.to_string())
}

#[tauri::command]
fn apply_window_size(window: tauri::Window, size: String) -> Result<(), String> {
    let (width, height) = size.split_once('x').ok_or("Invalid window size")?;
    let width: f64 = width.parse().map_err(|_| "Invalid width")?;
    let height: f64 = height.parse().map_err(|_| "Invalid height")?;
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn apply_initial_window_size(window: tauri::WebviewWindow, size: String) -> Result<(), String> {
    let (width, height) = size.split_once('x').ok_or("Invalid window size")?;
    let width: f64 = width.parse().map_err(|_| "Invalid width")?;
    let height: f64 = height.parse().map_err(|_| "Invalid height")?;
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    center_on_active_monitor(&window)?;
    #[cfg(not(target_os = "windows"))]
    window.center().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn read_edge_history() -> Result<Vec<db::EdgeRecord>, String> {
    match db::read_edge_history() {
        Ok(rows) => {
            println!("read_edge_history: found {} records", rows.len());
            Ok(rows)
        }
        Err(e) => Err(e.to_string()),
    }
}

// Safe variant: returns rows and optional error string so frontend can show diagnostics while receiving stub data
#[tauri::command]
fn read_edge_history_safe() -> Result<serde_json::Value, String> {
    match db::read_edge_history() {
        Ok(rows) => Ok(serde_json::json!({ "rows": rows, "error": null })),
        Err(e) => {
            let err_str = e.to_string();
            eprintln!("read_edge_history error: {}", err_str);
            // return stub data plus the error message
            let stub = db::read_edge_history_stub();
            Ok(serde_json::json!({ "rows": stub, "error": err_str }))
        }
    }
}

#[tauri::command]
fn read_edge_history_from_path(path: String) -> Result<Vec<db::EdgeRecord>, String> {
    let p = std::path::PathBuf::from(path);
    match db::read_edge_history_from_path(&p) {
        Ok(rows) => {
            println!("read_edge_history_from_path: found {} records", rows.len());
            Ok(rows)
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn refresh_edge_history() -> Result<db::EdgeImportSummary, String> {
    db::refresh_edge_history().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_edge_import_status() -> Result<db::EdgeImportStatus, String> {
    Ok(db::get_edge_import_status())
}

#[tauri::command]
fn clear_edge_history() -> Result<(), String> {
    db::clear_edge_history().map_err(|e| e.to_string())
}

#[tauri::command]
fn request_extension_full_sync() -> Result<(), String> {
    db::request_extension_full_sync().map_err(|e| e.to_string())
}

// Save diagnostics text to a temp file and return the path
#[tauri::command]
fn save_edge_diagnostics(error: String) -> Result<String, String> {
    let mut tmp = std::env::temp_dir();
    tmp.push(format!(
        "edge_diagnostics_{}.txt",
        chrono::Utc::now().timestamp()
    ));
    let content = format!(
        "Edge diagnostics\nTime: {}\nError: {}\n",
        chrono::Utc::now().to_rfc3339(),
        error
    );
    match std::fs::write(&tmp, content) {
        Ok(_) => Ok(tmp.display().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn open_url(window: tauri::Window, url: String) -> Result<(), String> {
    // Use the cross-platform 'open' crate to open a URL in the default browser
    match tauri::async_runtime::spawn_blocking(move || open::that(&url).map_err(|e|e.to_string())).await.map_err(|e|e.to_string())? {
        Ok(_) => {
            // keep app resident in tray
            let _ = window.hide();
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(target_os = "windows")]
fn resolve_installed_exe() -> Option<std::path::PathBuf> {
    // Return the installed product exe under LOCALAPPDATA\search-launcher-app\ only.
    // Never fall back to current_exe() which would register the debug build when
    // running from the development environment.
    let app_name = "search-launcher-app";
    if let Ok(current) = std::env::current_exe() {
        let text = current.to_string_lossy().to_ascii_lowercase();
        if current.exists()
            && !text.contains("\\target\\debug\\")
            && !text.contains("\\target\\release\\")
            && !text.contains("/target/debug/")
            && !text.contains("/target/release/")
        {
            return Some(current);
        }
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let installed = std::path::PathBuf::from(&local_app_data)
            .join(app_name)
            .join(format!("{}.exe", app_name));
        if installed.exists() {
            return Some(installed);
        }
    }
    None
}

/// Returns true if the registered autostart path looks like a development/debug build path.
/// Used to detect and clean up stale registrations from debug runs.
#[cfg(target_os = "windows")]
fn is_debug_exe_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("\\target\\debug\\") || lower.contains("/target/debug/")
}

#[tauri::command]
fn set_autostart(enable: bool) -> Result<String, String> {
    #[cfg(debug_assertions)]
    if std::env::var_os("FAVORITE_LAUNCHER_TEST_DIR").is_some() { return Ok("UI test: startup unchanged".into()); }
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _disp) = hkcu
            .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .map_err(|e| e.to_string())?;
        let name = "search-launcher-app";
        if enable {
            let exe = resolve_installed_exe().ok_or_else(|| {
                let expected = std::env::var("LOCALAPPDATA")
                    .map(|d| format!("{}\\search-launcher-app\\search-launcher-app.exe", d))
                    .unwrap_or_else(|_| {
                        "%LOCALAPPDATA%\\search-launcher-app\\search-launcher-app.exe".to_string()
                    });
                format!(
                    "インストール済みのexeが見つかりません。\
                         先にインストーラを実行してください。\
                         期待パス: {}",
                    expected
                )
            })?;
            let value = format!("\"{}\"", exe.display());
            key.set_value(name, &value).map_err(|e| e.to_string())?;
            Ok(format!("autostart enabled: {}", value))
        } else {
            let _ = key.delete_value(name);
            Ok("autostart disabled".to_string())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("autostart not supported on this OS".to_string())
    }
}

#[tauri::command]
fn is_autostart_enabled() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        match hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run") {
            Ok(key) => {
                match key.get_value::<String, &str>("search-launcher-app") {
                    Ok(val) => {
                        // If the registered path points to a debug build, treat it as disabled
                        // and remove the stale entry so the user can re-enable with the correct path.
                        if is_debug_exe_path(&val) {
                            eprintln!("Stale debug autostart entry detected ({}), removing.", val);
                            let _ = key.delete_value("search-launcher-app");
                            return Ok(false);
                        }
                        Ok(true)
                    }
                    Err(_) => Ok(false),
                }
            }
            Err(_) => Ok(false),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if !APP_EXITING.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        // plugins
        .plugin(tauri_plugin_opener::init())
        .plugin({
            let shortcut_builder =
                if cfg!(debug_assertions) && std::env::var_os("FAVORITE_LAUNCHER_TEST_DIR").is_some() {GlobalShortcutBuilder::new()} else {match GlobalShortcutBuilder::new().with_shortcuts(["CommandOrControl+Space"]) {
                    Ok(builder) => builder,
                    Err(error) => {
                        eprintln!(
                            "Failed to register CommandOrControl+Space shortcut: {}",
                            error
                        );
                        GlobalShortcutBuilder::new()
                    }
                }};
            shortcut_builder
                .with_handler(|app, shortcut, _event| {
                    let shortcut_text = shortcut.to_string().to_ascii_lowercase();
                    if let Some(window) = app.get_webview_window("main") {
                        if shortcut_text.contains("space") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build()
        })
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "windows")]
                let _ = center_on_active_monitor(&window);
                #[cfg(not(target_os = "windows"))]
                let _ = window.center();
            }
            let show_item = MenuItemBuilder::with_id("show", "表示").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "終了").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;
            let mut tray_builder = TrayIconBuilder::with_id("main-tray")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        APP_EXITING.store(true, Ordering::SeqCst);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            let _tray = tray_builder.build(app)?;

            // Store AppHandle globally so db module can access paths
            let handle = app.handle().clone();
            if let Ok(mut stored_handle) = APP_HANDLE.lock() {
                *stored_handle = Some(handle);
            }

            // Initialize DB on startup; log errors but continue
            match db::init_and_seed_db() {
                Ok(path) => {
                    println!("DB initialized at {}", path.display());
                    let _ = db::create_automatic_backup();
                }
                Err(e) => eprintln!("DB init error: {}", e),
            }

            // Installed builds always start with Windows. Development binaries under
            // target/debug or target/release are deliberately never registered.
            if let Err(error) = set_autostart(true) {
                eprintln!("Autostart registration skipped: {}", error);
            }
            #[cfg(target_os = "windows")]
            if let Err(error) = ensure_edge_native_history_host() {
                eprintln!("Edge extension host registration skipped: {}", error);
            }

            // Clean up any stale autostart registry entry that points to a debug build.
            #[cfg(target_os = "windows")]
            if !(cfg!(debug_assertions) && std::env::var_os("FAVORITE_LAUNCHER_TEST_DIR").is_some()) {
                use winreg::enums::*;
                use winreg::RegKey;
                let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                if let Ok(key) = hkcu.open_subkey_with_flags(
                    "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                    KEY_READ | KEY_WRITE,
                ) {
                    if let Ok(val) = key.get_value::<String, &str>("search-launcher-app") {
                        if is_debug_exe_path(&val) {
                            eprintln!("Startup: removing stale debug autostart entry: {}", val);
                            let _ = key.delete_value("search-launcher-app");
                        }
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            init_db,
            fetch_history,
            get_settings,
            save_settings,
            search_history,
            read_edge_history,
            read_edge_history_from_path,
            read_edge_history_safe,
            refresh_edge_history,
            clear_edge_history,
            request_extension_full_sync,
            get_edge_import_status,
            save_edge_diagnostics,
            open_url,
            set_autostart,
            is_autostart_enabled,
            toggle_favorite,
            get_favorites,
            list_favorite_items,
            list_favorite_tabs,
            place_favorite_tabs,
            list_favorite_columns,
            prepare_favorite_board,
            save_board_layout,
            get_ui_preferences,
            save_ui_preferences,
            move_board_item,
            open_edge_extensions,
            get_file_filter,
            save_file_filter,
            configure_store_extension,
            add_favorite_tab,
            update_favorite_tab,
            delete_favorite_tab,
            add_favorite_column,
            update_favorite_column,
            move_favorite_column,
            place_favorite_columns,
            delete_favorite_column,
            delete_favorite_group,
            record_favorite_open,
            list_deleted_favorites,
            restore_deleted_favorite,
            permanently_delete_favorite,
            add_favorite_heading,
            add_manual_favorite,
            rename_favorite_item,
            delete_favorite_item,
            place_favorite_items,
            set_favorite_color,
            get_favorite_text_styles,
            set_favorite_text_style,
            set_all_heading_colors,
            save_window_size,
            hide_main_window,
            get_edge_favicon,
            get_file_icon,
            toggle_file_favorite,
            is_file_index_initialized,
            rebuild_file_index,
            get_app_logs,
            clear_app_logs,
            create_backup,
            restore_backup,
            diagnostics_text,
            extension_connection_status,
            import_extension_history,
            search_file_index,
            open_local_path,
            apply_window_size,
            apply_initial_window_size
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

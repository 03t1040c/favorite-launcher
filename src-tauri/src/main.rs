// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn run_native_history_host() -> std::io::Result<()> {
    use std::io::{Read, Write};
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    let dir = std::path::PathBuf::from(local).join("search-launcher-app");
    std::fs::create_dir_all(&dir)?;
    let queue = dir.join("edge-extension-queue.jsonl");
    let mut input = std::io::stdin().lock();
    loop {
        let mut length = [0u8; 4];
        if input.read_exact(&mut length).is_err() {
            break;
        }
        let size = u32::from_le_bytes(length) as usize;
        if size == 0 || size > 32 * 1024 * 1024 {
            break;
        }
        let mut message = vec![0u8; size];
        input.read_exact(&mut message)?;
        let value = serde_json::from_slice::<serde_json::Value>(&message).ok();
        let favorite_url = value.as_ref().and_then(|v|v.get("favorite")).and_then(|v|v.get("url")).and_then(|v|v.as_str());
        let mut location = None;
        let mut duplicate = false;
        if let Some(url) = favorite_url {
            if let Ok(path) = std::fs::read_to_string(dir.join("edge-native-db-path.txt")) {
                if let Ok(conn) = rusqlite::Connection::open_with_flags(path.trim(),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
                    location = conn.query_row("SELECT COALESCE(t.name,'ホーム') || ' / ' || COALESCE(c.name,'未整理') FROM favorite_items f LEFT JOIN favorite_columns c ON c.id=f.pane LEFT JOIN favorite_tabs t ON t.id=c.tab_id WHERE f.deleted_at IS NULL AND lower(f.target)=lower(?1) LIMIT 1",rusqlite::params![url],|r|r.get::<_,String>(0)).ok();
                    duplicate = location.is_some();
                }
            }
            if !duplicate {
                duplicate = std::fs::read_to_string(&queue).unwrap_or_default().lines().any(|line|serde_json::from_str::<serde_json::Value>(line).ok().and_then(|v|v.get("favorite").and_then(|f|f.get("url")).and_then(|v|v.as_str()).map(|v|v.eq_ignore_ascii_case(url))).unwrap_or(false));
            }
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&queue)?;
        if !duplicate { file.write_all(&message)?; file.write_all(b"\n")?; }
        file.flush()?;
        let _ = std::fs::write(
            dir.join("edge-extension-last-contact.txt"),
            format!("{:?}", std::time::SystemTime::now()),
        );
        let full_sync_request = dir.join("edge-extension-full-sync.request");
        let full_sync_requested = full_sync_request.exists();
        let is_full_sync_payload = serde_json::from_slice::<serde_json::Value>(&message)
            .ok()
            .and_then(|value| value.get("fullSync").and_then(|item| item.as_bool()))
            .unwrap_or(false);
        if is_full_sync_payload {
            let _ = std::fs::remove_file(&full_sync_request);
        }
        let interval = std::fs::read_to_string(dir.join("edge-extension-config.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| {
                value
                    .get("syncIntervalSeconds")
                    .and_then(|item| item.as_i64())
            })
            .unwrap_or(30)
            .clamp(30, 3600);
        let response = serde_json::json!({"ok":true,"syncIntervalSeconds":interval,"fullSyncRequested":full_sync_requested,"duplicate":duplicate,"location":location}).to_string();
        let mut output = std::io::stdout().lock();
        output.write_all(&(response.len() as u32).to_le_bytes())?;
        output.write_all(response.as_bytes())?;
        output.flush()?;
    }
    Ok(())
}

fn main() {
    if std::env::args()
        .skip(1)
        .any(|arg| arg.starts_with("chrome-extension://"))
    {
        let _ = run_native_history_host();
        return;
    }
    #[cfg(target_os = "windows")]
    let _instance_mutex = unsafe {
        use windows::core::w;
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;
        use windows::Win32::UI::WindowsAndMessaging::{
            FindWindowW, SetForegroundWindow, ShowWindow, SW_SHOW,
        };
        let test_mode = cfg!(debug_assertions) && std::env::var_os("FAVORITE_LAUNCHER_TEST_DIR").is_some();
        let mutex_name = if test_mode {w!("Local\\SearchLauncherApp.UITestInstance")} else {w!("Local\\SearchLauncherApp.SingleInstance")};
        let mutex = CreateMutexW(None, false, mutex_name).ok();
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let window = FindWindowW(None, w!("search-launcher-app"));
            if window.0 != 0 {
                ShowWindow(window, SW_SHOW);
                let _ = SetForegroundWindow(window);
            }
            return;
        }
        mutex
    };
    search_launcher_app_lib::run()
}

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::webview::{DownloadEvent, NewWindowResponse, PageLoadEvent, WebviewWindowBuilder};
use tauri::{include_image, Manager, Url, WebviewUrl};

mod updates;

const APP_NAME: &str = "Wali";
const APP_ID: &str = "io.github.mo999salah.wali";
const MAIN_WINDOW: &str = "main";
const SETTINGS_WINDOW: &str = "settings";
const WHATSAPP_HOST: &str = "web.whatsapp.com";
const WHATSAPP_ORIGIN: &str = "https://web.whatsapp.com";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PrefsFile {
    close_to_tray: bool,
    desktop_notifications: bool,
    auto_update_checks: bool,
    last_notified_version: Option<String>,
}

impl Default for PrefsFile {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            desktop_notifications: true,
            auto_update_checks: true,
            last_notified_version: None,
        }
    }
}

fn parse_prefs_json(raw: &str) -> PrefsFile {
    serde_json::from_str(raw).unwrap_or_default()
}

struct Prefs {
    close_to_tray: AtomicBool,
    desktop_notifications: AtomicBool,
    auto_update_checks: AtomicBool,
    last_notified_version: Mutex<Option<String>>,
    persist: Mutex<()>,
}

impl Prefs {
    fn from_file(file: PrefsFile) -> Self {
        Self {
            close_to_tray: AtomicBool::new(file.close_to_tray),
            desktop_notifications: AtomicBool::new(file.desktop_notifications),
            auto_update_checks: AtomicBool::new(file.auto_update_checks),
            last_notified_version: Mutex::new(file.last_notified_version),
            persist: Mutex::new(()),
        }
    }

    fn to_file(&self) -> PrefsFile {
        PrefsFile {
            close_to_tray: self.close_to_tray.load(Ordering::Relaxed),
            desktop_notifications: self.desktop_notifications.load(Ordering::Relaxed),
            auto_update_checks: self.auto_update_checks.load(Ordering::Relaxed),
            last_notified_version: self
                .last_notified_version
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        }
    }
}

fn prefs_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json"))
}

fn load_prefs(app: &tauri::AppHandle) -> Prefs {
    let file = prefs_path(app)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|raw| parse_prefs_json(&raw))
        .unwrap_or_default();
    Prefs::from_file(file)
}

fn save_prefs(app: &tauri::AppHandle, prefs: &Prefs) -> Result<(), String> {
    let path = prefs_path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    persist_prefs(&path, prefs)
}

fn prefs_tmp_path(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

fn persist_prefs(path: &Path, prefs: &Prefs) -> Result<(), String> {
    let _write = prefs.persist.lock().unwrap_or_else(|e| e.into_inner());
    let json = serde_json::to_string_pretty(&prefs.to_file()).map_err(|e| e.to_string())?;
    atomic_write(path, json.as_bytes())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = prefs_tmp_path(path);
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsSnapshot {
    version: String,
    downloads_dir: String,
    close_to_tray: bool,
    desktop_notifications: bool,
    auto_update_checks: bool,
}

fn show_window(app: &tauri::AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn downloads_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path().download_dir().unwrap_or_else(|_| {
        PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Downloads")
    })
}

fn open_settings(app: &tauri::AppHandle) -> Result<(), String> {
    if app.get_webview_window(SETTINGS_WINDOW).is_some() {
        show_window(app, SETTINGS_WINDOW);
        return Ok(());
    }

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("settings-webview");
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let settings = WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW,
        WebviewUrl::App("settings.html".into()),
    )
    .title("Settings")
    .icon(include_image!("icons/icon.png"))
    .map_err(|e| e.to_string())?
    .inner_size(820.0, 560.0)
    .min_inner_size(680.0, 440.0)
    .resizable(true)
    .decorations(true)
    .visible(true)
    .data_directory(data_dir)
    .on_navigation(is_settings_url)
    .on_new_window(|_, _| NewWindowResponse::Deny)
    .build()
    .map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    apply_native_linux_chrome(&settings);
    Ok(())
}

#[tauri::command]
fn settings_snapshot(app: tauri::AppHandle) -> Result<SettingsSnapshot, String> {
    let prefs = app.state::<Prefs>().to_file();
    Ok(SettingsSnapshot {
        version: env!("CARGO_PKG_VERSION").into(),
        downloads_dir: downloads_dir(&app).display().to_string(),
        close_to_tray: prefs.close_to_tray,
        desktop_notifications: prefs.desktop_notifications,
        auto_update_checks: prefs.auto_update_checks,
    })
}

#[tauri::command]
fn update_setting(app: tauri::AppHandle, key: String, value: bool) -> Result<(), String> {
    let prefs = app.state::<Prefs>();
    match key.as_str() {
        "closeToTray" => prefs.close_to_tray.store(value, Ordering::Relaxed),
        "desktopNotifications" => prefs.desktop_notifications.store(value, Ordering::Relaxed),
        "autoUpdateChecks" => prefs.auto_update_checks.store(value, Ordering::Relaxed),
        _ => return Err("unknown setting".into()),
    }
    save_prefs(&app, &prefs)
}

#[tauri::command]
fn open_downloads_dir(app: tauri::AppHandle) -> Result<(), String> {
    let dir = downloads_dir(&app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    xdg_open(&dir.display().to_string());
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCheck {
    status: String,
    version: Option<String>,
    html_url: Option<String>,
}

#[tauri::command]
async fn check_for_updates() -> Result<UpdateCheck, String> {
    let result =
        tauri::async_runtime::spawn_blocking(|| updates::check_latest(env!("CARGO_PKG_VERSION")))
            .await
            .map_err(|_| "could not check for updates".to_string())?;
    match result {
        Ok(updates::Outcome::UpToDate) => Ok(UpdateCheck {
            status: "upToDate".into(),
            version: None,
            html_url: None,
        }),
        Ok(updates::Outcome::Available { version, html_url }) => Ok(UpdateCheck {
            status: "available".into(),
            version: Some(version),
            html_url: Some(html_url),
        }),
        Err(e) => {
            eprintln!("update check: {e}");
            Err("could not check for updates".into())
        }
    }
}

#[tauri::command]
fn open_release_url(url: String) -> Result<(), String> {
    if !updates::is_release_url(&url) {
        return Err("invalid release url".into());
    }
    xdg_open(&url);
    Ok(())
}

fn main() {
    println!("application start");
    #[cfg(target_os = "linux")]
    {
        gtk::glib::set_prgname(Some(APP_ID));
        gtk::glib::set_application_name(APP_NAME);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app, MAIN_WINDOW);
        }))
        .invoke_handler(tauri::generate_handler![
            settings_snapshot,
            open_downloads_dir,
            update_setting,
            check_for_updates,
            open_release_url
        ])
        .setup(|app| {
            app.manage(load_prefs(app.handle()));
            let data_dir = app.path().app_data_dir()?.join("webview");
            std::fs::create_dir_all(&data_dir)?;
            let handle = app.handle().clone();

            let win = WebviewWindowBuilder::new(
                app,
                MAIN_WINDOW,
                WebviewUrl::External(WHATSAPP_ORIGIN.parse()?),
            )
            .title(APP_NAME)
            .icon(include_image!("icons/icon.png"))?
            .inner_size(1280.0, 800.0)
            .min_inner_size(800.0, 600.0)
            .resizable(true)
            .decorations(true)
            .visible(true)
            .data_directory(data_dir)
            .on_page_load(|_, payload| {
                let phase = match payload.event() {
                    PageLoadEvent::Started => "started",
                    PageLoadEvent::Finished => "finished",
                };
                println!("page load {phase} {}", payload.url());
            })
            .on_navigation(|url| {
                if is_whatsapp_url(url) {
                    true
                } else if url.scheme() == "http" || url.scheme() == "https" {
                    xdg_open(url.as_str());
                    false
                } else {
                    matches!(url.scheme(), "about" | "blob" | "data")
                }
            })
            .on_new_window({
                let handle = handle.clone();
                move |url, _| {
                    if is_whatsapp_url(&url) {
                        if let Some(w) = handle.get_webview_window(MAIN_WINDOW) {
                            let _ = w.navigate(url);
                        }
                    } else if url.scheme() == "http" || url.scheme() == "https" {
                        xdg_open(url.as_str());
                    }
                    NewWindowResponse::Deny
                }
            })
            .on_download({
                let handle = handle.clone();
                move |_, event| match event {
                    DownloadEvent::Requested { destination, .. } => {
                        let dir = downloads_dir(&handle);
                        let _ = std::fs::create_dir_all(&dir);
                        let suggested = destination
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("download");
                        *destination = unique_download_path(&dir, &safe_basename(suggested));
                        true
                    }
                    DownloadEvent::Finished { url, path, success } => {
                        if !success {
                            eprintln!("download failed {url} {path:?}");
                        }
                        true
                    }
                    _ => true,
                }
            })
            .build()?;

            #[cfg(target_os = "linux")]
            apply_native_linux_chrome(&win);

            #[cfg(target_os = "linux")]
            {
                let notify_app = handle.clone();
                win.with_webview(move |webview| {
                    use webkit2gtk::glib::prelude::*;
                    use webkit2gtk::{
                        DeviceInfoPermissionRequest, NotificationExt,
                        NotificationPermissionRequest, PermissionRequestExt,
                        UserMediaPermissionRequest, WebViewExt,
                    };

                    webview.inner().connect_permission_request(|view, request| {
                        let uri = WebViewExt::uri(view)
                            .map(|s| s.to_string())
                            .unwrap_or_default();
                        let allow = is_whatsapp_uri(&uri)
                            && (request.is::<UserMediaPermissionRequest>()
                                || request.is::<DeviceInfoPermissionRequest>()
                                || request.is::<NotificationPermissionRequest>());
                        if allow {
                            request.allow();
                        } else {
                            request.deny();
                        }
                        true
                    });

                    webview.inner().connect_show_notification(move |view, n| {
                        let uri = WebViewExt::uri(view)
                            .map(|s| s.to_string())
                            .unwrap_or_default();
                        if !is_whatsapp_uri(&uri) {
                            return false;
                        }
                        if !notify_app
                            .state::<Prefs>()
                            .desktop_notifications
                            .load(Ordering::Relaxed)
                        {
                            return true;
                        }
                        let title = n.title().unwrap_or_default().to_string();
                        let body = n.body().unwrap_or_default().to_string();
                        desktop_notify(title, body);
                        true
                    });
                })?;
            }

            let hide_win = win.clone();
            let close_app = handle.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if close_app
                        .state::<Prefs>()
                        .close_to_tray
                        .load(Ordering::Relaxed)
                    {
                        api.prevent_close();
                        let _ = hide_win.hide();
                    } else {
                        close_app.exit(0);
                    }
                }
            });

            let show = MenuItem::with_id(app, "show", "Show Wali", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &settings, &quit])?;
            let icon = include_image!("icons/48x48.png");
            let tray_dir = app.path().app_cache_dir()?;
            std::fs::create_dir_all(&tray_dir)?;

            TrayIconBuilder::new()
                .icon(icon)
                .temp_dir_path(tray_dir)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_window(app, MAIN_WINDOW),
                    "settings" => {
                        if let Err(e) = open_settings(app) {
                            eprintln!("open settings: {e}");
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_window(tray.app_handle(), MAIN_WINDOW);
                    }
                })
                .build(app)?;

            if app
                .state::<Prefs>()
                .auto_update_checks
                .load(Ordering::Relaxed)
            {
                let update_app = handle.clone();
                std::thread::spawn(move || auto_check_updates(update_app));
            }

            println!("main window created");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Wali");
}

/// Parsed origin: https, host web.whatsapp.com, default HTTPS port. Paths and queries allowed.
fn is_whatsapp_url(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str() == Some(WHATSAPP_HOST) && url.port().is_none()
}

fn is_whatsapp_uri(uri: &str) -> bool {
    uri.parse::<Url>().is_ok_and(|url| is_whatsapp_url(&url))
}

fn is_settings_url(url: &Url) -> bool {
    match url.scheme() {
        "http" | "https" => {
            matches!(
                url.host_str(),
                Some("tauri.localhost") | Some("ipc.localhost")
            )
        }
        "tauri" | "ipc" | "about" | "blob" | "data" => true,
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn apply_native_linux_chrome(win: &tauri::WebviewWindow) {
    use gtk::prelude::GtkWindowExt;
    if let Ok(gtk_win) = win.gtk_window() {
        gtk_win.set_titlebar(Option::<&gtk::Widget>::None);
        gtk_win.set_icon_name(Some("wali"));
    }
}

fn xdg_open(target: &str) {
    let _ = std::process::Command::new("xdg-open")
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn safe_basename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let cleaned: String = base
        .chars()
        .filter(|c| *c != '/' && *c != '\\' && *c != '\0' && !c.is_control())
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "download".into()
    } else {
        cleaned.into()
    }
}

fn unique_download_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let ext = Path::new(name).extension().and_then(|s| s.to_str());
    let mut n = 1u32;
    loop {
        let fname = match ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let path = dir.join(&fname);
        if !path.exists() {
            return path;
        }
        n += 1;
    }
}

#[cfg(target_os = "linux")]
fn desktop_notify(title: String, body: String) {
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .summary(&title)
            .body(&body)
            .appname(APP_NAME)
            .show();
    });
}

fn auto_check_updates(app: tauri::AppHandle) {
    match updates::check_latest(env!("CARGO_PKG_VERSION")) {
        Ok(updates::Outcome::Available { version, html_url }) => {
            let prefs = app.state::<Prefs>();
            {
                let last = prefs
                    .last_notified_version
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if last.as_deref() == Some(version.as_str()) {
                    return;
                }
            }
            {
                *prefs
                    .last_notified_version
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some(version.clone());
            }
            if let Err(e) = save_prefs(&app, &prefs) {
                eprintln!("update check persist: {e}");
            }
            notify_update_available(version, html_url);
        }
        Ok(updates::Outcome::UpToDate) => {}
        Err(e) => eprintln!("update check: {e}"),
    }
}

#[cfg(target_os = "linux")]
fn notify_update_available(version: String, html_url: String) {
    std::thread::spawn(move || {
        let title = format!("Wali {version} is available");
        let body = "A new version of Wali is ready to download.";
        let mut n = notify_rust::Notification::new();
        n.summary(&title)
            .body(body)
            .appname(APP_NAME)
            .icon("wali")
            .action("default", "View Release");
        if let Ok(handle) = n.show() {
            handle.wait_for_action(|action| {
                if action == "default" && updates::is_release_url(&html_url) {
                    xdg_open(&html_url);
                }
            });
        }
    });
}

#[cfg(not(target_os = "linux"))]
fn notify_update_available(_version: String, _html_url: String) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_empty_defaults_on() {
        let p = parse_prefs_json("");
        assert!(p.close_to_tray);
        assert!(p.desktop_notifications);
        assert!(p.auto_update_checks);
        assert_eq!(p.last_notified_version, None);
    }

    #[test]
    fn prefs_parse_off() {
        let p = parse_prefs_json(r#"{"closeToTray":false,"desktopNotifications":false}"#);
        assert!(!p.close_to_tray);
        assert!(!p.desktop_notifications);
        assert!(p.auto_update_checks);
        assert_eq!(p.last_notified_version, None);
    }

    #[test]
    fn persist_prefs_replaces_file_atomically() {
        let dir = std::env::temp_dir().join(format!(
            "wali-prefs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "stale").unwrap();
        let prefs = Prefs::from_file(PrefsFile {
            close_to_tray: false,
            desktop_notifications: true,
            auto_update_checks: false,
            last_notified_version: Some("0.2.0".into()),
        });
        persist_prefs(&path, &prefs).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        let parsed = parse_prefs_json(&raw);
        assert!(!parsed.close_to_tray);
        assert!(parsed.desktop_notifications);
        assert!(!parsed.auto_update_checks);
        assert_eq!(parsed.last_notified_version.as_deref(), Some("0.2.0"));
        assert!(!prefs_tmp_path(&path).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn url(s: &str) -> Url {
        s.parse().expect("url")
    }

    #[test]
    fn whatsapp_origin_allows_https_host_and_path() {
        assert!(is_whatsapp_url(&url(WHATSAPP_ORIGIN)));
        assert!(is_whatsapp_url(&url("https://web.whatsapp.com/")));
        assert!(is_whatsapp_url(&url(
            "https://web.whatsapp.com/send?phone=1#frag"
        )));
        assert!(is_whatsapp_uri("https://web.whatsapp.com/"));
    }

    #[test]
    fn whatsapp_origin_rejects_lookalikes() {
        assert!(!is_whatsapp_url(&url(
            "https://web.whatsapp.com.example.com/"
        )));
        assert!(!is_whatsapp_url(&url("https://notweb.whatsapp.com/")));
        assert!(!is_whatsapp_url(&url("http://web.whatsapp.com/")));
        assert!(!is_whatsapp_url(&url("https://web.whatsapp.com:8443/")));
        assert!(!is_whatsapp_uri("https://web.whatsapp.com.example.com/"));
        assert!(!is_whatsapp_uri("not a url"));
    }

    #[test]
    fn settings_url_allows_tauri_app_origin_only() {
        assert!(is_settings_url(&url(
            "http://tauri.localhost/settings.html"
        )));
        assert!(is_settings_url(&url(
            "https://tauri.localhost/settings.html"
        )));
        assert!(is_settings_url(&url("http://ipc.localhost/")));
        assert!(!is_settings_url(&url(WHATSAPP_ORIGIN)));
        assert!(!is_settings_url(&url("https://example.com/")));
    }

    #[test]
    fn basename_strips_traversal_and_controls() {
        assert_eq!(safe_basename("photo.jpg"), "photo.jpg");
        assert_eq!(safe_basename("../../etc/passwd"), "passwd");
        assert_eq!(safe_basename("a/b\\c\0d"), "bcd");
        assert_eq!(safe_basename(".."), "download");
        assert_eq!(safe_basename(""), "download");
        assert_eq!(safe_basename("ok\nname.jpg"), "okname.jpg");
    }
}

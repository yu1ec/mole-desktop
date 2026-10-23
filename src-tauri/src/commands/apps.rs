use std::path::{Path, PathBuf};

use tauri::State;

use crate::models::{AppEntry, HistoryEntry, Leftover, OperationResult};
use crate::state::AppState;
use crate::store::push_history;
use crate::util::{dir_stats, home, mtime, move_to_trash, now_rfc3339, remove_path};
use rayon::prelude::*;

#[tauri::command(async)]
pub fn list_apps() -> Vec<AppEntry> {
    #[cfg(target_os = "macos")]
    {
        macos::list()
    }
    #[cfg(windows)]
    {
        windows::list()
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        Vec::new()
    }
}

#[tauri::command(async)]
pub fn app_leftovers(app_id: String) -> Vec<Leftover> {
    #[cfg(target_os = "macos")]
    {
        macos::leftovers_for(&app_id)
    }
    #[cfg(windows)]
    {
        windows::leftovers_for(&app_id)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = app_id;
        Vec::new()
    }
}

#[tauri::command(async)]
pub fn uninstall_app(
    app_id: String,
    leftovers: Vec<String>,
    permanent: bool,
    state: State<AppState>,
) -> Result<OperationResult, String> {
    let mut result = OperationResult::default();
    let name = {
        #[cfg(target_os = "macos")]
        {
            let path = PathBuf::from(&app_id);
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| app_id.clone());
            if !is_macos_app(&path) {
                return Err("只能卸载 /Applications 或 ~/Applications 中的应用".into());
            }
            macos::quit(&path);
            let paths = if permanent {
                let (freed, files, errors) = remove_path(&path);
                result.freed += freed;
                result.files += files;
                result.errors.extend(errors);
                Vec::new()
            } else {
                vec![path]
            };
            if !paths.is_empty() {
                let (freed, files, errors) = move_to_trash(&paths);
                result.freed += freed;
                result.files += files;
                result.errors.extend(errors);
            }
            if result.files > 0 {
                result.removed.push(app_id.clone());
            }
            name
        }
        #[cfg(windows)]
        {
            windows::uninstall(&app_id, &mut result)?
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            return Err("当前平台不支持应用卸载".into());
        }
    };

    let home = home();
    let mut data_paths = Vec::new();
    for raw in leftovers {
        let path = PathBuf::from(&raw);
        if !path.starts_with(&home) || path == home {
            result
                .errors
                .push(format!("跳过不在用户目录中的数据：{raw}"));
            continue;
        }
        data_paths.push(path);
    }

    if !data_paths.is_empty() {
        if permanent {
            for path in &data_paths {
                let (freed, files, errors) = remove_path(path);
                result.freed += freed;
                result.files += files;
                result.errors.extend(errors);
                if files > 0 {
                    result.removed.push(path.to_string_lossy().to_string());
                }
            }
        } else {
            let (freed, files, errors) = move_to_trash(&data_paths);
            result.freed += freed;
            result.files += files;
            result.errors.extend(errors);
            if files > 0 {
                result
                    .removed
                    .extend(data_paths.iter().map(|p| p.to_string_lossy().to_string()));
            }
        }
    }
    result.moved_to_trash = !permanent;

    push_history(
        &state.data_dir,
        HistoryEntry {
            id: String::new(),
            timestamp: now_rfc3339(),
            kind: "uninstall".into(),
            title: format!("卸载应用 · {name}"),
            freed: result.freed,
            files: result.files,
            detail: result.removed.join("、"),
        },
    );
    Ok(result)
}

#[cfg(target_os = "macos")]
fn is_macos_app(path: &Path) -> bool {
    if path.extension().map(|e| e != "app").unwrap_or(true) {
        return false;
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        PathBuf::from("/Applications").join(path)
    };
    absolute.starts_with("/Applications") || absolute.starts_with(home().join("Applications"))
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    fn app_dirs() -> Vec<PathBuf> {
        vec![PathBuf::from("/Applications"), home().join("Applications")]
    }

    pub fn list() -> Vec<AppEntry> {
        let mut paths = Vec::new();
        for dir in app_dirs() {
            collect(&dir, 0, &mut paths);
        }
        let mut apps: Vec<AppEntry> = paths
            .par_iter()
            .filter_map(|path| read_app(path))
            .collect();
        apps.sort_by(|a, b| b.size.cmp(&a.size));
        apps
    }

    fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else { continue };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if name.ends_with(".app") {
                out.push(path);
            } else if depth < 2 {
                collect(&path, depth + 1, out);
            }
        }
    }

    fn read_app(path: &Path) -> Option<AppEntry> {
        let info_path = path.join("Contents/Info.plist");
        let info = plist::Value::from_file(&info_path).ok();
        let dict = info.as_ref().and_then(|value| value.as_dictionary());
        let get = |key: &str| -> String {
            dict.and_then(|d| d.get(key))
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string()
        };
        let bundle_id = get("CFBundleIdentifier");
        let name = {
            let display = get("CFBundleDisplayName");
            if !display.is_empty() {
                display
            } else {
                let base = get("CFBundleName");
                if !base.is_empty() {
                    base
                } else {
                    path.file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Unknown".into())
                }
            }
        };
        let version = {
            let short = get("CFBundleShortVersionString");
            if short.is_empty() {
                get("CFBundleVersion")
            } else {
                short
            }
        };
        let (size, _) = dir_stats(path);
        Some(AppEntry {
            id: path.to_string_lossy().to_string(),
            name,
            version,
            publisher: String::new(),
            path: path.to_string_lossy().to_string(),
            size,
            bundle_id,
            last_modified: mtime(path),
            uninstaller: None,
        })
    }

    fn push_candidate(
        out: &mut Vec<Leftover>,
        path: PathBuf,
        kind: &str,
        recommended: bool,
    ) {
        if !path.exists() {
            return;
        }
        if out.iter().any(|item| item.path == path.to_string_lossy()) {
            return;
        }
        let (size, _) = dir_stats(&path);
        let label = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| kind.to_string());
        out.push(Leftover {
            path: path.to_string_lossy().to_string(),
            label,
            size,
            kind: kind.to_string(),
            recommended,
        });
    }

    fn glob_candidates(out: &mut Vec<Leftover>, pattern: &str, kind: &str, recommended: bool) {
        if let Ok(paths) = glob::glob(pattern) {
            for path in paths.filter_map(Result::ok) {
                push_candidate(out, path, kind, recommended);
            }
        }
    }

    pub fn leftovers_for(app_id: &str) -> Vec<Leftover> {
        let app = Path::new(app_id);
        let info = plist::Value::from_file(app.join("Contents/Info.plist")).ok();
        let dict = info.as_ref().and_then(|value| value.as_dictionary());
        let get = |key: &str| -> String {
            dict.and_then(|d| d.get(key))
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string()
        };
        let bundle = get("CFBundleIdentifier");
        let mut name = get("CFBundleName");
        if name.is_empty() {
            name = app
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
        }
        let name = name.trim_end_matches(".app").to_string();
        let mut out = Vec::new();
        let library = home().join("Library");

        if !bundle.is_empty() {
            push_candidate(
                &mut out,
                library.join("Caches").join(&bundle),
                "cache",
                true,
            );
            push_candidate(
                &mut out,
                library.join("Preferences").join(format!("{bundle}.plist")),
                "preference",
                true,
            );
            push_candidate(
                &mut out,
                library.join("Logs").join(&bundle),
                "log",
                true,
            );
            push_candidate(
                &mut out,
                library
                    .join("Saved Application State")
                    .join(format!("{bundle}.savedState")),
                "state",
                true,
            );
            push_candidate(
                &mut out,
                library.join("WebKit").join(&bundle),
                "cache",
                true,
            );
            push_candidate(
                &mut out,
                library.join("HTTPStorages").join(&bundle),
                "cache",
                true,
            );
            push_candidate(
                &mut out,
                library.join("Cookies").join(format!("{bundle}.binarycookies")),
                "cache",
                true,
            );
            push_candidate(
                &mut out,
                library.join("Application Support").join(&bundle),
                "data",
                false,
            );
            push_candidate(
                &mut out,
                library.join("Containers").join(&bundle),
                "container",
                false,
            );
            push_candidate(
                &mut out,
                library.join("Application Scripts").join(&bundle),
                "data",
                false,
            );
            let by_host = format!(
                "{}/Preferences/ByHost/{}.*.plist",
                library.display(),
                glob::Pattern::escape(&bundle)
            );
            glob_candidates(&mut out, &by_host, "preference", true);
            let group = format!(
                "{}/Group Containers/*{}*",
                library.display(),
                glob::Pattern::escape(&bundle)
            );
            glob_candidates(&mut out, &group, "container", false);
            let agents = format!(
                "{}/LaunchAgents/*{}*.plist",
                library.display(),
                glob::Pattern::escape(&bundle)
            );
            glob_candidates(&mut out, &agents, "agent", true);
        }

        if !name.is_empty() {
            let escaped = glob::Pattern::escape(&name);
            for (folder, kind, recommended) in [
                ("Application Support", "data", false),
                ("Caches", "cache", true),
                ("Logs", "log", true),
            ] {
                let pattern = format!(
                    "{}/{}/{}*",
                    library.display(),
                    folder,
                    escaped
                );
                glob_candidates(&mut out, &pattern, kind, recommended);
            }
            let agents = format!(
                "{}/LaunchAgents/*{}*.plist",
                library.display(),
                escaped
            );
            glob_candidates(&mut out, &agents, "agent", true);
        }

        out.sort_by(|a, b| b.size.cmp(&a.size));
        out
    }

    pub fn quit(app: &Path) {
        let info = plist::Value::from_file(app.join("Contents/Info.plist")).ok();
        let bundle = info
            .as_ref()
            .and_then(|value| value.as_dictionary())
            .and_then(|d| d.get("CFBundleIdentifier"))
            .and_then(|v| v.as_string())
            .unwrap_or("");
        if bundle.is_empty() {
            return;
        }
        let script = format!(
            "tell application id \"{}\" to quit",
            bundle.replace('"', "")
        );
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output();
        std::thread::sleep(std::time::Duration::from_millis(350));
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    use winreg::RegKey;

    struct RawApp {
        id: String,
        name: String,
        version: String,
        publisher: String,
        location: String,
        size: u64,
        uninstaller: Option<String>,
        install_date: String,
    }

    fn uninstall_roots() -> Vec<(RegKey, &'static str)> {
        let mut roots = Vec::new();
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(key) = hklm.open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            KEY_READ | KEY_WOW64_64KEY,
        ) {
            roots.push((key, "hklm64"));
        }
        if let Ok(key) = hklm.open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            KEY_READ | KEY_WOW64_32KEY,
        ) {
            roots.push((key, "hklm32"));
        }
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            KEY_READ,
        ) {
            roots.push((key, "hkcu"));
        }
        roots
    }

    fn raw_apps() -> Vec<RawApp> {
        let mut apps = Vec::new();
        for (root, hive) in uninstall_roots() {
            for item in root.enum_keys().filter_map(Result::ok) {
                let Ok(key) = root.open_subkey_with_flags(&item, KEY_READ) else {
                    continue;
                };
                let name: String = key.get_value("DisplayName").unwrap_or_default();
                if name.trim().is_empty() {
                    continue;
                }
                let system_component: u32 = key.get_value("SystemComponent").unwrap_or(0);
                let parent: String = key.get_value("ParentKeyName").unwrap_or_default();
                if system_component == 1 || !parent.is_empty() {
                    continue;
                }
                let version: String = key.get_value("DisplayVersion").unwrap_or_default();
                let publisher: String = key.get_value("Publisher").unwrap_or_default();
                let location: String = key.get_value("InstallLocation").unwrap_or_default();
                let uninstaller: Option<String> = key
                    .get_value::<String, _>("QuietUninstallString")
                    .ok()
                    .or_else(|| key.get_value::<String, _>("UninstallString").ok())
                    .filter(|s| !s.trim().is_empty());
                let estimated_kb: u32 = key.get_value("EstimatedSize").unwrap_or(0);
                let install_date: String = key.get_value("InstallDate").unwrap_or_default();
                apps.push(RawApp {
                    id: format!("{hive}:{item}"),
                    name,
                    version,
                    publisher,
                    location,
                    size: estimated_kb as u64 * 1024,
                    uninstaller,
                    install_date,
                });
            }
        }
        apps.sort_by(|a, b| b.size.cmp(&a.size));
        apps
    }

    fn to_entry(app: RawApp) -> AppEntry {
        let mut size = app.size;
        if size == 0 && !app.location.is_empty() && Path::new(&app.location).exists() {
            let (bytes, _) = dir_stats(Path::new(&app.location));
            size = bytes;
        }
        let last_modified = if !app.install_date.is_empty() {
            chrono::NaiveDate::parse_from_str(&app.install_date, "%Y%m%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|dt| dt.and_utc().timestamp())
                .unwrap_or(0)
        } else if !app.location.is_empty() {
            mtime(Path::new(&app.location))
        } else {
            0
        };
        AppEntry {
            id: app.id,
            name: app.name,
            version: app.version,
            publisher: app.publisher,
            path: app.location,
            size,
            bundle_id: String::new(),
            last_modified,
            uninstaller: app.uninstaller,
        }
    }

    pub fn list() -> Vec<AppEntry> {
        raw_apps().into_par_iter().map(to_entry).collect()
    }

    pub fn leftovers_for(app_id: &str) -> Vec<Leftover> {
        let Some(app) = raw_apps().into_iter().find(|app| app.id == app_id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let needle = app.name.to_lowercase();
        if needle.len() < 4 {
            return out;
        }
        let roots = [
            std::env::var("LOCALAPPDATA").map(PathBuf::from).ok(),
            std::env::var("APPDATA").map(PathBuf::from).ok(),
        ];
        for root in roots.into_iter().flatten() {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.contains(&needle) {
                    let path = entry.path();
                    let (size, _) = dir_stats(&path);
                    out.push(Leftover {
                        path: path.to_string_lossy().to_string(),
                        label: entry.file_name().to_string_lossy().to_string(),
                        size,
                        kind: "data".into(),
                        recommended: false,
                    });
                }
            }
        }
        out.sort_by(|a, b| b.size.cmp(&a.size));
        out
    }

    pub fn uninstall(app_id: &str, result: &mut OperationResult) -> Result<String, String> {
        let Some(app) = raw_apps().into_iter().find(|app| app.id == app_id) else {
            return Err("找不到该应用的卸载信息".into());
        };
        let Some(command) = app.uninstaller.clone() else {
            return Err("该应用没有提供卸载程序，请使用系统设置卸载".into());
        };
        std::process::Command::new("cmd")
            .args(["/C", &command])
            .spawn()
            .map_err(|err| format!("无法启动卸载程序：{err}"))?;
        result.removed.push(format!("已启动 {}", app.name));
        Ok(app.name)
    }
}

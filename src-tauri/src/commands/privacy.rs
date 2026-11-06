//! 查看和管理 macOS 的隐私权限记录。
//!
//! macOS 把“系统设置 → 隐私与安全性”里的开关保存在两个 TCC 数据库中：
//! 用户级 `~/Library/Application Support/com.apple.TCC/TCC.db` 和系统级
//! `/Library/Application Support/com.apple.TCC/TCC.db`。扫描会返回所有第三方
//! 应用的记录，并标记对应应用是否已经不存在；已卸载应用拖进废纸篓后，这些
//! 记录不会跟着消失，于是设置里会留下一个点不开也删不掉的条目。
//!
//! 官方 `tccutil reset All <bundle>` 在 bundle 已不存在时会直接报错
//! （`No such bundle identifier`），但 tccutil 是唯一由 tccd 认可的修改入口。
//! 直接写系统级 TCC.db 需要完全磁盘访问权限；而通过 `osascript ... with
//! administrator privileges` 提权后，子进程会脱离主应用的 TCC 身份，即使拿到
//! root 也会被 `authorization denied` 拒绝。因此这里改为：先临时注册一个
//! 只含 Info.plist 的占位 bundle，再调用 tccutil 按 bundle id 重置记录。
//! tccutil 由 tccd 执行，用户级和系统级数据库会一起处理，也不需要管理员密码。
//! 路径型记录 tccutil 无法处理，用户级仍然直接写数据库，写完重启 `tccd`。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use std::time::Duration;

use tauri::State;

use crate::models::{
    HistoryEntry, PrivacyCleanResult, PrivacyLeftover, PrivacyScan, PrivacyService, PrivacyTarget,
};
use crate::state::AppState;
use crate::store::push_history;
use crate::util::{command, home, now_rfc3339, run_command};

const USER_DB_REL: &str = "Library/Application Support/com.apple.TCC/TCC.db";
const SYSTEM_DB: &str = "/Library/Application Support/com.apple.TCC/TCC.db";
const TCCUTIL: &str = "/usr/bin/tccutil";
const LSREGISTER: &str =
    "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
const FIELD_SEP: char = '\u{1f}';

/// 一条 `access` 记录。删除时按 `client + client_type` 整组删除，
/// 因此不需要保存 `indirect_object_identifier`。
#[derive(Debug, Clone)]
struct TccRow {
    service: String,
    client: String,
    client_type: i64,
    last_modified: i64,
}

struct ClientGroup {
    scope: String,
    client: String,
    client_type: i64,
    invalid: bool,
    services: BTreeMap<String, i64>,
}

#[tauri::command(async)]
pub fn scan_privacy_leftovers() -> PrivacyScan {
    #[cfg(target_os = "macos")]
    {
        scan_macos()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PrivacyScan {
            available: false,
            full_disk_access: true,
            apps: Vec::new(),
            total_entries: 0,
            warnings: vec!["隐私权限数据库只有 macOS 才有".into()],
            platform: std::env::consts::OS.into(),
            scanned_at: now_rfc3339(),
        }
    }
}

#[tauri::command(async)]
pub fn clean_privacy_leftovers(
    targets: Vec<PrivacyTarget>,
    state: State<'_, AppState>,
) -> PrivacyCleanResult {
    #[cfg(target_os = "macos")]
    {
        let result = clean_macos(&targets, &state.data_dir);
        if result.removed_apps > 0 {
            push_history(
                &state.data_dir,
                HistoryEntry {
                    id: String::new(),
                    timestamp: now_rfc3339(),
                    kind: "privacy".into(),
                    title: format!("隐私权限清理 · {} 个应用", result.removed_apps),
                    freed: 0,
                    files: result.removed_entries as u64,
                    detail: result.messages.join("；"),
                },
            );
        }
        result
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&targets, &state);
        PrivacyCleanResult {
            failed: vec!["隐私权限清理只有 macOS 才有".into()],
            ..PrivacyCleanResult::default()
        }
    }
}

/* --------------------------------- 读取 --------------------------------- */

#[cfg(target_os = "macos")]
fn db_paths() -> [(&'static str, PathBuf); 2] {
    [
        ("user", home().join(USER_DB_REL)),
        ("system", PathBuf::from(SYSTEM_DB)),
    ]
}

#[cfg(target_os = "macos")]
fn read_db(path: &Path) -> Result<Vec<TccRow>, String> {
    if !path.exists() {
        return Err(format!("数据库不存在：{}", path.display()));
    }
    let db = path.to_string_lossy().to_string();
    let sql = "SELECT service, client, client_type, last_modified FROM access;";
    let output = run_command(
        "/usr/bin/sqlite3",
        &["-readonly", "-separator", &FIELD_SEP.to_string(), &db, sql],
    )?;
    Ok(parse_rows(&output))
}

#[cfg(target_os = "macos")]
fn parse_rows(text: &str) -> Vec<TccRow> {
    text.lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut parts = line.split(FIELD_SEP);
            let service = parts.next()?.to_string();
            let client = parts.next()?.to_string();
            let client_type = parts.next()?.parse().ok()?;
            let last_modified = parts
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            Some(TccRow {
                service,
                client,
                client_type,
                last_modified,
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn scan_macos() -> PrivacyScan {
    let mut warnings = Vec::new();
    let mut rows: Vec<(&'static str, TccRow)> = Vec::new();
    let mut readable = 0usize;

    for (scope, path) in db_paths() {
        match read_db(&path) {
            Ok(found) => {
                readable += 1;
                rows.extend(
                    found
                        .into_iter()
                        .filter(|row| is_managed_client(&row.client))
                        .map(|row| (scope, row)),
                );
            }
            Err(err) => warnings.push(format!(
                "无法读取{} TCC 数据库：{err}。请在系统设置中为 Mole Desktop 开启完全磁盘访问权限后重启应用。",
                if scope == "user" { "用户级" } else { "系统级" }
            )),
        }
    }

    let full_disk_access = super::permissions::has_full_disk_access();
    if readable == 0 {
        return PrivacyScan {
            available: false,
            full_disk_access,
            apps: Vec::new(),
            total_entries: 0,
            warnings,
            platform: "macos".into(),
            scanned_at: now_rfc3339(),
        };
    }

    // 先判断每个 client 是否还安装着，再按 client 聚合成“应用”。
    let bundle_clients: HashSet<String> = rows
        .iter()
        .filter(|(_, row)| row.client_type == 0)
        .map(|(_, row)| row.client.clone())
        .collect();
    let (installed, installed_paths) = installed_clients(&bundle_clients);

    let mut groups: BTreeMap<(String, i64, String), ClientGroup> = BTreeMap::new();
    let mut total_entries = 0usize;
    for (scope, row) in rows {
        let invalid = match row.client_type {
            0 => !installed.contains(&row.client),
            1 => path_client_is_stale(&row.client) || row.client.contains("/.Trash/"),
            _ => path_client_is_stale(&row.client),
        };
        total_entries += 1;
        let key = (scope.to_string(), row.client_type, row.client.clone());
        let group = groups.entry(key).or_insert_with(|| ClientGroup {
            scope: scope.to_string(),
            client: row.client.clone(),
            client_type: row.client_type,
            invalid,
            services: BTreeMap::new(),
        });
        let last = group.services.entry(row.service).or_insert(0);
        *last = (*last).max(row.last_modified);
    }

    let trash = if groups.values().any(|group| group.invalid && group.client_type == 0) {
        trash_app_index()
    } else {
        HashMap::new()
    };

    let mut apps: Vec<PrivacyLeftover> = groups
        .into_values()
        .map(|group| {
            let last_modified = group.services.values().copied().max().unwrap_or(0);
            let mut services: Vec<PrivacyService> = group
                .services
                .iter()
                .map(|(service, modified)| PrivacyService {
                    service: service.clone(),
                    label: service_label(service),
                    last_modified: *modified,
                })
                .collect();
            services.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));

            let (name, path, in_trash) = if group.client_type == 0 {
                match if group.invalid {
                    trash.get(&group.client)
                } else {
                    None
                } {
                    Some((name, path)) => (name.clone(), Some(path.clone()), true),
                    None => match installed_paths.get(&group.client) {
                        Some(app) => (
                            bundle_display_name_from_path(app)
                                .unwrap_or_else(|| bundle_display_name(&group.client)),
                            Some(app.to_string_lossy().to_string()),
                            false,
                        ),
                        None => (bundle_display_name(&group.client), None, false),
                    },
                }
            } else {
                // 路径型记录只有在文件仍存在（通常是被拖进废纸篓）时才有可展示的位置。
                let exists = Path::new(&group.client).exists();
                (
                    path_display_name(&group.client),
                    if exists {
                        Some(group.client.clone())
                    } else {
                        None
                    },
                    group.client.contains("/.Trash/"),
                )
            };

            PrivacyLeftover {
                id: format!("{}:{}:{}", group.scope, group.client_type, group.client),
                name,
                client: group.client,
                client_type: group.client_type,
                scope: group.scope,
                path,
                services,
                last_modified,
                in_trash,
                invalid: group.invalid,
            }
        })
        .collect();

    apps.sort_by(|a, b| {
        b.invalid
            .cmp(&a.invalid)
            .then_with(|| a.scope.cmp(&b.scope))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    PrivacyScan {
        available: true,
        full_disk_access,
        apps,
        total_entries,
        warnings,
        platform: "macos".into(),
        scanned_at: now_rfc3339(),
    }
}

/// 跳过 Apple 自带组件，避免把系统权限也当成可管理项。
#[cfg(target_os = "macos")]
fn is_managed_client(client: &str) -> bool {
    if client.is_empty() || client.starts_with("com.apple.") || client == "com.apple" {
        return false;
    }
    if client.starts_with('/') {
        const PROTECTED: [&str; 8] = [
            "/System/",
            "/usr/bin/",
            "/usr/sbin/",
            "/usr/libexec/",
            "/bin/",
            "/sbin/",
            "/Library/Apple/",
            "/private/var/",
        ];
        return !PROTECTED.iter().any(|prefix| client.starts_with(prefix));
    }
    true
}

/* ------------------------------ 判定是否残留 ----------------------------- */

#[cfg(target_os = "macos")]
fn installed_clients(
    clients: &HashSet<String>,
) -> (HashSet<String>, HashMap<String, PathBuf>) {
    let mut installed = HashSet::new();
    let mut paths = HashMap::new();
    let mut unresolved = Vec::new();
    for client in clients {
        if client.starts_with("com.apple.") {
            installed.insert(client.clone());
            continue;
        }
        match bundle_path(client) {
            Some(path) => {
                installed.insert(client.clone());
                paths.insert(client.clone(), path);
            }
            None => unresolved.push(client.clone()),
        }
    }
    if unresolved.is_empty() {
        return (installed, paths);
    }

    // LaunchServices 通常能解析主应用和嵌套 helper；解析不到时再兜底：
    // 扫描应用目录 / 系统扩展，并查询 Spotlight（覆盖外接磁盘等位置）。
    let on_disk = collect_bundles();
    unresolved.retain(|client| {
        if let Some(path) = on_disk.get(client) {
            installed.insert(client.clone());
            paths.insert(client.clone(), path.clone());
            false
        } else if helper_installed(client) || spotlight_has_bundle(client) {
            installed.insert(client.clone());
            false
        } else {
            true
        }
    });
    (installed, paths)
}

/// 通过 LaunchServices 找到 bundle id 对应的应用路径。
#[cfg(target_os = "macos")]
pub(crate) fn bundle_path(bundle_id: &str) -> Option<PathBuf> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    objc2::rc::autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let identifier = NSString::from_str(bundle_id);
        let url = workspace.URLForApplicationWithBundleIdentifier(&identifier)?;
        url.path().map(|path| PathBuf::from(path.to_string()))
    })
}

/// 扫描常见应用目录，返回 bundle id 到应用包路径的映射。
#[cfg(target_os = "macos")]
pub(crate) fn collect_bundles() -> HashMap<String, PathBuf> {
    use walkdir::WalkDir;

    let mut bundles = HashMap::new();
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Library/CoreServices"),
        PathBuf::from("/System/Library/CoreServices/Applications"),
        // 输入法、屏幕保护、系统扩展等组件不会注册成普通应用，
        // 不单独扫描就会把它们仍然安装的权限记录误判成残留。
        PathBuf::from("/Library/Input Methods"),
        PathBuf::from("/Library/Screen Savers"),
        PathBuf::from("/Library/PreferencePanes"),
        PathBuf::from("/Library/Internet Plug-Ins"),
        PathBuf::from("/Library/QuickLook"),
        PathBuf::from("/Library/Spotlight"),
        PathBuf::from("/Library/ScriptingAdditions"),
        PathBuf::from("/Library/SystemExtensions"),
        // Karabiner 这类工具把辅助 .app 放在自己的支持目录里。
        PathBuf::from("/Library/Application Support"),
        PathBuf::from("/Users/Shared/Applications"),
    ];
    roots.push(home().join("Applications"));
    roots.push(home().join("Library/Input Methods"));
    roots.push(home().join("Library/Screen Savers"));

    for root in roots {
        if !root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&root)
            .max_depth(6)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            let is_bundle = name.ends_with(".app")
                || name.ends_with(".appex")
                || name.ends_with(".systemextension");
            if !is_bundle {
                continue;
            }
            if let Some(id) = read_info_plist_id(&entry.path().join("Contents/Info.plist")) {
                bundles
                    .entry(id)
                    .or_insert_with(|| entry.path().to_path_buf());
            }
        }
    }
    bundles
}

#[cfg(target_os = "macos")]
pub(crate) fn read_info_plist_id(info: &Path) -> Option<String> {
    let value = plist::Value::from_file(info).ok()?;
    value
        .as_dictionary()?
        .get("CFBundleIdentifier")?
        .as_string()
        .map(str::to_owned)
}

/// 有些权限属于 launchd helper / 系统扩展，它们不会注册成普通应用。
#[cfg(target_os = "macos")]
fn helper_installed(bundle_id: &str) -> bool {
    let plist = format!("{bundle_id}.plist");
    [
        PathBuf::from("/Library/PrivilegedHelperTools").join(bundle_id),
        PathBuf::from("/Library/LaunchDaemons").join(&plist),
        PathBuf::from("/Library/LaunchAgents").join(&plist),
        home().join("Library/LaunchAgents").join(&plist),
    ]
    .iter()
    .any(|path| path.exists())
}

#[cfg(target_os = "macos")]
fn spotlight_has_bundle(bundle_id: &str) -> bool {
    let escaped = bundle_id.replace('\\', "\\\\").replace('\'', "\\'");
    let query = format!("kMDItemCFBundleIdentifier == '{escaped}'");
    run_command("/usr/bin/mdfind", &[&query])
        .map(|output| !output.trim().is_empty())
        .unwrap_or(false)
}

/// 只有“绝对路径且文件确实不存在”的可执行文件才算残留；
/// 系统目录一律不碰，避免误删系统组件。
#[cfg(target_os = "macos")]
fn path_client_is_stale(client: &str) -> bool {
    let path = Path::new(client);
    if client.is_empty() || !path.is_absolute() || path.exists() {
        return false;
    }
    const PROTECTED: [&str; 6] = [
        "/System/",
        "/usr/",
        "/bin/",
        "/sbin/",
        "/Library/Apple/",
        "/private/var/",
    ];
    !PROTECTED.iter().any(|prefix| client.starts_with(prefix))
}

/* ------------------------------- 名称展示 ------------------------------- */

#[cfg(target_os = "macos")]
fn bundle_display_name(bundle_id: &str) -> String {
    bundle_id
        .rsplit('.')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(bundle_id)
        .to_string()
}

/// 从已安装应用的 Info.plist 里读取可读名称。
#[cfg(target_os = "macos")]
fn bundle_display_name_from_path(path: &Path) -> Option<String> {
    let value = plist::Value::from_file(path.join("Contents").join("Info.plist")).ok()?;
    let dict = value.as_dictionary()?;
    dict.get("CFBundleDisplayName")
        .or_else(|| dict.get("CFBundleName"))
        .and_then(|value| value.as_string())
        .map(str::to_owned)
        .filter(|name| !name.is_empty())
}

#[cfg(target_os = "macos")]
fn path_display_name(client: &str) -> String {
    let path = Path::new(client);
    for component in path.components() {
        let text = component.as_os_str().to_string_lossy();
        if let Some(name) = text.strip_suffix(".app") {
            return name.to_string();
        }
    }
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| client.to_string())
}

/// 应用被拖进废纸篓后，从 Info.plist 里取回可读名称。
#[cfg(target_os = "macos")]
fn trash_app_index() -> HashMap<String, (String, String)> {
    use walkdir::WalkDir;

    let mut found = HashMap::new();
    let root = home().join(".Trash");
    if !root.is_dir() {
        return found;
    }
    let mut visited = 0usize;
    for entry in WalkDir::new(&root)
        .max_depth(5)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        visited += 1;
        if visited > 4000 {
            break;
        }
        if !entry.file_type().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if !name.ends_with(".app") {
            continue;
        }
        let Some(id) = read_info_plist_id(&entry.path().join("Contents/Info.plist")) else {
            continue;
        };
        found.entry(id).or_insert_with(|| {
            (
                name.trim_end_matches(".app").to_string(),
                entry.path().to_string_lossy().to_string(),
            )
        });
    }
    found
}

#[cfg(target_os = "macos")]
fn service_label(service: &str) -> String {
    let label = match service {
        "kTCCServiceScreenCapture" => "屏幕与系统录音",
        "kTCCServiceAudioCapture" => "系统音频录制",
        "kTCCServiceMicrophone" => "麦克风",
        "kTCCServiceCamera" => "摄像头",
        "kTCCServiceAccessibility" => "辅助功能",
        "kTCCServicePostEvent" => "控制其他应用",
        "kTCCServiceListenEvent" => "输入监控",
        "kTCCServiceAppleEvents" => "自动化",
        "kTCCServiceSystemPolicyAllFiles" => "完全磁盘访问",
        "kTCCServiceSystemPolicyDesktopFolder" => "桌面文件夹",
        "kTCCServiceSystemPolicyDocumentsFolder" => "文稿文件夹",
        "kTCCServiceSystemPolicyDownloadsFolder" => "下载文件夹",
        "kTCCServiceSystemPolicyNetworkVolumes" => "网络宗卷",
        "kTCCServiceSystemPolicyRemovableVolumes" => "可移除宗卷",
        "kTCCServiceSystemPolicyAppData" => "其他应用数据",
        "kTCCServiceSystemPolicyAppBundles" => "应用管理",
        "kTCCServiceSystemPolicySysAdminFiles" => "系统管理文件",
        "kTCCServiceCalendar" => "日历",
        "kTCCServiceReminders" => "提醒事项",
        "kTCCServiceAddressBook" => "通讯录",
        "kTCCServiceContactsFull" | "kTCCServiceContactsLimited" => "联系人",
        "kTCCServicePhotos" => "照片",
        "kTCCServicePhotosAdd" => "照片（添加）",
        "kTCCServiceBluetoothAlways" => "蓝牙",
        "kTCCServiceMediaLibrary" => "媒体与 Apple Music",
        "kTCCServiceLocation" => "定位",
        "kTCCServiceSiri" => "Siri",
        "kTCCServiceSpeechRecognition" => "语音识别",
        "kTCCServiceUserTracking" => "跟踪",
        "kTCCServiceFileProviderDomain" => "文件提供程序",
        "kTCCServiceUbiquity" => "iCloud 文稿",
        "kTCCServiceLiverpool" => "系统服务",
        "kTCCServiceFocusStatus" => "专注状态",
        "kTCCServiceWillow" => "家庭",
        "kTCCServiceHealth" => "健康",
        "kTCCServiceDeveloperTool" => "开发者工具",
        "kTCCServiceEndpointSecurityClient" => "端点安全",
        _ => {
            return service
                .strip_prefix("kTCCService")
                .unwrap_or(service)
                .to_string()
        }
    };
    label.to_string()
}

/* --------------------------------- 清理 --------------------------------- */

#[cfg(target_os = "macos")]
fn clean_macos(targets: &[PrivacyTarget], data_dir: &Path) -> PrivacyCleanResult {
    let mut result = PrivacyCleanResult::default();
    let scan = scan_macos();
    if !scan.available {
        result
            .failed
            .push("无法读取隐私权限数据库，请先授予完全磁盘访问权限".into());
        return result;
    }

    let wanted: HashSet<(String, i64, String)> = targets
        .iter()
        .map(|target| {
            (
                target.scope.clone(),
                target.client_type,
                target.client.clone(),
            )
        })
        .collect();
    let selected: Vec<&PrivacyLeftover> = scan
        .apps
        .iter()
        .filter(|app| wanted.contains(&(app.scope.clone(), app.client_type, app.client.clone())))
        .collect();
    if selected.is_empty() {
        result
            .messages
            .push("没有需要处理的权限记录，可能已经被清理过了".into());
        return result;
    }

    let mut names: HashMap<(i64, String), String> = HashMap::new();
    for app in &selected {
        names
            .entry((app.client_type, app.client.clone()))
            .or_insert_with(|| app.name.clone());
    }

    let mut done_bundles: HashSet<String> = HashSet::new();
    let mut done_user_paths: HashSet<(String, i64)> = HashSet::new();

    // bundle 型记录统一交给 tccutil，用户级和系统级数据库会被一起处理。
    let mut bundle_clients: Vec<String> = selected
        .iter()
        .filter(|app| app.client_type == 0)
        .map(|app| app.client.clone())
        .collect();
    bundle_clients.sort();
    bundle_clients.dedup();

    for client in &bundle_clients {
        if result.cancelled {
            break;
        }
        let name = names
            .get(&(0, client.clone()))
            .cloned()
            .unwrap_or_else(|| client.clone());
        match reset_bundle_permissions(data_dir, client) {
            ResetOutcome::Done => {
                done_bundles.insert(client.clone());
            }
            ResetOutcome::Cancelled => {
                result.cancelled = true;
                result
                    .messages
                    .push("已取消管理员授权，未重置的记录保持原样".into());
            }
            ResetOutcome::Failed(err) => result
                .failed
                .push(format!("“{name}”的权限记录重置失败：{err}")),
        }
    }

    // 路径型记录 tccutil 无法按 bundle id 处理，用户级数据库可以直接写。
    let mut user_paths: Vec<(String, i64)> = selected
        .iter()
        .filter(|app| app.client_type != 0 && app.scope != "system")
        .map(|app| (app.client.clone(), app.client_type))
        .collect();
    dedup_clients(&mut user_paths);
    let user_path_entries: usize = selected
        .iter()
        .filter(|app| app.client_type != 0 && app.scope != "system")
        .map(|app| app.services.len())
        .sum();
    if !user_paths.is_empty() {
        match delete_rows(&home().join(USER_DB_REL), &user_paths) {
            Ok(()) => {
                for (client, client_type) in &user_paths {
                    done_user_paths.insert((client.clone(), *client_type));
                }
                result
                    .messages
                    .push(format!("已清理 {user_path_entries} 条路径型权限记录"));
            }
            Err(err) => result
                .failed
                .push(format!("用户级权限记录清理失败：{}", trim_script_error(&err))),
        }
        restart_tccd();
    }

    // 系统级数据库按路径记录的条目没有可用的公开入口，只能提示手动处理。
    let system_path_entries: usize = selected
        .iter()
        .filter(|app| app.client_type != 0 && app.scope == "system")
        .map(|app| app.services.len())
        .sum();
    if system_path_entries > 0 {
        result.failed.push(format!(
            "{system_path_entries} 条按可执行文件路径记录的系统级权限受 macOS 保护，无法自动清理，请在“系统设置 → 隐私与安全性”中手动移除"
        ));
    }

    for app in &selected {
        let done = if app.client_type == 0 {
            done_bundles.contains(&app.client)
        } else {
            done_user_paths.contains(&(app.client.clone(), app.client_type))
        };
        if done {
            result.removed_apps += 1;
            result.removed_entries += app.services.len();
        }
    }

    if !done_bundles.is_empty() {
        result.messages.push(format!(
            "已重置 {} 个应用的权限记录（含系统级）",
            done_bundles.len()
        ));
    }
    if result.removed_apps > 0 {
        result
            .messages
            .push("如系统设置仍显示旧条目，请关闭并重新打开“系统设置”后再查看。".into());
    }
    result
}

#[cfg(target_os = "macos")]
fn dedup_clients(clients: &mut Vec<(String, i64)>) {
    let mut seen = HashSet::new();
    clients.retain(|entry| seen.insert(entry.clone()));
}

/* ----------------------------- tccutil 重置 ----------------------------- */

#[cfg(target_os = "macos")]
enum ResetOutcome {
    Done,
    Cancelled,
    Failed(String),
}

/// 临时注册一个占位 bundle，让 `tccutil reset` 能按 bundle id 找到应用。
///
/// 应用被卸载后 TCC 记录还在，tccutil 会报 `No such bundle identifier`；
/// LaunchServices 只要能看到一个同 id 的 bundle 就会放行。占位应用只包含
/// Info.plist 和一个空壳可执行文件，重置结束后立即注销并删除。
#[cfg(target_os = "macos")]
struct ProxyBundle {
    path: PathBuf,
}

#[cfg(target_os = "macos")]
impl ProxyBundle {
    fn register(root: &Path, client: &str) -> Result<Self, String> {
        let dir = root.join("tcc-proxy");
        std::fs::create_dir_all(&dir).map_err(|err| format!("无法创建临时目录：{err}"))?;

        let safe: String = client
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        let path = dir.join(format!("{safe}-{}.app", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);

        let executable = path.join("Contents/MacOS/MoleTccProxy");
        std::fs::create_dir_all(
            executable
                .parent()
                .ok_or_else(|| "临时应用路径无效".to_string())?,
        )
        .map_err(|err| format!("无法创建临时应用：{err}"))?;

        let plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\"><dict>\n\
             <key>CFBundleIdentifier</key><string>{}</string>\n\
             <key>CFBundleExecutable</key><string>MoleTccProxy</string>\n\
             <key>CFBundleName</key><string>MoleTccProxy</string>\n\
             <key>CFBundlePackageType</key><string>APPL</string>\n\
             <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>\n\
             </dict></plist>\n",
            xml_escape(client)
        );
        std::fs::write(path.join("Contents/Info.plist"), plist)
            .map_err(|err| format!("无法写入临时应用信息：{err}"))?;
        std::fs::write(&executable, "#!/bin/sh\nexit 0\n")
            .map_err(|err| format!("无法写入临时应用：{err}"))?;
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755));

        let text = path.to_string_lossy().to_string();
        run_command(LSREGISTER, &["-f", text.as_str()])
            .map_err(|err| format!("无法注册临时应用：{}", trim_script_error(&err)))?;
        Ok(ProxyBundle { path })
    }
}

#[cfg(target_os = "macos")]
impl Drop for ProxyBundle {
    fn drop(&mut self) {
        let text = self.path.to_string_lossy().to_string();
        let _ = run_command(LSREGISTER, &["-u", text.as_str()]);
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(target_os = "macos")]
fn reset_bundle_permissions(data_dir: &Path, client: &str) -> ResetOutcome {
    let mut proxy: Option<ProxyBundle> = None;
    let mut last_error = String::new();

    // LaunchServices 注册后可能要过一小会儿才能被 tccutil 看到。
    for _ in 0..12 {
        match run_command(TCCUTIL, &["reset", "All", client]) {
            Ok(_) => return ResetOutcome::Done,
            Err(err) => {
                last_error = err;
                if bundle_missing(&last_error) {
                    if proxy.is_none() {
                        match ProxyBundle::register(data_dir, client) {
                            Ok(created) => proxy = Some(created),
                            Err(err) => return ResetOutcome::Failed(err),
                        }
                    }
                } else if needs_admin(&last_error) {
                    return reset_bundle_as_admin(client);
                } else {
                    return ResetOutcome::Failed(trim_script_error(&last_error));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    if bundle_missing(&last_error) {
        ResetOutcome::Failed("macOS 没有识别到对应的应用，无法重置权限记录".into())
    } else {
        ResetOutcome::Failed(trim_script_error(&last_error))
    }
}

/// 个别系统版本会要求 root 才能重置系统级记录；tccutil 通过 tccd 执行，
/// 不直接访问 TCC.db，所以走管理员授权仍然有效。
#[cfg(target_os = "macos")]
fn reset_bundle_as_admin(client: &str) -> ResetOutcome {
    let script = format!("{} reset All {}", sh_quote(TCCUTIL), sh_quote(client));
    match run_admin_script(&script) {
        Ok(_) => ResetOutcome::Done,
        Err(err) => {
            let text = trim_script_error(&err);
            if text.contains("(-128)") || text.to_lowercase().contains("cancel") {
                ResetOutcome::Cancelled
            } else {
                ResetOutcome::Failed(text)
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn bundle_missing(err: &str) -> bool {
    err.contains("-10814") || err.contains("No such bundle identifier")
}

#[cfg(target_os = "macos")]
fn needs_admin(err: &str) -> bool {
    let text = err.to_lowercase();
    ["not permitted", "permission", "denied", "authorization", "privilege", "run as root"]
        .iter()
        .any(|needle| text.contains(needle))
}

/// osascript 的错误会带上 `0:381: execution error: Error: ` 前缀，展示前去掉。
#[cfg(target_os = "macos")]
fn trim_script_error(err: &str) -> String {
    let text = err.trim();
    match text.split_once("execution error: ") {
        Some((_, rest)) => rest
            .strip_prefix("Error: ")
            .unwrap_or(rest)
            .trim()
            .to_string(),
        None => text.to_string(),
    }
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn delete_rows(path: &Path, clients: &[(String, i64)]) -> Result<(), String> {
    let sql = delete_sql(clients);
    let mut child = command("/usr/bin/sqlite3")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("无法启动 sqlite3：{err}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(sql.as_bytes())
            .map_err(|err| format!("写入清理指令失败：{err}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("执行清理失败：{err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("sqlite3 退出码 {}", output.status)
        } else {
            stderr
        })
    }
}

#[cfg(target_os = "macos")]
fn delete_sql(clients: &[(String, i64)]) -> String {
    let mut sql = String::from("PRAGMA busy_timeout=5000;\nBEGIN IMMEDIATE;\n");
    for (client, client_type) in clients {
        sql.push_str(&format!(
            "DELETE FROM access WHERE client = {} AND client_type = {};\n",
            sql_literal(client),
            client_type
        ));
    }
    sql.push_str("COMMIT;\n");
    sql
}

#[cfg(target_os = "macos")]
fn sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "macos")]
fn run_admin_script(script: &str) -> Result<String, String> {
    let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
    let applescript = format!("do shell script \"{escaped}\" with administrator privileges");
    run_command("osascript", &["-e", &applescript])
}

#[cfg(target_os = "macos")]
fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn restart_tccd() {
    let _ = Command::new("/usr/bin/killall")
        .arg("tccd")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}


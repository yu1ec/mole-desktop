//! 管理 macOS 的「登录时打开」和「App 后台活动」。
//!
//! macOS 13 起，这两份列表由 BackgroundTaskManagement（BTM）统一管理，
//! 数据存放在 `/var/db/com.apple.backgroundtaskmanagement/BackgroundItems-v*.btm`，
//! 这是一个 NSKeyedArchiver 格式的二进制 plist，普通用户可以读取。
//!
//! 扫描会返回所有项目（包括仍然有效的应用），并标记应用是否已经不存在。
//! 后台项目背后通常有 launchd plist：`~/Library/LaunchAgents`、`/Library/LaunchAgents`、
//! `/Library/LaunchDaemons`，这类项目可以删除 plist 或通过 launchctl 禁用/启用。
//! 没有独立 plist 的 BTM 项目只能在「系统设置 → 通用 → 登录项与扩展」中处理。

use tauri::State;

use crate::models::{
    HistoryEntry, StartupCleanResult, StartupLeftover, StartupScan, StartupTarget,
};
use crate::state::AppState;
use crate::store::push_history;
use crate::util::{home, now_rfc3339, run_command};

const BTM_DIR: &str = "/var/db/com.apple.backgroundtaskmanagement";

/// 这些位置属于系统，即使可执行文件缺失也不当作残留处理。
/// 注意不要用 `/usr/` 一刀切，`/usr/local` 下通常是第三方工具。
const PROTECTED_PREFIXES: [&str; 8] = [
    "/System/",
    "/usr/bin/",
    "/usr/sbin/",
    "/usr/libexec/",
    "/bin/",
    "/sbin/",
    "/Library/Apple/",
    "/private/var/",
];

#[tauri::command(async)]
pub fn scan_startup_leftovers() -> StartupScan {
    #[cfg(target_os = "macos")]
    {
        scan_macos()
    }
    #[cfg(not(target_os = "macos"))]
    {
        StartupScan {
            available: false,
            items: Vec::new(),
            warnings: vec!["登录项和后台项目只有 macOS 才有".into()],
            platform: std::env::consts::OS.into(),
            scanned_at: now_rfc3339(),
        }
    }
}

#[tauri::command(async)]
pub fn clean_startup_leftovers(
    targets: Vec<StartupTarget>,
    state: State<'_, AppState>,
) -> StartupCleanResult {
    #[cfg(target_os = "macos")]
    {
        let result = clean_macos(&targets);
        if result.removed_items > 0 {
            push_history(
                &state.data_dir,
                HistoryEntry {
                    id: String::new(),
                    timestamp: now_rfc3339(),
                    kind: "startup".into(),
                    title: format!("启动项清理 · {} 项", result.removed_items),
                    freed: 0,
                    files: result.removed_items as u64,
                    detail: result.messages.join("；"),
                },
            );
        }
        result
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&targets, &state);
        StartupCleanResult {
            failed: vec!["登录项和后台项目只有 macOS 才有".into()],
            ..StartupCleanResult::default()
        }
    }
}

/// 禁用或启用一个由 launchd 管理的项目。
#[tauri::command(async)]
pub fn set_startup_item_enabled(id: String, enabled: bool) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        toggle_macos(&id, enabled)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (id, enabled);
        Err("登录项和后台项目只有 macOS 才有".into())
    }
}

/* -------------------------------- 扫描 --------------------------------- */

#[cfg(target_os = "macos")]
fn scan_macos() -> StartupScan {
    let mut warnings = Vec::new();
    let mut available = false;

    let (mut items, launchd_errors, launchd_readable) = scan_launchd();
    if launchd_readable {
        available = true;
    }
    warnings.extend(launchd_errors);

    let mut plist_index: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (index, item) in items.iter().enumerate() {
        if let Some(path) = &item.plist_path {
            plist_index.insert(path.clone(), index);
        }
    }

    match scan_btm() {
        Ok(found) => {
            available = true;
            for btm in found {
                // BTM 与 launchd 描述的是同一个项目时合并成一行，
                // 保留 BTM 的展示信息和登录项语义，使用 launchd 的启用状态。
                if let Some(index) = btm
                    .plist_path
                    .as_ref()
                    .and_then(|path| plist_index.get(path).copied())
                {
                    merge_btm(&mut items[index], btm);
                } else {
                    items.push(btm);
                }
            }
        }
        Err(err) => warnings.push(format!("无法读取后台项目数据库：{err}")),
    }

    items.sort_by(|a, b| {
        b.invalid
            .cmp(&a.invalid)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    StartupScan {
        available,
        items,
        warnings,
        platform: "macos".into(),
        scanned_at: now_rfc3339(),
    }
}

/// launchd 决定项目是否失效、是否启用；BTM 补充名称、开发者和登录项语义。
#[cfg(target_os = "macos")]
fn merge_btm(target: &mut StartupLeftover, btm: StartupLeftover) {
    if btm.kind == "login" {
        target.kind = "login".into();
        if target.enabled.is_none() {
            target.enabled = btm.enabled;
        }
    }
    if target.name.is_empty() {
        target.name = btm.name;
    }
    if target.developer.is_none() {
        target.developer = btm.developer;
    }
    if target.bundle_id.is_none() {
        target.bundle_id = btm.bundle_id;
    }
    if target.path.is_none() {
        target.path = btm.path;
    }
    target.requires_admin |= btm.requires_admin;
    if btm.kind == "login" {
        // 登录项可以通过 System Events 移除，即使它同时有 launchd plist。
        target.removable = true;
    }
    if target.note.is_none() {
        target.note = btm.note;
    }
}

/* ------------------------------ BTM 数据库 ------------------------------ */

#[cfg(target_os = "macos")]
struct BtmArchive {
    objects: Vec<plist::Value>,
}

#[cfg(target_os = "macos")]
impl BtmArchive {
    fn resolve<'a>(&'a self, value: &'a plist::Value) -> Option<&'a plist::Value> {
        let mut current = value;
        for _ in 0..8 {
            match current {
                plist::Value::Uid(uid) => current = self.objects.get(uid.get() as usize)?,
                _ => return Some(current),
            }
        }
        None
    }

    fn dict<'a>(&'a self, value: &'a plist::Value) -> Option<&'a plist::Dictionary> {
        self.resolve(value)?.as_dictionary()
    }

    fn array<'a>(&'a self, value: &'a plist::Value) -> Option<&'a Vec<plist::Value>> {
        let resolved = self.resolve(value)?;
        if let Some(items) = resolved.as_array() {
            return Some(items);
        }
        // NSArray 有时会被归档成只带 NS.objects 的字典。
        let dict = resolved.as_dictionary()?;
        self.array(dict.get("NS.objects")?)
    }

    /// NSKeyedArchiver 里的 NSDictionary 把键和值分别放在 NS.keys / NS.objects。
    fn dictionary_values<'a>(&'a self, value: &'a plist::Value) -> Vec<&'a plist::Value> {
        let Some(dict) = self.dict(value) else {
            return Vec::new();
        };
        if let Some(values) = dict.get("NS.objects").and_then(|value| self.array(value)) {
            return values
                .iter()
                .filter_map(|value| self.resolve(value))
                .collect();
        }
        dict.values()
            .filter_map(|value| self.resolve(value))
            .collect()
    }

    fn string(&self, dict: &plist::Dictionary, key: &str) -> Option<String> {
        self.resolve(dict.get(key)?)?.as_string().map(str::to_owned)
    }

    fn integer(&self, dict: &plist::Dictionary, key: &str) -> Option<i64> {
        let value = self.resolve(dict.get(key)?)?;
        value
            .as_signed_integer()
            .or_else(|| value.as_unsigned_integer().map(|number| number as i64))
    }

    fn strings(&self, dict: &plist::Dictionary, key: &str) -> Vec<String> {
        dict.get(key)
            .and_then(|value| self.array(value))
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| self.resolve(value)?.as_string().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(target_os = "macos")]
fn btm_path() -> Option<std::path::PathBuf> {
    let dir = std::path::Path::new(BTM_DIR);
    let mut best: Option<(u32, std::path::PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(version) = name
            .strip_prefix("BackgroundItems-v")
            .and_then(|rest| rest.strip_suffix(".btm"))
            .and_then(|rest| rest.parse::<u32>().ok())
        else {
            continue;
        };
        let path = entry.path();
        if best
            .as_ref()
            .map(|(current, _)| version > *current)
            .unwrap_or(true)
        {
            best = Some((version, path));
        }
    }
    best.map(|(_, path)| path)
}

/// 一条 BTM 记录里我们关心的字段。
#[cfg(target_os = "macos")]
struct BtmEntry {
    base_type: u32,
    disposition: i64,
    identifier: String,
    name: String,
    developer: Option<String>,
    bundle_id: Option<String>,
    container: Option<String>,
    url: Option<String>,
    executable: Option<String>,
    program_arguments: Vec<String>,
}

#[cfg(target_os = "macos")]
impl BtmEntry {
    fn kind(&self) -> &'static str {
        // type 4 是嵌套在应用里的登录项；type 2 的应用只有在“登录时打开”开启时
        // （disposition 最低位）才属于登录项，否则归到后台活动。
        if self.base_type == 4 || (self.base_type == 2 && self.disposition & 1 != 0) {
            "login"
        } else {
            "background"
        }
    }

    fn item_type(&self) -> &'static str {
        match self.base_type {
            2 => "app",
            4 => "login-item",
            8 => "agent",
            16 => "daemon",
            64 => "importer",
            128 => "plugin",
            2048 => "app-extension",
            _ => "other",
        }
    }
}

#[cfg(target_os = "macos")]
fn scan_btm() -> Result<Vec<StartupLeftover>, String> {
    let path = btm_path().ok_or("找不到 BackgroundItems 数据库")?;
    let root =
        plist::Value::from_file(&path).map_err(|err| format!("{}：{err}", path.display()))?;
    let root_dict = root
        .as_dictionary()
        .ok_or("BackgroundItems 数据库格式不正确")?;
    let objects = root_dict
        .get("$objects")
        .and_then(|value| value.as_array())
        .ok_or("BackgroundItems 数据库缺少 $objects")?
        .to_vec();
    let archive = BtmArchive { objects };

    let top = root_dict
        .get("$top")
        .and_then(|value| archive.dict(value))
        .ok_or("BackgroundItems 数据库缺少 $top")?;
    let store = top
        .get("store")
        .and_then(|value| archive.dict(value))
        .ok_or("BackgroundItems 数据库缺少 store")?;
    let by_user = store
        .get("itemsByUserIdentifier")
        .ok_or("BackgroundItems 数据库缺少 itemsByUserIdentifier")?;

    let mut resolver = BundleResolver::default();
    let mut found: std::collections::BTreeMap<String, StartupLeftover> =
        std::collections::BTreeMap::new();

    for raw_items in archive.dictionary_values(by_user) {
        let Some(items) = archive.array(raw_items) else {
            continue;
        };
        for raw in items {
            let Some(dict) = archive.dict(raw) else {
                continue;
            };
            let Some(entry) = read_btm_entry(&archive, dict) else {
                continue;
            };
            // type 32 只是开发者分组，不是列表里单独的一行。
            if entry.base_type == 32 || entry.base_type == 0 {
                continue;
            }
            let Some(item) = evaluate_btm_entry(&entry, &mut resolver) else {
                continue;
            };
            let id = item.id.clone();
            match found.get_mut(&id) {
                // 同一个项目会出现在多个用户分组里，优先保留“登录时打开”的判定。
                Some(existing) if existing.kind == "login" || item.kind != "login" => {}
                Some(existing) => *existing = item,
                None => {
                    found.insert(id, item);
                }
            }
        }
    }

    Ok(found.into_values().collect())
}

#[cfg(target_os = "macos")]
fn read_btm_entry(archive: &BtmArchive, dict: &plist::Dictionary) -> Option<BtmEntry> {
    let raw_type = archive.integer(dict, "type")?;
    let identifier = archive
        .string(dict, "identifier")
        .or_else(|| archive.string(dict, "bundleIdentifier"))
        .unwrap_or_default();
    if identifier.is_empty() {
        return None;
    }
    Some(BtmEntry {
        base_type: (raw_type as u32) & 0xFFFF,
        identifier,
        disposition: archive.integer(dict, "disposition").unwrap_or(0),
        name: archive.string(dict, "name").unwrap_or_default(),
        developer: archive
            .string(dict, "developerName")
            .filter(|name| !is_placeholder(name)),
        bundle_id: archive
            .string(dict, "bundleIdentifier")
            .filter(|id| !is_placeholder(id)),
        container: archive
            .string(dict, "container")
            .filter(|name| !is_placeholder(name)),
        url: read_btm_url(archive, dict),
        executable: archive
            .string(dict, "executablePath")
            .filter(|path| !is_placeholder(path)),
        program_arguments: archive.strings(dict, "programArguments"),
    })
}

/// BTM 大量使用 `$null` 字符串占位，不能当成真实值。
#[cfg(target_os = "macos")]
fn is_placeholder(value: &str) -> bool {
    value.is_empty() || value == "$null"
}

#[cfg(target_os = "macos")]
fn read_btm_url(archive: &BtmArchive, dict: &plist::Dictionary) -> Option<String> {
    let url = archive.dict(dict.get("url")?)?;
    archive
        .string(url, "NS.relative")
        .or_else(|| archive.string(url, "NS.base"))
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "macos")]
fn evaluate_btm_entry(entry: &BtmEntry, resolver: &mut BundleResolver) -> Option<StartupLeftover> {
    let container_id = entry.container.as_deref().and_then(container_bundle_id);
    let container_path = container_id.as_deref().and_then(|id| resolver.resolve(id));

    let url_path = entry.url.as_deref().and_then(file_url_path);
    let relative_url = entry
        .url
        .as_deref()
        .filter(|url| !url.starts_with("file:") && !url.starts_with('/') && !url.is_empty());

    let mut path = url_path.clone();
    if path.is_none() {
        if let Some(executable) = entry.executable.clone() {
            path = Some(executable);
        } else if let Some(first) = entry
            .program_arguments
            .first()
            .filter(|argument| argument.starts_with('/'))
        {
            path = Some(first.clone());
        }
    }
    if path.is_none() {
        path = container_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string());
    }

    let plist_path = url_path
        .clone()
        .filter(|path| path.ends_with(".plist"))
        .filter(|path| std::path::Path::new(path).exists());

    if is_protected(entry, path.as_deref(), plist_path.as_deref()) {
        return None;
    }

    let container_exists = container_path
        .as_ref()
        .map(|path| path.exists())
        .unwrap_or(false);
    let bundle_installed = entry
        .bundle_id
        .as_deref()
        .map(|id| resolver.installed(id))
        .unwrap_or(false)
        || container_id
            .as_deref()
            .map(|id| resolver.installed(id))
            .unwrap_or(false);
    let target_exists = path
        .as_deref()
        .map(|path| std::path::Path::new(path).exists())
        .unwrap_or(false);
    let plist_exists = plist_path.is_some();

    // 有 plist 的项目由 launchd 扫描判断是否失效；没有 plist 时再看应用/可执行文件。
    let invalid = if plist_exists || container_exists || bundle_installed || target_exists {
        false
    } else if path.is_some() {
        true
    } else {
        relative_url.is_some()
    };

    let kind = entry.kind();
    let removable = kind == "login" || (invalid && plist_exists);
    let can_toggle = plist_exists;
    let requires_admin = plist_path
        .as_deref()
        .map(|path| path.starts_with("/Library/"))
        .unwrap_or(false);
    let note = if !removable && !can_toggle {
        Some("需要在「系统设置 → 通用 → 登录项与扩展」中关闭或移除".to_string())
    } else {
        None
    };

    let name = display_name(entry);
    Some(StartupLeftover {
        id: stable_id(
            kind,
            &format!("{}|{}", entry.identifier, path.as_deref().unwrap_or("")),
        ),
        name,
        kind: kind.to_string(),
        item_type: entry.item_type().to_string(),
        developer: entry.developer.clone(),
        bundle_id: entry.bundle_id.clone(),
        path,
        requires_admin,
        plist_path,
        removable,
        invalid,
        can_toggle,
        // type 2 的最低 bit 表示“登录时打开”是否开启；后台项目没有可靠的公开状态。
        enabled: if kind == "login" {
            Some(entry.disposition & 1 != 0)
        } else {
            None
        },
        note,
    })
}

#[cfg(target_os = "macos")]
fn display_name(entry: &BtmEntry) -> String {
    if !entry.name.is_empty() && entry.name != "true" {
        return entry.name.clone();
    }
    if let Some(path) = &entry.executable {
        if let Some(name) = app_name_from_path(path) {
            return name;
        }
        if let Some(name) = std::path::Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        {
            return name;
        }
    }
    entry
        .bundle_id
        .clone()
        .unwrap_or_else(|| entry.identifier.clone())
}

#[cfg(target_os = "macos")]
fn is_protected(entry: &BtmEntry, path: Option<&str>, plist: Option<&str>) -> bool {
    let identifiers = [entry.bundle_id.as_deref(), Some(entry.identifier.as_str())];
    if identifiers
        .iter()
        .flatten()
        .any(|id| id.contains("com.apple.") || *id == "com.apple")
    {
        return true;
    }
    if entry.developer.as_deref() == Some("Apple Inc.") {
        return true;
    }
    [path, plist].iter().flatten().any(|value| {
        PROTECTED_PREFIXES
            .iter()
            .any(|prefix| value.starts_with(prefix))
    })
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct BundleResolver {
    cache: std::collections::HashMap<String, Option<std::path::PathBuf>>,
    scanned: Option<std::collections::HashMap<String, std::path::PathBuf>>,
}

#[cfg(target_os = "macos")]
impl BundleResolver {
    fn resolve(&mut self, bundle_id: &str) -> Option<std::path::PathBuf> {
        if let Some(cached) = self.cache.get(bundle_id) {
            return cached.clone();
        }
        let mut found = super::privacy::bundle_path(bundle_id);
        if found.is_none() {
            let scanned = self
                .scanned
                .get_or_insert_with(super::privacy::collect_bundles);
            found = scanned.get(bundle_id).cloned();
        }
        self.cache.insert(bundle_id.to_string(), found.clone());
        found
    }

    fn installed(&mut self, bundle_id: &str) -> bool {
        self.resolve(bundle_id).is_some()
    }
}

/* --------------------------- launchd plist 扫描 -------------------------- */

/// 扫描所有第三方 launchd plist，包括仍然有效的项目。
#[cfg(target_os = "macos")]
fn scan_launchd() -> (Vec<StartupLeftover>, Vec<String>, bool) {
    let home = home();
    let dirs = [
        (home.join("Library/LaunchAgents"), false),
        (home.join("Library/LaunchDaemons"), false),
        (std::path::PathBuf::from("/Library/LaunchAgents"), true),
        (std::path::PathBuf::from("/Library/LaunchDaemons"), true),
    ];

    let uid = run_command("/usr/bin/id", &["-u"]).unwrap_or_else(|_| "0".into());
    let disabled_user = disabled_labels(&format!("gui/{}", uid.trim()));
    let disabled_system = disabled_labels("system");

    let mut items = Vec::new();
    let mut errors = Vec::new();
    let mut readable = false;
    for (dir, admin) in dirs {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => {
                readable = true;
                entries
            }
            Err(err) => {
                if dir.starts_with("/Library") && err.kind() != std::io::ErrorKind::NotFound {
                    errors.push(format!("无法读取 {}：{err}", dir.display()));
                }
                continue;
            }
        };
        let disabled = if dir.ends_with("LaunchDaemons") {
            &disabled_system
        } else {
            &disabled_user
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("plist") {
                continue;
            }
            let Some(item) = evaluate_launchd_plist(&path, admin, disabled) else {
                continue;
            };
            items.push(item);
        }
    }
    (items, errors, readable)
}

/// 解析 `launchctl print-disabled`，返回被明确禁用的 label。
#[cfg(target_os = "macos")]
fn disabled_labels(domain: &str) -> std::collections::HashSet<String> {
    let mut disabled = std::collections::HashSet::new();
    let Ok(output) = run_command("/bin/launchctl", &["print-disabled", domain]) else {
        return disabled;
    };
    for line in output.lines() {
        let Some((label, state)) = line.split_once("=>") else {
            continue;
        };
        let label = label.trim().trim_matches('"').to_string();
        if !label.is_empty() && state.trim() == "disabled" {
            disabled.insert(label);
        }
    }
    disabled
}

#[cfg(target_os = "macos")]
fn evaluate_launchd_plist(
    path: &std::path::Path,
    admin: bool,
    disabled: &std::collections::HashSet<String>,
) -> Option<StartupLeftover> {
    let value = plist::Value::from_file(path).ok()?;
    let dict = value.as_dictionary()?;
    let label = dict
        .get("Label")
        .and_then(|value| value.as_string())
        .unwrap_or("")
        .to_string();
    if label.is_empty() || label.contains("com.apple.") {
        return None;
    }

    let program = launchd_program(dict)?;
    let program = if program.starts_with('/') {
        std::path::PathBuf::from(program)
    } else {
        // 应用包里的 plist 使用相对路径，例如 Contents/MacOS/foo。
        match app_ancestor(path) {
            Some(app) => app.join(program),
            None => path.parent()?.join(program),
        }
    };
    let program_text = program.to_string_lossy().to_string();
    if PROTECTED_PREFIXES
        .iter()
        .any(|prefix| program_text.starts_with(prefix))
    {
        return None;
    }

    let app_path = app_ancestor(path);
    // 应用还在、只是这个 plist 指向的 helper 换了位置时视为有效。
    // 只有内容完整的 bundle 才算「应用还在」，避免残留的空壳目录掩盖真实残留。
    let app_exists = app_path
        .as_ref()
        .map(|app| app.join("Contents").join("Info.plist").exists())
        .unwrap_or(false);
    let invalid = !program.exists() && !app_exists;

    let name = app_name_from_path(&program_text)
        .or_else(|| {
            std::path::Path::new(&program_text)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| label.clone());
    let item_type = if path.to_string_lossy().contains("/LaunchDaemons/") {
        "daemon"
    } else {
        "agent"
    };
    let bundle_id = app_path.as_ref().and_then(|app| {
        super::privacy::read_info_plist_id(&app.join("Contents").join("Info.plist"))
    });

    Some(StartupLeftover {
        id: stable_id("plist", &path.to_string_lossy()),
        name,
        kind: "background".into(),
        item_type: item_type.into(),
        developer: None,
        bundle_id,
        path: Some(program_text),
        plist_path: Some(path.to_string_lossy().to_string()),
        requires_admin: admin,
        removable: true,
        invalid,
        can_toggle: true,
        enabled: Some(!disabled.contains(&label)),
        note: if invalid {
            Some("指向的可执行文件已不存在".into())
        } else {
            None
        },
    })
}

#[cfg(target_os = "macos")]
fn launchd_program(dict: &plist::Dictionary) -> Option<String> {
    if let Some(program) = dict.get("Program").and_then(|value| value.as_string()) {
        // `sh` 这种裸命令会由 launchd 从 PATH 里找，不能拼成 plist 同目录的路径。
        if program.contains('/') {
            return Some(program.to_string());
        }
    }
    if let Some(arguments) = dict
        .get("ProgramArguments")
        .and_then(|value| value.as_array())
    {
        // `ProgramArguments = ["sh", "/path/script.sh"]` 这类写法要跳过命令名。
        if let Some(first) = arguments
            .iter()
            .filter_map(|value| value.as_string())
            .find(|value| value.starts_with('/'))
        {
            return Some(first.to_string());
        }
    }
    // BundleProgram 相对于所在应用包。
    if let Some(program) = dict
        .get("BundleProgram")
        .and_then(|value| value.as_string())
    {
        if !program.is_empty() {
            return Some(program.to_string());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn app_ancestor(path: &std::path::Path) -> Option<std::path::PathBuf> {
    path.ancestors().find_map(|ancestor| {
        let name = ancestor.file_name()?.to_string_lossy();
        if name.ends_with(".app") {
            Some(ancestor.to_path_buf())
        } else {
            None
        }
    })
}

#[cfg(target_os = "macos")]
fn app_name_from_path(path: &str) -> Option<String> {
    let path = std::path::Path::new(path);
    for ancestor in path.ancestors() {
        let name = ancestor.file_name()?.to_string_lossy();
        if let Some(name) = name.strip_suffix(".app") {
            return Some(name.to_string());
        }
    }
    None
}

/* -------------------------------- 路径 --------------------------------- */

#[cfg(target_os = "macos")]
fn container_bundle_id(container: &str) -> Option<String> {
    let (prefix, rest) = container.split_once('.')?;
    if !prefix.is_empty() && prefix.chars().all(|ch| ch.is_ascii_digit()) {
        Some(rest.to_string())
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn file_url_path(value: &str) -> Option<String> {
    let rest = value
        .strip_prefix("file://")?
        .trim_start_matches("localhost");
    if !rest.starts_with('/') {
        return None;
    }
    Some(percent_decode(rest))
}

#[cfg(target_os = "macos")]
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                out.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(target_os = "macos")]
fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn stable_id(prefix: &str, key: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{prefix}-{hash:016x}")
}

/* -------------------------------- 清理 --------------------------------- */

#[cfg(target_os = "macos")]
fn clean_macos(targets: &[StartupTarget]) -> StartupCleanResult {
    let mut result = StartupCleanResult::default();
    if targets.is_empty() {
        return result;
    }
    let scan = scan_macos();
    if !scan.available {
        result
            .failed
            .push("无法读取登录项和后台项目，请稍后重试".into());
        return result;
    }

    let wanted: std::collections::HashSet<&str> =
        targets.iter().map(|target| target.id.as_str()).collect();
    let selected: Vec<&StartupLeftover> = scan
        .items
        .iter()
        .filter(|item| wanted.contains(item.id.as_str()))
        .collect();
    if selected.is_empty() {
        result
            .messages
            .push("没有需要清理的项目，可能已经被清理过了".into());
        return result;
    }

    let mut user_plists: Vec<(String, String)> = Vec::new();
    let mut admin_plists: Vec<(String, String)> = Vec::new();
    let mut login_items: Vec<&StartupLeftover> = Vec::new();
    let mut skipped = 0usize;
    for item in &selected {
        // 登录项可能同时带 launchd plist，两边都要处理。
        if item.kind == "login" {
            login_items.push(item);
        }
        match item.plist_path.as_deref() {
            Some(path) => {
                let label = launchd_label(std::path::Path::new(path)).unwrap_or_default();
                if item.requires_admin {
                    admin_plists.push((path.to_string(), label));
                } else {
                    user_plists.push((path.to_string(), label));
                }
            }
            None if item.kind == "login" => {}
            None => skipped += 1,
        }
    }

    // 系统级 plist 需要管理员权限，先处理，取消授权时不会留下半完成状态。
    if !admin_plists.is_empty() {
        match remove_admin_plists(&admin_plists) {
            Ok(()) => result
                .messages
                .push(format!("已删除系统级启动文件 {} 个", admin_plists.len())),
            Err(err) => {
                let cancelled = err.contains("(-128)") || err.to_lowercase().contains("cancel");
                result.cancelled = cancelled;
                result.failed.push(if cancelled {
                    "已取消管理员授权，未做任何修改".into()
                } else {
                    format!("系统级启动文件清理失败：{err}")
                });
                if cancelled {
                    result
                        .messages
                        .push("已取消管理员授权，用户级项目也未改动".into());
                }
                return result;
            }
        }
        result.removed_items += admin_plists.len();
    }

    for (path, label) in &user_plists {
        match std::fs::remove_file(path) {
            Ok(()) => {
                bootout_user(label);
                result.removed_items += 1;
            }
            Err(err) => result.failed.push(format!("无法删除 {}：{err}", path)),
        }
    }
    if !user_plists.is_empty() {
        result
            .messages
            .push(format!("已删除用户级启动文件 {} 个", user_plists.len()));
    }

    for item in login_items {
        match remove_login_item(item) {
            Ok(()) => {
                if item.plist_path.is_none() {
                    result.removed_items += 1;
                }
                result
                    .messages
                    .push(format!("已删除登录项「{}」", item.name));
            }
            Err(err) => {
                if item.plist_path.is_none() {
                    result.failed.push(format!(
                        "无法删除登录项「{}」：{err}。如系统未授权，请在「系统设置 → 隐私与安全性 → 自动化」中允许 Mole Desktop 控制 System Events",
                        item.name
                    ));
                } else {
                    result.messages.push(format!(
                        "「{}」的启动文件已处理；如果系统设置仍显示登录项，请在「登录项与扩展」中移除",
                        item.name
                    ));
                }
            }
        }
    }

    if skipped > 0 {
        result.messages.push(format!(
            "{skipped} 项没有独立启动文件，需要在「系统设置 → 通用 → 登录项与扩展」中移除"
        ));
    }

    if result.removed_items > 0 {
        // 让系统设置里的列表重新从磁盘读取。
        let _ = std::process::Command::new("/usr/bin/killall")
            .arg("BackgroundTaskManagementAgent")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        result
            .messages
            .push("如系统设置仍显示旧条目，请关闭并重新打开「系统设置」查看".into());
    }
    result
}

#[cfg(target_os = "macos")]
fn launchd_label(path: &std::path::Path) -> Option<String> {
    let value = plist::Value::from_file(path).ok()?;
    value
        .as_dictionary()?
        .get("Label")?
        .as_string()
        .map(str::to_owned)
}

/// 通过 launchctl 启用或禁用一条 launchd 项目。
#[cfg(target_os = "macos")]
fn toggle_macos(id: &str, enabled: bool) -> Result<String, String> {
    let scan = scan_macos();
    let item = scan
        .items
        .iter()
        .find(|item| item.id == id)
        .ok_or("没有找到这个项目，请重新扫描后再试")?;
    let plist = item.plist_path.as_deref().ok_or(
        "这个项目没有独立的启动文件，请到「系统设置 → 通用 → 登录项与扩展」中管理",
    )?;
    let label = launchd_label(std::path::Path::new(plist))
        .filter(|label| !label.is_empty())
        .ok_or("启动文件缺少 Label，无法切换状态")?;

    let uid = run_command("/usr/bin/id", &["-u"]).unwrap_or_else(|_| "0".into());
    let is_daemon = plist.contains("/LaunchDaemons/");
    let domain = if is_daemon {
        "system".to_string()
    } else {
        format!("gui/{}", uid.trim())
    };
    let service = format!("{domain}/{label}");

    if item.requires_admin {
        let script = if enabled {
            format!(
                "/bin/launchctl enable {service} 2>/dev/null || true\n\
                 /bin/launchctl bootstrap {domain} {plist} 2>/dev/null || true\n",
                service = sh_quote(&service),
                domain = sh_quote(&domain),
                plist = sh_quote(plist),
            )
        } else {
            format!(
                "/bin/launchctl bootout {service} 2>/dev/null || true\n\
                 /bin/launchctl disable {service}\n",
                service = sh_quote(&service),
            )
        };
        run_admin_script(&script)?;
    } else if enabled {
        let _ = run_command("/bin/launchctl", &["enable", &service]);
        if let Err(err) = run_command("/bin/launchctl", &["bootstrap", &domain, plist]) {
            // bootstrap 对已经加载的任务会报错；确认它确实仍处于禁用状态再失败。
            if disabled_labels(&domain).contains(&label) {
                return Err(format!("重新加载启动项失败：{err}"));
            }
        }
    } else {
        let _ = run_command("/bin/launchctl", &["bootout", &service]);
        run_command("/bin/launchctl", &["disable", &service])
            .map_err(|err| format!("禁用启动项失败：{err}"))?;
    }

    Ok(if enabled {
        format!("已启用「{}」", item.name)
    } else {
        format!("已禁用「{}」", item.name)
    })
}

#[cfg(target_os = "macos")]
fn remove_admin_plists(plists: &[(String, String)]) -> Result<(), String> {
    // 提权后的脚本里 `id -u` 会返回 0，必须提前取当前用户 uid。
    let uid = run_command("/usr/bin/id", &["-u"]).unwrap_or_else(|_| "0".into());
    let uid = uid.trim();
    let mut script = String::new();
    for (path, label) in plists {
        let is_daemon = path.contains("/LaunchDaemons/");
        let domain = if is_daemon {
            "system".to_string()
        } else {
            format!("gui/{uid}")
        };
        if !label.is_empty() {
            script.push_str(&format!(
                "/bin/launchctl bootout {}/{} 2>/dev/null || true\n",
                domain,
                sh_quote(label)
            ));
        }
        script.push_str(&format!("/bin/rm -f {}\n", sh_quote(path)));
    }
    run_admin_script(&script).map(|_| ())
}

#[cfg(target_os = "macos")]
fn bootout_user(label: &str) {
    if label.is_empty() {
        return;
    }
    let command = format!(
        "/bin/launchctl bootout gui/$(id -u)/{} 2>/dev/null || true",
        sh_quote(label)
    );
    let _ = run_command("/bin/sh", &["-c", &command]);
}

/// 通过 System Events 删除「登录时打开」项目。
/// 有路径时只按路径匹配，避免同名但仍在使用的登录项被误删。
#[cfg(target_os = "macos")]
fn remove_login_item(item: &StartupLeftover) -> Result<(), String> {
    let target_path = item
        .path
        .as_deref()
        .map(escape_applescript)
        .unwrap_or_default();
    let target_name = escape_applescript(&item.name);
    let script = format!(
        "tell application \"System Events\"\n\
         set targetPath to \"{target_path}\"\n\
         set targetName to \"{target_name}\"\n\
         repeat with li in (get every login item)\n\
         try\n\
         if targetPath is not \"\" then\n\
         if (path of li as text) is targetPath then\n\
         delete li\n\
         return \"removed\"\n\
         end if\n\
         else if (name of li as text) is targetName then\n\
         delete li\n\
         return \"removed\"\n\
         end if\n\
         end try\n\
         end repeat\n\
         return \"not-found\"\n\
         end tell"
    );
    let output = run_command("/usr/bin/osascript", &["-e", &script])?;
    if output.contains("removed") {
        Ok(())
    } else {
        Err("System Events 中没有找到这条登录项".into())
    }
}

#[cfg(target_os = "macos")]
fn escape_applescript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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

/* -------------------------------- 测试辅助 ------------------------------ */

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn decodes_file_urls() {
        assert_eq!(
            file_url_path("file:///Applications/iStat%20Menus.app/").as_deref(),
            Some("/Applications/iStat Menus.app/")
        );
        assert_eq!(
            file_url_path("file:///Library/LaunchDaemons/foo.plist").as_deref(),
            Some("/Library/LaunchDaemons/foo.plist")
        );
        assert_eq!(file_url_path("Contents/Library/LoginItems/foo.app"), None);
    }

    #[test]
    fn reads_container_bundle_identifier() {
        assert_eq!(
            container_bundle_id("2.app.pastenow.PasteNow").as_deref(),
            Some("app.pastenow.PasteNow")
        );
        assert_eq!(container_bundle_id("Tencent Lemon"), None);
    }

    /// itemsByUserIdentifier 是归档过的 NSDictionary，键和值分开放。
    #[test]
    fn decodes_archived_dictionaries() {
        use plist::{Dictionary, Uid, Value};

        let mut dictionary = Dictionary::new();
        dictionary.insert(
            "NS.keys".into(),
            Value::Array(vec![Value::Uid(Uid::new(2))]),
        );
        dictionary.insert(
            "NS.objects".into(),
            Value::Array(vec![Value::Uid(Uid::new(3))]),
        );
        let archive = BtmArchive {
            objects: vec![
                Value::String("$null".into()),
                Value::Dictionary(dictionary),
                Value::String("501".into()),
                Value::Array(vec![
                    Value::String("first".into()),
                    Value::String("second".into()),
                ]),
            ],
        };
        let values = archive.dictionary_values(&archive.objects[1]);
        assert_eq!(values.len(), 1);
        let items = archive.array(values[0]).expect("array");
        assert_eq!(items.len(), 2);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod orphan_tests {
    use super::*;

    fn write_plist(path: &std::path::Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn detects_orphan_plists() {
        let root = std::env::temp_dir().join("mole-startup-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let absolute = root.join("absolute.plist");
        write_plist(
            &absolute,
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
            <key>Label</key><string>com.example.orphan</string>
            <key>Program</key><string>/Applications/__MoleMissing__.app/Contents/MacOS/foo</string>
            </dict></plist>"#,
        );
        let found = evaluate_launchd_plist(&absolute, false, &std::collections::HashSet::new())
            .expect("absolute orphan");
        assert!(found.removable);
        assert!(found.invalid);

        let relative = root.join("__MoleMissing__.app/Contents/Library/LaunchDaemons/bar.plist");
        write_plist(
            &relative,
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
            <key>Label</key><string>com.example.helper</string>
            <key>Program</key><string>Contents/MacOS/helper</string>
            </dict></plist>"#,
        );
        let found = evaluate_launchd_plist(&relative, true, &std::collections::HashSet::new())
            .expect("relative orphan");
        assert!(found.requires_admin);
        assert!(found.invalid);

        let script = root.join("exists.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        let shell = root.join("shell.plist");
        write_plist(
            &shell,
            &format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
                <key>Label</key><string>com.example.shell</string>
                <key>ProgramArguments</key><array><string>sh</string><string>{}</string></array>
                </dict></plist>"#,
                script.display()
            ),
        );
        let found = evaluate_launchd_plist(&shell, false, &std::collections::HashSet::new())
            .expect("existing script");
        assert!(!found.invalid, "existing script is still valid");

        let _ = std::fs::remove_dir_all(&root);
    }
}

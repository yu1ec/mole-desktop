use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rayon::prelude::*;

use crate::models::{CleanCrumb, CleanDetail, CleanEntry, CleanItem, OperationResult};
use crate::util::{
    clear_dir_contents, deny_reason, dir_children, expand_pattern, expand_tilde, home,
    pattern_root, remove_path, unique_paths,
};

pub struct Rule {
    pub id: &'static str,
    pub category: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub patterns: Vec<&'static str>,
    pub risk: &'static str,
    pub requires_admin: bool,
    pub keep_root: bool,
}

#[cfg(target_os = "macos")]
fn platform_rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "user-cache",
            category: "应用缓存",
            label: "应用缓存",
            description: "~/Library/Caches 中可由应用重新生成的缓存数据",
            patterns: vec!["~/Library/Caches"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "user-logs",
            category: "应用缓存",
            label: "应用日志",
            description: "~/Library/Logs 中的运行日志与诊断记录",
            patterns: vec!["~/Library/Logs"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "saved-state",
            category: "应用缓存",
            label: "窗口恢复状态",
            description: "应用保存的窗口与恢复状态，可安全重建",
            patterns: vec!["~/Library/Saved Application State"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "trash",
            category: "用户文件",
            label: "废纸篓",
            description: "清空当前用户的废纸篓",
            patterns: vec!["~/.Trash"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "mail-downloads",
            category: "用户文件",
            label: "邮件附件缓存",
            description: "Mail 已下载的附件副本",
            patterns: vec!["~/Library/Containers/com.apple.mail/Data/Library/Mail Downloads"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "xcode-derived",
            category: "开发工具",
            label: "Xcode DerivedData",
            description: "Xcode 构建中间产物，可重新生成",
            patterns: vec!["~/Library/Developer/Xcode/DerivedData"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "simulator-cache",
            category: "开发工具",
            label: "模拟器缓存",
            description: "iOS 模拟器的运行时缓存",
            patterns: vec!["~/Library/Developer/CoreSimulator/Caches"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "npm-cache",
            category: "开发工具",
            label: "npm 缓存",
            description: "npm 下载与构建缓存",
            patterns: vec!["~/.npm/_cacache"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "pnpm-store",
            category: "开发工具",
            label: "pnpm 存储",
            description: "pnpm 全局内容寻址存储，可用时重新下载",
            patterns: vec!["~/Library/pnpm/store"],
            risk: "caution",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "cargo-cache",
            category: "开发工具",
            label: "Cargo 下载缓存",
            description: "Rust 依赖的压缩包缓存，不影响已编译项目",
            patterns: vec!["~/.cargo/registry/cache"],
            risk: "caution",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "go-mod-cache",
            category: "开发工具",
            label: "Go 模块缓存",
            description: "Go module 下载缓存，下次构建会重新拉取",
            patterns: vec!["~/go/pkg/mod/cache/download"],
            risk: "caution",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "gradle-cache",
            category: "开发工具",
            label: "Gradle 缓存",
            description: "Gradle 依赖与构建缓存",
            patterns: vec!["~/.gradle/caches"],
            risk: "caution",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "xcode-device-support",
            category: "开发工具",
            label: "Xcode 设备支持",
            description: "旧 iOS 设备的调试符号，重新连接设备会再次生成",
            patterns: vec!["~/Library/Developer/Xcode/iOS DeviceSupport"],
            risk: "caution",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "xcode-archives",
            category: "开发工具",
            label: "Xcode 归档",
            description: "历史构建归档，可能包含可提交的发布版本",
            patterns: vec!["~/Library/Developer/Xcode/Archives"],
            risk: "caution",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "ios-backup",
            category: "用户文件",
            label: "iPhone / iPad 备份",
            description: "本机保存的移动设备备份，删除后需重新备份",
            patterns: vec!["~/Library/Application Support/MobileSync/Backup"],
            risk: "caution",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "system-cache",
            category: "系统",
            label: "系统缓存（需管理员）",
            description: "/Library/Caches 下的系统级缓存",
            patterns: vec!["/Library/Caches"],
            risk: "caution",
            requires_admin: true,
            keep_root: true,
        },
        Rule {
            id: "system-logs",
            category: "系统",
            label: "系统日志（需管理员）",
            description: "/Library/Logs 下的系统级日志",
            patterns: vec!["/Library/Logs"],
            risk: "caution",
            requires_admin: true,
            keep_root: true,
        },
    ]
}

#[cfg(windows)]
fn platform_rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "user-temp",
            category: "用户文件",
            label: "用户临时文件",
            description: "%TEMP% 中的临时文件",
            patterns: vec!["%TEMP%"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "crash-dumps",
            category: "用户文件",
            label: "崩溃转储",
            description: "%LOCALAPPDATA%\\CrashDumps 中的转储文件",
            patterns: vec!["%LOCALAPPDATA%\\CrashDumps"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "chrome-cache",
            category: "浏览器",
            label: "Chrome 缓存",
            description: "所有 Chrome 配置文件的页面缓存",
            patterns: vec![
                "%LOCALAPPDATA%\\Google\\Chrome\\User Data\\*\\Cache",
                "%LOCALAPPDATA%\\Google\\Chrome\\User Data\\*\\Code Cache",
                "%LOCALAPPDATA%\\Google\\Chrome\\User Data\\*\\GPUCache",
            ],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "edge-cache",
            category: "浏览器",
            label: "Edge 缓存",
            description: "所有 Edge 配置文件的页面缓存",
            patterns: vec![
                "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data\\*\\Cache",
                "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data\\*\\Code Cache",
                "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data\\*\\GPUCache",
            ],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "firefox-cache",
            category: "浏览器",
            label: "Firefox 缓存",
            description: "所有 Firefox 配置文件的网络缓存",
            patterns: vec!["%LOCALAPPDATA%\\Mozilla\\Firefox\\Profiles\\*\\cache2"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "shader-cache",
            category: "应用缓存",
            label: "DirectX 着色器缓存",
            description: "游戏与图形程序可重新生成的着色器缓存",
            patterns: vec!["%LOCALAPPDATA%\\D3DSCache"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "thumb-cache",
            category: "应用缓存",
            label: "缩略图缓存",
            description: "资源管理器缩略图数据库，删除后自动重建",
            patterns: vec!["%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer\\thumbcache_*.db"],
            risk: "caution",
            requires_admin: false,
            keep_root: false,
        },
        Rule {
            id: "npm-cache",
            category: "开发工具",
            label: "npm 缓存",
            description: "npm 下载与构建缓存",
            patterns: vec!["%LOCALAPPDATA%\\npm-cache"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "pnpm-store",
            category: "开发工具",
            label: "pnpm 存储",
            description: "pnpm 全局内容寻址存储",
            patterns: vec!["%LOCALAPPDATA%\\pnpm\\store"],
            risk: "caution",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "yarn-cache",
            category: "开发工具",
            label: "Yarn 缓存",
            description: "Yarn 下载缓存",
            patterns: vec!["%LOCALAPPDATA%\\Yarn\\Cache"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "pip-cache",
            category: "开发工具",
            label: "pip 缓存",
            description: "Python 包下载缓存",
            patterns: vec!["%LOCALAPPDATA%\\pip\\Cache"],
            risk: "safe",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "cargo-cache",
            category: "开发工具",
            label: "Cargo 下载缓存",
            description: "Rust 依赖的压缩包缓存",
            patterns: vec!["%USERPROFILE%\\.cargo\\registry\\cache"],
            risk: "caution",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "gradle-cache",
            category: "开发工具",
            label: "Gradle 缓存",
            description: "Gradle 依赖与构建缓存",
            patterns: vec!["%USERPROFILE%\\.gradle\\caches"],
            risk: "caution",
            requires_admin: false,
            keep_root: true,
        },
        Rule {
            id: "windows-temp",
            category: "系统",
            label: "Windows 临时文件（需管理员）",
            description: "C:\\Windows\\Temp 中的系统临时文件",
            patterns: vec!["C:\\Windows\\Temp"],
            risk: "caution",
            requires_admin: true,
            keep_root: true,
        },
        Rule {
            id: "delivery-optimization",
            category: "系统",
            label: "更新分发缓存（需管理员）",
            description: "Windows 更新分发优化缓存",
            patterns: vec!["C:\\Windows\\SoftwareDistribution\\Download"],
            risk: "caution",
            requires_admin: true,
            keep_root: true,
        },
    ]
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_rules() -> Vec<Rule> {
    Vec::new()
}

fn expand_vars(input: &str) -> String {
    let mut text = input.to_string();
    let vars: Vec<(String, String)> = std::env::vars().collect();
    for (key, value) in vars {
        let upper = key.to_uppercase();
        text = text.replace(&format!("%{upper}%"), &value);
        text = text.replace(&format!("${{{upper}}}"), &value);
        text = text.replace(&format!("${upper}"), &value);
    }
    text
}

fn resolve(patterns: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for raw in patterns {
        let expanded = expand_vars(raw);
        let path = expand_tilde(&expanded);
        out.extend(expand_pattern(&path));
    }
    unique_paths(out)
}

pub(crate) fn display_path(path: &Path) -> String {
    let home = home();
    let text = path.to_string_lossy().to_string();
    let home_text = home.to_string_lossy().to_string();
    if text == home_text {
        "~".to_string()
    } else if let Some(rest) = text.strip_prefix(&format!("{home_text}/")) {
        format!("~/{rest}")
    } else if let Some(rest) = text.strip_prefix(&format!("{home_text}\\")) {
        format!("~\\{rest}")
    } else {
        text
    }
}

/// Direct children of every rule root, built in the same traversal that
/// produces the rule totals.
fn build_overview(rule: &Rule, roots: &[PathBuf]) -> CleanDetail {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut total_size = 0u64;
    let mut file_count = 0u64;
    let mut errors = 0usize;
    for root in roots {
        // Some Windows patterns resolve to files directly (e.g. thumbcache_*.db).
        if let Ok(meta) = std::fs::symlink_metadata(root) {
            if meta.is_file() {
                total_size = total_size.saturating_add(meta.len());
                file_count += 1;
                entries.push(detail_entry(root, false, meta.len(), 1));
                continue;
            }
        }
        let (size, files, children, child_errors) = dir_children(root);
        total_size = total_size.saturating_add(size);
        file_count = file_count.saturating_add(files);
        errors += child_errors;
        for (child, is_dir, child_size, child_files) in children {
            if seen.insert(child.to_string_lossy().to_string()) {
                entries.push(detail_entry(&child, is_dir, child_size, child_files));
            }
        }
    }
    entries.sort_by(|a, b| b.size.cmp(&a.size));
    let truncated = entries.len() > MAX_DETAIL_ENTRIES;
    entries.truncate(MAX_DETAIL_ENTRIES);
    CleanDetail {
        item_id: rule.id.to_string(),
        label: rule.label.to_string(),
        current: None,
        current_display: "全部位置".into(),
        roots: roots
            .iter()
            .map(|root| root.to_string_lossy().to_string())
            .collect(),
        roots_display: roots.iter().map(|root| display_path(root)).collect(),
        crumbs: vec![CleanCrumb {
            label: "全部位置".into(),
            path: None,
        }],
        total_size,
        file_count,
        entries,
        truncated,
        errors,
    }
}

pub fn scan() -> Vec<CleanItem> {
    let scanned: Vec<(CleanItem, CleanDetail)> = platform_rules()
        .into_par_iter()
        .map(|rule| {
            let paths = resolve(&rule.patterns);
            let overview = build_overview(&rule, &paths);
            let display: Vec<String> = paths.iter().take(8).map(|p| display_path(p)).collect();
            (
                CleanItem {
                    id: rule.id.to_string(),
                    category: rule.category.to_string(),
                    label: rule.label.to_string(),
                    description: rule.description.to_string(),
                    size: overview.total_size,
                    file_count: overview.file_count,
                    paths: display,
                    risk: rule.risk.to_string(),
                    requires_admin: rule.requires_admin,
                    available: !paths.is_empty(),
                },
                overview,
            )
        })
        .collect();

    if let Ok(mut cache) = overview_cache().lock() {
        cache.clear();
        for (_, overview) in &scanned {
            if !overview.roots.is_empty() {
                cache.insert(overview.item_id.clone(), overview.clone());
            }
        }
    }

    scanned.into_iter().map(|(item, _)| item).collect()
}

/// Upper bound for the entries sent to the UI. A directory can contain tens of
/// thousands of files; the largest ones matter and the rest would only bloat
/// the IPC payload and the render.
const MAX_DETAIL_ENTRIES: usize = 500;

/// Top-level breakdown captured during the free scan. Opening "查看明细" then
/// reuses this traversal instead of walking a multi-GB cache a second time.
static OVERVIEW_CACHE: OnceLock<Mutex<HashMap<String, CleanDetail>>> = OnceLock::new();

fn overview_cache() -> &'static Mutex<HashMap<String, CleanDetail>> {
    OVERVIEW_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Lists one directory level, sorted by size, capped at [`MAX_DETAIL_ENTRIES`].
/// Shared by rule details and project artifact details.
pub(crate) fn dir_detail_entries(
    target: &Path,
) -> (u64, u64, Vec<CleanEntry>, bool, usize) {
    let (total_size, file_count, children, errors) = dir_children(target);
    let mut entries: Vec<CleanEntry> = children
        .into_iter()
        .map(|(child, is_dir, size, files)| detail_entry(&child, is_dir, size, files))
        .collect();
    entries.sort_by(|a, b| b.size.cmp(&a.size));
    let truncated = entries.len() > MAX_DETAIL_ENTRIES;
    entries.truncate(MAX_DETAIL_ENTRIES);
    (total_size, file_count, entries, truncated, errors)
}

fn detail_entry(path: &Path, is_dir: bool, size: u64, file_count: u64) -> CleanEntry {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    CleanEntry {
        name,
        path: path.to_string_lossy().to_string(),
        is_dir,
        size,
        file_count,
    }
}

/// One level of a cleanup rule's tree.
///
/// `path = None` lists the direct children of every rule root; `path = Some`
/// drills into a directory that must stay inside one of those roots. Every call
/// walks each directory exactly once and never follows symlinks, so the totals
/// match what [`clean`] will remove.
pub fn detail(id: &str, path: Option<&str>) -> Result<CleanDetail, String> {
    let rule = platform_rules()
        .into_iter()
        .find(|rule| rule.id == id)
        .ok_or_else(|| format!("未知的清理项：{id}"))?;
    let roots = resolve(&rule.patterns);
    if roots.is_empty() {
        return Err("当前系统中没有找到该项目对应的目录".into());
    }
    let root_paths: Vec<String> = roots
        .iter()
        .map(|root| root.to_string_lossy().to_string())
        .collect();

    let Some(target) = path else {
        if let Ok(cache) = overview_cache().lock() {
            if let Some(cached) = cache.get(id) {
                return Ok(cached.clone());
            }
        }
        return Ok(build_overview(&rule, &roots));
    };

    let target = expand_tilde(target.trim());
    if !roots.iter().any(|root| target.starts_with(root)) {
        return Err("路径不在该清理项范围内".into());
    }
    if !target.exists() {
        return Err(format!("目录不存在：{}", target.display()));
    }
    if !target.is_dir() {
        return Err("请选择一个文件夹".into());
    }

    // The deepest matching root lets nested rule patterns keep short crumbs.
    let root = roots
        .iter()
        .filter(|root| target.starts_with(root))
        .max_by_key(|root| root.components().count())
        .expect("target is inside at least one root")
        .clone();

    let (total_size, file_count, entries, truncated, errors) = dir_detail_entries(&target);

    let mut crumbs = vec![
        CleanCrumb {
            label: "全部位置".into(),
            path: None,
        },
        CleanCrumb {
            label: display_path(&root),
            path: Some(root.to_string_lossy().to_string()),
        },
    ];
    if target != root {
        if let Ok(relative) = target.strip_prefix(&root) {
            let mut accumulated = root.clone();
            for part in relative.components() {
                accumulated.push(part.as_os_str());
                crumbs.push(CleanCrumb {
                    label: part.as_os_str().to_string_lossy().to_string(),
                    path: Some(accumulated.to_string_lossy().to_string()),
                });
            }
        }
    }

    Ok(CleanDetail {
        item_id: rule.id.to_string(),
        label: rule.label.to_string(),
        current_display: display_path(&target),
        current: Some(target.to_string_lossy().to_string()),
        roots: root_paths,
        roots_display: roots.iter().map(|root| display_path(root)).collect(),
        crumbs,
        total_size,
        file_count,
        entries,
        truncated,
        errors,
    })
}

pub fn clean(ids: &[String]) -> OperationResult {
    let mut result = OperationResult::default();
    let rules = platform_rules();
    for rule in rules.iter().filter(|r| ids.iter().any(|id| id == r.id)) {
        let paths = resolve(&rule.patterns);
        if paths.is_empty() {
            continue;
        }
        let mut rule_size = 0u64;
        let mut rule_files = 0u64;
        let mut rule_errors = Vec::new();
        for path in paths {
            let root = pattern_root(&expand_tilde(&expand_vars(rule.patterns[0])));
            let allowed = if root.as_os_str().is_empty() {
                path.parent().map(Path::to_path_buf).unwrap_or_default()
            } else {
                root
            };
            if let Some(reason) = deny_reason(&path, &allowed) {
                rule_errors.push(format!("{}：{}", path.display(), reason));
                continue;
            }
            let (freed, files, errors) = if rule.keep_root && path.is_dir() {
                clear_dir_contents(&path)
            } else {
                remove_path(&path)
            };
            rule_size = rule_size.saturating_add(freed);
            rule_files += files;
            rule_errors.extend(errors);
        }
        if rule_size > 0 || rule_files > 0 {
            result.removed.push(rule.label.to_string());
        }
        result.freed = result.freed.saturating_add(rule_size);
        result.files += rule_files;
        result.errors.extend(rule_errors);
    }
    if let Ok(mut cache) = overview_cache().lock() {
        cache.clear();
    }
    result
}

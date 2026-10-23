use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::models::{CleanItem, OperationResult};
use crate::util::{
    clear_dir_contents, deny_reason, dir_stats, expand_pattern, expand_tilde, home, pattern_root,
    remove_path, unique_paths,
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

fn display_path(path: &Path) -> String {
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

pub fn scan() -> Vec<CleanItem> {
    platform_rules()
        .into_par_iter()
        .map(|rule| {
            let paths = resolve(&rule.patterns);
            let mut size = 0u64;
            let mut file_count = 0u64;
            for path in &paths {
                let (s, c) = dir_stats(path);
                size = size.saturating_add(s);
                file_count += c;
            }
            let display: Vec<String> = paths.iter().take(8).map(|p| display_path(p)).collect();
            CleanItem {
                id: rule.id.to_string(),
                category: rule.category.to_string(),
                label: rule.label.to_string(),
                description: rule.description.to_string(),
                size,
                file_count,
                paths: display,
                risk: rule.risk.to_string(),
                requires_admin: rule.requires_admin,
                available: !paths.is_empty(),
            }
        })
        .collect()
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
    result
}

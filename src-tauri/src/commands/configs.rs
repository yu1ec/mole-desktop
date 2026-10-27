use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tauri::State;
use walkdir::WalkDir;

use crate::models::{
    CleanCrumb, CleanDetail, CleanEntry, ConfigPathInfo, ConfigTool, HistoryEntry, OperationResult,
};
use crate::rules::display_path;
use crate::state::AppState;
use crate::store::push_history;
use crate::util::{expand_tilde, home, move_to_trash, now_rfc3339};

struct ConfigPathSpec {
    path: PathBuf,
    kind: &'static str,
}

struct Profile {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    category: &'static str,
    commands: &'static [&'static str],
    paths: Vec<ConfigPathSpec>,
}

fn mac_app_support() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|home| home.join("Library/Application Support"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn path_of(path: PathBuf, kind: &'static str) -> ConfigPathSpec {
    ConfigPathSpec { path, kind }
}

/// Known developer / AI tool config locations. Only entries that actually
/// exist are returned, so an uninstalled tool that left data behind shows up.
fn profiles() -> Vec<Profile> {
    let home = home();
    let config = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
    let data = dirs::data_dir().unwrap_or_else(|| home.join(".local/share"));
    let cache = dirs::cache_dir().unwrap_or_else(|| home.join(".cache"));
    let support = mac_app_support();
    let mut list = vec![
        Profile {
            id: "codex",
            name: "Codex",
            description: "OpenAI Codex CLI 的登录信息、配置、会话与本地缓存",
            category: "AI 编程",
            commands: &["codex"],
            paths: vec![
                path_of(home.join(".codex"), "配置与会话"),
                path_of(config.join("codex"), "配置"),
                path_of(home.join(".config/codex"), "配置"),
            ],
        },
        Profile {
            id: "claude-code",
            name: "Claude Code",
            description: "Claude Code CLI 的配置、会话记录、插件与缓存",
            category: "AI 编程",
            commands: &["claude"],
            paths: vec![
                path_of(home.join(".claude"), "配置与会话"),
                path_of(home.join(".claude.json"), "配置文件"),
                path_of(config.join("claude"), "配置"),
            ],
        },
        Profile {
            id: "opencode",
            name: "OpenCode",
            description: "OpenCode 的配置、会话数据库、插件与日志",
            category: "AI 编程",
            commands: &["opencode"],
            paths: vec![
                path_of(config.join("opencode"), "配置与插件"),
                path_of(data.join("opencode"), "数据与会话"),
                path_of(cache.join("opencode"), "缓存"),
            ],
        },
        Profile {
            id: "pi",
            name: "Pi",
            description: "Pi coding agent 的配置、模型、扩展与会话",
            category: "AI 编程",
            commands: &["pi"],
            paths: vec![
                path_of(home.join(".pi"), "配置与会话"),
                path_of(config.join("pi"), "配置"),
                path_of(data.join("pi"), "数据"),
            ],
        },
        Profile {
            id: "gemini",
            name: "Gemini CLI",
            description: "Google Gemini CLI 的账号、配置与历史记录",
            category: "AI 编程",
            commands: &["gemini"],
            paths: vec![
                path_of(home.join(".gemini"), "配置与会话"),
                path_of(config.join("gemini"), "配置"),
            ],
        },
        Profile {
            id: "qwen",
            name: "Qwen Code",
            description: "Qwen Code CLI 的配置与本地数据",
            category: "AI 编程",
            commands: &["qwen"],
            paths: vec![
                path_of(home.join(".qwen"), "配置与会话"),
                path_of(config.join("qwen"), "配置"),
            ],
        },
        Profile {
            id: "copilot",
            name: "GitHub Copilot CLI",
            description: "Copilot CLI 的配置、登录信息与缓存",
            category: "AI 编程",
            commands: &["copilot", "github-copilot"],
            paths: vec![
                path_of(home.join(".copilot"), "配置与会话"),
                path_of(config.join("github-copilot"), "配置"),
            ],
        },
        Profile {
            id: "amp",
            name: "Amp",
            description: "Amp CLI 的配置与会话数据",
            category: "AI 编程",
            commands: &["amp"],
            paths: vec![
                path_of(home.join(".amp"), "配置与会话"),
                path_of(config.join("amp"), "配置"),
            ],
        },
        Profile {
            id: "aider",
            name: "Aider",
            description: "Aider 的配置文件、聊天记录与输入历史",
            category: "AI 编程",
            commands: &["aider"],
            paths: vec![
                path_of(home.join(".aider"), "配置与会话"),
                path_of(home.join(".aider.conf.yml"), "配置文件"),
                path_of(home.join(".aider.chat.history.md"), "聊天记录"),
                path_of(home.join(".aider.input.history"), "输入历史"),
            ],
        },
        Profile {
            id: "continue",
            name: "Continue",
            description: "Continue 插件的配置、索引与聊天记录",
            category: "AI 编程",
            commands: &["continue"],
            paths: vec![
                path_of(home.join(".continue"), "配置与会话"),
                path_of(config.join("continue"), "配置"),
            ],
        },
        Profile {
            id: "cursor",
            name: "Cursor",
            description: "Cursor 编辑器的 CLI 数据、扩展与会话",
            category: "编辑器",
            commands: &["cursor"],
            paths: vec![
                path_of(home.join(".cursor"), "配置与会话"),
                path_of(config.join("Cursor"), "配置"),
            ],
        },
        Profile {
            id: "zed",
            name: "Zed",
            description: "Zed 编辑器的配置、扩展与本地状态",
            category: "编辑器",
            commands: &["zed"],
            paths: vec![
                path_of(config.join("zed"), "配置"),
                path_of(home.join(".zed"), "配置与会话"),
            ],
        },
        Profile {
            id: "windsurf",
            name: "Windsurf",
            description: "Windsurf 编辑器的配置、扩展与缓存",
            category: "编辑器",
            commands: &["windsurf"],
            paths: vec![
                path_of(home.join(".codeium/windsurf"), "配置与会话"),
                path_of(config.join("Windsurf"), "配置"),
            ],
        },
    ];

    if let Some(support) = &support {
        for profile in list.iter_mut() {
            match profile.id {
                "codex" => profile
                    .paths
                    .push(path_of(support.join("Codex"), "桌面应用数据")),
                "cursor" => profile
                    .paths
                    .push(path_of(support.join("Cursor"), "应用数据")),
                "windsurf" => profile
                    .paths
                    .push(path_of(support.join("Windsurf"), "应用数据")),
                "zed" => profile.paths.push(path_of(support.join("Zed"), "应用数据")),
                _ => {}
            }
        }
    }

    // 同一台机器上 config_dir 可能正好等于 ~/.config，去重避免重复统计。
    for profile in &mut list {
        let mut seen = HashSet::new();
        profile
            .paths
            .retain(|spec| seen.insert(spec.path.to_string_lossy().to_string()));
    }

    list
}

fn executable(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn command_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    let home = home();
    for extra in [
        home.join(".local/bin"),
        home.join(".bun/bin"),
        home.join(".npm-global/bin"),
        home.join(".cargo/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ] {
        dirs.push(extra);
    }
    let mut seen = HashSet::new();
    dirs.retain(|dir| seen.insert(dir.to_string_lossy().to_string()));
    dirs
}

fn command_available(names: &[&str]) -> bool {
    let dirs = command_dirs();
    let extensions: &[&str] = if cfg!(windows) {
        &["exe", "cmd", "bat", ""]
    } else {
        &[""]
    };
    for name in names {
        for dir in &dirs {
            for extension in extensions {
                let candidate = if extension.is_empty() {
                    dir.join(name)
                } else {
                    dir.join(format!("{name}.{extension}"))
                };
                if executable(&candidate) {
                    return true;
                }
            }
        }
    }
    false
}

/// One traversal per path that returns size, file count and the newest mtime.
fn path_stats(path: &Path) -> (u64, u64, i64) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return (0, 0, 0);
    };
    let root_modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    if !meta.is_dir() {
        return (meta.len(), 1, root_modified);
    }

    let mut size = 0u64;
    let mut files = 0u64;
    let mut modified = root_modified;
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.depth() == 0 {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            size = size.saturating_add(meta.len());
            files += 1;
            if let Ok(time) = meta.modified() {
                if let Ok(duration) = time.duration_since(std::time::UNIX_EPOCH) {
                    modified = modified.max(duration.as_secs() as i64);
                }
            }
        }
    }
    (size, files, modified)
}

/// Scans all known tools in parallel; only tools with at least one existing
/// path are returned, sorted by disk usage.
pub fn scan() -> Vec<ConfigTool> {
    let mut tools: Vec<ConfigTool> = profiles()
        .into_par_iter()
        .filter_map(|profile| {
            let mut found: Vec<(ConfigPathInfo, i64)> = profile
                .paths
                .into_iter()
                .filter(|spec| spec.path.exists())
                .map(|spec| {
                    let (size, file_count, modified) = path_stats(&spec.path);
                    (
                        ConfigPathInfo {
                            display: display_path(&spec.path),
                            path: spec.path.to_string_lossy().to_string(),
                            kind: spec.kind.to_string(),
                            is_dir: spec.path.is_dir(),
                            size,
                            file_count,
                            modified,
                        },
                        modified,
                    )
                })
                .collect();
            if found.is_empty() {
                return None;
            }
            found.sort_by(|a, b| b.0.size.cmp(&a.0.size));
            let modified = found.iter().map(|(_, time)| *time).max().unwrap_or(0);
            let paths: Vec<ConfigPathInfo> = found.into_iter().map(|(item, _)| item).collect();
            Some(ConfigTool {
                id: profile.id.to_string(),
                name: profile.name.to_string(),
                description: profile.description.to_string(),
                category: profile.category.to_string(),
                command_available: command_available(profile.commands),
                total_size: paths.iter().map(|item| item.size).sum(),
                total_files: paths.iter().map(|item| item.file_count).sum(),
                modified,
                paths,
            })
        })
        .collect();
    tools.sort_by(|a, b| b.total_size.cmp(&a.total_size));
    tools
}

fn find_profile(id: &str) -> Option<Profile> {
    profiles().into_iter().find(|profile| profile.id == id)
}

/// Directory browsing for the detail modal. `path = None` lists the tool's
/// config roots; a directory below a root can then be drilled into.
pub fn detail(tool_id: &str, path: Option<&str>) -> Result<CleanDetail, String> {
    let profile = find_profile(tool_id).ok_or_else(|| format!("未知的配置项：{tool_id}"))?;
    let roots: Vec<PathBuf> = profile
        .paths
        .into_iter()
        .map(|spec| spec.path)
        .filter(|path| path.exists())
        .collect();
    if roots.is_empty() {
        return Err("该工具没有检测到配置文件".into());
    }
    let root_paths: Vec<String> = roots
        .iter()
        .map(|root| root.to_string_lossy().to_string())
        .collect();

    let Some(target) = path else {
        let mut entries: Vec<CleanEntry> = roots
            .iter()
            .map(|root| {
                let (size, file_count, _) = path_stats(root);
                CleanEntry {
                    name: display_path(root),
                    path: root.to_string_lossy().to_string(),
                    is_dir: root.is_dir(),
                    size,
                    file_count,
                }
            })
            .collect();
        entries.sort_by(|a, b| b.size.cmp(&a.size));
        let total_size = entries.iter().map(|entry| entry.size).sum();
        let file_count = entries.iter().map(|entry| entry.file_count).sum();
        return Ok(CleanDetail {
            item_id: profile.id.to_string(),
            label: profile.name.to_string(),
            current: None,
            current_display: "全部位置".into(),
            roots: root_paths,
            roots_display: roots.iter().map(|root| display_path(root)).collect(),
            crumbs: vec![CleanCrumb {
                label: "全部位置".into(),
                path: None,
            }],
            total_size,
            file_count,
            entries,
            truncated: false,
            errors: 0,
        });
    };

    let target = expand_tilde(target.trim());
    if !roots.iter().any(|root| target.starts_with(root)) {
        return Err("路径不在该工具的配置范围内".into());
    }
    if !target.exists() {
        return Err(format!("路径不存在：{}", target.display()));
    }

    let root = roots
        .iter()
        .filter(|root| target.starts_with(root))
        .max_by_key(|root| root.components().count())
        .cloned()
        .expect("target is inside at least one root");

    // 单文件配置（如 ~/.claude.json）直接作为一条明细返回，避免点击根路径出错。
    if !target.is_dir() {
        let (size, file_count, _) = path_stats(&target);
        let name = target
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| display_path(&target));
        return Ok(CleanDetail {
            item_id: profile.id.to_string(),
            label: profile.name.to_string(),
            current_display: display_path(&target),
            current: Some(target.to_string_lossy().to_string()),
            roots: root_paths,
            roots_display: roots.iter().map(|root| display_path(root)).collect(),
            crumbs: vec![
                CleanCrumb {
                    label: "全部位置".into(),
                    path: None,
                },
                CleanCrumb {
                    label: display_path(&root),
                    path: Some(root.to_string_lossy().to_string()),
                },
            ],
            total_size: size,
            file_count,
            entries: vec![CleanEntry {
                name,
                path: target.to_string_lossy().to_string(),
                is_dir: false,
                size,
                file_count,
            }],
            truncated: false,
            errors: 0,
        });
    }

    let (total_size, file_count, entries, truncated, errors) =
        crate::rules::dir_detail_entries(&target);

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
        item_id: profile.id.to_string(),
        label: profile.name.to_string(),
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

#[tauri::command(async)]
pub fn scan_configs() -> Vec<ConfigTool> {
    scan()
}

#[tauri::command(async)]
pub fn config_detail(tool_id: String, path: Option<String>) -> Result<CleanDetail, String> {
    detail(&tool_id, path.as_deref())
}

/// Only paths from the known catalog can be removed. Everything goes to the
/// system trash so a mistaken click stays recoverable.
#[tauri::command(async)]
pub fn delete_config_paths(
    paths: Vec<String>,
    state: State<AppState>,
) -> Result<OperationResult, String> {
    if paths.is_empty() {
        return Err("没有选择要清理的配置".into());
    }
    let allowed: HashSet<String> = profiles()
        .into_iter()
        .flat_map(|profile| profile.paths)
        .map(|spec| spec.path.to_string_lossy().to_string())
        .collect();

    let mut result = OperationResult::default();
    for raw in paths {
        let path = expand_tilde(&raw);
        let key = path.to_string_lossy().to_string();
        if !allowed.contains(&key) {
            result
                .errors
                .push(format!("跳过不在配置目录清单中的路径：{raw}"));
            continue;
        }
        if !path.exists() {
            result.errors.push(format!("路径不存在：{raw}"));
            continue;
        }
        let (freed, files, errors) = move_to_trash(&[path]);
        result.freed = result.freed.saturating_add(freed);
        result.files += files;
        result.errors.extend(errors);
        if files > 0 {
            result.moved_to_trash = true;
            result.removed.push(raw);
        }
    }

    if result.files > 0 {
        push_history(
            &state.data_dir,
            HistoryEntry {
                id: String::new(),
                timestamp: now_rfc3339(),
                kind: "config".into(),
                title: format!("配置清理 · {} 项", result.removed.len()),
                freed: result.freed,
                files: result.files,
                detail: result.removed.join("、"),
            },
        );
    }
    Ok(result)
}

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tauri::State;

use crate::models::{
    CleanCrumb, CleanDetail, EntryTrashResult, HistoryEntry, OperationResult, ProjectArtifact,
};
use crate::rules::{dir_detail_entries, display_path};
use crate::state::AppState;
use crate::store::{load_settings, push_history};
use crate::util::{
    dir_stats, expand_tilde, home, mtime, now_rfc3339, now_ts, remove_path, trash_entry,
    unique_paths,
};

const ARTIFACT_NAMES: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".output",
    ".turbo",
    ".parcel-cache",
    ".svelte-kit",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "coverage",
    "vendor",
];

fn default_roots() -> Vec<PathBuf> {
    let home = home();
    [
        "Projects",
        "GitHub",
        "dev",
        "code",
        "src",
        "workspace",
        "Repos",
        "repos",
        "Work",
        "Documents/GitHub",
        "Documents/Projects",
        "Documents/code",
    ]
    .iter()
    .map(|part| home.join(part))
    .filter(|path| path.is_dir())
    .collect()
}

fn active_roots(data_dir: &Path) -> Vec<PathBuf> {
    let settings = load_settings(data_dir);
    if settings.purge_roots.is_empty() {
        default_roots()
    } else {
        settings
            .purge_roots
            .iter()
            .map(|raw| expand_tilde(raw))
            .filter(|path| path.is_dir())
            .collect()
    }
}

fn is_artifact(name: &str) -> bool {
    ARTIFACT_NAMES.contains(&name)
}

fn skip_dir(name: &str) -> bool {
    is_artifact(name)
        || matches!(
            name,
            ".git" | ".svn" | ".hg" | ".idea" | ".vscode" | "Library" | "AppData"
        )
}

#[tauri::command]
pub fn purge_roots(state: State<AppState>) -> Vec<String> {
    active_roots(&state.data_dir)
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}

fn walk_projects(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if is_artifact(&name) {
            out.push(entry.path());
        } else if !skip_dir(&name) {
            walk_projects(&entry.path(), depth + 1, out);
        }
    }
}

#[tauri::command(async)]
pub fn scan_projects(state: State<AppState>) -> Vec<ProjectArtifact> {
    let roots = active_roots(&state.data_dir);
    let mut paths = Vec::new();
    for root in &roots {
        walk_projects(root, 0, &mut paths);
    }
    let paths = unique_paths(paths);
    let now = now_ts();
    let mut artifacts: Vec<ProjectArtifact> = paths
        .par_iter()
        .map(|path| {
            let (size, _) = dir_stats(path);
            let modified = mtime(path);
            let kind = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let parent = path.parent().unwrap_or(path);
            let home = home();
            let project = parent
                .strip_prefix(&home)
                .map(|p| format!("~/{}", p.to_string_lossy()))
                .unwrap_or_else(|_| parent.to_string_lossy().to_string());
            ProjectArtifact {
                path: path.to_string_lossy().to_string(),
                project,
                kind,
                size,
                modified,
                recent: now.saturating_sub(modified) < 7 * 24 * 3600,
            }
        })
        .collect();
    artifacts.sort_by(|a, b| b.size.cmp(&a.size));
    artifacts
}

#[tauri::command(async)]
pub fn project_artifact_detail(
    path: String,
    sub: Option<String>,
) -> Result<CleanDetail, String> {
    let root = PathBuf::from(&path);
    let Some(kind) = root.file_name().map(|name| name.to_string_lossy().to_string()) else {
        return Err("无效的构建产物路径".into());
    };
    if !is_artifact(&kind) {
        return Err("不是可清理的构建产物".into());
    }
    if !root.is_dir() {
        return Err(format!("目录不存在：{}", root.display()));
    }

    let target = sub.map(PathBuf::from).unwrap_or_else(|| root.clone());
    if !target.starts_with(&root) {
        return Err("路径不在该构建产物范围内".into());
    }
    if !target.is_dir() {
        return Err("请选择一个文件夹".into());
    }

    let (total_size, file_count, entries, truncated, errors) = dir_detail_entries(&target);
    let mut crumbs = vec![CleanCrumb {
        label: display_path(&root),
        path: Some(root.to_string_lossy().to_string()),
    }];
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
        item_id: path,
        label: kind,
        current: Some(target.to_string_lossy().to_string()),
        current_display: display_path(&target),
        roots: vec![root.to_string_lossy().to_string()],
        roots_display: vec![display_path(&root)],
        crumbs,
        total_size,
        file_count,
        entries,
        truncated,
        errors,
    })
}

#[tauri::command(async)]
pub fn delete_project_entry(
    root: String,
    path: String,
    state: State<AppState>,
) -> Result<EntryTrashResult, String> {
    let root = PathBuf::from(root);
    let Some(kind) = root.file_name().map(|name| name.to_string_lossy().to_string()) else {
        return Err("无效的构建产物路径".into());
    };
    if !is_artifact(&kind) {
        return Err("不是可清理的构建产物".into());
    }
    if !root.is_dir() {
        return Err(format!("目录不存在：{}", root.display()));
    }
    if !active_roots(&state.data_dir)
        .iter()
        .any(|allowed| root.starts_with(allowed))
    {
        return Err("该构建产物不在扫描目录中".into());
    }

    let target = PathBuf::from(path.trim());
    if !target.starts_with(&root) {
        return Err("路径不在该构建产物范围内".into());
    }
    if !target.exists() {
        return Err(format!("路径不存在：{}", target.display()));
    }
    let restorable = trash_entry(&target)?;
    Ok(EntryTrashResult { path, restorable })
}

#[tauri::command(async)]
pub fn purge_projects(
    paths: Vec<String>,
    state: State<AppState>,
) -> Result<OperationResult, String> {
    if paths.is_empty() {
        return Err("没有选择要清理的构建产物".into());
    }
    let roots = active_roots(&state.data_dir);
    let mut result = OperationResult::default();
    for raw in paths {
        let path = PathBuf::from(&raw);
        let Some(name) = path.file_name().map(|s| s.to_string_lossy().to_string()) else {
            continue;
        };
        if !is_artifact(&name) {
            result.errors.push(format!("跳过非构建产物：{raw}"));
            continue;
        }
        if !roots.iter().any(|root| path.starts_with(root)) {
            result
                .errors
                .push(format!("跳过不在扫描目录中的路径：{raw}"));
            continue;
        }
        if path.is_symlink() {
            result.errors.push(format!("跳过符号链接：{raw}"));
            continue;
        }
        let (freed, files, errors) = remove_path(&path);
        result.freed = result.freed.saturating_add(freed);
        result.files += files;
        result.errors.extend(errors);
        if files > 0 {
            result.removed.push(raw);
        }
    }
    if result.files > 0 {
        push_history(
            &state.data_dir,
            HistoryEntry {
                id: String::new(),
                timestamp: now_rfc3339(),
                kind: "purge".into(),
                title: format!("项目清理 · {} 个产物", result.removed.len()),
                freed: result.freed,
                files: result.files,
                detail: result.removed.join("、"),
            },
        );
    }
    Ok(result)
}

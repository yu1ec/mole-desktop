use std::path::{Path, PathBuf};

use tauri::State;

use crate::models::{
    CleanDetail, CleanScan, EntryTrashResult, HistoryEntry, InstallerFile, OperationResult,
};
use crate::rules;
use crate::state::AppState;
use crate::store::push_history;
use crate::util::{
    deny_reason, expand_tilde, free_space, home, now_rfc3339, os_open, remove_path, trash_entry,
};

#[tauri::command(async)]
pub fn scan_clean() -> CleanScan {
    let items = rules::scan();
    let total_size = items.iter().map(|item| item.size).sum();
    let file_count = items.iter().map(|item| item.file_count).sum();
    CleanScan {
        items,
        total_size,
        file_count,
        free_space: free_space(&home()),
        scanned_at: now_rfc3339(),
        platform: std::env::consts::OS.to_string(),
    }
}

#[tauri::command(async)]
pub fn clean_item_detail(item_id: String, path: Option<String>) -> Result<CleanDetail, String> {
    rules::detail(&item_id, path.as_deref())
}

/// Removes one entry shown in the detail browser. The path must stay inside
/// the rule's roots, and it goes to the trash so the UI can offer an undo.
#[tauri::command(async)]
pub fn delete_clean_entry(item_id: String, path: String) -> Result<EntryTrashResult, String> {
    let roots = rules::guard_roots(&item_id)?;
    let target = expand_tilde(path.trim());
    let root = roots
        .iter()
        .filter(|root| target.starts_with(root))
        .max_by_key(|root| root.components().count())
        .ok_or_else(|| "路径不在该清理项范围内".to_string())?;
    if let Some(reason) = deny_reason(&target, root) {
        return Err(reason);
    }
    if !target.exists() {
        return Err(format!("路径不存在：{}", target.display()));
    }
    let restorable = trash_entry(&target)?;
    rules::clear_overview_cache();
    Ok(EntryTrashResult { path, restorable })
}

#[tauri::command(async)]
pub fn run_clean(
    items: Vec<String>,
    state: State<AppState>,
) -> Result<OperationResult, String> {
    if items.is_empty() {
        return Err("没有选择要清理的项目".into());
    }
    let result = rules::clean(&items);
    let title = format!("深度清理 · {} 项", items.len());
    let detail = result.removed.join("、");
    push_history(
        &state.data_dir,
        HistoryEntry {
            id: String::new(),
            timestamp: now_rfc3339(),
            kind: "clean".into(),
            title,
            freed: result.freed,
            files: result.files,
            detail,
        },
    );
    Ok(result)
}

#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    os_open(&expand_tilde(&path))
}

#[tauri::command]
pub fn home_dir() -> String {
    home().to_string_lossy().to_string()
}

/* ----------------------------- installers ----------------------------- */

fn installer_roots() -> Vec<(PathBuf, String)> {
    let home = home();
    let mut roots = vec![
        (home.join("Downloads"), "下载".to_string()),
        (home.join("Desktop"), "桌面".to_string()),
    ];
    #[cfg(target_os = "macos")]
    {
        roots.push((
            home.join("Library/Caches/Homebrew/downloads"),
            "Homebrew".to_string(),
        ));
        roots.push((
            home.join("Library/Mobile Documents/com~apple~CloudDocs/Downloads"),
            "iCloud".to_string(),
        ));
    }
    #[cfg(windows)]
    {
        roots.push((std::env::temp_dir(), "临时目录".to_string()));
    }
    roots
}

fn installer_extensions() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    {
        &["dmg", "pkg", "mpkg", "iso", "xip"]
    }
    #[cfg(windows)]
    {
        &["msi", "msix", "msixbundle", "appx", "appxbundle", "iso", "img"]
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        &[]
    }
}

fn is_installer(path: &Path) -> bool {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    installer_extensions().contains(&ext.as_str())
}

#[tauri::command(async)]
pub fn list_installers() -> Vec<InstallerFile> {
    let mut files = Vec::new();
    for (root, source) in installer_roots() {
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root)
            .max_depth(2)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() || !is_installer(entry.path()) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            files.push(InstallerFile {
                path: entry.path().to_string_lossy().to_string(),
                name: entry.file_name().to_string_lossy().to_string(),
                size: meta.len(),
                modified: crate::util::mtime(entry.path()),
                source: source.clone(),
            });
        }
    }
    files.sort_by(|a, b| b.size.cmp(&a.size));
    files.dedup_by(|a, b| a.path == b.path);
    files
}

#[tauri::command(async)]
pub fn delete_installers(
    paths: Vec<String>,
    state: State<AppState>,
) -> Result<OperationResult, String> {
    let roots: Vec<PathBuf> = installer_roots().into_iter().map(|(root, _)| root).collect();
    let mut result = OperationResult::default();
    for raw in paths {
        let path = PathBuf::from(&raw);
        if !path.is_file() || !is_installer(&path) {
            result.errors.push(format!("跳过非安装包：{raw}"));
            continue;
        }
        let allowed = roots.iter().any(|root| path.starts_with(root));
        if !allowed {
            result.errors.push(format!("跳过不在扫描目录中的文件：{raw}"));
            continue;
        }
        if let Some(reason) = deny_reason(&path, path.parent().unwrap_or(Path::new("/"))) {
            result.errors.push(format!("{raw}：{reason}"));
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
                kind: "installer".into(),
                title: format!("清理安装包 · {} 个", result.removed.len()),
                freed: result.freed,
                files: result.files,
                detail: result.removed.join("、"),
            },
        );
    }
    Ok(result)
}


use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::models::{AnalyzeEntry, AnalyzeFile, AnalyzeReport};
use crate::util::{allocated_size, dir_stats, dir_usage, expand_tilde, free_space};

#[tauri::command(async)]
pub fn analyze_dir(path: String) -> Result<AnalyzeReport, String> {
    let root = expand_tilde(&path);
    if !root.exists() {
        return Err(format!("路径不存在：{}", root.display()));
    }
    if !root.is_dir() {
        return Err("请选择一个文件夹".into());
    }

    let mut children: Vec<PathBuf> = Vec::new();
    let errors = AtomicUsize::new(0);
    match std::fs::read_dir(&root) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        match entry.file_type() {
                            Ok(file_type) if !file_type.is_symlink() => children.push(path),
                            _ => {
                                errors.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
        Err(err) => return Err(format!("无法读取 {}：{err}", root.display())),
    }

    let mut results: Vec<(AnalyzeEntry, Vec<AnalyzeEntry>)> = children
        .par_iter()
        .map(|path| {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let is_dir = path.is_dir();
            let mut nested = Vec::new();
            let (size, file_count) = if is_dir {
                // One walk gives both the directory total and every nested one.
                let (size, file_count, dirs) = dir_usage(path);
                nested = dirs
                    .into_iter()
                    .filter(|(dir, _, _)| dir != &root)
                    .filter(|(_, size, _)| *size > 0)
                    .map(|(dir, size, files)| AnalyzeEntry {
                        name: dir
                            .strip_prefix(&root)
                            .unwrap_or(&dir)
                            .to_string_lossy()
                            .to_string(),
                        path: dir.to_string_lossy().to_string(),
                        size,
                        is_dir: true,
                        file_count: files,
                    })
                    .collect();
                (size, file_count)
            } else {
                dir_stats(path)
            };
            (
                AnalyzeEntry {
                    name,
                    path: path.to_string_lossy().to_string(),
                    size,
                    is_dir,
                    file_count,
                },
                nested,
            )
        })
        .collect();

    let mut entries: Vec<AnalyzeEntry> = results.iter().map(|(entry, _)| entry.clone()).collect();
    entries.retain(|entry| entry.size > 0);
    entries.sort_by(|a, b| b.size.cmp(&a.size));

    let mut subdirs: Vec<AnalyzeEntry> = results.drain(..).flat_map(|(_, nested)| nested).collect();
    subdirs.sort_by(|a, b| b.size.cmp(&a.size));
    subdirs.truncate(200);

    let total_size: u64 = entries.iter().map(|entry| entry.size).sum();
    let file_count: u64 = entries.iter().map(|entry| entry.file_count).sum();

    let mut large_files: Vec<AnalyzeFile> = children
        .iter()
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let meta = std::fs::metadata(path).ok()?;
            Some(AnalyzeFile {
                name: path.file_name()?.to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                size: allocated_size(&meta),
            })
        })
        .collect();
    large_files.sort_by(|a, b| b.size.cmp(&a.size));
    large_files.truncate(20);

    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| root.to_string_lossy().to_string());
    let parent = root.parent().map(|p| p.to_string_lossy().to_string());

    Ok(AnalyzeReport {
        path: root.to_string_lossy().to_string(),
        name,
        parent,
        total_size,
        file_count,
        free_space: free_space(&root),
        entries,
        subdirs,
        large_files,
        errors: errors.load(Ordering::Relaxed),
    })
}

#[tauri::command]
pub fn analyze_shortcuts() -> Vec<(String, String)> {
    let home = crate::util::home();
    let mut out = vec![("主目录".to_string(), home.to_string_lossy().to_string())];
    for (label, path) in [
        ("下载", home.join("Downloads")),
        ("桌面", home.join("Desktop")),
        ("文稿", home.join("Documents")),
        ("应用程序", PathBuf::from("/Applications")),
        ("用户资源库", home.join("Library")),
    ] {
        if path.exists() {
            out.push((label.to_string(), path.to_string_lossy().to_string()));
        }
    }
    out
}

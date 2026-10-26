use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn expand_tilde(input: &str) -> PathBuf {
    let input = input.trim();
    if input == "~" {
        home()
    } else if let Some(rest) = input.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(input)
    }
}

pub fn expand_pattern(pattern: &Path) -> Vec<PathBuf> {
    let text = pattern.to_string_lossy();
    if text.contains('*') || text.contains('?') || text.contains('[') {
        match glob::glob(&text) {
            Ok(paths) => paths.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    } else if pattern.exists() {
        vec![pattern.to_path_buf()]
    } else {
        Vec::new()
    }
}

/// Literal directory prefix of a pattern (everything before the first wildcard).
pub fn pattern_root(pattern: &Path) -> PathBuf {
    let mut root = PathBuf::new();
    for component in pattern.components() {
        let text = component.as_os_str().to_string_lossy();
        if text.contains('*') || text.contains('?') || text.contains('[') {
            break;
        }
        root.push(component);
    }
    if root.as_os_str().is_empty() {
        pattern.parent().unwrap_or(Path::new("/")).to_path_buf()
    } else {
        root
    }
}

pub fn dir_stats(path: &Path) -> (u64, u64) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return (0, 0);
    };
    if !meta.is_dir() {
        return (meta.len(), 1);
    }
    let mut size = 0u64;
    let mut files = 0u64;
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.depth() == 0 {
            continue;
        }
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                size = size.saturating_add(meta.len());
                files += 1;
            }
        }
    }
    (size, files)
}

/// Single-pass walk that aggregates sizes for every directory below `path`.
///
/// Returns the totals for `path` itself plus `(path, size, files)` for each
/// directory it contains. Symlinks are never followed, matching [`dir_stats`].
pub fn dir_usage(path: &Path) -> (u64, u64, Vec<(PathBuf, u64, u64)>) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return (0, 0, Vec::new());
    };
    if !meta.is_dir() {
        return (meta.len(), 1, Vec::new());
    }

    // Children are visited before their parent, so a directory's size is
    // complete by the time its own entry is yielded.
    let mut pending: HashMap<PathBuf, (u64, u64)> = HashMap::new();
    let mut dirs = Vec::new();
    for entry in WalkDir::new(path)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.depth() == 0 {
            continue;
        }
        let entry_path = entry.path();
        let Some(parent) = entry_path.parent() else {
            continue;
        };
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                let slot = pending.entry(parent.to_path_buf()).or_insert((0, 0));
                slot.0 = slot.0.saturating_add(meta.len());
                slot.1 += 1;
            }
        } else if entry.file_type().is_dir() {
            let (size, files) = pending.remove(entry_path).unwrap_or((0, 0));
            dirs.push((entry_path.to_path_buf(), size, files));
            let slot = pending.entry(parent.to_path_buf()).or_insert((0, 0));
            slot.0 = slot.0.saturating_add(size);
            slot.1 += files;
        }
    }

    let (total, files) = pending.remove(path).unwrap_or((0, 0));
    (total, files, dirs)
}

/// Removes a file or directory tree without following symlinks.
pub fn remove_path(path: &Path) -> (u64, u64, Vec<String>) {
    let mut freed = 0;
    let mut files = 0;
    let mut errors = Vec::new();
    remove_inner(path, &mut freed, &mut files, &mut errors);
    (freed, files, errors)
}

fn remove_inner(path: &Path, freed: &mut u64, files: &mut u64, errors: &mut Vec<String>) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    if meta.file_type().is_symlink() {
        if fs::remove_file(path).is_ok() {
            *files += 1;
        } else {
            errors.push(format!("无法删除链接：{}", path.display()));
        }
        return;
    }
    if meta.is_dir() {
        match fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries.filter_map(Result::ok) {
                    remove_inner(&entry.path(), freed, files, errors);
                }
            }
            Err(err) => {
                errors.push(format!("无法读取 {}：{err}", path.display()));
                return;
            }
        }
        if let Err(err) = fs::remove_dir(path) {
            errors.push(format!("无法删除 {}：{err}", path.display()));
        }
    } else {
        *freed = freed.saturating_add(meta.len());
        if fs::remove_file(path).is_ok() {
            *files += 1;
        } else {
            errors.push(format!("无法删除 {}（可能正在使用）", path.display()));
        }
    }
}

/// Empties a directory while keeping the directory itself.
pub fn clear_dir_contents(path: &Path) -> (u64, u64, Vec<String>) {
    let mut freed = 0;
    let mut files = 0;
    let mut errors = Vec::new();
    let Ok(entries) = fs::read_dir(path) else {
        return (0, 0, vec![format!("无法读取 {}", path.display())]);
    };
    for entry in entries.filter_map(Result::ok) {
        remove_inner(&entry.path(), &mut freed, &mut files, &mut errors);
    }
    (freed, files, errors)
}

pub fn move_to_trash(paths: &[PathBuf]) -> (u64, u64, Vec<String>) {
    let mut freed = 0;
    let mut files = 0;
    let mut errors = Vec::new();
    for path in paths {
        let (size, count) = dir_stats(path);
        match trash::delete(path) {
            Ok(()) => {
                freed += size;
                files += count;
            }
            Err(err) => errors.push(format!("无法移入废纸篓 {}：{err}", path.display())),
        }
    }
    (freed, files, errors)
}

pub fn free_space(path: &Path) -> u64 {
    space_for(path).map(|(_, free)| free).unwrap_or(0)
}

fn space_for(path: &Path) -> Option<(u64, u64)> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut best: Option<(usize, u64, u64)> = None;
    for disk in disks.list() {
        let mount = disk.mount_point();
        if path.starts_with(mount) {
            let len = mount.as_os_str().len();
            if best.as_ref().map(|(l, _, _)| len > *l).unwrap_or(true) {
                best = Some((len, disk.total_space(), disk.available_space()));
            }
        }
    }
    best.map(|(_, total, free)| (total, free))
}

pub fn os_open(path: &Path) -> Result<(), String> {
    let result = if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()
    } else if cfg!(windows) {
        Command::new("explorer").arg(path).spawn()
    } else {
        Command::new("xdg-open").arg(path).spawn()
    };
    result.map(|_| ()).map_err(|err| err.to_string())
}

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339()
}

pub fn mtime(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn unique_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for path in paths {
        let key = path.to_string_lossy().to_string();
        if seen.insert(key) {
            out.push(path);
        }
    }
    out
}

/// Guard used before any destructive cleanup. Only paths produced by a known
/// rule root, or explicit user-selected app data under `$HOME`, pass.
pub fn deny_reason(path: &Path, allowed_root: &Path) -> Option<String> {
    if !path.starts_with(allowed_root) {
        return Some(format!("路径不在允许范围 {}", allowed_root.display()));
    }
    if path == allowed_root && allowed_root == home() {
        return Some("拒绝删除用户主目录".into());
    }
    let text = path.to_string_lossy();
    let forbidden = [
        "/System",
        "/usr",
        "/bin",
        "/sbin",
        "/etc",
        "/private/var/db",
        "/Library/Apple",
        "/Library/Extensions",
        "/Volumes",
        "C:\\Windows",
        "C:\\Program Files",
        "C:\\Program Files (x86)",
        "C:\\ProgramData\\Microsoft",
    ];
    for prefix in forbidden {
        if text == prefix || text.starts_with(&format!("{prefix}/")) || text.starts_with(&format!("{prefix}\\")) {
            return Some(format!("系统保护路径：{prefix}"));
        }
    }
    None
}

pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        let probe = std::env::temp_dir().join(format!("mole-admin-check-{}", std::process::id()));
        let elevated = fs::write(&probe, b"1").is_ok();
        let _ = fs::remove_file(&probe);
        if elevated {
            // A writable temp dir does not prove elevation; check a system path.
            return fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(Path::new("C:\\Windows\\Temp\\mole-admin-check.tmp"))
                .and_then(|_| fs::remove_file("C:\\Windows\\Temp\\mole-admin-check.tmp"))
                .is_ok();
        }
        false
    }
    #[cfg(not(windows))]
    {
        // Unix: root or a sudo ticket is not assumed. Tasks that need it report failures.
        libc_geteuid() == 0
    }
}

#[cfg(not(windows))]
fn libc_geteuid() -> u32 {
    // Avoid a libc dependency for a single call.
    let uid = Command::new("id").arg("-u").output();
    uid.ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(1)
}

pub fn run_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("{program}: {err}"))?;
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !err.is_empty() {
            text = format!("{text}\n{err}");
        }
        if text.is_empty() {
            text = format!("{program} 退出码 {}", output.status);
        }
    }
    if output.status.success() {
        Ok(text)
    } else {
        Err(text)
    }
}

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use rayon::prelude::*;
use walkdir::WalkDir;

/// Creates a command that never opens a console window on Windows.
///
/// The app itself is a GUI binary, so any console child (`powershell`, `cmd`,
/// `ipconfig`, `DISM`, ...) would otherwise get a fresh, visible console.
/// `CREATE_NO_WINDOW` suppresses it and is ignored by GUI children.
pub fn command<S: AsRef<std::ffi::OsStr>>(program: S) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

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

/// Bytes a file actually occupies on disk.
///
/// `Metadata::len` is the logical size. On APFS and other filesystems a file
/// can reserve far fewer blocks than its length (sparse disk images, clones,
/// transparent compression), which made totals exceed the physical capacity of
/// the volume. `du` and Finder report the allocated blocks, so match them.
pub fn allocated_size(meta: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.blocks().saturating_mul(512)
    }
    #[cfg(not(unix))]
    {
        // Windows exposes allocation through `GetCompressedFileSize`, which
        // `std` does not wrap; the logical length is the best portable value.
        meta.len()
    }
}

/// Whether a walk should step over a duplicate mount of the data volume.
///
/// macOS mounts the data volume at `/System/Volumes/Data` and also exposes the
/// same files at the root through firmlinks (`/Users`, `/Applications`, ...).
/// Walking `/` reaches both copies, so every user file was counted twice and
/// totals could exceed the physical capacity of the disk. Skip the mount point
/// unless the walk actually starts inside `/System/Volumes`.
fn is_duplicate_volume(path: &Path, root: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        const DATA_VOLUME: &str = "/System/Volumes/Data";
        const VOLUMES: &str = "/System/Volumes";
        path.starts_with(DATA_VOLUME) && !root.starts_with(VOLUMES)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, root);
        false
    }
}

pub fn dir_stats(path: &Path) -> (u64, u64) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return (0, 0);
    };
    if !meta.is_dir() {
        return (allocated_size(&meta), 1);
    }
    let mut size = 0u64;
    let mut files = 0u64;
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_duplicate_volume(entry.path(), path))
        .filter_map(Result::ok)
    {
        if entry.depth() == 0 {
            continue;
        }
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                size = size.saturating_add(allocated_size(&meta));
                files += 1;
            }
        }
    }
    (size, files)
}

/// Aggregates the direct children of `path` in one pass.
///
/// Returns `(total size, total files, children, unreadable entries)` where each
/// child is `(path, is_dir, size, files)`. Directories are measured with
/// [`dir_stats`], and the whole level is walked in parallel so browsing a large
/// cache folder costs the same single traversal as the initial scan while using
/// every core.
pub fn dir_children(path: &Path) -> (u64, u64, Vec<(PathBuf, bool, u64, u64)>, usize) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return (0, 0, Vec::new(), 1);
    };
    if !meta.is_dir() {
        return (allocated_size(&meta), 1, Vec::new(), 0);
    }
    let Ok(iter) = fs::read_dir(path) else {
        return (0, 0, Vec::new(), 1);
    };

    let mut errors = 0usize;
    let entries: Vec<fs::DirEntry> = iter
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(_) => {
                errors += 1;
                None
            }
        })
        .collect();
    let errors = AtomicUsize::new(errors);

    let mut children: Vec<(PathBuf, bool, u64, u64)> = entries
        .into_par_iter()
        .filter_map(|entry| {
            let child = entry.path();
            let Ok(file_type) = entry.file_type() else {
                errors.fetch_add(1, Ordering::Relaxed);
                return None;
            };
            if file_type.is_symlink() {
                return None;
            }
            if file_type.is_dir() {
                let (size, files) = dir_stats(&child);
                Some((child, true, size, files))
            } else if file_type.is_file() {
                let size = entry.metadata().map(|meta| allocated_size(&meta)).unwrap_or(0);
                Some((child, false, size, 1))
            } else {
                None
            }
        })
        .collect();

    children.sort_by(|a, b| b.2.cmp(&a.2));
    let total = children.iter().fold(0u64, |sum, child| sum.saturating_add(child.2));
    let files = children.iter().fold(0u64, |sum, child| sum.saturating_add(child.3));
    (total, files, children, errors.load(Ordering::Relaxed))
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
        return (allocated_size(&meta), 1, Vec::new());
    }

    // Children are visited before their parent, so a directory's size is
    // complete by the time its own entry is yielded.
    let mut pending: HashMap<PathBuf, (u64, u64)> = HashMap::new();
    let mut dirs = Vec::new();
    for entry in WalkDir::new(path)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_entry(|entry| !is_duplicate_volume(entry.path(), path))
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
                slot.0 = slot.0.saturating_add(allocated_size(&meta));
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
        *freed = freed.saturating_add(allocated_size(&meta));
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

/// Trashed entries that the app can still move back, mapped to their location
/// inside the trash folder. Only populated on platforms where moving into the
/// trash is a plain rename (macOS), so the mapping survives until restart.
static TRASH_UNDO: OnceLock<Mutex<HashMap<PathBuf, PathBuf>>> = OnceLock::new();

fn trash_undo() -> &'static Mutex<HashMap<PathBuf, PathBuf>> {
    TRASH_UNDO.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Moves one entry to the system trash. Returns whether it can be restored by
/// [`restore_trashed_entry`] (i.e. whether it went to the user's trash folder
/// via a rename).
pub fn trash_entry(path: &Path) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(moved) = move_into_trash_folder(path) {
            let restorable = match trash_undo().lock() {
                Ok(mut undo) => {
                    undo.insert(path.to_path_buf(), moved);
                    true
                }
                Err(_) => false,
            };
            return Ok(restorable);
        }
        // Different volume: fall back to the platform trash, which cannot be
        // moved back programmatically.
    }
    trash::delete(path).map_err(|err| format!("无法移入废纸篓 {}：{err}", path.display()))?;
    Ok(false)
}

/// Restores an entry previously moved by [`trash_entry`] to its original path.
pub fn restore_trashed_entry(path: &Path) -> Result<(), String> {
    let undo = trash_undo();
    let moved = {
        let mut map = undo.lock().map_err(|_| "恢复队列不可用".to_string())?;
        match map.get(path).cloned() {
            Some(moved) => {
                map.remove(path);
                moved
            }
            None => {
                return Err("无法自动恢复：该条目已不在本次会话的删除记录中，请从系统废纸篓手动恢复"
                    .into())
            }
        }
    };
    let result = (|| {
        if path.exists() {
            return Err(format!("原位置已存在同名文件：{}", path.display()));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("无法创建恢复目录 {}：{err}", parent.display()))?;
        }
        fs::rename(&moved, path).map_err(|err| format!("无法恢复 {}：{err}", path.display()))
    })();
    if result.is_err() {
        if let Ok(mut map) = undo.lock() {
            map.insert(path.to_path_buf(), moved);
        }
    }
    result
}

#[cfg(target_os = "macos")]
fn move_into_trash_folder(path: &Path) -> Option<PathBuf> {
    let trash = home().join(".Trash");
    if fs::create_dir_all(&trash).is_err() {
        return None;
    }
    let name = path.file_name()?;
    let mut candidate = trash.join(name);
    let mut index = 1;
    while candidate.exists() {
        index += 1;
        let mut renamed = name.to_os_string();
        renamed.push(format!(" {index}"));
        candidate = trash.join(renamed);
    }
    fs::rename(path, &candidate).ok().map(|_| candidate)
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
        command("open").arg(path).spawn()
    } else if cfg!(windows) {
        command("explorer").arg(path).spawn()
    } else {
        command("xdg-open").arg(path).spawn()
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
    let uid = command("id").arg("-u").output();
    uid.ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(1)
}

pub fn run_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = command(program)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn allocated_size_ignores_sparse_holes() {
        let path = std::env::temp_dir().join(format!("mole-sparse-{}", std::process::id()));
        let file = fs::File::create(&path).expect("create sparse file");
        file.set_len(64 * 1024 * 1024).expect("extend file");
        drop(file);

        let meta = fs::metadata(&path).expect("stat sparse file");
        assert_eq!(meta.len(), 64 * 1024 * 1024);
        assert!(
            allocated_size(&meta) < meta.len(),
            "sparse file should occupy fewer bytes than its logical length"
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn data_volume_only_skipped_from_outside() {
        let data = Path::new("/System/Volumes/Data/Users");
        let expected = cfg!(target_os = "macos");
        assert_eq!(is_duplicate_volume(data, Path::new("/")), expected);
        assert_eq!(is_duplicate_volume(data, Path::new("/System")), expected);
        assert!(!is_duplicate_volume(data, Path::new("/System/Volumes")));
        assert!(!is_duplicate_volume(data, Path::new("/System/Volumes/Data")));
        assert!(!is_duplicate_volume(Path::new("/Users"), Path::new("/")));
    }
}

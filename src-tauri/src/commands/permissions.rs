#[cfg(target_os = "macos")]
use std::process::Command;

use serde::Serialize;

#[cfg(target_os = "macos")]
use crate::util::home;

/// 受系统隐私保护的用户目录。
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PermissionFolder {
    pub key: String,
    pub label: String,
    pub path: String,
    pub exists: bool,
    /// `None` 表示尚未探测，避免仅打开页面就触发系统授权弹窗。
    pub granted: Option<bool>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DiskPermissionStatus {
    pub platform: String,
    /// Windows / Linux 没有对应的目录授权机制。
    pub supported: bool,
    /// macOS 完全磁盘访问权限（Full Disk Access）。
    pub full_disk_access: bool,
    /// 是否运行在 `.app` 包内。开发模式下权限会记在启动它的终端/IDE 名下。
    pub bundled: bool,
    pub app_path: String,
    pub folders: Vec<PermissionFolder>,
}

#[cfg(target_os = "macos")]
fn protected_folders() -> Vec<(String, String, std::path::PathBuf)> {
    let home = home();
    vec![
        ("desktop".into(), "桌面".into(), home.join("Desktop")),
        ("documents".into(), "文稿".into(), home.join("Documents")),
        ("downloads".into(), "下载".into(), home.join("Downloads")),
    ]
}

/// 只在用户明确操作时调用：首次访问会由系统弹出“文件与文件夹”授权。
#[cfg(target_os = "macos")]
fn probe_folders(probe: bool) -> Vec<PermissionFolder> {
    protected_folders()
        .into_iter()
        .map(|(key, label, path)| {
            let exists = path.is_dir();
            let granted = if probe && exists {
                Some(std::fs::read_dir(&path).is_ok())
            } else {
                None
            };
            PermissionFolder {
                key,
                label,
                path: path.to_string_lossy().to_string(),
                exists,
                granted,
            }
        })
        .collect()
}

/// 完全磁盘访问权限无法通过弹窗申请，只能读取受保护文件来检测。
/// 读取 TCC.db / 邮件 / Safari 数据失败即视为未授权。
#[cfg(target_os = "macos")]
fn has_full_disk_access() -> bool {
    let home = home();
    let probes = [
        home.join("Library/Application Support/com.apple.TCC/TCC.db"),
        home.join("Library/Safari/History.db"),
        home.join("Library/Mail"),
    ];
    probes.iter().any(|path| {
        if path.is_dir() {
            std::fs::read_dir(path).is_ok()
        } else {
            std::fs::File::open(path).is_ok()
        }
    })
}

fn app_bundle() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.ancestors()
        .find(|path| path.extension().map(|ext| ext == "app").unwrap_or(false))
        .map(|path| path.to_path_buf())
        .unwrap_or(exe)
}

fn is_bundled() -> bool {
    app_bundle()
        .extension()
        .map(|ext| ext == "app")
        .unwrap_or(false)
}

#[tauri::command]
pub fn disk_permission_status() -> DiskPermissionStatus {
    #[cfg(target_os = "macos")]
    {
        DiskPermissionStatus {
            platform: "macos".into(),
            supported: true,
            full_disk_access: has_full_disk_access(),
            bundled: is_bundled(),
            app_path: app_bundle().to_string_lossy().to_string(),
            folders: probe_folders(false),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        DiskPermissionStatus {
            platform: std::env::consts::OS.into(),
            supported: false,
            full_disk_access: true,
            bundled: true,
            app_path: app_bundle().to_string_lossy().to_string(),
            folders: Vec::new(),
        }
    }
}

/// 主动触碰受保护目录，让 macOS 弹出“文件与文件夹”授权。
#[tauri::command]
pub fn request_file_access() -> Vec<PermissionFolder> {
    #[cfg(target_os = "macos")]
    {
        probe_folders(true)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

/// 打开“系统设置 → 隐私与安全性”对应的面板。
#[tauri::command]
pub fn open_privacy_settings(pane: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let anchor = match pane.as_str() {
            "files-and-folders" => "Privacy_FilesAndFolders",
            "removable-volumes" => "Privacy_RemovableVolumes",
            _ => "Privacy_AllFiles",
        };
        let url = format!("x-apple.systempreferences:com.apple.preference.security?{anchor}");
        Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("无法打开系统设置：{err}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pane;
        Err("当前平台不需要此权限".into())
    }
}

/// 在 Finder 中定位应用本体，方便拖入完全磁盘访问列表。
#[tauri::command]
pub fn reveal_app() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let target = app_bundle();
        if target.as_os_str().is_empty() {
            return Err("无法定位应用路径".into());
        }
        Command::new("open")
            .arg("-R")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("无法打开 Finder：{err}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("当前平台不需要此权限".into())
    }
}

/// macOS 会在进程首次被拒绝后缓存 TCC 结论，授予完全磁盘访问后必须重启才生效。
#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

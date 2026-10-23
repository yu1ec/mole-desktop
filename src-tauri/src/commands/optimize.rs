use tauri::State;

use crate::models::{HistoryEntry, OptimizeResult, OptimizeTask};
use crate::state::AppState;
use crate::store::push_history;
use crate::util::{is_elevated, now_rfc3339, run_command};

struct TaskDef {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    category: &'static str,
    requires_admin: bool,
    danger: bool,
}

fn defs() -> Vec<TaskDef> {
    #[cfg(target_os = "macos")]
    {
        vec![
            TaskDef {
                id: "dns",
                title: "刷新 DNS 缓存",
                description: "清空 DNS 解析缓存并通知 mDNSResponder 重新加载",
                category: "网络",
                requires_admin: true,
                danger: false,
            },
            TaskDef {
                id: "quicklook",
                title: "刷新 QuickLook 缓存",
                description: "重新生成快速预览与缩略图缓存",
                category: "Finder",
                requires_admin: false,
                danger: false,
            },
            TaskDef {
                id: "ql-reset",
                title: "重建 QuickLook 预览",
                description: "重置 QuickLook 生成器注册，修复预览失效",
                category: "Finder",
                requires_admin: false,
                danger: false,
            },
            TaskDef {
                id: "periodic",
                title: "运行系统维护脚本",
                description: "执行 daily、weekly、monthly 维护脚本（可能需要几分钟）",
                category: "系统",
                requires_admin: true,
                danger: false,
            },
            TaskDef {
                id: "disk-verify",
                title: "验证启动磁盘",
                description: "只读检查启动卷的文件系统结构，不会修改数据",
                category: "磁盘",
                requires_admin: false,
                danger: false,
            },
        ]
    }
    #[cfg(windows)]
    {
        vec![
            TaskDef {
                id: "dns",
                title: "刷新 DNS 缓存",
                description: "清空 Windows DNS 解析缓存",
                category: "网络",
                requires_admin: false,
                danger: false,
            },
            TaskDef {
                id: "recycle-bin",
                title: "清空回收站",
                description: "永久删除回收站中的所有文件",
                category: "用户文件",
                requires_admin: false,
                danger: true,
            },
            TaskDef {
                id: "thumbnails",
                title: "重建缩略图缓存",
                description: "删除缩略图数据库并重启资源管理器",
                category: "资源管理器",
                requires_admin: false,
                danger: true,
            },
            TaskDef {
                id: "windows-temp",
                title: "清理系统临时文件",
                description: "清空 C:\\Windows\\Temp，需要管理员权限",
                category: "系统",
                requires_admin: true,
                danger: false,
            },
            TaskDef {
                id: "component-cleanup",
                title: "清理组件存储",
                description: "DISM 组件清理，可能需要较长时间，需要管理员权限",
                category: "系统",
                requires_admin: true,
                danger: false,
            },
        ]
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        Vec::new()
    }
}

#[tauri::command]
pub fn list_optimize_tasks() -> Vec<OptimizeTask> {
    defs()
        .into_iter()
        .map(|task| OptimizeTask {
            id: task.id.to_string(),
            title: task.title.to_string(),
            description: task.description.to_string(),
            category: task.category.to_string(),
            requires_admin: task.requires_admin,
            danger: task.danger,
            available: true,
        })
        .collect()
}

fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn mac_commands(id: &str) -> Vec<Vec<String>> {
    match id {
        "dns" => vec![
            vec!["dscacheutil".into(), "-flushcache".into()],
            vec!["killall".into(), "-HUP".into(), "mDNSResponder".into()],
        ],
        "quicklook" => vec![vec!["qlmanage".into(), "-r".into(), "cache".into()]],
        "ql-reset" => vec![vec!["qlmanage".into(), "-r".into()]],
        "periodic" => vec![vec![
            "/usr/sbin/periodic".into(),
            "daily".into(),
            "weekly".into(),
            "monthly".into(),
        ]],
        "disk-verify" => vec![vec!["diskutil".into(), "verifyVolume".into(), "/".into()]],
        _ => Vec::new(),
    }
}

#[cfg(windows)]
fn win_commands(id: &str) -> Vec<Vec<String>> {
    match id {
        "dns" => vec![vec!["ipconfig".into(), "/flushdns".into()]],
        "recycle-bin" => vec![vec![
            "powershell".into(),
            "-NoProfile".into(),
            "-Command".into(),
            "Clear-RecycleBin -Force -ErrorAction SilentlyContinue".into(),
        ]],
        "thumbnails" => vec![vec![
            "powershell".into(),
            "-NoProfile".into(),
            "-Command".into(),
            "Remove-Item \"$env:LOCALAPPDATA\\Microsoft\\Windows\\Explorer\\thumbcache_*.db\" -Force -ErrorAction SilentlyContinue; Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue; Start-Process explorer".into(),
        ]],
        "windows-temp" => vec![vec![
            "powershell".into(),
            "-NoProfile".into(),
            "-Command".into(),
            "Remove-Item \"$env:WINDIR\\Temp\\*\" -Recurse -Force -ErrorAction SilentlyContinue".into(),
        ]],
        "component-cleanup" => vec![vec![
            "DISM".into(),
            "/Online".into(),
            "/Cleanup-Image".into(),
            "/StartComponentCleanup".into(),
        ]],
        _ => Vec::new(),
    }
}

#[cfg(target_os = "macos")]
fn run_admin(commands: &[Vec<String>]) -> Result<String, String> {
    let joined = commands
        .iter()
        .map(|parts| {
            parts
                .iter()
                .map(|part| sh_quote(part))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" && ");
    let escaped = joined.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("do shell script \"{escaped}\" with administrator privileges");
    run_command("osascript", &["-e", &script])
}

fn execute(id: &str, requires_admin: bool) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    let commands = mac_commands(id);
    #[cfg(windows)]
    let commands = win_commands(id);
    #[cfg(not(any(target_os = "macos", windows)))]
    let commands: Vec<Vec<String>> = Vec::new();

    if commands.is_empty() {
        return Err("未知的优化任务".into());
    }

    #[cfg(target_os = "macos")]
    if requires_admin && !is_elevated() {
        return run_admin(&commands);
    }
    #[cfg(windows)]
    if requires_admin && !is_elevated() {
        return Err("需要以管理员身份重新启动 Mole Desktop 才能执行".into());
    }

    let mut messages = Vec::new();
    for parts in commands {
        let program = &parts[0];
        let args: Vec<&str> = parts.iter().skip(1).map(String::as_str).collect();
        let output = run_command(program, &args)?;
        if !output.is_empty() {
            messages.push(output);
        }
    }
    Ok(messages.join("\n"))
}

#[tauri::command(async)]
pub fn run_optimize(ids: Vec<String>, state: State<AppState>) -> OptimizeResult {
    let mut result = OptimizeResult::default();
    for task in defs() {
        if !ids.iter().any(|id| id == task.id) {
            continue;
        }
        match execute(task.id, task.requires_admin) {
            Ok(output) => {
                result.applied.push(task.title.to_string());
                if !output.is_empty() {
                    result.messages.push(format!("{}：{}", task.title, output));
                } else {
                    result.messages.push(format!("{}：完成", task.title));
                }
            }
            Err(err) => {
                result.failed.push(task.title.to_string());
                result.messages.push(format!("{}：{err}", task.title));
            }
        }
    }
    if result.applied.is_empty() && result.failed.is_empty() {
        result.skipped.push("没有选择可执行的任务".into());
    }
    push_history(
        &state.data_dir,
        HistoryEntry {
            id: String::new(),
            timestamp: now_rfc3339(),
            kind: "optimize".into(),
            title: format!("系统优化 · {} 项", result.applied.len()),
            freed: 0,
            files: 0,
            detail: result.messages.join("；"),
        },
    );
    result
}

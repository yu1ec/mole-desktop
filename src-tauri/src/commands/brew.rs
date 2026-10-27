use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;
use tauri::State;

use crate::models::{
    BrewFailure, BrewOverview, BrewPackage, BrewResult, BrewTap, BrewTarget, HistoryEntry,
};
use crate::state::AppState;
use crate::store::push_history;
use crate::util::now_rfc3339;

/* ------------------------------ discovery ------------------------------ */

/// Finder-launched apps inherit a minimal PATH, so probe the standard
/// Homebrew locations before falling back to PATH.
fn discover_brew() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(prefix) = std::env::var_os("HOMEBREW_PREFIX") {
        candidates.push(PathBuf::from(prefix).join("bin/brew"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/brew"));
    candidates.push(PathBuf::from("/usr/local/bin/brew"));
    candidates.push(PathBuf::from("/home/linuxbrew/.linuxbrew/bin/brew"));
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".linuxbrew/bin/brew"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            candidates.push(dir.join("brew"));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn brew_command(brew: &Path) -> Command {
    let bin_dir = brew
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/opt/homebrew/bin"));
    let prefix = bin_dir.parent().map(Path::to_path_buf);

    let mut dirs = vec![bin_dir];
    if let Some(prefix) = prefix {
        dirs.push(prefix.join("sbin"));
    }
    for dir in [
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
        "/opt/homebrew/bin",
        "/usr/local/bin",
    ] {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(existing) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&existing));
    }
    let path = std::env::join_paths(dirs).unwrap_or_default();

    let mut command = Command::new(brew);
    command
        .env("PATH", path)
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_COLOR", "0")
        .env("NONINTERACTIVE", "1")
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    text.chars().skip(count - max).collect()
}

fn run_brew(brew: &Path, args: &[&str]) -> Result<String, String> {
    let output = brew_command(brew)
        .args(args)
        .output()
        .map_err(|err| format!("无法执行 brew {}：{err}", args.join(" ")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(if stdout.is_empty() { stderr } else { stdout })
    } else {
        let combined = if stderr.is_empty() { stdout } else { stderr };
        Err(if combined.is_empty() {
            format!("brew {} 退出码 {}", args.join(" "), output.status)
        } else {
            tail(&combined, 2000)
        })
    }
}

/* ------------------------------- parsing ------------------------------- */

fn text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Array(items) => items
            .iter()
            .map(text)
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => String::new(),
    }
}

fn opt_text(value: &Value) -> Option<String> {
    let value = text(value);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn formula_package(value: &Value) -> BrewPackage {
    let installed = value["installed"].as_array();
    let installed_versions: Vec<String> = installed
        .map(|items| {
            items
                .iter()
                .filter_map(|item| opt_text(&item["version"]))
                .collect()
        })
        .unwrap_or_default();
    let stable = text(&value["versions"]["stable"]);
    let version = if installed_versions.is_empty() {
        stable.clone()
    } else {
        installed_versions.join(", ")
    };
    let latest_version = if stable.is_empty() {
        None
    } else {
        Some(stable)
    };
    let installed_at =
        installed.and_then(|items| items.iter().filter_map(|item| item["time"].as_i64()).max());
    let requested = installed
        .and_then(|items| items.first())
        .and_then(|item| item["installed_on_request"].as_bool())
        .unwrap_or(false);

    let name = text(&value["name"]);
    BrewPackage {
        display_name: name.clone(),
        name,
        full_name: text(&value["full_name"]),
        kind: "formula".into(),
        version,
        latest_version,
        outdated: value["outdated"].as_bool().unwrap_or(false),
        pinned: value["pinned"].as_bool().unwrap_or(false),
        description: text(&value["desc"]),
        homepage: text(&value["homepage"]),
        tap: text(&value["tap"]),
        installed_at,
        requested,
        dependencies: string_list(&value["dependencies"]),
    }
}

fn cask_package(value: &Value) -> BrewPackage {
    let name = text(&value["token"]);
    let display_name = opt_text(&value["name"]).unwrap_or_else(|| name.clone());
    let latest_version = opt_text(&value["version"]);
    let installed = opt_text(&value["installed"]);
    let version = installed
        .or_else(|| latest_version.clone())
        .unwrap_or_default();

    BrewPackage {
        name,
        display_name,
        full_name: text(&value["full_token"]),
        kind: "cask".into(),
        version,
        latest_version,
        outdated: value["outdated"].as_bool().unwrap_or(false),
        pinned: value["pinned"].as_bool().unwrap_or(false),
        description: text(&value["desc"]),
        homepage: text(&value["homepage"]),
        tap: text(&value["tap"]),
        installed_at: value["installed_time"].as_i64(),
        requested: true,
        dependencies: Vec::new(),
    }
}

fn parse_packages(value: &Value) -> Vec<BrewPackage> {
    let mut packages: Vec<BrewPackage> = Vec::new();
    if let Some(formulae) = value["formulae"].as_array() {
        packages.extend(formulae.iter().map(formula_package));
    }
    if let Some(casks) = value["casks"].as_array() {
        packages.extend(casks.iter().map(cask_package));
    }
    packages.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    packages
}

fn parse_taps(value: &Value) -> Vec<BrewTap> {
    let mut taps: Vec<BrewTap> = value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|item| item["installed"].as_bool().unwrap_or(false))
                .map(|item| BrewTap {
                    name: text(&item["name"]),
                    official: item["official"].as_bool().unwrap_or(false),
                    path: text(&item["path"]),
                    formula_count: item["formula_names"].as_array().map(Vec::len).unwrap_or(0),
                    cask_count: item["cask_tokens"].as_array().map(Vec::len).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    taps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    taps
}

fn unavailable() -> BrewOverview {
    BrewOverview {
        available: false,
        brew_path: None,
        version: None,
        prefix: None,
        packages: Vec::new(),
        taps: Vec::new(),
        formula_count: 0,
        cask_count: 0,
        outdated_count: 0,
    }
}

fn load_overview() -> Result<BrewOverview, String> {
    let Some(brew) = discover_brew() else {
        return Ok(unavailable());
    };
    let version = run_brew(&brew, &["--version"]).ok().and_then(|output| {
        output
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string)
    });
    let prefix = run_brew(&brew, &["--prefix"])
        .ok()
        .map(|output| output.trim().to_string())
        .filter(|output| !output.is_empty());

    let installed_text = run_brew(&brew, &["info", "--json=v2", "--installed"])?;
    let installed: Value = serde_json::from_str(&installed_text)
        .map_err(|err| format!("解析 Homebrew 软件列表失败：{err}"))?;
    let taps = run_brew(&brew, &["tap-info", "--json", "--installed"])
        .ok()
        .and_then(|output| serde_json::from_str::<Value>(&output).ok())
        .map(|value| parse_taps(&value))
        .unwrap_or_default();

    let packages = parse_packages(&installed);
    let formula_count = packages
        .iter()
        .filter(|item| item.kind == "formula")
        .count();
    let cask_count = packages.iter().filter(|item| item.kind == "cask").count();
    let outdated_count = packages.iter().filter(|item| item.outdated).count();

    Ok(BrewOverview {
        available: true,
        brew_path: Some(brew.to_string_lossy().to_string()),
        version,
        prefix,
        packages,
        taps,
        formula_count,
        cask_count,
        outdated_count,
    })
}

/* ------------------------------- actions ------------------------------- */

fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '-' | '_' | '.' | '/' | '@' | '+')
        })
}

fn valid_tap_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
                })
        })
}

fn upgrade_args(target: &BrewTarget) -> Vec<String> {
    let mut args = vec!["upgrade".to_string()];
    if target.kind == "cask" {
        args.push("--cask".into());
    }
    args.push(target.name.clone());
    args
}

fn run_upgrade(brew: &Path, targets: Vec<BrewTarget>) -> BrewResult {
    let mut result = BrewResult::default();
    let mut chunks = Vec::new();
    for target in targets {
        let args = upgrade_args(&target);
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        match run_brew(brew, &borrowed) {
            Ok(output) => {
                result.ok.push(target.name);
                if !output.is_empty() {
                    chunks.push(output);
                }
            }
            Err(message) => {
                chunks.push(message.clone());
                result.failed.push(BrewFailure {
                    name: target.name,
                    message,
                });
            }
        }
    }
    result.output = tail(&chunks.join("\n"), 4000);
    result
}

fn run_uninstall(brew: &Path, targets: Vec<BrewTarget>, zap: bool) -> BrewResult {
    let mut result = BrewResult::default();
    let mut chunks = Vec::new();
    for target in targets {
        let mut args = vec!["uninstall".to_string()];
        if target.kind == "cask" {
            args.push("--cask".into());
            if zap {
                args.push("--zap".into());
            }
        } else {
            args.push("--formula".into());
        }
        args.push(target.name.clone());
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        match run_brew(brew, &borrowed) {
            Ok(output) => {
                result.ok.push(target.name);
                if !output.is_empty() {
                    chunks.push(output);
                }
            }
            Err(message) => {
                chunks.push(message.clone());
                result.failed.push(BrewFailure {
                    name: target.name,
                    message,
                });
            }
        }
    }
    result.output = tail(&chunks.join("\n"), 4000);
    result
}

async fn with_brew<F>(task: F) -> Result<BrewResult, String>
where
    F: FnOnce(&Path) -> BrewResult + Send + 'static,
{
    let joined = tauri::async_runtime::spawn_blocking(move || -> Result<BrewResult, String> {
        let brew = discover_brew().ok_or_else(|| "未检测到 Homebrew".to_string())?;
        Ok(task(&brew))
    })
    .await
    .map_err(|err| err.to_string())?;
    joined
}

fn push_brew_history(data_dir: &Path, kind: &str, title: String, detail: String) {
    push_history(
        data_dir,
        HistoryEntry {
            id: String::new(),
            timestamp: now_rfc3339(),
            kind: kind.to_string(),
            title,
            freed: 0,
            files: 0,
            detail,
        },
    );
}

/* ------------------------------- commands ------------------------------ */

#[tauri::command]
pub async fn brew_overview() -> Result<BrewOverview, String> {
    let joined = tauri::async_runtime::spawn_blocking(load_overview)
        .await
        .map_err(|err| err.to_string())?;
    joined
}

#[tauri::command]
pub async fn brew_update() -> Result<BrewResult, String> {
    with_brew(|brew| match run_brew(brew, &["update"]) {
        Ok(output) => BrewResult {
            output: tail(&output, 4000),
            ..Default::default()
        },
        Err(message) => BrewResult {
            failed: vec![BrewFailure {
                name: "brew update".into(),
                message: message.clone(),
            }],
            output: tail(&message, 4000),
            ..Default::default()
        },
    })
    .await
}

#[tauri::command]
pub async fn brew_upgrade(
    targets: Vec<BrewTarget>,
    state: State<'_, AppState>,
) -> Result<BrewResult, String> {
    if targets.is_empty() {
        return Err("没有选择要升级的软件".into());
    }
    if let Some(target) = targets
        .iter()
        .find(|target| !valid_package_name(&target.name))
    {
        return Err(format!("无效的软件名称：{}", target.name));
    }
    let data_dir = state.data_dir.clone();
    let result = with_brew(move |brew| run_upgrade(brew, targets)).await?;
    if !result.ok.is_empty() {
        push_brew_history(
            &data_dir,
            "brew",
            format!("Homebrew 升级 · {} 个", result.ok.len()),
            result.ok.join("、"),
        );
    }
    Ok(result)
}

#[tauri::command]
pub async fn brew_upgrade_all(state: State<'_, AppState>) -> Result<BrewResult, String> {
    let data_dir = state.data_dir.clone();
    let result = with_brew(|brew| match run_brew(brew, &["upgrade"]) {
        Ok(output) => BrewResult {
            output: tail(&output, 4000),
            ..Default::default()
        },
        Err(message) => BrewResult {
            failed: vec![BrewFailure {
                name: "brew upgrade".into(),
                message: message.clone(),
            }],
            output: tail(&message, 4000),
            ..Default::default()
        },
    })
    .await?;
    if result.failed.is_empty() {
        push_brew_history(
            &data_dir,
            "brew",
            "Homebrew 全部升级".into(),
            "执行 brew upgrade".into(),
        );
    }
    Ok(result)
}

#[tauri::command]
pub async fn brew_uninstall(
    targets: Vec<BrewTarget>,
    zap: bool,
    state: State<'_, AppState>,
) -> Result<BrewResult, String> {
    if targets.is_empty() {
        return Err("没有选择要卸载的软件".into());
    }
    if let Some(target) = targets
        .iter()
        .find(|target| !valid_package_name(&target.name))
    {
        return Err(format!("无效的软件名称：{}", target.name));
    }
    let data_dir = state.data_dir.clone();
    let result = with_brew(move |brew| run_uninstall(brew, targets, zap)).await?;
    if !result.ok.is_empty() {
        push_brew_history(
            &data_dir,
            "brew",
            format!("Homebrew 卸载 · {} 个", result.ok.len()),
            result.ok.join("、"),
        );
    }
    Ok(result)
}

#[tauri::command]
pub async fn brew_add_tap(name: String) -> Result<BrewResult, String> {
    let name = name.trim().to_string();
    if !valid_tap_name(&name) {
        return Err("请输入 user/repo 格式的软件源名称".into());
    }
    let target = name.clone();
    with_brew(move |brew| match run_brew(brew, &["tap", &target]) {
        Ok(output) => BrewResult {
            ok: vec![target],
            output: tail(&output, 4000),
            ..Default::default()
        },
        Err(message) => BrewResult {
            failed: vec![BrewFailure {
                name: target,
                message: message.clone(),
            }],
            output: tail(&message, 4000),
            ..Default::default()
        },
    })
    .await
}

#[tauri::command]
pub async fn brew_remove_tap(name: String) -> Result<BrewResult, String> {
    let name = name.trim().to_string();
    if !valid_tap_name(&name) {
        return Err("无效的软件源名称".into());
    }
    let target = name.clone();
    with_brew(move |brew| match run_brew(brew, &["untap", &target]) {
        Ok(output) => BrewResult {
            ok: vec![target],
            output: tail(&output, 4000),
            ..Default::default()
        },
        Err(message) => BrewResult {
            failed: vec![BrewFailure {
                name: target,
                message: message.clone(),
            }],
            output: tail(&message, 4000),
            ..Default::default()
        },
    })
    .await
}

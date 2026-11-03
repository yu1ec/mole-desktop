//! 获取已安装软件，按安装渠道（Homebrew / Mac App Store / Sparkle / winget /
//! Microsoft Store / 手动安装）检查版本差异，并执行更新。
//!
//! macOS 与 Windows 的检测和更新方式完全不同，分别在 `macos` / `windows`
//! 子模块中实现；命令层只负责汇总结果与写操作历史。

use std::time::Duration;

use serde::Serialize;
use tauri::State;

use crate::models::{
    AppSignatureInfo, AppUpdateEntry, AppUpdateFailure, AppUpdateResult, AppUpdateScan,
    AppUpdateTarget, HistoryEntry,
};
use crate::state::AppState;
use crate::store::push_history;
use crate::util::now_rfc3339;

const USER_AGENT: &str = "mole-desktop";
const SCAN_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(900);
const APP_UPDATE_PROGRESS: &str = "app-update-progress";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppUpdateProgress {
    id: String,
    name: String,
    downloaded: u64,
    total: Option<u64>,
    bytes_per_sec: u64,
}

/* ------------------------------- commands ------------------------------ */

#[tauri::command]
pub async fn scan_app_updates() -> Result<AppUpdateScan, String> {
    #[cfg(target_os = "macos")]
    {
        macos::scan().await
    }
    #[cfg(windows)]
    {
        let scan = tauri::async_runtime::spawn_blocking(windows::scan)
            .await
            .map_err(|err| err.to_string())?;
        Ok(scan)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        Ok(empty_scan())
    }
}

/// 通过 Homebrew 安装 `mas`（Mac App Store 命令行工具）。
/// 安装后重新扫描即可检查 App Store 应用的更新。
#[tauri::command]
pub async fn install_mas() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        blocking(|| {
            let brew = crate::commands::brew::discover_brew().ok_or_else(|| {
                "未检测到 Homebrew，请先安装 Homebrew 后再安装 mas".to_string()
            })?;
            crate::commands::brew::run_brew(&brew, &["install", "mas"])
        })
        .await
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("当前平台不支持 mas".into())
    }
}

#[tauri::command]
pub async fn update_apps(
    app: tauri::AppHandle,
    targets: Vec<AppUpdateTarget>,
    state: State<'_, AppState>,
) -> Result<AppUpdateResult, String> {
    if targets.is_empty() {
        return Err("没有选择要更新的软件".into());
    }

    let mut result = AppUpdateResult::default();
    let mut chunks = Vec::new();
    for target in targets {
        match run_target(&app, &target).await {
            Ok(message) => {
                result.ok.push(target.name.clone());
                if !message.trim().is_empty() {
                    chunks.push(message);
                }
            }
            Err(message) => {
                chunks.push(format!("{}：{}", target.name, message));
                result.failed.push(AppUpdateFailure {
                    name: target.name.clone(),
                    message,
                });
            }
        }
    }
    result.output = tail(&chunks.join("\n"), 4000);

    if !result.ok.is_empty() {
        push_history(
            &state.data_dir,
            HistoryEntry {
                id: String::new(),
                timestamp: now_rfc3339(),
                kind: "update".into(),
                title: format!("应用更新 · {} 个", result.ok.len()),
                freed: 0,
                files: 0,
                detail: result.ok.join("、"),
            },
        );
    }

    Ok(result)
}

async fn run_target(app: &tauri::AppHandle, target: &AppUpdateTarget) -> Result<String, String> {
    match target.channel.as_str() {
        "homebrew" => {
            #[cfg(target_os = "macos")]
            {
                let target = target.clone();
                blocking(move || macos::upgrade_brew(&target)).await
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = target;
                Err("当前平台不支持 Homebrew 更新".into())
            }
        }
        "appstore" => {
            #[cfg(target_os = "macos")]
            {
                let target = target.clone();
                blocking(move || macos::upgrade_mas(&target)).await
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = target;
                Err("当前平台不支持 App Store 更新".into())
            }
        }
        "sparkle" => {
            #[cfg(target_os = "macos")]
            {
                macos::install_sparkle(app, target).await
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = app;
                Err("当前平台不支持该更新方式".into())
            }
        }
        "winget" | "store" => {
            #[cfg(windows)]
            {
                let target = target.clone();
                blocking(move || windows::upgrade_winget(&target)).await
            }
            #[cfg(not(windows))]
            {
                let _ = target;
                Err("当前平台不支持 winget 更新".into())
            }
        }
        _ => Err("该软件没有可用的自动更新渠道".into()),
    }
}

async fn blocking<F>(task: F) -> Result<String, String>
where
    F: FnOnce() -> Result<String, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|err| err.to_string())?
}

/* -------------------------------- helpers ------------------------------ */

#[cfg(not(any(target_os = "macos", windows)))]
fn empty_scan() -> AppUpdateScan {
    AppUpdateScan {
        apps: Vec::new(),
        updatable: 0,
        brew_available: false,
        mas_available: false,
        winget_available: false,
        warnings: Vec::new(),
        platform: platform_name().into(),
        scanned_at: now_rfc3339(),
    }
}

fn platform_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    text.chars().skip(count - max).collect()
}

/// 比较版本号。应用版本不一定是 semver，按数字分段比较最稳妥。
fn numeric_version_parts(version: &str) -> Vec<u64> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let left = left.trim().trim_start_matches(['v', 'V']);
    let right = right.trim().trim_start_matches(['v', 'V']);
    let left_parts = numeric_version_parts(left);
    let right_parts = numeric_version_parts(right);
    if left_parts.is_empty() || right_parts.is_empty() {
        return Ordering::Equal;
    }
    for index in 0..left_parts.len().max(right_parts.len()) {
        let a = left_parts.get(index).copied().unwrap_or(0);
        let b = right_parts.get(index).copied().unwrap_or(0);
        if a != b {
            return a.cmp(&b);
        }
    }
    Ordering::Equal
}

pub(crate) fn version_is_newer(latest: &str, current: &str) -> bool {
    if latest.trim().is_empty() || current.trim().is_empty() {
        return false;
    }
    compare_versions(latest, current) == std::cmp::Ordering::Greater
}

/// 名称归一化：忽略大小写、空格、连字符和 `.app` 后缀，用于跨渠道匹配。
fn normalize_name(name: &str) -> String {
    let name = name.trim().trim_end_matches(".app");
    name.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect()
}

fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('-');
    if cleaned.is_empty() {
        "app-update".into()
    } else {
        cleaned.to_string()
    }
}

fn http_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(15))
        .build()
        .expect("reqwest client")
}

/* ------------------------------ winget table --------------------------- */

#[cfg(any(windows, test))]
#[derive(Debug, Clone)]
struct WingetItem {
    name: String,
    id: String,
    version: String,
    available: String,
    source: String,
}

#[cfg(any(windows, test))]
impl WingetItem {
    fn channel(&self) -> &'static str {
        if self.source.eq_ignore_ascii_case("msstore") {
            "store"
        } else {
            "winget"
        }
    }
}

/// winget 没有稳定的 JSON 输出，只能解析固定宽度的表格。
/// 列位置由表头下方的 `---` 分隔线决定，因此不受界面语言影响。
#[cfg(any(windows, test))]
fn parse_winget_table(text: &str) -> Result<Vec<WingetItem>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(separator_index) = lines.iter().position(|line| is_table_separator(line)) else {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        return Err("winget 输出中没有可解析的表格".into());
    };
    let columns = column_ranges(lines[separator_index]);
    if columns.len() < 3 {
        return Err("winget 表格列数不足".into());
    }

    let mut items = Vec::new();
    for line in lines.iter().skip(separator_index + 1) {
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            continue;
        }
        let cells: Vec<String> = columns
            .iter()
            .map(|(start, end)| slice_chars(&chars, *start, *end))
            .collect();
        let name = cells.first().cloned().unwrap_or_default();
        let id = cells.get(1).cloned().unwrap_or_default();
        let version = cells.get(2).cloned().unwrap_or_default();
        if name.is_empty() || !is_valid_winget_id(&id) {
            continue;
        }
        items.push(WingetItem {
            name,
            id,
            version,
            available: cells.get(3).cloned().unwrap_or_default(),
            source: cells.get(4).cloned().unwrap_or_default(),
        });
    }
    Ok(items)
}

#[cfg(any(windows, test))]
fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 || !trimmed.chars().all(|c| c == '-' || c.is_whitespace()) {
        return false;
    }
    trimmed
        .split_whitespace()
        .filter(|part| part.chars().all(|c| c == '-'))
        .count()
        >= 2
}

#[cfg(any(windows, test))]
fn column_ranges(line: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let mut starts = Vec::new();
    let mut previous_dash = false;
    for (index, character) in chars.iter().enumerate() {
        let is_dash = *character == '-';
        if is_dash && !previous_dash {
            starts.push(index);
        }
        previous_dash = is_dash;
    }
    let mut ranges = Vec::new();
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(chars.len());
        ranges.push((*start, end));
    }
    ranges
}

#[cfg(any(windows, test))]
fn slice_chars(chars: &[char], start: usize, end: usize) -> String {
    chars
        .get(start..end)
        .unwrap_or(&[])
        .iter()
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(any(windows, test))]
fn is_valid_winget_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    '.' | '-' | '_' | '+' | '\\' | '/' | '{' | '}' | ':'
                )
        })
}

/* ------------------------------- sparkle ------------------------------- */

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy)]
enum PackageKind {
    Zip,
    Dmg,
    Pkg,
    /// `.tar.xz` / `.tar.gz` 等，用系统 tar 解包（活动更新源较少见）。
    Tar,
}

#[cfg(any(target_os = "macos", test))]
impl PackageKind {
    fn from_url(url: &str, content_type: Option<&str>) -> Option<Self> {
        let path = url.split('?').next().unwrap_or(url).to_lowercase();
        if path.ends_with(".zip") {
            return Some(Self::Zip);
        }
        if path.ends_with(".dmg") {
            return Some(Self::Dmg);
        }
        if path.ends_with(".pkg") {
            return Some(Self::Pkg);
        }
        if [
            ".tar", ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".tbz2",
        ]
        .iter()
        .any(|suffix| path.ends_with(suffix))
        {
            return Some(Self::Tar);
        }
        let content_type = content_type.unwrap_or("").to_lowercase();
        if content_type.contains("zip") {
            Some(Self::Zip)
        } else if content_type.contains("dmg") || content_type.contains("diskimage") {
            Some(Self::Dmg)
        } else if content_type.contains("pkg") {
            Some(Self::Pkg)
        } else if content_type.contains("tar") {
            Some(Self::Tar)
        } else {
            None
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Zip => ".zip",
            Self::Dmg => ".dmg",
            Self::Pkg => ".pkg",
            Self::Tar => ".tar",
        }
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone)]
struct SparkleRelease {
    short_version: Option<String>,
    version: Option<String>,
    download_url: Option<String>,
    content_type: Option<String>,
}

#[cfg(any(target_os = "macos", test))]
impl SparkleRelease {
    fn latest_version(&self) -> Option<String> {
        self.short_version.clone().or_else(|| self.version.clone())
    }

    fn package_kind(&self) -> Option<PackageKind> {
        let url = self.download_url.as_deref()?;
        PackageKind::from_url(url, self.content_type.as_deref())
    }
}

#[cfg(any(target_os = "macos", test))]
fn parse_appcast(xml: &str) -> Option<SparkleRelease> {
    let mut items = Vec::new();
    let mut cursor = 0;
    while let Some(relative_start) = xml[cursor..].find("<item>") {
        let start = cursor + relative_start + "<item>".len();
        let Some(relative_end) = xml[start..].find("</item>") else {
            break;
        };
        let end = start + relative_end;
        items.push(&xml[start..end]);
        cursor = end + "</item>".len();
    }
    if items.is_empty() {
        return None;
    }

    // Sparkle 支持 beta 频道；默认只使用没有 channel 标记的稳定版本。
    let stable: Vec<&str> = items
        .iter()
        .copied()
        .filter(|item| extract_tag(item, "sparkle:channel").is_none())
        .collect();
    let pool = if stable.is_empty() { items } else { stable };

    let mut best: Option<(SparkleRelease, String)> = None;
    for item in pool {
        let enclosure = extract_enclosure(item);
        // Sparkle 2 常把版本写成 enclosure 的属性，旧版本才用子节点。
        let build = extract_tag(item, "sparkle:version").or(enclosure.build);
        let short = extract_tag(item, "sparkle:shortVersionString")
            .or(enclosure.short_version)
            .or_else(|| {
                if build.is_some() {
                    None
                } else {
                    extract_tag(item, "title").and_then(|title| version_token(&title))
                }
            });
        let Some(version) = short.clone().or_else(|| build.clone()) else {
            continue;
        };
        let release = SparkleRelease {
            short_version: short,
            version: build,
            download_url: enclosure.url,
            content_type: enclosure.content_type,
        };
        let better = match &best {
            None => true,
            Some((_, best_version)) => {
                compare_versions(&version, best_version) == std::cmp::Ordering::Greater
            }
        };
        if better {
            best = Some((release, version));
        }
    }
    best.map(|(release, _)| release)
}

/// 版本对比要使用同一类版本号：优先短版本，缺失时退回内部构建号。
#[cfg(any(target_os = "macos", test))]
fn release_is_newer(release: &SparkleRelease, short: &str, build: &str) -> bool {
    if let Some(latest) = release.short_version.as_deref() {
        if !short.trim().is_empty() {
            return version_is_newer(latest, short);
        }
    }
    if let Some(latest) = release.version.as_deref() {
        if !build.trim().is_empty() {
            return version_is_newer(latest, build);
        }
    }
    false
}

#[cfg(any(target_os = "macos", test))]
fn extract_tag(source: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = source.find(&open)? + open.len();
    let end = source[start..].find(&close)? + start;
    let value = source[start..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(decode_xml_entities(value))
    }
}

#[cfg(any(target_os = "macos", test))]
struct Enclosure {
    url: Option<String>,
    content_type: Option<String>,
    short_version: Option<String>,
    build: Option<String>,
}

#[cfg(any(target_os = "macos", test))]
fn extract_enclosure(item: &str) -> Enclosure {
    let empty = Enclosure {
        url: None,
        content_type: None,
        short_version: None,
        build: None,
    };
    // `<sparkle:deltas>` 里的 enclosure 是增量补丁，不能当作完整安装包。
    let item = match item.find("<sparkle:deltas") {
        Some(index) => &item[..index],
        None => item,
    };
    let Some(start) = item.find("<enclosure") else {
        return empty;
    };
    let Some(relative_end) = item[start..].find('>') else {
        return empty;
    };
    let tag = &item[start..start + relative_end];
    Enclosure {
        url: extract_attr(tag, "url"),
        content_type: extract_attr(tag, "type"),
        short_version: extract_attr(tag, "sparkle:shortVersionString"),
        build: extract_attr(tag, "sparkle:version"),
    }
}

#[cfg(any(target_os = "macos", test))]
fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let mut search = 0;
    while let Some(relative) = tag[search..].find(&needle) {
        let index = search + relative;
        let before = tag[..index].chars().last();
        let at_boundary = before
            .map(|character| character.is_whitespace() || character == '<')
            .unwrap_or(true);
        if !at_boundary {
            search = index + needle.len();
            continue;
        }
        let rest = tag[index + needle.len()..].trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let rest = &rest[1..];
        let end = rest.find(quote)?;
        let value = decode_xml_entities(rest[..end].trim());
        return if value.is_empty() { None } else { Some(value) };
    }
    None
}

#[cfg(any(target_os = "macos", test))]
fn version_token(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|part| part.trim_matches(|c: char| !(c.is_ascii_digit() || c == '.')))
        .find(|part| {
            !part.is_empty()
                && part.chars().any(|c| c.is_ascii_digit())
                && part.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
        .map(str::to_string)
}

#[cfg(any(target_os = "macos", test))]
fn decode_xml_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/* --------------------------------- macos ------------------------------- */

#[cfg(target_os = "macos")]
mod macos {
    use std::collections::{HashMap, HashSet};
    use std::io::Write as _;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;

    use futures_util::StreamExt as _;
    use rayon::prelude::*;
    use tauri::Emitter as _;

    use crate::commands::apps::{running_pids, terminate_processes};
    use crate::commands::brew::{discover_brew, run_brew, valid_package_name};
    use crate::macos_install::{inspect_signature, SignatureInfo};
    use crate::util::{command, home, strip_ansi};

    use super::*;

    struct MacApp {
        path: PathBuf,
        bundle_id: String,
        name: String,
        version: String,
        /// `CFBundleVersion`，Sparkle 更新源只提供构建号时用它比较。
        build: String,
        feed_url: Option<String>,
        /// `Contents/_MASReceipt` 存在说明来自 Mac App Store，即使 mas 未安装。
        mas_receipt: bool,
        /// 代码签名状态，用于提示是否被重签或篡改。
        signature: SignatureInfo,
    }

    struct Cask {
        token: String,
        name: String,
        installed_version: String,
        homepage: Option<String>,
        app_names: Vec<String>,
    }

    struct MasApp {
        id: String,
        name: String,
        version: String,
    }

    struct LocalScan {
        apps: Vec<MacApp>,
        casks: Vec<Cask>,
        outdated_casks: HashMap<String, String>,
        mas_apps: Vec<MasApp>,
        mas_outdated: HashMap<String, String>,
        brew_available: bool,
        mas_available: bool,
        /// 本机 App Store 所在国家/地区代码（如 `cn`），用于生成带国家前缀的链接。
        storefront: Option<String>,
        warnings: Vec<String>,
    }

    pub async fn scan() -> Result<AppUpdateScan, String> {
        let local = tauri::async_runtime::spawn_blocking(collect_local)
            .await
            .map_err(|err| err.to_string())?;
        Ok(build_scan(local).await)
    }

    fn collect_local() -> LocalScan {
        let mut warnings = Vec::new();
        let apps = collect_apps();
        let brew_available = discover_brew().is_some();
        let mas_path = discover_tool("mas");
        let mas_available = mas_path.is_some();
        let storefront = current_storefront(mas_path.as_deref());

        let (casks, outdated_casks) = if brew_available {
            match load_brew_casks() {
                Ok(data) => data,
                Err(err) => {
                    warnings.push(format!("读取 Homebrew 应用失败：{err}"));
                    (Vec::new(), HashMap::new())
                }
            }
        } else {
            (Vec::new(), HashMap::new())
        };

        let (mas_apps, mas_outdated) = if let Some(mas) = &mas_path {
            match load_mas_apps(mas) {
                Ok(data) => data,
                Err(err) => {
                    warnings.push(format!("读取 App Store 应用失败：{err}"));
                    (Vec::new(), HashMap::new())
                }
            }
        } else {
            (Vec::new(), HashMap::new())
        };

        LocalScan {
            apps,
            casks,
            outdated_casks,
            mas_apps,
            mas_outdated,
            brew_available,
            mas_available,
            storefront,
            warnings,
        }
    }

    async fn build_scan(local: LocalScan) -> AppUpdateScan {
        let mut entries = Vec::new();

        let mut cask_matches: HashMap<String, usize> = HashMap::new();
        for (index, cask) in local.casks.iter().enumerate() {
            for app_name in &cask.app_names {
                cask_matches
                    .entry(normalize_name(app_name))
                    .or_insert(index);
            }
            cask_matches
                .entry(normalize_name(&cask.name))
                .or_insert(index);
            cask_matches
                .entry(normalize_name(&cask.token))
                .or_insert(index);
        }
        let mas_matches: HashMap<String, usize> = local
            .mas_apps
            .iter()
            .enumerate()
            .map(|(index, app)| (normalize_name(&app.name), index))
            .collect();

        let mut used_casks = HashSet::new();
        let mut used_mas = HashSet::new();
        // (entries 下标, App Store id)，扫描结束后统一解析出带国家前缀的页面链接。
        let mut mas_links: Vec<(usize, String)> = Vec::new();
        let mut sparkle_apps = Vec::new();
        // mas 可能因为权限、版本差异或应用已下架而漏报，先收集起来按 bundle id 查 App Store。
        let mut unmatched_mas = Vec::new();

        for app in local.apps {
            let file_stem = app
                .path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let keys = [normalize_name(&app.name), normalize_name(file_stem)];

            if let Some(index) = keys.iter().find_map(|key| cask_matches.get(key).copied()) {
                let cask = &local.casks[index];
                used_casks.insert(index);
                let latest = local.outdated_casks.get(&cask.token).cloned();
                let update_available = latest
                    .as_deref()
                    .map(|version| version_is_newer(version, &app.version))
                    .unwrap_or(false);
                entries.push(AppUpdateEntry {
                    id: format!("brew:{}", cask.token),
                    name: app.name,
                    path: Some(app.path.to_string_lossy().to_string()),
                    channel: "homebrew".into(),
                    installed_version: app.version,
                    latest_version: latest,
                    update_available,
                    can_update: update_available,
                    source: cask.token.clone(),
                    homepage: cask.homepage.clone(),
                    note: None,
                    signature: Some(signature_info(&app.signature)),
                });
                continue;
            }

            if let Some(index) = keys.iter().find_map(|key| mas_matches.get(key).copied()) {
                let mas_app = &local.mas_apps[index];
                used_mas.insert(index);
                let latest = local.mas_outdated.get(&mas_app.id).cloned();
                let update_available = latest
                    .as_deref()
                    .map(|version| version_is_newer(version, &app.version))
                    .unwrap_or(false);
                entries.push(AppUpdateEntry {
                    id: format!("mas:{}", mas_app.id),
                    name: app.name,
                    path: Some(app.path.to_string_lossy().to_string()),
                    channel: "appstore".into(),
                    installed_version: app.version,
                    latest_version: latest,
                    update_available,
                    can_update: update_available,
                    source: mas_app.id.clone(),
                    homepage: None,
                    note: None,
                    signature: Some(signature_info(&app.signature)),
                });
                mas_links.push((entries.len() - 1, mas_app.id.clone()));
                continue;
            }

            if app.mas_receipt {
                unmatched_mas.push(app);
                continue;
            }

            if app.feed_url.is_some() {
                sparkle_apps.push(app);
            } else {
                entries.push(manual_entry(&app));
            }
        }

        // appcast 需要联网，限制并发，避免一次扫描打开过多连接。
        let fetched = futures_util::stream::iter(
            sparkle_apps
                .into_iter()
                .map(|app| async move { sparkle_entry(app).await }),
        )
        .buffer_unordered(8)
        .collect::<Vec<_>>()
        .await;
        entries.extend(fetched);

        // mas 未列出的 App Store 应用按 bundle id 查询 App Store 兜底，
        // 同样限制并发，避免一次扫描打开过多连接。
        let mas_available = local.mas_available;
        let fetched = futures_util::stream::iter(
            unmatched_mas
                .into_iter()
                .map(|app| async move { appstore_lookup_entry(app, mas_available).await }),
        )
        .buffer_unordered(8)
        .collect::<Vec<_>>()
        .await;
        entries.extend(fetched);

        // 应用目录里没找到对应 .app、但 Homebrew 仍标记为过期的 cask，
        // 也纳入列表，避免漏掉安装在自定义目录的软件。
        for (index, cask) in local.casks.iter().enumerate() {
            if used_casks.contains(&index) {
                continue;
            }
            let Some(latest) = local.outdated_casks.get(&cask.token) else {
                continue;
            };
            if cask.app_names.is_empty() {
                continue;
            }
            entries.push(AppUpdateEntry {
                id: format!("brew:{}", cask.token),
                name: cask.name.clone(),
                path: None,
                channel: "homebrew".into(),
                installed_version: cask.installed_version.clone(),
                latest_version: Some(latest.clone()),
                update_available: true,
                can_update: true,
                source: cask.token.clone(),
                homepage: cask.homepage.clone(),
                note: Some("未在应用目录中找到对应的 .app".into()),
                signature: None,
            });
        }

        // 同理，补上未匹配到本地应用的 App Store 更新。
        for (index, mas_app) in local.mas_apps.iter().enumerate() {
            if used_mas.contains(&index) {
                continue;
            }
            let Some(latest) = local.mas_outdated.get(&mas_app.id) else {
                continue;
            };
            entries.push(AppUpdateEntry {
                id: format!("mas:{}", mas_app.id),
                name: mas_app.name.clone(),
                path: None,
                channel: "appstore".into(),
                installed_version: mas_app.version.clone(),
                latest_version: Some(latest.clone()),
                update_available: true,
                can_update: true,
                source: mas_app.id.clone(),
                homepage: None,
                note: None,
                signature: None,
            });
            mas_links.push((entries.len() - 1, mas_app.id.clone()));
        }

        if !mas_links.is_empty() {
            let ids: Vec<String> = mas_links.iter().map(|(_, id)| id.clone()).collect();
            let links = resolve_app_store_links(&ids, local.storefront.as_deref()).await;
            for (index, id) in mas_links {
                entries[index].homepage =
                    Some(links.get(&id).cloned().unwrap_or_else(|| {
                        app_store_fallback_link(&id, local.storefront.as_deref())
                    }));
            }
        }

        finish_scan(
            entries,
            local.brew_available,
            local.mas_available,
            false,
            local.warnings,
        )
    }

    fn manual_entry(app: &MacApp) -> AppUpdateEntry {
        AppUpdateEntry {
            id: format!("manual:{}", app.path.to_string_lossy()),
            name: app.name.clone(),
            path: Some(app.path.to_string_lossy().to_string()),
            channel: "manual".into(),
            installed_version: app.version.clone(),
            latest_version: None,
            update_available: false,
            can_update: false,
            source: app.path.to_string_lossy().to_string(),
            homepage: None,
            note: Some("未识别安装渠道，无法自动检查更新".into()),
            signature: Some(signature_info(&app.signature)),
        }
    }

    fn signature_info(info: &SignatureInfo) -> AppSignatureInfo {
        AppSignatureInfo {
            state: info.state.as_str().to_string(),
            signer: info.signer.clone(),
            team_id: info.team_id.clone(),
        }
    }

    async fn sparkle_entry(app: MacApp) -> AppUpdateEntry {
        let Some(feed_url) = app.feed_url.as_deref() else {
            return manual_entry(&app);
        };
        let client = http_client(SCAN_HTTP_TIMEOUT);
        match fetch_appcast(&client, feed_url).await {
            Ok(release) => {
                let latest = release.latest_version();
                let update_available = release_is_newer(&release, &app.version, &app.build);
                let supported = release.package_kind().is_some();
                let note = match (&latest, update_available, supported) {
                    (Some(_), true, false) => {
                        Some("更新包格式暂不支持自动安装，请打开应用检查更新".into())
                    }
                    _ => None,
                };
                AppUpdateEntry {
                    id: format!("sparkle:{}", app.path.to_string_lossy()),
                    name: app.name,
                    path: Some(app.path.to_string_lossy().to_string()),
                    channel: "sparkle".into(),
                    installed_version: app.version,
                    latest_version: latest,
                    update_available,
                    can_update: update_available && supported,
                    source: app.path.to_string_lossy().to_string(),
                    homepage: None,
                    note,
                    signature: Some(signature_info(&app.signature)),
                }
            }
            Err(err) => AppUpdateEntry {
                id: format!("sparkle:{}", app.path.to_string_lossy()),
                name: app.name.clone(),
                path: Some(app.path.to_string_lossy().to_string()),
                channel: "sparkle".into(),
                installed_version: app.version,
                latest_version: None,
                update_available: false,
                can_update: false,
                source: app.path.to_string_lossy().to_string(),
                homepage: None,
                note: Some(format!("无法读取更新源：{err}")),
                signature: Some(signature_info(&app.signature)),
            },
        }
    }

    fn finish_scan(
        mut entries: Vec<AppUpdateEntry>,
        brew_available: bool,
        mas_available: bool,
        winget_available: bool,
        warnings: Vec<String>,
    ) -> AppUpdateScan {
        entries.sort_by(|a, b| {
            b.update_available
                .cmp(&a.update_available)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let updatable = entries
            .iter()
            .filter(|entry| entry.update_available)
            .count();
        AppUpdateScan {
            apps: entries,
            updatable,
            brew_available,
            mas_available,
            winget_available,
            warnings,
            platform: platform_name().into(),
            scanned_at: now_rfc3339(),
        }
    }

    /* --------------------------- local discovery --------------------------- */

    fn app_dirs() -> Vec<PathBuf> {
        vec![PathBuf::from("/Applications"), home().join("Applications")]
    }

    fn collect_apps() -> Vec<MacApp> {
        let mut paths = Vec::new();
        for dir in app_dirs() {
            collect_paths(&dir, 0, &mut paths);
        }
        paths.par_iter().filter_map(|path| read_app(path)).collect()
    }

    fn collect_paths(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if name.ends_with(".app") {
                out.push(path);
            } else if depth < 2 {
                collect_paths(&path, depth + 1, out);
            }
        }
    }

    fn read_app(path: &Path) -> Option<MacApp> {
        let info = plist::Value::from_file(path.join("Contents/Info.plist")).ok()?;
        let dict = info.as_dictionary()?;
        let get = |key: &str| -> String {
            dict.get(key)
                .and_then(|value| value.as_string())
                .unwrap_or("")
                .to_string()
        };
        let bundle_id = get("CFBundleIdentifier");
        let name = {
            let display = get("CFBundleDisplayName");
            if !display.is_empty() {
                display
            } else {
                let bundle_name = get("CFBundleName");
                if !bundle_name.is_empty() {
                    bundle_name
                } else {
                    path.file_stem()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Unknown".into())
                }
            }
        };
        let version = {
            let short = get("CFBundleShortVersionString");
            if short.is_empty() {
                get("CFBundleVersion")
            } else {
                short
            }
        };
        let build = get("CFBundleVersion");
        let feed_url = {
            let value = get("SUFeedURL");
            if value.starts_with("https://") || value.starts_with("http://") {
                Some(value)
            } else {
                None
            }
        };
        let mas_receipt = path.join("Contents/_MASReceipt/receipt").is_file();
        Some(MacApp {
            path: path.to_path_buf(),
            bundle_id,
            name,
            version,
            build,
            feed_url,
            mas_receipt,
            signature: inspect_signature(path),
        })
    }

    fn discover_tool(name: &str) -> Option<PathBuf> {
        let mut candidates = vec![
            PathBuf::from("/opt/homebrew/bin").join(name),
            PathBuf::from("/usr/local/bin").join(name),
            home().join(".local/bin").join(name),
            home().join(".cargo/bin").join(name),
        ];
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path) {
                candidates.push(dir.join(name));
            }
        }
        candidates.into_iter().find(|path| path.is_file())
    }

    fn run_tool(program: &Path, args: &[&str]) -> Result<String, String> {
        let output = command(program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|err| format!("无法执行 {}：{err}", program.display()))?;
        let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout));
        let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
        if output.status.success() {
            Ok(if stdout.trim().is_empty() {
                stderr
            } else {
                stdout
            })
        } else {
            let combined = if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            };
            Err(if combined.trim().is_empty() {
                format!("{} 退出码 {}", program.display(), output.status)
            } else {
                tail(&combined, 2000)
            })
        }
    }

    /* ------------------------------ homebrew ------------------------------ */

    fn text(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(value) => value.clone(),
            serde_json::Value::Array(items) => items
                .iter()
                .map(text)
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
                .join(", "),
            serde_json::Value::Number(value) => value.to_string(),
            serde_json::Value::Bool(value) => value.to_string(),
            _ => String::new(),
        }
    }

    fn opt_text(value: &serde_json::Value) -> Option<String> {
        let value = text(value);
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    }

    fn load_brew_casks() -> Result<(Vec<Cask>, HashMap<String, String>), String> {
        let brew = discover_brew().ok_or_else(|| "未检测到 Homebrew".to_string())?;
        let info = run_brew(&brew, &["info", "--json=v2", "--cask", "--installed"])?;
        let info: serde_json::Value = serde_json::from_str(&info)
            .map_err(|err| format!("解析 Homebrew 应用列表失败：{err}"))?;

        let mut casks = Vec::new();
        if let Some(items) = info["casks"].as_array() {
            for item in items {
                let token = text(&item["token"]);
                if token.is_empty() {
                    continue;
                }
                let name = opt_text(&item["name"]).unwrap_or_else(|| token.clone());
                let installed_version = opt_text(&item["installed"]).unwrap_or_default();
                let homepage = opt_text(&item["homepage"]);
                let mut app_names = Vec::new();
                if let Some(artifacts) = item["artifacts"].as_array() {
                    for artifact in artifacts {
                        if let Some(object) = artifact.as_object() {
                            for key in ["app", "suite"] {
                                if let Some(items) =
                                    object.get(key).and_then(|value| value.as_array())
                                {
                                    for entry in items {
                                        if let Some(value) = entry.as_str() {
                                            app_names.push(value.to_string());
                                        }
                                    }
                                }
                            }
                        } else if let Some(value) = artifact.as_str() {
                            if value.ends_with(".app") {
                                app_names.push(value.to_string());
                            }
                        }
                    }
                }
                casks.push(Cask {
                    token,
                    name,
                    installed_version,
                    homepage,
                    app_names,
                });
            }
        }

        let outdated = run_brew(&brew, &["outdated", "--json=v2", "--cask"])?;
        let outdated: serde_json::Value = serde_json::from_str(&outdated)
            .map_err(|err| format!("解析 Homebrew 更新列表失败：{err}"))?;
        let mut versions = HashMap::new();
        if let Some(items) = outdated["casks"].as_array() {
            for item in items {
                let token = text(&item["name"]);
                let latest = text(&item["current_version"]);
                if !token.is_empty() && !latest.is_empty() {
                    versions.insert(token, latest);
                }
            }
        }
        Ok((casks, versions))
    }

    pub fn upgrade_brew(target: &AppUpdateTarget) -> Result<String, String> {
        if !valid_package_name(&target.source) {
            return Err("无效的 Homebrew cask 名称".into());
        }
        let brew = discover_brew().ok_or_else(|| "未检测到 Homebrew".to_string())?;
        run_brew(&brew, &["upgrade", "--cask", &target.source])
    }

    /* ------------------------------ app store ----------------------------- */

    fn load_mas_apps(mas: &Path) -> Result<(Vec<MasApp>, HashMap<String, String>), String> {
        let list = run_tool(mas, &["list"])?;
        let mut apps = Vec::new();
        for line in list.lines() {
            if let Some((id, name, version)) = parse_mas_line(line) {
                apps.push(MasApp { id, name, version });
            }
        }

        let outdated = run_tool(mas, &["outdated"])?;
        let mut versions = HashMap::new();
        for line in outdated.lines() {
            if let Some((id, _name, _current, latest)) = parse_mas_outdated(line) {
                versions.insert(id, latest);
            }
        }
        Ok((apps, versions))
    }

    fn parse_mas_line(line: &str) -> Option<(String, String, String)> {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Warning") || line.starts_with("No ") {
            return None;
        }
        let (id, rest) = line.split_once(char::is_whitespace)?;
        if id.is_empty() || !id.chars().all(|character| character.is_ascii_digit()) {
            return None;
        }
        let rest = rest.trim();
        let open = rest.rfind(" (")?;
        let version = rest[open + 2..].strip_suffix(')')?.trim().to_string();
        let name = rest[..open].trim().to_string();
        if name.is_empty() || version.is_empty() {
            return None;
        }
        Some((id.to_string(), name, version))
    }

    fn parse_mas_outdated(line: &str) -> Option<(String, String, String, String)> {
        let (id, name, versions) = parse_mas_line(line)?;
        let (current, latest) = versions.split_once("->")?;
        Some((
            id,
            name,
            current.trim().to_string(),
            latest.trim().to_string(),
        ))
    }

    /* --------------------------- app store links -------------------------- */

    /// 把 iTunes 返回的网页链接转成 App Store 深链，保留其中的国家/地区前缀。
    fn app_store_deep_link(url: &str) -> String {
        match url.strip_prefix("https://apps.apple.com/") {
            Some(rest) => format!("macappstore://apps.apple.com/{rest}"),
            None => url.to_string(),
        }
    }

    /// 查不到页面时按已知国家/地区拼一个兜底链接。不带国家前缀时 App Store
    /// 会使用当前账号的店面，应用不在该区上架就会提示“所在国家或地区尚不提供”。
    fn app_store_fallback_link(id: &str, country: Option<&str>) -> String {
        match country.map(str::trim).filter(|country| !country.is_empty()) {
            Some(country) => format!("macappstore://apps.apple.com/{country}/app/id{id}?mt=12"),
            None => format!("macappstore://apps.apple.com/app/id{id}?mt=12"),
        }
    }

    fn normalized_country(code: &str) -> Option<String> {
        (code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()))
            .then(|| code.to_ascii_lowercase())
    }

    /// 解析 `mas config` 输出里的 `store` 行，例如 `store ▁▁ CN`。
    fn parse_mas_storefront(text: &str) -> Option<String> {
        text.lines().find_map(|line| {
            let mut parts = line.split_whitespace();
            if parts.next()? != "store" {
                return None;
            }
            normalized_country(parts.last()?)
        })
    }

    /// 从 `defaults read -g AppleLocale` 的结果（如 `zh_CN`）取地区代码。
    fn parse_locale_region(text: &str) -> Option<String> {
        let (_, region) = text.trim().rsplit_once(['_', '-'])?;
        normalized_country(region)
    }

    /// 读取本机 App Store 店面。优先用 mas（反映真实账号所在区），
    /// 没有 mas 时退回系统地区设置。
    fn current_storefront(mas: Option<&Path>) -> Option<String> {
        if let Some(mas) = mas {
            if let Ok(output) = command(mas).arg("config").output() {
                if output.status.success() {
                    if let Some(country) =
                        parse_mas_storefront(&String::from_utf8_lossy(&output.stdout))
                    {
                        return Some(country);
                    }
                }
            }
        }
        let output = command("defaults")
            .args(["read", "-g", "AppleLocale"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_locale_region(&String::from_utf8_lossy(&output.stdout))
    }

    /// 批量查询应用页面链接：先按本机店面查，找不到的再试美区、中区，
    /// 让只在特定国家上架的应用也能打开正确页面的深链。
    async fn resolve_app_store_links(
        ids: &[String],
        storefront: Option<&str>,
    ) -> HashMap<String, String> {
        let mut pending: Vec<String> = ids
            .iter()
            .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
            .cloned()
            .collect();
        pending.sort();
        pending.dedup();
        if pending.is_empty() {
            return HashMap::new();
        }

        let mut countries = Vec::new();
        if let Some(country) = storefront
            .map(str::trim)
            .filter(|country| !country.is_empty())
        {
            countries.push(country.to_ascii_lowercase());
        }
        for country in ["us", "cn"] {
            if !countries.iter().any(|item| item == country) {
                countries.push(country.into());
            }
        }

        let client = http_client(SCAN_HTTP_TIMEOUT);
        let mut links = HashMap::new();
        for country in countries {
            if pending.is_empty() {
                break;
            }
            for chunk in pending.chunks(40) {
                let id_list = chunk.join(",");
                let Ok(url) = reqwest::Url::parse_with_params(
                    "https://itunes.apple.com/lookup",
                    &[("id", id_list.as_str()), ("country", country.as_str())],
                ) else {
                    continue;
                };
                let Ok(response) = client.get(url).send().await else {
                    continue;
                };
                let Ok(response) = response.error_for_status() else {
                    continue;
                };
                let Ok(value) = response.json::<serde_json::Value>().await else {
                    continue;
                };
                let Some(results) = value["results"].as_array() else {
                    continue;
                };
                for item in results {
                    let Some(id) = item["trackId"]
                        .as_u64()
                        .map(|id| id.to_string())
                        .or_else(|| item["trackId"].as_str().map(str::to_string))
                    else {
                        continue;
                    };
                    let Some(view_url) = item["trackViewUrl"]
                        .as_str()
                        .filter(|url| url.starts_with("https://apps.apple.com/"))
                    else {
                        continue;
                    };
                    links
                        .entry(id)
                        .or_insert_with(|| app_store_deep_link(view_url));
                }
            }
            pending.retain(|id| !links.contains_key(id));
        }
        links
    }

    /// App Store 联网查询结果（iTunes Lookup API）。
    struct AppStoreLookup {
        track_id: Option<String>,
        version: Option<String>,
        view_url: Option<String>,
    }

    fn parse_app_store_lookup(value: &serde_json::Value) -> Option<AppStoreLookup> {
        let result = value["results"].as_array()?.first()?;
        let version = result["version"]
            .as_str()
            .map(str::trim)
            .filter(|version| !version.is_empty())
            .map(str::to_string);
        let track_id = result["trackId"]
            .as_u64()
            .map(|id| id.to_string())
            .or_else(|| result["trackId"].as_str().map(str::to_string));
        let view_url = result["trackViewUrl"]
            .as_str()
            .filter(|url| url.starts_with("https://"))
            .map(str::to_string);
        if version.is_none() && track_id.is_none() {
            return None;
        }
        Some(AppStoreLookup {
            track_id,
            version,
            view_url,
        })
    }

    /// 查询 App Store 上架版本。先查中国区再查美国区，覆盖只在单一区上架的应用。
    async fn fetch_app_store_lookup(
        client: &reqwest::Client,
        bundle_id: &str,
    ) -> Result<Option<AppStoreLookup>, String> {
        let mut last_error = None;
        let mut succeeded = false;
        for country in ["cn", "us"] {
            let url = reqwest::Url::parse_with_params(
                "https://itunes.apple.com/lookup",
                &[("bundleId", bundle_id), ("country", country)],
            )
            .map_err(|err| err.to_string())?;
            match client.get(url).send().await {
                Ok(response) => match response.error_for_status() {
                    Ok(response) => match response.json::<serde_json::Value>().await {
                        Ok(value) => {
                            succeeded = true;
                            if let Some(lookup) = parse_app_store_lookup(&value) {
                                return Ok(Some(lookup));
                            }
                        }
                        Err(err) => last_error = Some(err.to_string()),
                    },
                    Err(err) => last_error = Some(err.to_string()),
                },
                Err(err) => last_error = Some(err.to_string()),
            }
        }
        if succeeded {
            Ok(None)
        } else {
            Err(last_error.unwrap_or_else(|| "无法连接 App Store".into()))
        }
    }

    /// mas 没收录的 App Store 应用：按 bundle id 查询 App Store 版本。
    /// 这类应用无法保证 `mas upgrade` 能定位到，因此只提供 App Store 页面入口。
    async fn appstore_lookup_entry(app: MacApp, mas_available: bool) -> AppUpdateEntry {
        let path = app.path.to_string_lossy().to_string();
        let lookup = if app.bundle_id.is_empty() {
            Ok(None)
        } else {
            fetch_app_store_lookup(&http_client(SCAN_HTTP_TIMEOUT), &app.bundle_id).await
        };

        let (lookup, latest_version, update_available, note) = match lookup {
            Ok(Some(lookup)) => {
                let latest_version = lookup.version.clone();
                let update_available = latest_version
                    .as_deref()
                    .map(|version| version_is_newer(version, &app.version))
                    .unwrap_or(false);
                let note = if update_available {
                    Some(if mas_available {
                        "mas 未列出该应用，请通过 App Store 页面更新".into()
                    } else {
                        "App Store 应用，可打开 App Store 页面更新；安装 mas 后可自动更新".into()
                    })
                } else {
                    None
                };
                (Some(lookup), latest_version, update_available, note)
            }
            Ok(None) => (
                None,
                None,
                false,
                Some(if mas_available {
                    "App Store 应用，未在 mas 列表中找到，无法检查更新".into()
                } else {
                    "App Store 应用，安装 mas 后可检查更新".into()
                }),
            ),
            Err(err) => (
                None,
                None,
                false,
                Some(if mas_available {
                    format!("无法从 App Store 查询更新信息：{err}")
                } else {
                    "App Store 应用，安装 mas 后可检查更新".into()
                }),
            ),
        };

        let homepage = lookup.as_ref().and_then(|lookup| {
            lookup
                .view_url
                .as_deref()
                .map(app_store_deep_link)
                .or_else(|| {
                    lookup
                        .track_id
                        .as_deref()
                        .map(|id| app_store_fallback_link(id, None))
                })
        });
        let source = lookup
            .as_ref()
            .and_then(|lookup| lookup.track_id.clone())
            .unwrap_or_else(|| app.bundle_id.clone());

        AppUpdateEntry {
            id: format!("appstore:{path}"),
            name: app.name,
            path: Some(path),
            channel: "appstore".into(),
            installed_version: app.version,
            latest_version,
            update_available,
            can_update: false,
            source,
            homepage,
            note,
            signature: Some(signature_info(&app.signature)),
        }
    }

    pub fn upgrade_mas(target: &AppUpdateTarget) -> Result<String, String> {
        if target.source.is_empty()
            || !target
                .source
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            return Err("无效的 App Store 应用 ID".into());
        }
        let mas = discover_tool("mas").ok_or_else(|| "未检测到 mas 命令行工具".to_string())?;
        run_tool(&mas, &["upgrade", &target.source])
    }

    /* ------------------------------- sparkle ------------------------------ */

    async fn fetch_appcast(client: &reqwest::Client, url: &str) -> anyhow::Result<SparkleRelease> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            anyhow::bail!("更新源不是 HTTP 地址");
        }
        let response = client
            .get(url)
            .header("Accept", "application/xml, text/xml, */*")
            .send()
            .await?
            .error_for_status()?;
        let bytes = read_limited(response, 4 * 1024 * 1024).await?;
        let text = String::from_utf8_lossy(&bytes);
        parse_appcast(&text).ok_or_else(|| anyhow::anyhow!("更新源中没有可用的版本信息"))
    }

    async fn read_limited(response: reqwest::Response, max: usize) -> anyhow::Result<Vec<u8>> {
        let mut stream = response.bytes_stream();
        let mut data = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if data.len() + chunk.len() > max {
                anyhow::bail!("更新源返回内容过大");
            }
            data.extend_from_slice(&chunk);
        }
        Ok(data)
    }

    pub async fn install_sparkle(
        app: &tauri::AppHandle,
        target: &AppUpdateTarget,
    ) -> Result<String, String> {
        let path = target
            .path
            .as_deref()
            .filter(|path| path.ends_with(".app"))
            .ok_or_else(|| "无效的应用路径".to_string())?;
        let app_path = PathBuf::from(path);
        if !app_path.is_dir() {
            return Err("应用已不存在，请刷新列表".into());
        }

        let info = plist::Value::from_file(app_path.join("Contents/Info.plist"))
            .map_err(|err| format!("读取应用信息失败：{err}"))?;
        let dict = info
            .as_dictionary()
            .ok_or_else(|| "应用信息格式不正确".to_string())?;
        let get = |key: &str| {
            dict.get(key)
                .and_then(|value| value.as_string())
                .unwrap_or("")
                .to_string()
        };
        let installed = {
            let short = get("CFBundleShortVersionString");
            if short.is_empty() {
                get("CFBundleVersion")
            } else {
                short
            }
        };
        let installed_build = get("CFBundleVersion");
        let feed_url = get("SUFeedURL");
        if !(feed_url.starts_with("https://") || feed_url.starts_with("http://")) {
            return Err("该应用没有可用的更新源".into());
        }

        let client = http_client(SCAN_HTTP_TIMEOUT);
        let release = fetch_appcast(&client, &feed_url)
            .await
            .map_err(|err| err.to_string())?;
        if !release_is_newer(&release, &installed, &installed_build) {
            return Err("已经是最新版本，请刷新列表".into());
        }
        let url = release
            .download_url
            .clone()
            .ok_or_else(|| "更新包没有下载地址".to_string())?;
        let kind = release
            .package_kind()
            .ok_or_else(|| "更新包格式暂不支持自动安装".to_string())?;

        let dir = std::env::temp_dir().join(format!("mole-app-updates-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|err| format!("创建临时目录失败：{err}"))?;
        let archive = dir.join(format!(
            "{}{}",
            sanitize_file_name(&target.name),
            kind.extension()
        ));

        let download_client = http_client(DOWNLOAD_TIMEOUT);
        let downloaded = download_to(&download_client, &url, &archive, app, target).await;
        if let Err(err) = downloaded {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(err);
        }

        let app_path_for_install = app_path.clone();
        let archive_for_install = archive.clone();
        let installed = tauri::async_runtime::spawn_blocking(move || {
            install_downloaded(&app_path_for_install, &archive_for_install, kind)
        })
        .await
        .map_err(|err| err.to_string());
        let _ = std::fs::remove_dir_all(&dir);
        installed?
    }

    async fn download_to(
        client: &reqwest::Client,
        url: &str,
        dest: &Path,
        app: &tauri::AppHandle,
        target: &AppUpdateTarget,
    ) -> Result<(), String> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err("更新包地址无效".into());
        }
        let response = client
            .get(url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await
            .map_err(|err| format!("下载更新包失败：{err}"))?
            .error_for_status()
            .map_err(|err| format!("下载更新包失败：{err}"))?;
        let total = response.content_length();
        let mut file =
            std::fs::File::create(dest).map_err(|err| format!("创建临时文件失败：{err}"))?;
        let mut stream = response.bytes_stream();
        let mut downloaded = 0u64;
        let started = std::time::Instant::now();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|err| format!("读取更新包失败：{err}"))?;
            file.write_all(&chunk)
                .map_err(|err| format!("写入更新包失败：{err}"))?;
            downloaded += chunk.len() as u64;
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            let _ = app.emit(
                APP_UPDATE_PROGRESS,
                AppUpdateProgress {
                    id: target.id.clone(),
                    name: target.name.clone(),
                    downloaded,
                    total,
                    bytes_per_sec: (downloaded as f64 / elapsed) as u64,
                },
            );
        }
        Ok(())
    }

    fn install_downloaded(
        app_path: &Path,
        archive: &Path,
        kind: PackageKind,
    ) -> Result<String, String> {
        let preferred = app_path
            .file_stem()
            .and_then(|name| name.to_str())
            .map(str::to_string);

        match kind {
            PackageKind::Zip => {
                let unpacked = archive.with_extension("unpacked");
                prepare_unpack_dir(&unpacked)?;
                let output = command("ditto")
                    .args(["-x", "-k"])
                    .arg(archive)
                    .arg(&unpacked)
                    .output()
                    .map_err(|err| format!("解压更新包失败：{err}"))?;
                if !output.status.success() {
                    let _ = std::fs::remove_dir_all(&unpacked);
                    return Err(format!(
                        "解压更新包失败：{}",
                        strip_ansi(&String::from_utf8_lossy(&output.stderr)).trim()
                    ));
                }
                install_from_dir(app_path, &unpacked, preferred.as_deref())
            }
            PackageKind::Tar => {
                let unpacked = archive.with_extension("unpacked");
                prepare_unpack_dir(&unpacked)?;
                let output = command("tar")
                    .arg("-xf")
                    .arg(archive)
                    .arg("-C")
                    .arg(&unpacked)
                    .output()
                    .map_err(|err| format!("解压更新包失败：{err}"))?;
                if !output.status.success() {
                    let _ = std::fs::remove_dir_all(&unpacked);
                    return Err(format!(
                        "解压更新包失败：{}",
                        strip_ansi(&String::from_utf8_lossy(&output.stderr)).trim()
                    ));
                }
                install_from_dir(app_path, &unpacked, preferred.as_deref())
            }
            PackageKind::Dmg => {
                let mounted = crate::macos_install::mount_dmg(archive)
                    .map_err(|err| format!("挂载更新包失败：{err}"))?;
                let source = crate::macos_install::find_app_prefer(
                    &mounted.mount_point,
                    preferred.as_deref(),
                )
                .ok_or_else(|| "更新包中未找到 .app".to_string())?;
                let result = replace_installed(app_path, &source);
                drop(mounted);
                result
            }
            PackageKind::Pkg => {
                command("open")
                    .arg(archive)
                    .spawn()
                    .map_err(|err| format!("无法打开安装程序：{err}"))?;
                Ok("已打开安装程序，请按提示完成安装".into())
            }
        }
    }

    fn prepare_unpack_dir(path: &Path) -> Result<(), String> {
        let _ = std::fs::remove_dir_all(path);
        std::fs::create_dir_all(path).map_err(|err| format!("创建解压目录失败：{err}"))
    }

    fn install_from_dir(
        app_path: &Path,
        unpacked: &Path,
        preferred: Option<&str>,
    ) -> Result<String, String> {
        let result = crate::macos_install::find_app_prefer(unpacked, preferred)
            .ok_or_else(|| "更新包中未找到 .app".to_string())
            .and_then(|source| replace_installed(app_path, &source));
        let _ = std::fs::remove_dir_all(unpacked);
        result
    }

    fn replace_installed(app_path: &Path, source: &Path) -> Result<String, String> {
        crate::macos_install::verify_same_signer(source, app_path)
            .map_err(|err| format!("签名校验失败：{err}"))?;

        // Sparkle 要求应用先退出；只结束执行文件位于该 .app 内的进程。
        let pids = running_pids(app_path);
        if !pids.is_empty() {
            let remaining = terminate_processes(&pids);
            if !remaining.is_empty() {
                return Err("无法退出应用，请手动退出后重试".into());
            }
        }

        crate::macos_install::replace_bundle(source, app_path)
            .map_err(|err| format!("替换应用失败：{err}"))?;
        let _ = command("touch").arg(app_path).status();
        Ok("应用已更新".into())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_mas_lines() {
            assert_eq!(
                parse_mas_line("497799835  Xcode (15.0)"),
                Some(("497799835".into(), "Xcode".into(), "15.0".into()))
            );
            assert_eq!(
                parse_mas_outdated("497799835  Xcode (15.0 -> 15.1)"),
                Some((
                    "497799835".into(),
                    "Xcode".into(),
                    "15.0".into(),
                    "15.1".into()
                ))
            );
            assert_eq!(parse_mas_line("Warning: something"), None);
        }

        #[test]
        fn parses_storefront_and_locale() {
            let config = "mas ▁▁ 7.0.0\nstore ▁▁ CN\nregion ▁ CN\n";
            assert_eq!(parse_mas_storefront(config).as_deref(), Some("cn"));
            assert_eq!(parse_mas_storefront("nothing here"), None);

            assert_eq!(parse_locale_region("zh_CN\n").as_deref(), Some("cn"));
            assert_eq!(parse_locale_region("en-US").as_deref(), Some("us"));
            assert_eq!(parse_locale_region("zh-Hans_CN").as_deref(), Some("cn"));
            assert_eq!(parse_locale_region("zh"), None);
        }

        #[test]
        fn builds_storefront_aware_app_store_links() {
            assert_eq!(
                app_store_deep_link("https://apps.apple.com/cn/app/xcode/id497799835?mt=12&uo=4"),
                "macappstore://apps.apple.com/cn/app/xcode/id497799835?mt=12&uo=4"
            );
            assert_eq!(
                app_store_fallback_link("497799835", Some("us")),
                "macappstore://apps.apple.com/us/app/id497799835?mt=12"
            );
            assert_eq!(
                app_store_fallback_link("497799835", None),
                "macappstore://apps.apple.com/app/id497799835?mt=12"
            );
        }

        #[test]
        fn parses_app_store_lookup() {
            let value = serde_json::json!({
                "resultCount": 1,
                "results": [{
                    "trackId": 497799835,
                    "version": "15.1",
                    "trackViewUrl": "https://apps.apple.com/app/id497799835"
                }]
            });
            let lookup = parse_app_store_lookup(&value).expect("lookup");
            assert_eq!(lookup.track_id.as_deref(), Some("497799835"));
            assert_eq!(lookup.version.as_deref(), Some("15.1"));
            assert_eq!(
                lookup.view_url.as_deref(),
                Some("https://apps.apple.com/app/id497799835")
            );

            let empty = serde_json::json!({ "resultCount": 0, "results": [] });
            assert!(parse_app_store_lookup(&empty).is_none());
        }
    }
}

/* -------------------------------- windows ------------------------------ */

#[cfg(windows)]
mod windows {
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};
    use std::process::Stdio;

    use crate::commands::apps::windows::raw_apps;
    use crate::util::{command, strip_ansi};

    use super::*;

    pub fn scan() -> AppUpdateScan {
        let mut warnings = Vec::new();
        let registry = raw_apps();
        let winget_path = discover_winget();
        let winget_available = winget_path.is_some();
        let winget_items = match &winget_path {
            Some(path) => match load_winget(path) {
                Ok(items) => items,
                Err(err) => {
                    warnings.push(format!("读取 winget 软件列表失败：{err}"));
                    Vec::new()
                }
            },
            None => Vec::new(),
        };

        let mut winget_by_name: HashMap<String, usize> = HashMap::new();
        for (index, item) in winget_items.iter().enumerate() {
            winget_by_name
                .entry(normalize_name(&item.name))
                .or_insert(index);
        }

        let mut entries = Vec::new();
        let mut used = HashSet::new();
        let mut seen_registry = HashSet::new();

        for app in &registry {
            let key = normalize_name(&app.name);
            if !seen_registry.insert(key.clone()) {
                continue;
            }
            let matched = winget_by_name
                .get(&key)
                .copied()
                .filter(|index| !used.contains(index));
            if let Some(index) = matched {
                let item = &winget_items[index];
                used.insert(index);
                let current = if app.version.is_empty() {
                    item.version.as_str()
                } else {
                    app.version.as_str()
                };
                let update_available = winget_update_available(item, current);
                entries.push(AppUpdateEntry {
                    id: format!("{}:{}", item.channel(), item.id),
                    name: app.name.clone(),
                    path: if app.location.is_empty() {
                        None
                    } else {
                        Some(app.location.clone())
                    },
                    channel: item.channel().into(),
                    installed_version: current.to_string(),
                    latest_version: if item.available.is_empty() {
                        None
                    } else {
                        Some(item.available.clone())
                    },
                    update_available,
                    can_update: update_available,
                    source: item.id.clone(),
                    homepage: None,
                    note: None,
                    signature: None,
                });
            } else {
                entries.push(AppUpdateEntry {
                    id: format!("manual:{}", app.id),
                    name: app.name.clone(),
                    path: if app.location.is_empty() {
                        None
                    } else {
                        Some(app.location.clone())
                    },
                    channel: "manual".into(),
                    installed_version: app.version.clone(),
                    latest_version: None,
                    update_available: false,
                    can_update: false,
                    source: app.id.clone(),
                    homepage: None,
                    note: if winget_available {
                        Some("未在 winget 中找到该软件".into())
                    } else {
                        Some("需要安装 winget 才能检查更新".into())
                    },
                    signature: None,
                });
            }
        }

        for (index, item) in winget_items.iter().enumerate() {
            if used.contains(&index) {
                continue;
            }
            let update_available = winget_update_available(item, &item.version);
            entries.push(AppUpdateEntry {
                id: format!("{}:{}", item.channel(), item.id),
                name: item.name.clone(),
                path: None,
                channel: item.channel().into(),
                installed_version: item.version.clone(),
                latest_version: if item.available.is_empty() {
                    None
                } else {
                    Some(item.available.clone())
                },
                update_available,
                can_update: update_available,
                source: item.id.clone(),
                homepage: None,
                note: None,
                signature: None,
            });
        }

        entries.sort_by(|a, b| {
            b.update_available
                .cmp(&a.update_available)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let updatable = entries
            .iter()
            .filter(|entry| entry.update_available)
            .count();
        AppUpdateScan {
            apps: entries,
            updatable,
            brew_available: false,
            mas_available: false,
            winget_available,
            warnings,
            platform: platform_name().into(),
            scanned_at: now_rfc3339(),
        }
    }

    fn winget_update_available(item: &WingetItem, current: &str) -> bool {
        if item.available.is_empty() {
            return false;
        }
        current.is_empty() || item.available != current
    }

    fn discover_winget() -> Option<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local).join("Microsoft/WindowsApps/winget.exe"));
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            let pattern = PathBuf::from(program_files)
                .join("WindowsApps/Microsoft.DesktopAppInstaller_*_x64__8wekyb3d8bbwe/winget.exe");
            if let Ok(paths) = glob::glob(&pattern.to_string_lossy()) {
                candidates.extend(paths.filter_map(Result::ok));
            }
        }
        candidates.push(PathBuf::from(r"C:\Program Files\WindowsApps\winget.exe"));
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path) {
                candidates.push(dir.join("winget.exe"));
            }
        }
        candidates.into_iter().find(|path| path.is_file())
    }

    fn load_winget(path: &Path) -> Result<Vec<WingetItem>, String> {
        let mut last_error = None;
        for include_unknown in [true, false] {
            match run_winget_list(path, include_unknown) {
                Ok(text) => match parse_winget_table(&text) {
                    Ok(items) => return Ok(items),
                    Err(err) => last_error = Some(err),
                },
                Err(err) => last_error = Some(err),
            }
        }
        Err(last_error.unwrap_or_else(|| "无法读取 winget 软件列表".into()))
    }

    fn run_winget_list(path: &Path, include_unknown: bool) -> Result<String, String> {
        let mut args = vec![
            "list".to_string(),
            "--accept-source-agreements".to_string(),
            "--disable-interactivity".to_string(),
        ];
        if include_unknown {
            args.push("--include-unknown".to_string());
        }
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = command(path)
            .args(&borrowed)
            .stdin(Stdio::null())
            .output()
            .map_err(|err| format!("无法执行 winget：{err}"))?;
        let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout));
        let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
        if output.status.success() {
            Ok(if stdout.trim().is_empty() {
                stderr
            } else {
                stdout
            })
        } else {
            let combined = if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            };
            Err(tail(&combined, 2000))
        }
    }

    pub fn upgrade_winget(target: &AppUpdateTarget) -> Result<String, String> {
        if !is_valid_winget_id(&target.source) {
            return Err("无效的 winget 软件 ID".into());
        }
        let winget = discover_winget().ok_or_else(|| "未检测到 winget".to_string())?;
        let mut args = vec![
            "upgrade".to_string(),
            "--id".to_string(),
            target.source.clone(),
            "--exact".to_string(),
            "--silent".to_string(),
            "--accept-package-agreements".to_string(),
            "--accept-source-agreements".to_string(),
            "--disable-interactivity".to_string(),
        ];
        if target.channel == "store" {
            args.push("--source".to_string());
            args.push("msstore".to_string());
        }
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = command(&winget)
            .args(&borrowed)
            .stdin(Stdio::null())
            .output()
            .map_err(|err| format!("无法执行 winget：{err}"))?;
        let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout));
        let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
        if output.status.success() {
            Ok(if stdout.trim().is_empty() {
                stderr
            } else {
                stdout
            })
        } else {
            let combined = if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            };
            Err(if combined.trim().is_empty() {
                format!("winget 退出码 {}", output.status)
            } else {
                tail(&combined, 2000)
            })
        }
    }
}

/* --------------------------------- tests ------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison_handles_non_semver() {
        assert!(version_is_newer("1.2.10", "1.2.9"));
        assert!(version_is_newer("2024.1", "2023.12"));
        assert!(version_is_newer("v2.0.0", "1.9.9"));
        assert!(!version_is_newer("1.0", "1.0.0"));
        assert!(!version_is_newer("1.0.0", "1.0.1"));
        assert!(!version_is_newer("", "1.0"));
    }

    #[test]
    fn name_normalization_matches_channels() {
        assert_eq!(normalize_name("Google Chrome.app"), "googlechrome");
        assert_eq!(normalize_name("google-chrome"), "googlechrome");
        assert_eq!(normalize_name("Visual Studio Code"), "visualstudiocode");
    }

    #[test]
    fn parses_localized_winget_table_by_column_positions() {
        let header = format!(
            "{:<40} {:<32} {:<14} {:<14} {:<8}",
            "名称", "ID", "版本", "可用", "源"
        );
        let separator = format!(
            "{:<40} {:<32} {:<14} {:<14} {:<8}",
            "-".repeat(40),
            "-".repeat(32),
            "-".repeat(14),
            "-".repeat(14),
            "-".repeat(8)
        );
        let firefox = format!(
            "{:<40} {:<32} {:<14} {:<14} {:<8}",
            "Mozilla Firefox", "Mozilla.Firefox", "120.0", "121.0", "winget"
        );
        let notepad = format!(
            "{:<40} {:<32} {:<14} {:<14} {:<8}",
            "Notepad++", "Notepad++.Notepad++", "8.6.4", "", "winget"
        );
        let text = format!("{header}\n{separator}\n{firefox}\n{notepad}\n");

        let items = parse_winget_table(&text).expect("parse");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "Mozilla Firefox");
        assert_eq!(items[0].id, "Mozilla.Firefox");
        assert_eq!(items[0].version, "120.0");
        assert_eq!(items[0].available, "121.0");
        assert_eq!(items[0].channel(), "winget");
        assert_eq!(items[1].id, "Notepad++.Notepad++");
        assert!(items[1].available.is_empty());
    }

    #[test]
    fn parses_sparkle_appcast() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <item>
      <title>Version 1.9.0</title>
      <sparkle:shortVersionString>1.9.0</sparkle:shortVersionString>
      <sparkle:version>190</sparkle:version>
      <enclosure url="https://example.com/app-1.9.0.zip" length="1" type="application/zip" sparkle:version="190" />
    </item>
    <item>
      <title>Version 1.10.0</title>
      <sparkle:shortVersionString>1.10.0</sparkle:shortVersionString>
      <sparkle:version>200</sparkle:version>
      <enclosure url="https://example.com/app-1.10.0.dmg?download=1" length="2" type="application/x-apple-diskimage" />
    </item>
    <item>
      <title>Version 2.0.0 beta</title>
      <sparkle:shortVersionString>2.0.0</sparkle:shortVersionString>
      <sparkle:channel>beta</sparkle:channel>
      <enclosure url="https://example.com/beta.zip" length="3" type="application/zip" />
    </item>
  </channel>
</rss>"#;
        let release = parse_appcast(xml).expect("parse appcast");
        assert_eq!(release.latest_version().as_deref(), Some("1.10.0"));
        assert_eq!(
            release.download_url.as_deref(),
            Some("https://example.com/app-1.10.0.dmg?download=1")
        );
        assert!(matches!(release.package_kind(), Some(PackageKind::Dmg)));
    }

    #[test]
    fn parses_sparkle_versions_from_enclosure_attributes() {
        let xml = r#"<rss version="2.0"><channel><item>
      <enclosure url="https://example.com/TablePlus.dmg" type="application/octet-stream" sparkle:version="840" sparkle:shortVersionString="27.2.0" />
      <title>Build 840</title>
    </item></channel></rss>"#;
        let release = parse_appcast(xml).expect("parse appcast");
        assert_eq!(release.latest_version().as_deref(), Some("27.2.0"));
        assert!(!release_is_newer(&release, "27.2.0", "840"));
        assert!(release_is_newer(&release, "27.1.0", "839"));
        assert!(matches!(release.package_kind(), Some(PackageKind::Dmg)));
    }

    #[test]
    fn sparkle_build_only_feeds_compare_build_numbers() {
        let xml = r#"<rss version="2.0"><channel><item>
      <title>Build 841</title>
      <sparkle:version>841</sparkle:version>
      <enclosure url="https://example.com/app.zip" type="application/zip" />
    </item></channel></rss>"#;
        let release = parse_appcast(xml).expect("parse appcast");
        assert_eq!(release.latest_version().as_deref(), Some("841"));
        // 构建号不能和短版本号混用，否则 841 > 1.0 会误判为有更新。
        assert!(!release_is_newer(&release, "1.0", "841"));
        assert!(release_is_newer(&release, "1.0", "840"));
    }

    #[test]
    fn rejects_invalid_winget_ids() {
        assert!(is_valid_winget_id("Microsoft.VisualStudioCode"));
        assert!(is_valid_winget_id("{AC76BA86-7AD7-1033-7B44-AC0F074E4100}"));
        assert!(!is_valid_winget_id(""));
        assert!(!is_valid_winget_id("bad id"));
        assert!(!is_valid_winget_id("bad;id"));
    }
}

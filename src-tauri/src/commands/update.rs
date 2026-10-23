//! 应用更新：检查 GitHub Releases、经国内镜像下载安装包并启动安装程序。

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _};
use futures_util::StreamExt as _;
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter as _};

const GITHUB_OWNER: &str = "yu1ec";
const GITHUB_REPO: &str = "mole-desktop";
const USER_AGENT: &str = "mole-desktop";

/// GitHub Release 资源镜像（前缀 + 原始 URL），开启加速时按顺序尝试。
const MIRROR_PREFIXES: &[&str] = &[
    "https://ghfast.top/",
    "https://gh-proxy.com/",
    "https://mirror.ghproxy.com/",
];

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const CHECK_TIMEOUT: Duration = Duration::from_secs(25);
const PROGRESS_EVENT: &str = "update-download-progress";

fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 发布流程上传的稳定安装包名称，按优先级排列。
fn installer_asset_candidates() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["Mole-Desktop-macos-universal.dmg"]
    } else if cfg!(target_os = "windows") {
        &[
            "Mole-Desktop-windows-x86_64.msi",
            "Mole-Desktop-windows-x86_64-setup.exe",
        ]
    } else if cfg!(target_os = "linux") {
        &["Mole-Desktop-linux-x86_64.AppImage"]
    } else {
        &[]
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_url: String,
    pub asset_name: Option<String>,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateDownloadProgress {
    downloaded: u64,
    total: Option<u64>,
    bytes_per_sec: u64,
}

fn is_update_available(current: &str, latest: &str) -> anyhow::Result<bool> {
    let current = Version::parse(current.trim().trim_start_matches('v'))
        .with_context(|| format!("invalid current version `{current}`"))?;
    let latest = Version::parse(latest.trim().trim_start_matches('v'))
        .with_context(|| format!("invalid latest version `{latest}`"))?;
    Ok(latest > current)
}

async fn fetch_latest_release(accelerate: bool) -> anyhow::Result<GitHubRelease> {
    let url = format!("https://api.github.com/repos/{GITHUB_OWNER}/{GITHUB_REPO}/releases/latest");
    let client = http_client(CHECK_TIMEOUT);
    let mut candidates = vec![(url.clone(), true)];
    if accelerate {
        candidates.extend(
            MIRROR_PREFIXES
                .iter()
                .map(|prefix| (format!("{prefix}{url}"), false)),
        );
    }

    let mut last_err: Option<anyhow::Error> = None;
    for (candidate, allow_token) in candidates {
        match fetch_release(&client, &candidate, allow_token).await {
            Ok(release) => return Ok(release),
            Err(err) => {
                eprintln!("update check failed for `{candidate}`: {err:#}");
                last_err = Some(err);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no release URLs to try")))
}

async fn fetch_release(
    client: &reqwest::Client,
    url: &str,
    allow_token: bool,
) -> anyhow::Result<GitHubRelease> {
    let mut request = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json");
    // 不要把 token 泄露给第三方镜像。
    if allow_token {
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            if !token.is_empty() {
                request = request.bearer_auth(token);
            }
        }
    }
    request
        .send()
        .await
        .context("requesting GitHub latest release")?
        .error_for_status()
        .context("GitHub API returned an error")?
        .json()
        .await
        .context("decoding GitHub release JSON")
}

async fn check_update(accelerate: bool) -> anyhow::Result<UpdateCheckResult> {
    let release = fetch_latest_release(accelerate).await?;
    let latest = release.tag_name.trim_start_matches('v').to_string();
    let update_available = is_update_available(current_version(), &latest)?;
    let asset = installer_asset_candidates()
        .iter()
        .find_map(|name| release.assets.iter().find(|asset| asset.name == *name));

    Ok(UpdateCheckResult {
        current_version: current_version().to_string(),
        latest_version: Some(latest),
        update_available,
        release_url: release.html_url,
        asset_name: asset.map(|asset| asset.name.clone()),
        download_url: asset.map(|asset| asset.browser_download_url.clone()),
    })
}

/// 构建下载候选地址：开启加速时优先国内镜像，最后回退到 GitHub 原始地址。
fn candidate_download_urls(github_url: &str, accelerate: bool) -> Vec<String> {
    if !accelerate {
        return vec![github_url.to_string()];
    }
    let mut urls: Vec<String> = MIRROR_PREFIXES
        .iter()
        .map(|prefix| format!("{prefix}{github_url}"))
        .collect();
    urls.push(github_url.to_string());
    urls
}

async fn download_with_mirrors(
    github_url: &str,
    filename: &str,
    accelerate: bool,
    mut on_progress: impl FnMut(u64, Option<u64>, u64),
) -> anyhow::Result<PathBuf> {
    let client = http_client(DOWNLOAD_TIMEOUT);
    let mut last_err: Option<anyhow::Error> = None;
    for url in candidate_download_urls(github_url, accelerate) {
        match download_url_to_temp(&client, &url, filename, &mut on_progress).await {
            Ok(path) => return Ok(path),
            Err(err) => {
                eprintln!("update download failed for `{url}`: {err:#}");
                last_err = Some(err);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no download URLs to try")))
}

async fn download_url_to_temp(
    client: &reqwest::Client,
    url: &str,
    filename: &str,
    on_progress: &mut impl FnMut(u64, Option<u64>, u64),
) -> anyhow::Result<PathBuf> {
    let response = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .with_context(|| format!("requesting `{url}`"))?
        .error_for_status()
        .with_context(|| format!("HTTP error for `{url}`"))?;

    let total = response.content_length();
    let dir = std::env::temp_dir().join("mole-desktop-updates");
    std::fs::create_dir_all(&dir).context("creating update temp dir")?;
    let path = dir.join(filename);

    let mut file =
        std::fs::File::create(&path).with_context(|| format!("creating `{}`", path.display()))?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let started = Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("reading body from `{url}`"))?;
        file.write_all(&chunk)
            .with_context(|| format!("writing `{}`", path.display()))?;
        downloaded += chunk.len() as u64;
        let elapsed = started.elapsed().as_secs_f64().max(0.001);
        on_progress(downloaded, total, (downloaded as f64 / elapsed) as u64);
    }

    Ok(path)
}

fn http_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .expect("reqwest client")
}

fn launch_installer(path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .context("opening DMG installer")?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let spawned = if path.extension().and_then(|ext| ext.to_str()) == Some("msi") {
            std::process::Command::new("msiexec")
                .args(["/i", path.to_str().unwrap_or("")])
                .spawn()
        } else {
            std::process::Command::new(path).spawn()
        };
        spawned.context("launching Windows installer")?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)?;
        std::process::Command::new(path)
            .spawn()
            .context("launching AppImage")?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = path;
        bail!("unsupported platform for installer launch");
    }
}

async fn install_update(app: &AppHandle, accelerate: bool) -> anyhow::Result<()> {
    let check = check_update(accelerate).await?;
    if !check.update_available {
        bail!("already up to date (v{})", check.current_version);
    }
    let url = check
        .download_url
        .context("release has no installer asset for this platform")?;
    let name = check
        .asset_name
        .context("release has no installer asset for this platform")?;

    let path = download_with_mirrors(
        &url,
        &name,
        accelerate,
        |downloaded, total, bytes_per_sec| {
            let _ = app.emit(
                PROGRESS_EVENT,
                UpdateDownloadProgress {
                    downloaded,
                    total,
                    bytes_per_sec,
                },
            );
        },
    )
    .await?;

    launch_installer(&path)
}

#[tauri::command]
pub async fn check_for_update(accelerate: Option<bool>) -> Result<UpdateCheckResult, String> {
    check_update(accelerate.unwrap_or(false))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn download_and_install_update(app: AppHandle, accelerate: bool) -> Result<(), String> {
    install_update(&app, accelerate)
        .await
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_compare() {
        assert!(is_update_available("0.1.0", "0.2.0").unwrap());
        assert!(!is_update_available("0.2.0", "0.2.0").unwrap());
        assert!(!is_update_available("0.3.0", "0.2.0").unwrap());
    }

    #[test]
    fn strip_v_prefix() {
        assert!(is_update_available("v0.1.0", "v0.2.0").unwrap());
    }

    #[test]
    fn candidate_urls_accelerate_order() {
        let github = "https://github.com/yu1ec/mole-desktop/releases/download/v0.2.0/foo.dmg";
        let urls = candidate_download_urls(github, true);
        assert_eq!(urls.len(), MIRROR_PREFIXES.len() + 1);
        assert!(urls[0].starts_with("https://ghfast.top/"));
        assert_eq!(urls.last().map(String::as_str), Some(github));
    }

    #[test]
    fn candidate_urls_no_accelerate() {
        let github = "https://github.com/foo/bar";
        let urls = candidate_download_urls(github, false);
        assert_eq!(urls, vec![github.to_string()]);
    }
}

//! 应用更新：检查 GitHub Releases、经国内镜像下载安装包；macOS 上直接替换
//! `.app` 后自动重启，其他平台回退为启动官方安装程序。

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
const INSTALLING_EVENT: &str = "update-installing";

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallResult {
    /// `restarting`：已原地替换并重启；`installer`：无法自动安装，已打开安装包。
    pub mode: String,
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
        crate::util::command("open")
            .arg(path)
            .spawn()
            .context("opening DMG installer")?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let spawned = if path.extension().and_then(|ext| ext.to_str()) == Some("msi") {
            crate::util::command("msiexec")
                .args(["/i", path.to_str().unwrap_or("")])
                .spawn()
        } else {
            crate::util::command(path).spawn()
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
        crate::util::command(path)
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

/// 下载完成后安装：macOS 直接替换当前 `.app` 并重启，其他平台交给安装程序。
fn install_downloaded(app: &AppHandle, path: &Path) -> anyhow::Result<UpdateInstallResult> {
    #[cfg(target_os = "macos")]
    {
        match macos_install::replace_and_restart(app, path) {
            Ok(()) => unreachable!("replace_and_restart always restarts"),
            Err(err) => {
                // 自动安装失败时保留原有行为：打开 DMG 让用户手动拖拽。
                eprintln!("in-place update failed, falling back to installer: {err:#}");
                launch_installer(path)?;
                Ok(UpdateInstallResult {
                    mode: "installer".into(),
                })
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        launch_installer(path)?;
        Ok(UpdateInstallResult {
            mode: "installer".into(),
        })
    }
}

async fn install_update(app: &AppHandle, accelerate: bool) -> anyhow::Result<UpdateInstallResult> {
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

    install_downloaded(app, &path)
}

#[cfg(target_os = "macos")]
mod macos_install {
    use std::fs;
    use std::process::Command;

    use anyhow::{bail, Context as _};
    use tauri::{AppHandle, Emitter as _};

    use super::{Path, PathBuf, INSTALLING_EVENT};

    struct MountedDmg {
        mount_point: PathBuf,
    }

    impl Drop for MountedDmg {
        fn drop(&mut self) {
            let detached = Command::new("hdiutil")
                .args(["detach", "-quiet"])
                .arg(&self.mount_point)
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
            if !detached {
                let _ = Command::new("hdiutil")
                    .args(["detach", "-quiet", "-force"])
                    .arg(&self.mount_point)
                    .status();
            }
            let _ = fs::remove_dir_all(&self.mount_point);
        }
    }

    /// 挂载 DMG，替换当前 `.app`，然后重启进程。
    /// 成功时进程会退出，不会返回。
    pub fn replace_and_restart(app: &AppHandle, dmg: &Path) -> anyhow::Result<()> {
        let current = current_bundle().context("未在 .app 内运行，无法自动安装")?;
        let destination = install_destination(&current);

        let (mounted, source) = mount_dmg(dmg)?;
        verify_same_signer(&source, &current)?;
        replace_bundle(&source, &destination).with_context(|| {
            format!(
                "替换应用失败（{} → {}）",
                source.display(),
                destination.display()
            )
        })?;
        drop(mounted);
        let _ = fs::remove_file(dmg);
        // 刷新 LaunchServices 缓存，避免 Finder 里仍显示旧版本元数据。
        let _ = Command::new("touch").arg(&destination).status();

        // 给前端一点时间显示“正在安装”，随后自己重启进入新版本。
        let _ = app.emit(INSTALLING_EVENT, ());
        std::thread::sleep(std::time::Duration::from_millis(400));
        if current == destination {
            app.restart();
        }
        // 原本从 DMG 或 AppTranslocation 运行：启动新位置的应用。
        let _ = Command::new("open").arg("-n").arg(&destination).status();
        std::process::exit(0);
    }

    /// 校验新包的签名完整、且签名身份与当前应用一致，避免镜像被替换后
    /// 静默安装其他来源的应用。当前应用未签名（开发构建）时不做身份限制。
    fn verify_same_signer(source: &Path, current: &Path) -> anyhow::Result<()> {
        let output = Command::new("codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(source)
            .output()
            .context("运行 codesign 校验失败")?;
        if !output.status.success() {
            bail!(
                "新版本安装包签名校验失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let Some(current_requirement) = designated_requirement(current) else {
            return Ok(());
        };
        let source_requirement =
            designated_requirement(source).context("新版本安装包没有有效签名")?;
        if source_requirement != current_requirement {
            bail!("新版本的签名与当前应用不一致，已取消自动安装");
        }
        Ok(())
    }

    /// 读取稳定的 designated requirement；ad-hoc 签名含 cdhash，每次构建都会变，视为不可校验。
    fn designated_requirement(app: &Path) -> Option<String> {
        let output = Command::new("codesign")
            .args(["-d", "-r-"])
            .arg(app)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        // codesign 把诊断信息写到 stderr。
        let text = String::from_utf8_lossy(&output.stderr);
        let requirement = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("designated =>"))?
            .trim();
        if requirement.contains("cdhash") {
            return None;
        }
        Some(requirement.to_string())
    }

    fn current_bundle() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        exe.ancestors()
            .find(|path| path.extension().map(|ext| ext == "app").unwrap_or(false))
            .map(Path::to_path_buf)
    }

    /// App Translocation 或直接从 DMG 运行时，更新应落到 `/Applications`。
    fn install_destination(current: &Path) -> PathBuf {
        let text = current.to_string_lossy();
        let relocated = text.contains("/AppTranslocation/") || text.starts_with("/Volumes/");
        if relocated {
            if let Some(name) = current.file_name() {
                return PathBuf::from("/Applications").join(name);
            }
        }
        current.to_path_buf()
    }

    fn mount_dmg(dmg: &Path) -> anyhow::Result<(MountedDmg, PathBuf)> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let mount_point =
            std::env::temp_dir().join(format!("mole-update-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&mount_point).context("创建挂载点失败")?;

        let output = Command::new("hdiutil")
            .args([
                "attach",
                "-nobrowse",
                "-readonly",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(&mount_point)
            .arg(dmg)
            .output()
            .context("运行 hdiutil 挂载安装包失败")?;
        if !output.status.success() {
            let _ = fs::remove_dir_all(&mount_point);
            bail!(
                "挂载安装包失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let mounted = MountedDmg {
            mount_point: mount_point.clone(),
        };
        let source = find_app(&mount_point).context("安装包中未找到 .app")?;
        Ok((mounted, source))
    }

    fn find_app(root: &Path) -> Option<PathBuf> {
        fn scan(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                // 不跟随符号链接：DMG 根目录的 Applications 链接指向本机
                // 已安装应用，跟进去会把旧版本当成更新包。
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_symlink() || !file_type.is_dir() {
                    continue;
                }
                let path = entry.path();
                if path.extension().map(|ext| ext == "app").unwrap_or(false) {
                    found.push(path);
                } else if depth > 0 {
                    scan(&path, depth - 1, found);
                }
            }
        }

        let mut apps = Vec::new();
        scan(root, 1, &mut apps);
        apps.sort();
        apps.iter()
            .find(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().contains("Mole"))
                    .unwrap_or(false)
            })
            .cloned()
            .or_else(|| apps.into_iter().next())
    }

    /// 先在目标目录里复制出完整副本，再用 rename 原子替换，避免半成品。
    fn replace_bundle(source: &Path, destination: &Path) -> anyhow::Result<()> {
        let parent = destination.parent().context("应用路径没有父目录")?;
        let name = destination
            .file_name()
            .and_then(|name| name.to_str())
            .context("应用路径没有名称")?;
        let staging = parent.join(format!(".{name}.staging"));
        let backup = parent.join(format!(".{name}.old"));

        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_dir_all(&backup);

        if !dir_writable(parent) {
            return elevated_replace(source, destination, &staging, &backup);
        }

        copy_bundle(source, &staging).inspect_err(|_| {
            let _ = fs::remove_dir_all(&staging);
        })?;
        if destination.exists() {
            fs::rename(destination, &backup).context("备份旧版本失败")?;
        }
        if let Err(err) = fs::rename(&staging, destination) {
            if backup.exists() {
                let _ = fs::rename(&backup, destination);
            }
            let _ = fs::remove_dir_all(&staging);
            return Err(err).context("替换应用失败");
        }
        let _ = fs::remove_dir_all(&backup);
        Ok(())
    }

    fn dir_writable(dir: &Path) -> bool {
        let probe = dir.join(format!(".mole-write-test-{}", std::process::id()));
        match fs::File::create(&probe) {
            Ok(_) => {
                let _ = fs::remove_file(&probe);
                true
            }
            Err(_) => false,
        }
    }

    fn copy_bundle(source: &Path, destination: &Path) -> anyhow::Result<()> {
        let output = Command::new("ditto")
            .arg(source)
            .arg(destination)
            .output()
            .context("运行 ditto 复制应用失败")?;
        if !output.status.success() {
            bail!(
                "复制新版本失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }

    /// `/Applications` 不可写时（例如以 root 安装），通过系统密码框提权。
    fn elevated_replace(
        source: &Path,
        destination: &Path,
        staging: &Path,
        backup: &Path,
    ) -> anyhow::Result<()> {
        let script = format!(
            "set -e; rm -rf {staging}; ditto {source} {staging}; rm -rf {backup}; \
             if [ -e {destination} ]; then mv {destination} {backup}; fi; \
             mv {staging} {destination}; rm -rf {backup}",
            staging = shell_quote(staging),
            source = shell_quote(source),
            backup = shell_quote(backup),
            destination = shell_quote(destination),
        );
        let apple_script = format!(
            "do shell script {} with administrator privileges",
            applescript_quote(&script)
        );
        let output = Command::new("osascript")
            .args(["-e", &apple_script])
            .output()
            .context("请求管理员权限失败")?;
        if !output.status.success() {
            bail!(
                "自动安装失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }

    fn shell_quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
    }

    fn applescript_quote(text: &str) -> String {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn relocated_apps_install_into_applications() {
            let expected = PathBuf::from("/Applications/Mole Desktop.app");
            assert_eq!(
                install_destination(Path::new(
                    "/private/var/folders/x/AppTranslocation/ABC/d/Mole Desktop.app"
                )),
                expected
            );
            assert_eq!(
                install_destination(Path::new("/Volumes/Mole Desktop/Mole Desktop.app")),
                expected
            );
            assert_eq!(
                install_destination(Path::new("/Applications/Mole Desktop.app")),
                expected
            );
        }

        #[test]
        fn shell_quote_escapes_single_quotes() {
            assert_eq!(
                shell_quote(Path::new("/Applications/Mole Desktop.app")),
                "'/Applications/Mole Desktop.app'"
            );
            assert_eq!(shell_quote(Path::new("/tmp/a'b")), "'/tmp/a'\\''b'");
        }

        #[test]
        fn find_app_does_not_follow_applications_symlink() {
            let root = std::env::temp_dir().join(format!(
                "mole-find-app-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_nanos())
                    .unwrap_or_default()
            ));
            let installed = root.join("installed");
            fs::create_dir_all(installed.join("Mole Desktop.app")).unwrap();
            let real = root.join("Mole Desktop.app");
            fs::create_dir_all(&real).unwrap();
            std::os::unix::fs::symlink(&installed, root.join("Applications")).unwrap();

            assert_eq!(find_app(&root), Some(real));

            let _ = fs::remove_dir_all(&root);
        }
    }
}

#[tauri::command]
pub async fn check_for_update(accelerate: Option<bool>) -> Result<UpdateCheckResult, String> {
    check_update(accelerate.unwrap_or(false))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn download_and_install_update(
    app: AppHandle,
    accelerate: bool,
) -> Result<UpdateInstallResult, String> {
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

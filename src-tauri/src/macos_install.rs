//! macOS 应用包的通用安装工具：挂载 DMG、查找 `.app`、校验签名并原子替换。
//!
//! 自更新（`commands::update`）与第三方应用更新（`commands::app_updates`）
//! 共用这套逻辑，避免维护两份容易出错的替换流程。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context as _};

pub struct MountedDmg {
    pub mount_point: PathBuf,
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

pub fn mount_dmg(dmg: &Path) -> anyhow::Result<MountedDmg> {
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

    Ok(MountedDmg { mount_point })
}

#[cfg(test)]
pub fn find_app(root: &Path) -> Option<PathBuf> {
    find_app_prefer(root, None)
}

/// 在挂载点或解压目录中查找 `.app`。`preferred` 用于优先选中名称匹配的应用，
/// 避免安装包里同时包含主应用和 Helper 时选错。
pub fn find_app_prefer(root: &Path, preferred: Option<&str>) -> Option<PathBuf> {
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
    scan(root, 2, &mut apps);
    apps.sort();
    if let Some(preferred) = preferred {
        if let Some(found) = apps.iter().find(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().contains(preferred))
                .unwrap_or(false)
        }) {
            return Some(found.clone());
        }
    }
    apps.into_iter().next()
}

/// 校验新包的签名完整、且签名身份与当前应用一致，避免镜像被替换后
/// 静默安装其他来源的应用。当前应用未签名（开发构建）时不做身份限制。
pub fn verify_same_signer(source: &Path, current: &Path) -> anyhow::Result<()> {
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
    let source_requirement = designated_requirement(source).context("新版本安装包没有有效签名")?;
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

/* ------------------------------ signatures ------------------------------ */

/// 应用签名状态，用于在软件更新列表里提示来源是否可信。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureState {
    /// 有受信任的开发者 / App Store 签名，且校验通过。
    Official,
    /// ad-hoc 签名：没有开发者身份，常见于破解重签或本地构建。
    Adhoc,
    /// 有签名但校验失败，说明包内容被改动却没有重新签名。
    Damaged,
    /// 完全没有签名。
    Unsigned,
}

impl SignatureState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Adhoc => "adhoc",
            Self::Damaged => "damaged",
            Self::Unsigned => "unsigned",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignatureInfo {
    pub state: SignatureState,
    pub signer: Option<String>,
    pub team_id: Option<String>,
}

struct SignatureDetail {
    adhoc: bool,
    /// Authority 链是否包含 Apple 根证书；自签名证书不算官方签名。
    trusted_chain: bool,
    signer: Option<String>,
    team_id: Option<String>,
}

/// 解析 `codesign -d --verbose=4` 的诊断输出（写在 stderr）。
fn parse_signature_detail(text: &str) -> SignatureDetail {
    let mut adhoc = false;
    let mut trusted_chain = false;
    let mut signer = None;
    let mut team_id = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("Signature=") {
            adhoc = value.trim().eq_ignore_ascii_case("adhoc");
        } else if let Some(value) = line.strip_prefix("Authority=") {
            let value = value.trim();
            if value.starts_with("Apple Root CA") {
                trusted_chain = true;
            }
            if !value.is_empty() && signer.is_none() {
                signer = Some(value.to_string());
            }
        } else if let Some(value) = line.strip_prefix("TeamIdentifier=") {
            let value = value.trim();
            if !value.is_empty() && !value.eq_ignore_ascii_case("not set") {
                team_id = Some(value.to_string());
            }
        }
    }
    SignatureDetail {
        adhoc,
        trusted_chain,
        signer,
        team_id,
    }
}

fn classify_signature(verified: bool, detail: &SignatureDetail) -> SignatureState {
    if !verified {
        SignatureState::Damaged
    } else if detail.adhoc || !detail.trusted_chain {
        SignatureState::Adhoc
    } else {
        SignatureState::Official
    }
}

/// 检查 `.app` 的签名类型与完整性。
///
/// 只校验顶层 bundle 的签名，不使用 `--deep`：递归校验大型应用（如 Xcode）
/// 会让扫描耗时从几秒涨到几十秒。破解重签的典型特征是 ad-hoc 签名，
/// 修改内容后未重签则会在这里的 `--verify` 失败。
/// 同样不使用 `--strict`：它会把 resource fork / Finder 信息当成错误，
/// 误判很多正常应用（尤其是 ad-hoc 签名的开源软件）为签名损坏。
pub fn inspect_signature(app: &Path) -> SignatureInfo {
    let display = Command::new("codesign")
        .args(["-d", "--verbose=4"])
        .arg(app)
        .output();
    let Ok(display) = display else {
        return SignatureInfo {
            state: SignatureState::Unsigned,
            signer: None,
            team_id: None,
        };
    };
    if !display.status.success() {
        return SignatureInfo {
            state: SignatureState::Unsigned,
            signer: None,
            team_id: None,
        };
    }
    let detail = parse_signature_detail(&String::from_utf8_lossy(&display.stderr));

    let verified = Command::new("codesign")
        .arg("--verify")
        .arg(app)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    let state = classify_signature(verified, &detail);

    SignatureInfo {
        state,
        signer: detail.signer,
        team_id: detail.team_id,
    }
}

/// 先在目标目录里复制出完整副本，再用 rename 原子替换，避免半成品。
pub fn replace_bundle(source: &Path, destination: &Path) -> anyhow::Result<()> {
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

pub fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn applescript_quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_developer_id_signature_detail() {
        let text = "Executable=/Applications/Foo.app/Contents/MacOS/Foo\n\
                    Identifier=com.example.foo\n\
                    Authority=Developer ID Application: Example Inc. (ABCDE12345)\n\
                    Authority=Developer ID Certification Authority\n\
                    Authority=Apple Root CA\n\
                    TeamIdentifier=ABCDE12345\n";
        let detail = parse_signature_detail(text);
        assert!(!detail.adhoc);
        assert!(detail.trusted_chain);
        assert_eq!(
            detail.signer.as_deref(),
            Some("Developer ID Application: Example Inc. (ABCDE12345)")
        );
        assert_eq!(detail.team_id.as_deref(), Some("ABCDE12345"));
        assert_eq!(
            classify_signature(true, &detail),
            SignatureState::Official
        );
        assert_eq!(
            classify_signature(false, &detail),
            SignatureState::Damaged
        );
    }

    #[test]
    fn parses_adhoc_signature_detail() {
        let text = "Identifier=com.example.foo\nSignature=adhoc\nTeamIdentifier=not set\n";
        let detail = parse_signature_detail(text);
        assert!(detail.adhoc);
        assert!(!detail.trusted_chain);
        assert!(detail.signer.is_none());
        assert!(detail.team_id.is_none());
        assert_eq!(classify_signature(true, &detail), SignatureState::Adhoc);
    }

    #[test]
    fn self_signed_certificate_is_not_official() {
        let text = "Authority=Example Self Signed\nTeamIdentifier=not set\n";
        let detail = parse_signature_detail(text);
        assert!(!detail.adhoc);
        assert!(!detail.trusted_chain);
        assert_eq!(classify_signature(true, &detail), SignatureState::Adhoc);
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
    fn find_app_prefers_named_bundle_and_ignores_symlinks() {
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

        assert_eq!(find_app(&root), Some(real.clone()));
        assert_eq!(find_app_prefer(&root, Some("Mole")), Some(real));

        let _ = fs::remove_dir_all(&root);
    }
}

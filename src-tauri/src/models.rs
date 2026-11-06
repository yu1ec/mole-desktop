use serde::{Deserialize, Serialize};

/* ------------------------------- cleanup ------------------------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanItem {
    pub id: String,
    pub category: String,
    pub label: String,
    pub description: String,
    pub size: u64,
    pub file_count: u64,
    pub paths: Vec<String>,
    pub risk: String,
    pub requires_admin: bool,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanScan {
    pub items: Vec<CleanItem>,
    pub total_size: u64,
    pub file_count: u64,
    pub free_space: u64,
    pub scanned_at: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub file_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanCrumb {
    pub label: String,
    /// `None` 表示该项的全部根目录。
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanDetail {
    pub item_id: String,
    pub label: String,
    pub current: Option<String>,
    pub current_display: String,
    /// 绝对路径，用于继续下钻。
    pub roots: Vec<String>,
    /// `~` 简写，用于展示。
    pub roots_display: Vec<String>,
    pub crumbs: Vec<CleanCrumb>,
    pub total_size: u64,
    pub file_count: u64,
    pub entries: Vec<CleanEntry>,
    pub truncated: bool,
    pub errors: usize,
}

/// Result of removing a single entry from the detail browser. `restorable` is
/// false for platforms where the app cannot move the item back by itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryTrashResult {
    pub path: String,
    pub restorable: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub freed: u64,
    pub files: u64,
    pub removed: Vec<String>,
    pub errors: Vec<String>,
    pub moved_to_trash: bool,
}

/* ------------------------------- system -------------------------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub total: u64,
    pub free: u64,
    pub used_percent: f32,
    pub file_system: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfo {
    pub received_per_sec: u64,
    pub transmitted_per_sec: u64,
    pub total_received: u64,
    pub total_transmitted: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryInfo {
    pub percent: f32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
    pub exe: String,
    /// 当前正在运行的 Mole Desktop 进程，结束操作会被拒绝。
    pub is_self: bool,
    /// 最近一次采样周期内的下行速率（字节/秒）。
    pub received: u64,
    /// 最近一次采样周期内的上行速率（字节/秒）。
    pub transmitted: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    pub host: String,
    pub platform: String,
    pub os_name: String,
    pub os_version: String,
    pub arch: String,
    pub uptime: u64,
    pub health_score: u8,
    pub cpu_usage: f32,
    pub cpu_cores: usize,
    pub cpu_core_usage: Vec<f32>,
    pub load_avg: [f64; 3],
    pub memory_total: u64,
    pub memory_used: u64,
    pub memory_percent: f32,
    pub swap_total: u64,
    pub swap_used: u64,
    pub disks: Vec<DiskInfo>,
    pub network: NetworkInfo,
    pub battery: Option<BatteryInfo>,
    pub processes: Vec<ProcessInfo>,
}

/* ------------------------------- apps ---------------------------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Leftover {
    pub path: String,
    pub label: String,
    pub size: u64,
    pub kind: String,
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRunningInfo {
    pub running: bool,
    pub is_self: bool,
    pub pids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub path: String,
    pub size: u64,
    pub bundle_id: String,
    pub last_modified: i64,
    pub uninstaller: Option<String>,
}

/* ------------------------------- analyze ------------------------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub file_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeFile {
    pub name: String,
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeReport {
    pub path: String,
    pub name: String,
    pub parent: Option<String>,
    pub total_size: u64,
    pub file_count: u64,
    pub free_space: u64,
    pub entries: Vec<AnalyzeEntry>,
    /// Every directory below `path`, flattened and sorted by size.
    pub subdirs: Vec<AnalyzeEntry>,
    pub large_files: Vec<AnalyzeFile>,
    pub errors: usize,
}

/* ------------------------------- projects ------------------------------ */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArtifact {
    pub path: String,
    pub project: String,
    pub kind: String,
    pub size: u64,
    pub modified: i64,
    pub recent: bool,
}

/* ------------------------------ installers ----------------------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallerFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified: i64,
    pub source: String,
}

/* ------------------------------- optimize ------------------------------ */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeTask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub requires_admin: bool,
    pub danger: bool,
    pub available: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeResult {
    pub applied: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
    pub messages: Vec<String>,
}

/* -------------------------------- history ------------------------------ */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: String,
    pub kind: String,
    pub title: String,
    pub freed: u64,
    pub files: u64,
    pub detail: String,
}

/* ------------------------------- configs ------------------------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPathInfo {
    pub path: String,
    pub display: String,
    pub kind: String,
    /// 这条路径里具体存了什么。
    pub description: String,
    /// 删除风险：`safe` 可自动重建、`caution` 会丢失配置、`keep` 禁止删除。
    pub safety: String,
    pub is_dir: bool,
    pub size: u64,
    pub file_count: u64,
    pub modified: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigTool {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    /// 该工具是否提供 CLI（为 false 时只看应用包是否安装）。
    pub has_cli: bool,
    /// 命令行工具是否仍能在 PATH 中找到。
    pub command_available: bool,
    /// 是否在应用目录中找到对应的桌面应用。
    pub app_installed: bool,
    pub total_size: u64,
    pub total_files: u64,
    pub modified: i64,
    pub paths: Vec<ConfigPathInfo>,
}

/* -------------------------------- brew --------------------------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrewPackage {
    pub name: String,
    pub display_name: String,
    pub full_name: String,
    /// `formula` 或 `cask`
    pub kind: String,
    pub version: String,
    pub latest_version: Option<String>,
    pub outdated: bool,
    pub pinned: bool,
    pub description: String,
    pub homepage: String,
    pub tap: String,
    pub installed_at: Option<i64>,
    pub requested: bool,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrewTap {
    pub name: String,
    pub official: bool,
    pub path: String,
    pub formula_count: usize,
    pub cask_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrewOverview {
    pub available: bool,
    pub brew_path: Option<String>,
    pub version: Option<String>,
    pub prefix: Option<String>,
    pub packages: Vec<BrewPackage>,
    pub taps: Vec<BrewTap>,
    pub formula_count: usize,
    pub cask_count: usize,
    pub outdated_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrewTarget {
    pub name: String,
    /// `formula` 或 `cask`
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrewFailure {
    pub name: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrewResult {
    pub ok: Vec<String>,
    pub failed: Vec<BrewFailure>,
    pub output: String,
}

/* ----------------------------- app updates ----------------------------- */

/// macOS 应用包的代码签名状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSignatureInfo {
    /// `official` / `adhoc` / `damaged` / `unsigned`
    pub state: String,
    /// 签名身份，例如 `Developer ID Application: ...`。
    pub signer: Option<String>,
    pub team_id: Option<String>,
}

/// 单个已安装软件及其更新渠道信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateEntry {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    /// `homebrew` / `appstore` / `sparkle` / `winget` / `store` / `manual`
    pub channel: String,
    pub installed_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub can_update: bool,
    pub source: String,
    pub homepage: Option<String>,
    /// 额外说明，例如“更新前会退出应用”或更新源不可用。
    pub note: Option<String>,
    /// 代码签名状态；仅 macOS 上扫描到本地 `.app` 时才有值。
    pub signature: Option<AppSignatureInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateScan {
    pub apps: Vec<AppUpdateEntry>,
    pub updatable: usize,
    pub brew_available: bool,
    pub mas_available: bool,
    pub winget_available: bool,
    pub warnings: Vec<String>,
    pub platform: String,
    pub scanned_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateTarget {
    pub id: String,
    pub channel: String,
    pub source: String,
    pub name: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateFailure {
    pub name: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateResult {
    pub ok: Vec<String>,
    pub failed: Vec<AppUpdateFailure>,
    pub output: String,
}

/* ------------------------------- privacy ------------------------------- */

/// 残留记录里的单个隐私权限。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyService {
    pub service: String,
    pub label: String,
    pub last_modified: i64,
}

/// 一个应用的隐私权限组。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyLeftover {
    pub id: String,
    pub name: String,
    /// bundle id 或可执行文件路径。
    pub client: String,
    pub client_type: i64,
    /// `user` / `system`，对应记录所在的 TCC 数据库。
    pub scope: String,
    /// 应用或可执行文件的已知路径。
    pub path: Option<String>,
    pub services: Vec<PrivacyService>,
    pub last_modified: i64,
    /// 是否能在废纸篓中找到对应应用。
    pub in_trash: bool,
    /// 对应应用或可执行文件是否已经不存在。
    pub invalid: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyScan {
    pub available: bool,
    pub full_disk_access: bool,
    pub apps: Vec<PrivacyLeftover>,
    pub total_entries: usize,
    pub warnings: Vec<String>,
    pub platform: String,
    pub scanned_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyTarget {
    pub scope: String,
    pub client: String,
    pub client_type: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyCleanResult {
    pub removed_apps: usize,
    pub removed_entries: usize,
    pub failed: Vec<String>,
    pub messages: Vec<String>,
    /// 重置系统级记录时取消了管理员授权（仅部分 macOS 版本需要提权）。
    pub cancelled: bool,
}

/* ------------------------------- startup ------------------------------- */

/// 一条「登录时打开」或「App 后台活动」项目。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupLeftover {
    pub id: String,
    pub name: String,
    /// `login`：登录时打开；`background`：App 后台活动。
    pub kind: String,
    /// 系统里的具体类型：`app` / `login-item` / `agent` / `daemon` / ...
    pub item_type: String,
    pub developer: Option<String>,
    pub bundle_id: Option<String>,
    /// 项目指向的应用或可执行文件路径。
    pub path: Option<String>,
    /// 可以直接删除的 launchd plist；为空表示需要其他清理方式。
    pub plist_path: Option<String>,
    pub requires_admin: bool,
    /// 是否可以直接移除（删除 plist 或删除登录项）。
    pub removable: bool,
    /// 对应应用或可执行文件是否已经不存在。
    pub invalid: bool,
    /// 是否可以通过 launchctl 切换启用状态。
    pub can_toggle: bool,
    /// 当前是否启用；`None` 表示系统没有提供可读状态。
    pub enabled: Option<bool>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupScan {
    pub available: bool,
    pub items: Vec<StartupLeftover>,
    pub warnings: Vec<String>,
    pub platform: String,
    pub scanned_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupTarget {
    pub id: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupCleanResult {
    pub removed_items: usize,
    pub failed: Vec<String>,
    pub messages: Vec<String>,
    /// 用户取消了系统级 plist 所需的管理员授权。
    pub cancelled: bool,
}

/* ------------------------------- settings ------------------------------ */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub purge_roots: Vec<String>,
    pub analyze_root: String,
    pub confirm_before_delete: bool,
    /// 下载更新时优先使用国内镜像。
    #[serde(default = "default_true")]
    pub accelerate_download: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            purge_roots: Vec::new(),
            analyze_root: dirs::home_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            confirm_before_delete: true,
            accelerate_download: true,
        }
    }
}

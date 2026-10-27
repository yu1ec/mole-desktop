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
    /// 命令行工具是否仍能在 PATH 中找到；为 false 时这些目录更可能是残留。
    pub command_available: bool,
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

export interface CleanItem {
  id: string;
  category: string;
  label: string;
  description: string;
  size: number;
  fileCount: number;
  paths: string[];
  risk: "safe" | "caution";
  requiresAdmin: boolean;
  available: boolean;
}

export interface CleanScan {
  items: CleanItem[];
  totalSize: number;
  fileCount: number;
  freeSpace: number;
  scannedAt: string;
  platform: string;
}

export interface CleanEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  fileCount: number;
}

export interface CleanCrumb {
  label: string;
  /** null 表示该项的全部根目录 */
  path: string | null;
}

export interface CleanDetail {
  itemId: string;
  label: string;
  /** 当前浏览的目录；null 表示所有根目录的概览 */
  current: string | null;
  currentDisplay: string;
  roots: string[];
  rootsDisplay: string[];
  crumbs: CleanCrumb[];
  totalSize: number;
  fileCount: number;
  entries: CleanEntry[];
  truncated: boolean;
  errors: number;
}

export interface OperationResult {
  freed: number;
  files: number;
  removed: string[];
  errors: string[];
  movedToTrash: boolean;
}

export interface EntryTrashResult {
  path: string;
  /** 是否可以在本次会话中恢复到原位置 */
  restorable: boolean;
}

export interface DiskInfo {
  name: string;
  mount: string;
  total: number;
  free: number;
  usedPercent: number;
  fileSystem: string;
}

export interface NetworkInfo {
  receivedPerSec: number;
  transmittedPerSec: number;
  totalReceived: number;
  totalTransmitted: number;
}

export interface BatteryInfo {
  percent: number;
  status: string;
}

export interface ProcessInfo {
  pid: number;
  name: string;
  cpu: number;
  memory: number;
  exe: string;
}

export interface SystemSnapshot {
  host: string;
  platform: "macos" | "windows" | "linux";
  osName: string;
  osVersion: string;
  arch: string;
  uptime: number;
  healthScore: number;
  cpuUsage: number;
  cpuCores: number;
  cpuCoreUsage: number[];
  loadAvg: [number, number, number];
  memoryTotal: number;
  memoryUsed: number;
  memoryPercent: number;
  swapTotal: number;
  swapUsed: number;
  disks: DiskInfo[];
  network: NetworkInfo;
  battery: BatteryInfo | null;
  processes: ProcessInfo[];
}

export interface Leftover {
  path: string;
  label: string;
  size: number;
  kind: string;
  recommended: boolean;
}

export interface AppRunningInfo {
  running: boolean;
  isSelf: boolean;
  pids: number[];
}

export interface AppEntry {
  id: string;
  name: string;
  version: string;
  publisher: string;
  path: string;
  size: number;
  bundleId: string;
  lastModified: number;
  uninstaller: string | null;
}

export interface AnalyzeEntry {
  name: string;
  path: string;
  size: number;
  isDir: boolean;
  fileCount: number;
}

export interface AnalyzeFile {
  name: string;
  path: string;
  size: number;
}

export interface AnalyzeReport {
  path: string;
  name: string;
  parent: string | null;
  totalSize: number;
  fileCount: number;
  freeSpace: number;
  entries: AnalyzeEntry[];
  /** 当前目录下所有层级子目录，按占用从大到小排列 */
  subdirs: AnalyzeEntry[];
  largeFiles: AnalyzeFile[];
  errors: number;
}

export interface ProjectArtifact {
  path: string;
  project: string;
  kind: string;
  size: number;
  modified: number;
  recent: boolean;
}

export interface InstallerFile {
  path: string;
  name: string;
  size: number;
  modified: number;
  source: string;
}

export interface OptimizeTask {
  id: string;
  title: string;
  description: string;
  category: string;
  requiresAdmin: boolean;
  danger: boolean;
  available: boolean;
}

export interface OptimizeResult {
  applied: string[];
  skipped: string[];
  failed: string[];
  messages: string[];
}

/** 删除风险：safe 可自动重建、caution 会丢失配置、keep 禁止删除。 */
export type ConfigSafety = "safe" | "caution" | "keep";

export interface ConfigPathInfo {
  path: string;
  display: string;
  kind: string;
  /** 这条路径里具体存了什么。 */
  description: string;
  safety: ConfigSafety;
  isDir: boolean;
  size: number;
  fileCount: number;
  modified: number;
}

export interface ConfigTool {
  id: string;
  name: string;
  description: string;
  category: string;
  /** 该工具是否提供 CLI。 */
  hasCli: boolean;
  /** false 表示命令已不在 PATH 中，这些目录更可能是残留。 */
  commandAvailable: boolean;
  /** 是否在应用目录中找到对应的桌面应用。 */
  appInstalled: boolean;
  totalSize: number;
  totalFiles: number;
  modified: number;
  paths: ConfigPathInfo[];
}

export interface BrewPackage {
  name: string;
  displayName: string;
  fullName: string;
  /** `formula` 或 `cask` */
  kind: string;
  version: string;
  latestVersion: string | null;
  outdated: boolean;
  pinned: boolean;
  description: string;
  homepage: string;
  tap: string;
  installedAt: number | null;
  requested: boolean;
  dependencies: string[];
}

export interface BrewTap {
  name: string;
  official: boolean;
  path: string;
  formulaCount: number;
  caskCount: number;
}

export interface BrewOverview {
  available: boolean;
  brewPath: string | null;
  version: string | null;
  prefix: string | null;
  packages: BrewPackage[];
  taps: BrewTap[];
  formulaCount: number;
  caskCount: number;
  outdatedCount: number;
}

export interface BrewTarget {
  name: string;
  /** `formula` 或 `cask` */
  kind: string;
}

export interface BrewFailure {
  name: string;
  message: string;
}

export interface BrewResult {
  ok: string[];
  failed: BrewFailure[];
  output: string;
}

export interface HistoryEntry {
  id: string;
  timestamp: string;
  kind: string;
  title: string;
  freed: number;
  files: number;
  detail: string;
}

export interface Settings {
  purgeRoots: string[];
  analyzeRoot: string;
  confirmBeforeDelete: boolean;
  accelerateDownload: boolean;
}

export type ThemeMode = "system" | "light" | "dark";

export interface UpdateCheckResult {
  currentVersion: string;
  latestVersion: string | null;
  updateAvailable: boolean;
  releaseUrl: string;
  assetName: string | null;
  downloadUrl: string | null;
}

export interface UpdateDownloadProgress {
  downloaded: number;
  total: number | null;
  bytesPerSec: number;
}

export interface UpdateInstallResult {
  /** `restarting`：已原地替换并重启；`installer`：无法自动安装，已打开安装包。 */
  mode: "restarting" | "installer";
}

export interface AppInfo {
  platform: string;
  home: string;
  version: string;
  dataDir: string;
}

export interface PermissionFolder {
  key: string;
  label: string;
  path: string;
  exists: boolean;
  /** null 表示尚未探测，避免仅打开页面就触发系统授权弹窗 */
  granted: boolean | null;
}

export interface DiskPermissionStatus {
  platform: string;
  supported: boolean;
  fullDiskAccess: boolean;
  /** 是否运行在 .app 包内；开发模式下权限记在终端/IDE 名下 */
  bundled: boolean;
  appPath: string;
  folders: PermissionFolder[];
}

export type PageKey =
  | "dashboard"
  | "clean"
  | "uninstall"
  | "configs"
  | "brew"
  | "analyze"
  | "optimize"
  | "monitor"
  | "projects"
  | "installers"
  | "history"
  | "changelog"
  | "settings";

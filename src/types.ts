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
  | "analyze"
  | "optimize"
  | "monitor"
  | "projects"
  | "installers"
  | "history"
  | "settings";

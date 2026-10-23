import { invoke } from "@tauri-apps/api/core";
import type {
  AnalyzeReport,
  AppEntry,
  AppInfo,
  CleanScan,
  DiskPermissionStatus,
  HistoryEntry,
  InstallerFile,
  Leftover,
  OperationResult,
  OptimizeResult,
  OptimizeTask,
  PermissionFolder,
  ProjectArtifact,
  Settings,
  SystemSnapshot,
  UpdateCheckResult,
} from "./types";

export const api = {
  snapshot: () => invoke<SystemSnapshot>("system_snapshot"),

  scanClean: () => invoke<CleanScan>("scan_clean"),
  runClean: (items: string[]) => invoke<OperationResult>("run_clean", { items }),

  listApps: () => invoke<AppEntry[]>("list_apps"),
  appLeftovers: (appId: string) => invoke<Leftover[]>("app_leftovers", { appId }),
  uninstallApp: (appId: string, leftovers: string[], permanent: boolean) =>
    invoke<OperationResult>("uninstall_app", { appId, leftovers, permanent }),

  analyze: (path: string) => invoke<AnalyzeReport>("analyze_dir", { path }),
  analyzeShortcuts: () => invoke<[string, string][]>("analyze_shortcuts"),

  listInstallers: () => invoke<InstallerFile[]>("list_installers"),
  deleteInstallers: (paths: string[]) =>
    invoke<OperationResult>("delete_installers", { paths }),

  purgeRoots: () => invoke<string[]>("purge_roots"),
  scanProjects: () => invoke<ProjectArtifact[]>("scan_projects"),
  purgeProjects: (paths: string[]) =>
    invoke<OperationResult>("purge_projects", { paths }),

  optimizeTasks: () => invoke<OptimizeTask[]>("list_optimize_tasks"),
  runOptimize: (ids: string[]) => invoke<OptimizeResult>("run_optimize", { ids }),

  history: () => invoke<HistoryEntry[]>("get_history"),
  clearHistory: () => invoke<void>("clear_history"),

  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),

  appInfo: () => invoke<AppInfo>("app_info"),
  checkForUpdate: (accelerate: boolean) =>
    invoke<UpdateCheckResult>("check_for_update", { accelerate }),
  downloadAndInstallUpdate: (accelerate: boolean) =>
    invoke<void>("download_and_install_update", { accelerate }),
  reveal: (path: string) => invoke<void>("reveal_path", { path }),
  openExternal: (path: string) => invoke<void>("open_external", { path }),
  openDataDir: () => invoke<void>("open_data_dir"),

  diskPermission: () => invoke<DiskPermissionStatus>("disk_permission_status"),
  requestFileAccess: () => invoke<PermissionFolder[]>("request_file_access"),
  openPrivacySettings: (pane: "full-disk-access" | "files-and-folders" | "removable-volumes") =>
    invoke<void>("open_privacy_settings", { pane }),
  revealApp: () => invoke<void>("reveal_app"),
  restartApp: () => invoke<void>("restart_app"),
};

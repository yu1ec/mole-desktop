import { invoke } from "@tauri-apps/api/core";
import type {
  AnalyzeReport,
  AppEntry,
  AppInfo,
  AppRunningInfo,
  BrewOverview,
  BrewResult,
  BrewTarget,
  CleanDetail,
  CleanScan,
  ConfigTool,
  DiskPermissionStatus,
  EntryTrashResult,
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
  UpdateInstallResult,
} from "./types";

export const api = {
  snapshot: () => invoke<SystemSnapshot>("system_snapshot"),

  scanClean: () => invoke<CleanScan>("scan_clean"),
  cleanItemDetail: (itemId: string, path: string | null) =>
    invoke<CleanDetail>("clean_item_detail", { itemId, path }),
  deleteCleanEntry: (itemId: string, path: string) =>
    invoke<EntryTrashResult>("delete_clean_entry", { itemId, path }),
  runClean: (items: string[]) => invoke<OperationResult>("run_clean", { items }),

  listApps: () => invoke<AppEntry[]>("list_apps"),
  appRunning: (appId: string) => invoke<AppRunningInfo>("app_running", { appId }),
  appLeftovers: (appId: string) => invoke<Leftover[]>("app_leftovers", { appId }),
  uninstallApp: (appId: string, leftovers: string[], permanent: boolean, quitProcess: boolean) =>
    invoke<OperationResult>("uninstall_app", { appId, leftovers, permanent, quitProcess }),

  scanConfigs: () => invoke<ConfigTool[]>("scan_configs"),
  configDetail: (toolId: string, path: string | null) =>
    invoke<CleanDetail>("config_detail", { toolId, path }),
  deleteConfigEntry: (toolId: string, path: string) =>
    invoke<EntryTrashResult>("delete_config_entry", { toolId, path }),
  deleteConfigPaths: (paths: string[]) =>
    invoke<OperationResult>("delete_config_paths", { paths }),

  brewOverview: () => invoke<BrewOverview>("brew_overview"),
  brewUpdate: () => invoke<BrewResult>("brew_update"),
  brewUpgrade: (targets: BrewTarget[]) => invoke<BrewResult>("brew_upgrade", { targets }),
  brewUpgradeAll: () => invoke<BrewResult>("brew_upgrade_all"),
  brewUninstall: (targets: BrewTarget[], zap: boolean) =>
    invoke<BrewResult>("brew_uninstall", { targets, zap }),
  brewAddTap: (name: string) => invoke<BrewResult>("brew_add_tap", { name }),
  brewRemoveTap: (name: string) => invoke<BrewResult>("brew_remove_tap", { name }),

  analyze: (path: string) => invoke<AnalyzeReport>("analyze_dir", { path }),
  analyzeShortcuts: () => invoke<[string, string][]>("analyze_shortcuts"),

  listInstallers: () => invoke<InstallerFile[]>("list_installers"),
  deleteInstallers: (paths: string[]) =>
    invoke<OperationResult>("delete_installers", { paths }),

  purgeRoots: () => invoke<string[]>("purge_roots"),
  scanProjects: () => invoke<ProjectArtifact[]>("scan_projects"),
  projectArtifactDetail: (path: string, sub: string | null) =>
    invoke<CleanDetail>("project_artifact_detail", { path, sub }),
  deleteProjectEntry: (root: string, path: string) =>
    invoke<EntryTrashResult>("delete_project_entry", { root, path }),
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
    invoke<UpdateInstallResult>("download_and_install_update", { accelerate }),
  reveal: (path: string) => invoke<void>("reveal_path", { path }),
  restoreTrashedEntry: (path: string) => invoke<void>("restore_trashed_entry", { path }),
  openExternal: (path: string) => invoke<void>("open_external", { path }),
  openDataDir: () => invoke<void>("open_data_dir"),

  diskPermission: () => invoke<DiskPermissionStatus>("disk_permission_status"),
  requestFileAccess: () => invoke<PermissionFolder[]>("request_file_access"),
  openPrivacySettings: (pane: "full-disk-access" | "files-and-folders" | "removable-volumes") =>
    invoke<void>("open_privacy_settings", { pane }),
  revealApp: () => invoke<void>("reveal_app"),
  restartApp: () => invoke<void>("restart_app"),
};

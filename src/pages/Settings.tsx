import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Download,
  ExternalLink,
  FolderCog,
  FolderOpen,
  FolderSearch,
  Info,
  Monitor,
  Moon,
  Palette,
  RefreshCw,
  RotateCcw,
  Save,
  ScrollText,
  ShieldAlert,
  ShieldCheck,
  Sun,
  Terminal,
} from "lucide-react";
import { api } from "../api";
import type {
  AppInfo,
  DiskPermissionStatus,
  PageKey,
  Settings,
  ThemeMode,
  UpdateCheckResult,
  UpdateDownloadProgress,
} from "../types";
import {
  Button,
  Card,
  Checkbox,
  ProgressBar,
  SectionTitle,
  Spinner,
  useToast,
} from "../components/ui";
import { PermissionRestartModal, usePermissionRestart } from "../components/permissionRestart";
import { useTheme } from "../theme";
import { formatBytes, formatSpeed } from "../format";

const RELEASES_URL = "https://github.com/yu1ec/mole-desktop/releases/latest";

const THEME_OPTIONS: { value: ThemeMode; label: string; icon: typeof Sun }[] = [
  { value: "system", label: "跟随系统", icon: Monitor },
  { value: "light", label: "亮色", icon: Sun },
  { value: "dark", label: "暗色", icon: Moon },
];

export function SettingsPage({
  initialUpdate = null,
  onNavigate,
}: {
  initialUpdate?: UpdateCheckResult | null;
  onNavigate?: (page: PageKey) => void;
}) {
  const toast = useToast();
  const { mode: themeMode, setMode: setThemeMode } = useTheme();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [rootsText, setRootsText] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [update, setUpdate] = useState<UpdateCheckResult | null>(initialUpdate);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<UpdateDownloadProgress | null>(null);
  const [installPhase, setInstallPhase] = useState<"downloading" | "installing" | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [permission, setPermission] = useState<DiskPermissionStatus | null>(null);
  const [probing, setProbing] = useState(false);
  const [checkingPermission, setCheckingPermission] = useState(false);
  const {
    open: restartPromptOpen,
    close: closeRestartPrompt,
    restart,
    restarting,
    expectRestart,
  } = usePermissionRestart();

  useEffect(() => {
    Promise.all([api.appInfo(), api.settings()])
      .then(([appInfo, stored]) => {
        setInfo(appInfo);
        setSettings(stored);
        setRootsText(stored.purgeRoots.join("\n"));
      })
      .catch((err) => toast(String(err), "error"))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    api
      .diskPermission()
      .then(setPermission)
      .catch(() => {});
  }, []);

  // 后台检查完成得比进入设置页更晚时，补上结果。
  useEffect(() => {
    if (initialUpdate?.updateAvailable) setUpdate(initialUpdate);
  }, [initialUpdate]);

  // 从系统设置切回窗口时静默刷新权限状态。
  useEffect(() => {
    const recheck = () => {
      if (document.visibilityState === "visible") {
        api
          .diskPermission()
          .then(setPermission)
          .catch(() => {});
      }
    };
    window.addEventListener("focus", recheck);
    document.addEventListener("visibilitychange", recheck);
    return () => {
      window.removeEventListener("focus", recheck);
      document.removeEventListener("visibilitychange", recheck);
    };
  }, []);

  const openFullDiskAccessSettings = () => {
    expectRestart();
    api
      .openPrivacySettings("full-disk-access")
      .catch((err) => toast(String(err), "error"));
  };

  const refreshPermission = async () => {
    setCheckingPermission(true);
    try {
      const next = await api.diskPermission();
      setPermission(next);
      toast(
        next.fullDiskAccess
          ? "已检测到完全磁盘访问权限"
          : "仍未检测到完全磁盘访问；若刚刚在系统设置中勾选，请重启应用",
        next.fullDiskAccess ? "success" : "info",
      );
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setCheckingPermission(false);
    }
  };

  const probeFolders = async () => {
    setProbing(true);
    try {
      const folders = await api.requestFileAccess();
      setPermission((current) => (current ? { ...current, folders } : current));
      const denied = folders.filter((folder) => folder.granted === false);
      const granted = folders.filter((folder) => folder.granted === true);
      if (denied.length > 0) {
        toast(
          `未获得 ${denied.map((folder) => folder.label).join("、")} 的访问权限，可在系统设置中开启`,
          "error",
        );
      } else if (granted.length > 0) {
        toast(`已获得 ${granted.map((folder) => folder.label).join("、")} 的访问权限`, "success");
      }
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setProbing(false);
    }
  };

  const save = async () => {
    if (!settings) return;
    setSaving(true);
    try {
      const roots = rootsText
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean);
      await api.saveSettings({ ...settings, purgeRoots: roots });
      setSettings({ ...settings, purgeRoots: roots });
      toast("设置已保存", "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setSaving(false);
    }
  };

  const checkUpdate = async () => {
    if (!settings) return;
    setChecking(true);
    setUpdateError(null);
    try {
      const result = await api.checkForUpdate(settings.accelerateDownload);
      setUpdate(result);
      if (result.updateAvailable && result.latestVersion) {
        toast(`发现新版本 v${result.latestVersion}`, "info");
      } else {
        toast(`已是最新版本 v${result.currentVersion}`, "success");
      }
    } catch (err) {
      setUpdate(null);
      setUpdateError(String(err));
      toast(`检查更新失败：${err}`, "error");
    } finally {
      setChecking(false);
    }
  };

  const installUpdate = async () => {
    if (!settings) return;
    setInstalling(true);
    setProgress(null);
    setInstallPhase("downloading");
    setUpdateError(null);
    let unlisten: (() => void) | undefined;
    let unlistenInstalling: (() => void) | undefined;
    try {
      unlisten = await listen<UpdateDownloadProgress>("update-download-progress", (event) => {
        setProgress(event.payload);
      });
      unlistenInstalling = await listen("update-installing", () => {
        setInstallPhase("installing");
      });
      const result = await api.downloadAndInstallUpdate(settings.accelerateDownload);
      if (result.mode === "installer") {
        toast(
          info?.platform === "macos"
            ? "自动安装未完成，已打开安装包，请拖入「应用程序」完成更新"
            : "安装程序已启动，请按提示完成更新",
          "info",
        );
      }
    } catch (err) {
      setUpdateError(String(err));
      toast("更新失败，可尝试手动下载安装包", "error");
    } finally {
      unlisten?.();
      unlistenInstalling?.();
      setInstalling(false);
      setInstallPhase(null);
      setProgress(null);
    }
  };

  if (loading || !settings || !info) return <Spinner label="正在读取设置…" />;

  const updateHint = updateError
    ? updateError
    : update?.updateAvailable && update.latestVersion
      ? `发现新版本 v${update.latestVersion}，当前 v${update.currentVersion}`
      : update
        ? `已是最新版本 v${update.currentVersion}`
        : "从 GitHub Releases 检查最新版本";
  const progressPercent =
    progress?.total && progress.total > 0
      ? Math.min(100, (progress.downloaded / progress.total) * 100)
      : 0;

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>设置</h1>
          <p>扫描范围、数据目录与应用信息</p>
        </div>
        <Button variant="primary" icon={<Save size={16} />} loading={saving} onClick={save}>
          保存设置
        </Button>
      </header>

      <section>
        <SectionTitle title="外观" hint="切换后立即生效；跟随系统时会随 macOS 外观自动切换" />
        <Card className="settings-card">
          <div className="setting-row theme-setting-row">
            <span className="setting-icon">
              <Palette size={18} />
            </span>
            <div className="setting-body">
              <label>主题</label>
              <p className="small muted">选择亮色、暗色，或跟随系统外观自动切换。</p>
            </div>
            <div className="theme-toggle" role="group" aria-label="主题">
              {THEME_OPTIONS.map(({ value, label, icon: Icon }) => (
                <button
                  key={value}
                  className={`theme-option ${themeMode === value ? "is-active" : ""}`}
                  aria-pressed={themeMode === value}
                  onClick={() => setThemeMode(value)}
                >
                  <Icon size={14} />
                  <span>{label}</span>
                </button>
              ))}
            </div>
          </div>
        </Card>
      </section>

      <section>
        <SectionTitle title="项目清理" hint="只扫描这些目录下的构建产物，每行一个路径" />
        <Card className="settings-card">
          <div className="setting-row">
            <span className="setting-icon">
              <FolderCog size={18} />
            </span>
            <div className="setting-body">
              <label>扫描目录</label>
              <p className="small muted">
                留空时使用默认目录（~/Projects、~/GitHub、~/dev、~/code、~/src 等）。支持 ~ 开头的路径。
              </p>
              <textarea
                className="textarea"
                rows={6}
                value={rootsText}
                onChange={(event) => setRootsText(event.target.value)}
                placeholder={"~/Projects\n~/dev\n~/code"}
              />
            </div>
          </div>
        </Card>
      </section>

      <section>
        <SectionTitle title="磁盘分析" hint="打开磁盘分析时默认加载的目录" />
        <Card className="settings-card">
          <div className="setting-row">
            <span className="setting-icon">
              <FolderOpen size={18} />
            </span>
            <div className="setting-body">
              <label>默认分析目录</label>
              <input
                className="input"
                value={settings.analyzeRoot}
                onChange={(event) => setSettings({ ...settings, analyzeRoot: event.target.value })}
                placeholder={info.home}
              />
            </div>
          </div>
        </Card>
      </section>

      {permission?.supported && (
        <section>
          <SectionTitle title="隐私与权限" hint="扫描磁盘与读取下载、桌面等目录时的系统授权" />
          <Card className="settings-card">
            <div className="setting-row">
              <span className="setting-icon">
                {permission.fullDiskAccess ? <ShieldCheck size={18} /> : <ShieldAlert size={18} />}
              </span>
              <div className="setting-body">
                <label>
                  完全磁盘访问
                  <span className={`badge ${permission.fullDiskAccess ? "badge-ok" : "badge-warn"}`}>
                    {permission.fullDiskAccess ? "已授权" : "未授权"}
                  </span>
                </label>
                <p className="small muted">
                  磁盘分析、项目清理和卸载残留扫描需要读取受保护目录。macOS 不允许应用弹窗申请完全磁盘访问，
                  需要点击下方按钮，在系统设置中手动添加本应用；授权后需重启应用才会生效。
                </p>
                {!permission.bundled && (
                  <p className="small muted">
                    当前为开发模式（未打包），macOS 会把权限记在启动本应用的终端/IDE 名下。
                    请为对应程序授权，或使用打包后的 .app 验证。
                  </p>
                )}
                <div className="path-box" title={permission.appPath}>
                  {permission.appPath}
                </div>
                <div className="permission-folders">
                  {permission.folders
                    .filter((folder) => folder.exists)
                    .map((folder) => (
                      <div className="permission-folder" key={folder.key}>
                        <strong>{folder.label}</strong>
                        <code title={folder.path}>{folder.path}</code>
                        <span
                          className={`badge ${
                            folder.granted === true
                              ? "badge-ok"
                              : folder.granted === false
                                ? "badge-warn"
                                : ""
                          }`}
                        >
                          {folder.granted === true
                            ? "已允许"
                            : folder.granted === false
                              ? "未允许"
                              : "未检测"}
                        </span>
                      </div>
                    ))}
                </div>
              </div>
              <div className="setting-actions">
                <Button
                  size="sm"
                  icon={<FolderSearch size={14} />}
                  loading={probing}
                  onClick={() => void probeFolders()}
                >
                  允许文件访问
                </Button>
                <Button
                  size="sm"
                  variant="quiet"
                  icon={<FolderOpen size={14} />}
                  onClick={() =>
                    api
                      .openPrivacySettings("files-and-folders")
                      .catch((err) => toast(String(err), "error"))
                  }
                >
                  文件与文件夹
                </Button>
                <Button
                  size="sm"
                  icon={<ShieldCheck size={14} />}
                  onClick={openFullDiskAccessSettings}
                >
                  完全磁盘访问
                </Button>
                <Button
                  size="sm"
                  variant="quiet"
                  icon={<ExternalLink size={14} />}
                  onClick={() => api.revealApp().catch((err) => toast(String(err), "error"))}
                >
                  在 Finder 中显示
                </Button>
                <Button
                  size="sm"
                  variant="quiet"
                  icon={<RotateCcw size={14} />}
                  loading={restarting}
                  onClick={() => void restart()}
                >
                  重启应用
                </Button>
                <Button
                  size="sm"
                  variant="quiet"
                  icon={<RefreshCw size={14} />}
                  loading={checkingPermission}
                  onClick={() => void refreshPermission()}
                >
                  重新检查
                </Button>
              </div>
            </div>
          </Card>
        </section>
      )}

      <section>
        <SectionTitle title="数据与隐私" hint="所有记录都保存在本机" />
        <Card className="settings-card">
          <div className="setting-row">
            <span className="setting-icon">
              <ShieldCheck size={18} />
            </span>
            <div className="setting-body">
              <label>本地数据目录</label>
              <p className="small muted">操作历史与设置保存在这里，不会上传到任何服务器。</p>
              <div className="path-box">{info.dataDir}</div>
            </div>
            <Button size="sm" onClick={() => api.openDataDir().catch(() => {})}>
              打开目录
            </Button>
          </div>
        </Card>
      </section>

      <section>
        <SectionTitle title="应用更新" hint="从 GitHub Releases 检查新版本，支持国内镜像加速下载" />
        <Card className="settings-card">
          <div className="setting-row">
            <span className="setting-icon">
              <Download size={18} />
            </span>
            <div className="setting-body">
              <label>
                Mole Desktop v{info.version}
                {info.platform === "windows" && (
                  <span className="badge badge-warn" title="Windows 版本仍在测试中">
                    Beta
                  </span>
                )}
              </label>
              <p className={`small ${updateError ? "" : "muted"}`}>{updateHint}</p>
              {installing && (
                <div className="update-progress">
                  <ProgressBar value={progressPercent} />
                  <div className="update-progress-meta">
                    <span>
                      {installPhase === "installing"
                        ? "正在安装并重启…"
                        : progress
                          ? `${formatBytes(progress.downloaded)}${
                              progress.total ? ` / ${formatBytes(progress.total)}` : ""
                            }`
                          : "正在连接下载源…"}
                    </span>
                    {progress && installPhase !== "installing" && (
                      <span>{formatSpeed(progress.bytesPerSec)}</span>
                    )}
                  </div>
                </div>
              )}
            </div>
            <div className="setting-actions">
              <Button
                size="sm"
                icon={<RefreshCw size={14} />}
                loading={checking}
                disabled={installing}
                onClick={() => void checkUpdate()}
              >
                检查更新
              </Button>
              {onNavigate && (
                <Button
                  size="sm"
                  variant="quiet"
                  icon={<ScrollText size={14} />}
                  onClick={() => onNavigate("changelog")}
                >
                  变更记录
                </Button>
              )}
              {update?.updateAvailable && (
                <Button
                  size="sm"
                  variant="primary"
                  icon={<Download size={14} />}
                  loading={installing}
                  onClick={() => void installUpdate()}
                >
                  更新到 v{update.latestVersion}
                </Button>
              )}
            </div>
          </div>
          <div className="update-options">
            <Checkbox
              checked={settings.accelerateDownload}
              onChange={(checked) => setSettings({ ...settings, accelerateDownload: checked })}
              label="使用国内镜像加速下载"
              hint="优先尝试 ghfast.top / gh-proxy.com 等镜像，失败后自动回退到 GitHub"
            />
            {updateError && (
              <Button
                size="sm"
                variant="quiet"
                icon={<ExternalLink size={14} />}
                onClick={() => api.openExternal(update?.releaseUrl ?? RELEASES_URL).catch(() => {})}
              >
                手动下载
              </Button>
            )}
          </div>
        </Card>
      </section>

      <section>
        <SectionTitle title="关于" hint="应用版本与项目说明" />
        <Card className="about-card">
          <div className="about-head">
            <span className="about-logo">M</span>
            <div>
              <h3>Mole Desktop</h3>
              <p>跨平台清理、磁盘分析与系统监控工具</p>
            </div>
            <div className="about-badges">
              {info.platform === "windows" && <span className="badge badge-warn">Beta</span>}
              <span className="badge">v{info.version}</span>
            </div>
          </div>
          <div className="about-grid">
            <div>
              <span>运行平台</span>
              <strong>{info.platform === "macos" ? "macOS" : info.platform === "windows" ? "Windows" : "Linux"}</strong>
            </div>
            <div>
              <span>技术栈</span>
              <strong>Tauri 2 · Rust · React</strong>
            </div>
            <div>
              <span>用户目录</span>
              <strong className="ellipsis" title={info.home}>
                {info.home}
              </strong>
            </div>
          </div>
          <div className="about-note">
            <Info size={15} />
            <p>
              本应用是一个独立实现的开源图形客户端，功能思路参考了 tw93/Mole 项目，但未打包或调用其代码。
              与 Mole 官方及其付费桌面应用无关联。删除类操作均需确认，重要数据请提前备份。
            </p>
          </div>
          <div className="about-note">
            <Terminal size={15} />
            <p>
              macOS 部分优化任务会调用系统自带命令并请求管理员授权；Windows 的系统级任务需要以管理员身份运行本应用。
            </p>
          </div>
        </Card>
      </section>

      <PermissionRestartModal
        open={restartPromptOpen}
        onClose={closeRestartPrompt}
        onRestart={() => void restart()}
        restarting={restarting}
      />
    </div>
  );
}

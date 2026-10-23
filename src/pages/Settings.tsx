import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Download,
  ExternalLink,
  FolderCog,
  FolderOpen,
  Info,
  RefreshCw,
  Save,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import { api } from "../api";
import type {
  AppInfo,
  Settings,
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
import { formatBytes, formatSpeed } from "../format";

const RELEASES_URL = "https://github.com/yu1ec/mole-desktop/releases/latest";

export function SettingsPage() {
  const toast = useToast();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [rootsText, setRootsText] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [update, setUpdate] = useState<UpdateCheckResult | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<UpdateDownloadProgress | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);

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
    setUpdateError(null);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listen<UpdateDownloadProgress>("update-download-progress", (event) => {
        setProgress(event.payload);
      });
      await api.downloadAndInstallUpdate(settings.accelerateDownload);
      toast("安装包已下载，正在启动安装程序", "success");
    } catch (err) {
      setUpdateError(String(err));
      toast("更新失败，可尝试手动下载安装包", "error");
    } finally {
      unlisten?.();
      setInstalling(false);
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
              <label>Mole Desktop v{info.version}</label>
              <p className={`small ${updateError ? "" : "muted"}`}>{updateHint}</p>
              {installing && (
                <div className="update-progress">
                  <ProgressBar value={progressPercent} />
                  <div className="update-progress-meta">
                    <span>
                      {progress
                        ? `${formatBytes(progress.downloaded)}${
                            progress.total ? ` / ${formatBytes(progress.total)}` : ""
                          }`
                        : "正在连接下载源…"}
                    </span>
                    {progress && <span>{formatSpeed(progress.bytesPerSec)}</span>}
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
            <span className="badge">v{info.version}</span>
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
    </div>
  );
}

import { useCallback, useEffect, useState } from "react";
import { Download, Sparkles, X } from "lucide-react";
import { Sidebar } from "./components/Sidebar";
import { Button, Modal, ToastProvider } from "./components/ui";
import { MarkdownView } from "./components/MarkdownView";
import { notesFor, type ReleaseNotes } from "./changelog";
import { api } from "./api";
import type { PageKey, SystemSnapshot, UpdateCheckResult } from "./types";
import { Dashboard } from "./pages/Dashboard";
import { Clean } from "./pages/Clean";
import { Uninstall } from "./pages/Uninstall";
import { Configs } from "./pages/Configs";
import { Homebrew } from "./pages/Homebrew";
import { Analyze } from "./pages/Analyze";
import { Optimize } from "./pages/Optimize";
import { Monitor } from "./pages/Monitor";
import { Projects } from "./pages/Projects";
import { Installers } from "./pages/Installers";
import { HistoryPage } from "./pages/History";
import { ChangelogPage } from "./pages/Changelog";
import { SettingsPage } from "./pages/Settings";

export default function App() {
  const [page, setPage] = useState<PageKey>("dashboard");
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [update, setUpdate] = useState<UpdateCheckResult | null>(null);
  const [dismissedVersion, setDismissedVersion] = useState<string | null>(null);
  const [changelog, setChangelog] = useState<ReleaseNotes | null>(null);

  const refreshSnapshot = useCallback(async () => {
    try {
      setSnapshot(await api.snapshot());
    } catch {
      // The monitor page surfaces connection errors; the sidebar stays quiet.
    }
  }, []);

  useEffect(() => {
    refreshSnapshot();
    const timer = setInterval(refreshSnapshot, 8000);
    return () => clearInterval(timer);
  }, [refreshSnapshot]);

  // 启动后静默检查更新，并每 6 小时复查一次；失败不打扰用户。
  useEffect(() => {
    let cancelled = false;
    const check = async () => {
      try {
        const settings = await api.settings();
        const result = await api.checkForUpdate(settings.accelerateDownload);
        if (!cancelled && result.updateAvailable) setUpdate(result);
      } catch {
        // 设置页仍提供手动检查更新。
      }
    };
    const timer = window.setTimeout(check, 1500);
    const interval = window.setInterval(check, 6 * 60 * 60 * 1000);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      window.clearInterval(interval);
    };
  }, []);

  // 更新后重新打开应用时，弹出一次新版本的变更记录。首次安装只记录版本，
  // 不额外打扰；记录保存在 localStorage，跨版本更新后仍会保留。
  useEffect(() => {
    let cancelled = false;
    const key = "mole.changelog.seen-version";
    api
      .appInfo()
      .then((info) => {
        if (cancelled) return;
        const seen = localStorage.getItem(key);
        if (seen === info.version) return;
        localStorage.setItem(key, info.version);
        const notes = notesFor(info.version);
        if (seen !== null && notes) setChangelog(notes);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // 桌面应用里 Cmd/Ctrl+A 不应该全选整个界面，只保留输入框内的全选。
  useEffect(() => {
    const blockSelectAll = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "a") return;
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, [contenteditable='true']")) return;
      event.preventDefault();
    };
    document.addEventListener("keydown", blockSelectAll);
    return () => document.removeEventListener("keydown", blockSelectAll);
  }, []);

  return (
    <ToastProvider>
      <div className="app">
        <Sidebar
          page={page}
          onNavigate={setPage}
          snapshot={snapshot}
          updateAvailable={Boolean(update?.updateAvailable)}
        />
        <main className="main">
          {page !== "settings" &&
            update?.updateAvailable &&
            update.latestVersion !== dismissedVersion && (
              <div className="update-banner-wrap">
                <div className="update-banner" role="status">
                  <span className="update-banner-icon">
                    <Sparkles size={16} />
                  </span>
                  <div className="update-banner-copy">
                    <strong>发现新版本 v{update.latestVersion}</strong>
                    <span>当前版本 v{update.currentVersion}</span>
                  </div>
                  <Button
                    size="sm"
                    variant="primary"
                    icon={<Download size={14} />}
                    onClick={() => setPage("settings")}
                  >
                    立即更新
                  </Button>
                  <button
                    className="update-banner-close"
                    aria-label="稍后提醒"
                    onClick={() => setDismissedVersion(update.latestVersion)}
                  >
                    <X size={15} />
                  </button>
                </div>
              </div>
            )}
          {page === "dashboard" && <Dashboard onNavigate={setPage} />}
          {page === "clean" && <Clean />}
          {page === "uninstall" && <Uninstall />}
          {page === "configs" && <Configs />}
          {page === "brew" && <Homebrew />}
          {page === "analyze" && <Analyze />}
          {page === "optimize" && <Optimize />}
          {page === "monitor" && <Monitor />}
          {page === "projects" && <Projects />}
          {page === "installers" && <Installers />}
          {page === "history" && <HistoryPage />}
          {page === "changelog" && <ChangelogPage />}
          {page === "settings" && (
            <SettingsPage initialUpdate={update} onNavigate={setPage} />
          )}
        </main>

        {changelog && (
          <Modal
            open
            title={`变更记录 v${changelog.version}`}
            description="更新已完成，来看看这次有哪些变化"
            onClose={() => setChangelog(null)}
            footer={
              <Button variant="primary" onClick={() => setChangelog(null)}>
                知道了
              </Button>
            }
          >
            <MarkdownView source={changelog.body} />
          </Modal>
        )}
      </div>
    </ToastProvider>
  );
}

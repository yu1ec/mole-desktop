import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  ArrowRight,
  ArrowUpCircle,
  ExternalLink,
  FolderOpen,
  RefreshCw,
  Search,
  ShieldAlert,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import { useCachedQuery } from "../cache";
import { formatBytes, formatDate, formatSpeed } from "../format";
import type {
  AppSignatureInfo,
  AppSignatureState,
  AppUpdateChannel,
  AppUpdateEntry,
  AppUpdateProgress,
  AppUpdateResult,
  AppUpdateScan,
  AppUpdateTarget,
} from "../types";
import {
  Button,
  Card,
  Checkbox,
  EmptyState,
  Modal,
  ProgressBar,
  Spinner,
  useToast,
} from "../components/ui";
import { usePathMenu } from "../components/ContextMenu";
import { usePlatform } from "../platform";

type Filter = "all" | "updatable" | "risky" | AppUpdateChannel;

const SIGNATURE_META: Record<
  AppSignatureState,
  { label: string; tone: "ok" | "warn" | "danger"; hint: string }
> = {
  official: {
    label: "官方签名",
    tone: "ok",
    hint: "使用受信任的开发者或 App Store 签名，且完整性校验通过",
  },
  adhoc: {
    label: "ad-hoc 重签",
    tone: "warn",
    hint: "没有开发者签名，可能是破解重签或本地构建的版本",
  },
  damaged: {
    label: "签名已损坏",
    tone: "danger",
    hint: "应用内容与签名不一致，可能被篡改或文件损坏",
  },
  unsigned: {
    label: "无签名",
    tone: "warn",
    hint: "没有代码签名，无法验证应用来源和完整性",
  },
};

function signatureTitle(signature: AppSignatureInfo) {
  const meta = SIGNATURE_META[signature.state];
  const detail = signature.signer ?? signature.teamId;
  return detail ? `${meta.hint}（${detail}）` : meta.hint;
}

function isRisky(entry: AppUpdateEntry) {
  return !!entry.signature && entry.signature.state !== "official";
}

const CHANNEL_META: Record<
  AppUpdateChannel,
  { label: string; tone: string; group: "macos" | "windows" | "manual" }
> = {
  homebrew: { label: "Homebrew", tone: "lime", group: "macos" },
  appstore: { label: "App Store", tone: "blue", group: "macos" },
  sparkle: { label: "Sparkle", tone: "violet", group: "macos" },
  winget: { label: "winget", tone: "lime", group: "windows" },
  store: { label: "Microsoft Store", tone: "blue", group: "windows" },
  manual: { label: "手动安装", tone: "amber", group: "manual" },
};

const CHANNEL_ORDER: AppUpdateChannel[] = [
  "homebrew",
  "appstore",
  "sparkle",
  "winget",
  "store",
  "manual",
];

function channelMeta(channel: string) {
  return CHANNEL_META[channel as AppUpdateChannel] ?? CHANNEL_META.manual;
}

function toTarget(entry: AppUpdateEntry): AppUpdateTarget {
  return {
    id: entry.id,
    channel: entry.channel,
    source: entry.source,
    name: entry.name,
    path: entry.path,
  };
}

export function AppUpdates() {
  const toast = useToast();
  const pathMenu = usePathMenu();
  const platform = usePlatform();
  const {
    data: scan,
    loading,
    refreshing,
    error,
    refresh,
  } = useCachedQuery<AppUpdateScan>("app-updates", api.scanAppUpdates, {
    ttl: 120_000,
  });

  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [installingMas, setInstallingMas] = useState(false);
  const [activeIds, setActiveIds] = useState<Set<string>>(new Set());
  const [progress, setProgress] = useState<Record<string, AppUpdateProgress>>({});
  const [result, setResult] = useState<AppUpdateResult | null>(null);
  const initialized = useRef(false);

  const apps = scan?.apps ?? [];

  // 首次默认选中全部可更新项；之后刷新时只保留仍然可更新的选择。
  useEffect(() => {
    if (!scan) return;
    const updatable = new Set(scan.apps.filter((app) => app.canUpdate).map((app) => app.id));
    if (!initialized.current) {
      initialized.current = true;
      setSelected(updatable);
      return;
    }
    setSelected((current) => new Set([...current].filter((id) => updatable.has(id))));
  }, [scan]);

  // 从其他窗口切回时重新校验：缓存已过期就重新扫描，避免列表停留在旧结果。
  useEffect(() => {
    const revalidate = () => {
      if (document.visibilityState === "visible") void refresh();
    };
    window.addEventListener("focus", revalidate);
    document.addEventListener("visibilitychange", revalidate);
    return () => {
      window.removeEventListener("focus", revalidate);
      document.removeEventListener("visibilitychange", revalidate);
    };
  }, [refresh]);

  const riskyApps = useMemo(() => apps.filter(isRisky), [apps]);

  const filtered = useMemo(() => {
    const base =
      filter === "all"
        ? apps
        : filter === "updatable"
          ? apps.filter((app) => app.updateAvailable)
          : filter === "risky"
            ? apps.filter(isRisky)
            : apps.filter((app) => app.channel === filter);
    const needle = query.trim().toLowerCase();
    if (!needle) return base;
    return base.filter(
      (app) =>
        app.name.toLowerCase().includes(needle) ||
        app.source.toLowerCase().includes(needle) ||
        (app.path ?? "").toLowerCase().includes(needle),
    );
  }, [apps, filter, query]);

  const channelCounts = useMemo(() => {
    const counts = new Map<AppUpdateChannel, number>();
    for (const app of apps) counts.set(app.channel, (counts.get(app.channel) ?? 0) + 1);
    return counts;
  }, [apps]);

  const selectedEntries = apps.filter((app) => selected.has(app.id) && app.canUpdate);
  const selectable = filtered.filter((app) => app.canUpdate);
  const updatable = scan?.updatable ?? 0;
  const allSelected = selectable.length > 0 && selectable.every((app) => selected.has(app.id));

  const toggleSelectAll = () => {
    setSelected((current) => {
      const next = new Set(current);
      if (allSelected) selectable.forEach((app) => next.delete(app.id));
      else selectable.forEach((app) => next.add(app.id));
      return next;
    });
  };

  const runUpdate = async (targets: AppUpdateTarget[]) => {
    if (targets.length === 0) return;
    setConfirmOpen(false);
    setUpdating(true);
    setActiveIds(new Set(targets.map((target) => target.id)));
    setProgress({});
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listen<AppUpdateProgress>("app-update-progress", (event) => {
        setProgress((current) => ({ ...current, [event.payload.id]: event.payload }));
      });
      const data = await api.updateApps(targets);
      setResult(data);
      if (data.failed.length === 0) {
        toast(`已更新 ${data.ok.length} 个软件`, "success");
      } else {
        toast(`更新完成，${data.failed.length} 个失败`, "error");
      }
      await refresh({ force: true });
    } catch (err) {
      toast(String(err), "error");
    } finally {
      unlisten?.();
      setUpdating(false);
      setActiveIds(new Set());
      setProgress({});
    }
  };

  const installMas = async () => {
    setInstallingMas(true);
    try {
      await api.installMas();
      toast("mas 已安装，正在重新扫描", "success");
      await refresh({ force: true });
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setInstallingMas(false);
    }
  };

  const availability = [
    platform === "macos" && {
      label: "Homebrew",
      ok: scan?.brewAvailable ?? false,
      hint: "用于 cask 应用与更新检测",
    },
    platform === "macos" && {
      label: "mas",
      ok: scan?.masAvailable ?? false,
      hint: "App Store 应用更新需要 mas",
    },
    platform === "windows" && {
      label: "winget",
      ok: scan?.wingetAvailable ?? false,
      hint: "Windows 程序包管理器",
    },
  ].filter(Boolean) as { label: string; ok: boolean; hint: string }[];

  const header = (
    <header className="page-head">
      <div>
        <h1>软件更新</h1>
        <p>
          {platform === "windows"
            ? "通过 winget 与 Microsoft Store 检查已安装软件的版本差异"
            : "汇总 Homebrew、App Store 与 Sparkle 应用的版本差异"}
        </p>
      </div>
      <div className="head-actions">
        <Button
          icon={<RefreshCw size={16} />}
          loading={loading || refreshing}
          disabled={updating}
          onClick={() => refresh({ force: true })}
        >
          重新扫描
        </Button>
        <Button
          variant="primary"
          icon={<ArrowUpCircle size={16} />}
          disabled={selectedEntries.length === 0 || updating}
          loading={updating}
          onClick={() => setConfirmOpen(true)}
        >
          更新选中{selectedEntries.length > 0 ? ` (${selectedEntries.length})` : ""}
        </Button>
      </div>
    </header>
  );

  if (platform !== "macos" && platform !== "windows") {
    return (
      <div className="page">
        {header}
        <EmptyState
          icon={<ArrowUpCircle size={24} />}
          title="当前平台暂不支持软件更新"
          hint="软件更新目前支持 macOS（Homebrew / App Store / Sparkle）与 Windows（winget / Microsoft Store）。"
        />
      </div>
    );
  }

  return (
    <div className="page">
      {header}

      {error && <Card className="error-card">{error}</Card>}

      {loading && !scan ? (
        <Spinner label="正在检查已安装软件与可用版本…" />
      ) : apps.length === 0 ? (
        <EmptyState
          icon={<ArrowUpCircle size={24} />}
          title="没有找到可检查的软件"
          hint={
            platform === "macos"
              ? "请确认应用安装在「应用程序」目录，并检查 Homebrew / mas 是否可用。"
              : "请确认已安装 winget，并能在终端执行 winget list。"
          }
          action={
            <Button icon={<RefreshCw size={16} />} onClick={() => refresh({ force: true })}>
              重新扫描
            </Button>
          }
        />
      ) : (
        <>
          <Card className="summary-strip">
            <div className="summary-main">
              <span className="summary-icon">
                <ArrowUpCircle size={22} />
              </span>
              <div>
                <strong>{updatable} 个软件可更新</strong>
                <span>
                  已检查 {apps.length} 个软件 · {formatDate(scan?.scannedAt ?? "")}
                </span>
              </div>
            </div>
            <div className="summary-side">
              <div>
                <span>已选择</span>
                <strong>{selectedEntries.length}</strong>
              </div>
              <div>
                <span>安装渠道</span>
                <strong>{channelCounts.size}</strong>
              </div>
            </div>
          </Card>

          {availability.length > 0 && (
            <div className="app-update-sources">
              {availability.map((source) => (
                <span className="app-update-source" key={source.label}>
                  <span
                    className={`badge ${source.ok ? "badge-ok" : "badge-warn"}`}
                    title={source.ok ? source.hint : `未检测到：${source.hint}`}
                  >
                    {source.label}
                    {source.ok ? " 已就绪" : " 不可用"}
                  </span>
                  {!source.ok && source.label === "mas" && (
                    <button
                      className="link-btn"
                      disabled={installingMas}
                      onClick={() => void installMas()}
                      title="通过 Homebrew 安装 mas 命令行工具"
                    >
                      {installingMas ? "安装中…" : "安装 mas"}
                    </button>
                  )}
                </span>
              ))}
            </div>
          )}

          {scan && scan.warnings.length > 0 && (
            <Card className="clean-note">
              <AlertTriangle size={16} />
              <p>{scan.warnings.join("；")}</p>
            </Card>
          )}

          {riskyApps.length > 0 && (
            <Card className="signature-warning">
              <ShieldAlert size={16} />
              <div>
                <strong>{riskyApps.length} 个软件签名异常</strong>
                <p>
                  {riskyApps.slice(0, 6).map((app) => app.name).join("、")}
                  {riskyApps.length > 6 ? ` 等 ${riskyApps.length} 个应用` : ""}
                  {" 的签名不是官方签名。"}
                  {riskyApps.some((app) => app.signature?.state === "damaged") &&
                    "标记为「签名已损坏」的应用文件可能与签名不一致，存在被篡改的风险。"}
                  {"ad-hoc 重签或无签名常见于破解版与本地构建，更新后破解可能失效，请确认来源后再操作。"}
                </p>
              </div>
            </Card>
          )}

          <div className="app-update-toolbar">
            <div className="filter-row">
              <button
                className={`chip ${filter === "all" ? "is-active" : ""}`}
                onClick={() => setFilter("all")}
              >
                全部 {apps.length}
              </button>
              <button
                className={`chip ${filter === "updatable" ? "is-active" : ""}`}
                onClick={() => setFilter("updatable")}
              >
                可更新 {updatable}
              </button>
              {riskyApps.length > 0 && (
                <button
                  className={`chip ${filter === "risky" ? "is-active" : ""}`}
                  onClick={() => setFilter("risky")}
                >
                  签名异常 {riskyApps.length}
                </button>
              )}
              {CHANNEL_ORDER.filter((channel) => channelCounts.has(channel)).map((channel) => (
                <button
                  key={channel}
                  className={`chip ${filter === channel ? "is-active" : ""}`}
                  onClick={() => setFilter(channel)}
                >
                  {channelMeta(channel).label} {channelCounts.get(channel)}
                </button>
              ))}
            </div>
            <div className="search">
              <Search size={15} />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="搜索软件名称、ID 或路径"
              />
            </div>
          </div>

          {filtered.length === 0 ? (
            <EmptyState
              icon={<Search size={24} />}
              title="没有符合筛选条件的软件"
              hint="换个关键词，或切换到「全部」查看。"
            />
          ) : (
            <Card className="installer-list">
              <div className="installer-list-head">
                <button className="link-btn" onClick={toggleSelectAll} disabled={selectable.length === 0}>
                  {allSelected ? "取消全选" : "全选可更新项"}
                </button>
                <span>
                  共 {filtered.length} 个 · 可更新 {filtered.filter((app) => app.updateAvailable).length} 个
                </span>
              </div>
              {filtered.map((entry) => {
                const meta = channelMeta(entry.channel);
                const active = activeIds.has(entry.id);
                const itemProgress = progress[entry.id];
                const percent =
                  itemProgress && itemProgress.total
                    ? (itemProgress.downloaded / itemProgress.total) * 100
                    : null;
                return (
                  <div
                    className={`installer-row app-update-row ${
                      entry.updateAvailable ? "is-outdated" : ""
                    }`}
                    key={entry.id}
                    onContextMenu={(event) => {
                      if (entry.path) pathMenu(event, entry.path, { isDir: true });
                    }}
                  >
                    <Checkbox
                      checked={selected.has(entry.id)}
                      disabled={!entry.canUpdate || updating}
                      onChange={(value) =>
                        setSelected((current) => {
                          const next = new Set(current);
                          if (value) next.add(entry.id);
                          else next.delete(entry.id);
                          return next;
                        })
                      }
                      className="installer-check"
                      label={
                        <span className="installer-label">
                          <span className="installer-name">{entry.name}</span>
                          <span className={`badge app-update-channel tone-${meta.tone}`}>
                            {meta.label}
                          </span>
                          {entry.signature && (
                            <span
                              className={`badge badge-${SIGNATURE_META[entry.signature.state].tone}`}
                              title={signatureTitle(entry.signature)}
                            >
                              {SIGNATURE_META[entry.signature.state].label}
                            </span>
                          )}
                          {entry.updateAvailable && (
                            <span className="badge badge-warn">可更新</span>
                          )}
                        </span>
                      }
                      hint={
                        <span className="app-update-hint">
                          <span className="app-update-versions">
                            <span>{entry.installedVersion || "版本未知"}</span>
                            {entry.updateAvailable && entry.latestVersion && (
                              <>
                                <ArrowRight size={12} />
                                <em>{entry.latestVersion}</em>
                              </>
                            )}
                          </span>
                          {entry.note && <span className="app-update-note">{entry.note}</span>}
                          {entry.path && (
                            <span className="path-text" title={entry.path}>
                              {entry.path}
                            </span>
                          )}
                        </span>
                      }
                    />

                    {active && (
                      <div className="app-update-progress">
                        {itemProgress ? (
                          <>
                            <span>
                              {formatBytes(itemProgress.downloaded)}
                              {itemProgress.total ? ` / ${formatBytes(itemProgress.total)}` : ""}
                              {" · "}
                              {formatSpeed(itemProgress.bytesPerSec)}
                            </span>
                            {percent !== null && <ProgressBar value={percent} />}
                          </>
                        ) : (
                          <span className="app-update-running">正在更新…</span>
                        )}
                      </div>
                    )}

                    <div className="app-update-actions">
                      {entry.homepage && (
                        <button
                          className="icon-btn"
                          title={
                            entry.channel === "appstore" ? "打开 App Store 页面" : "打开软件主页"
                          }
                          onClick={() => api.openExternal(entry.homepage!).catch(() => {})}
                        >
                          <ExternalLink size={16} />
                        </button>
                      )}
                      {entry.path && (
                        <button
                          className="icon-btn"
                          title="在文件夹中显示"
                          onClick={() => api.reveal(entry.path!).catch(() => {})}
                        >
                          <FolderOpen size={16} />
                        </button>
                      )}
                      {entry.channel === "appstore" && entry.note?.includes("安装 mas") && (
                        <Button
                          size="sm"
                          variant="quiet"
                          loading={installingMas}
                          disabled={updating}
                          onClick={() => void installMas()}
                        >
                          安装 mas
                        </Button>
                      )}
                      {entry.channel === "appstore" &&
                        entry.updateAvailable &&
                        !entry.canUpdate &&
                        entry.homepage && (
                          <Button
                            size="sm"
                            variant="primary"
                            onClick={() => api.openExternal(entry.homepage!).catch(() => {})}
                          >
                            前往 App Store
                          </Button>
                        )}
                      <Button
                        size="sm"
                        variant={entry.updateAvailable && entry.canUpdate ? "primary" : "quiet"}
                        disabled={!entry.canUpdate || updating}
                        onClick={() => void runUpdate([toTarget(entry)])}
                      >
                        更新
                      </Button>
                    </div>
                  </div>
                );
              })}
            </Card>
          )}

          <Card className="clean-note">
            <ArrowUpCircle size={16} />
            <p>
              {platform === "windows"
                ? "winget 更新会以静默方式执行，部分软件可能需要管理员权限；Microsoft Store 应用通过 msstore 源更新。"
                : "Sparkle 更新会先退出对应应用再原地替换；Homebrew 与 App Store 更新会调用命令行工具，首次使用可能需要授权。"}
            </p>
          </Card>
        </>
      )}

      <Modal
        open={confirmOpen}
        title={`更新 ${selectedEntries.length} 个软件？`}
        description={
          platform === "windows"
            ? "更新过程中会逐个调用 winget，可能需要几分钟。"
            : "更新过程中会逐个处理，Sparkle 应用会被自动退出。"
        }
        onClose={() => setConfirmOpen(false)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setConfirmOpen(false)}>
              取消
            </Button>
            <Button
              variant="primary"
              onClick={() => void runUpdate(selectedEntries.map(toTarget))}
            >
              开始更新
            </Button>
          </>
        }
      >
        <ul className="confirm-list">
          {selectedEntries.slice(0, 12).map((entry) => (
            <li key={entry.id}>
              <span>{entry.name}</span>
              <em>
                {entry.installedVersion || "未知"} → {entry.latestVersion ?? "最新"}
              </em>
            </li>
          ))}
        </ul>
        {selectedEntries.length > 12 && (
          <p className="app-update-more">以及另外 {selectedEntries.length - 12} 个软件</p>
        )}
      </Modal>

      <Modal
        open={!!result}
        title={result && result.failed.length === 0 ? "更新完成" : "更新已结束"}
        description={
          result
            ? `成功 ${result.ok.length} 个${result.failed.length > 0 ? `，失败 ${result.failed.length} 个` : ""}。`
            : ""
        }
        onClose={() => setResult(null)}
        footer={
          <Button variant="primary" onClick={() => setResult(null)}>
            完成
          </Button>
        }
      >
        {result && result.failed.length > 0 && (
          <div className="result-errors">
            <strong>{result.failed.length} 个软件未能更新</strong>
            <ul>
              {result.failed.map((failure) => (
                <li key={failure.name}>
                  {failure.name}：{failure.message}
                </li>
              ))}
            </ul>
          </div>
        )}
        {result?.output && (
          <details className="app-update-output">
            <summary>查看命令输出</summary>
            <pre>{result.output}</pre>
          </details>
        )}
      </Modal>
    </div>
  );
}

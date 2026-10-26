import { useEffect, useMemo, useRef, useState } from "react";
import {
  AppWindow,
  Box,
  ExternalLink,
  FolderSearch,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";
import { api } from "../api";
import { invalidateCachePrefix, useCachedQuery } from "../cache";
import { formatBytes, relativeTime } from "../format";
import type { AppEntry, AppRunningInfo, Leftover, OperationResult } from "../types";
import {
  Button,
  Card,
  Checkbox,
  EmptyState,
  Modal,
  Spinner,
  useToast,
} from "../components/ui";
import { PermissionNotice } from "../components/PermissionNotice";

const KIND_LABEL: Record<string, string> = {
  cache: "缓存",
  preference: "偏好设置",
  log: "日志",
  state: "窗口状态",
  data: "应用数据",
  container: "沙盒容器",
  agent: "启动项",
  other: "其他",
};

const EMPTY_APPS: AppEntry[] = [];
const EMPTY_LEFTOVERS: Leftover[] = [];

export function Uninstall() {
  const toast = useToast();
  const {
    data: appData,
    loading,
    refreshing,
    error,
    refresh: refreshApps,
  } = useCachedQuery<AppEntry[]>("apps", api.listApps, { ttl: 30_000 });
  const apps = appData ?? EMPTY_APPS;
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<"size" | "name">("size");
  const [active, setActive] = useState<AppEntry | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [permanent, setPermanent] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [running, setRunning] = useState(false);
  const [runningInfo, setRunningInfo] = useState<AppRunningInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<OperationResult | null>(null);

  useEffect(() => {
    if (!appData) return;
    setActive((current) =>
      current && appData.some((app) => app.id === current.id) ? current : appData[0] ?? null,
    );
  }, [appData]);

  const leftoversKey = active ? `app-leftovers:${active.id}` : null;
  const { data: leftoverData, loading: leftoversLoading } = useCachedQuery<Leftover[]>(
    leftoversKey,
    () => api.appLeftovers(active?.id ?? ""),
    { ttl: 30_000 },
  );
  const leftovers = leftoverData ?? EMPTY_LEFTOVERS;
  const lastLeftoverKey = useRef<string | null>(null);

  // Recommend items on first visit; keep manual choices when the app list refreshes.
  useEffect(() => {
    if (!leftoversKey) {
      lastLeftoverKey.current = null;
      setChecked(new Set());
      return;
    }
    if (!leftoverData) return;
    if (lastLeftoverKey.current !== leftoversKey) {
      lastLeftoverKey.current = leftoversKey;
      setChecked(
        new Set(leftoverData.filter((item) => item.recommended).map((item) => item.path)),
      );
      return;
    }
    setChecked((current) => {
      const paths = new Set(leftoverData.map((item) => item.path));
      return new Set([...current].filter((path) => paths.has(path)));
    });
  }, [leftoversKey, leftoverData]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const list = apps.filter(
      (app) =>
        !needle ||
        app.name.toLowerCase().includes(needle) ||
        app.publisher.toLowerCase().includes(needle) ||
        app.bundleId.toLowerCase().includes(needle),
    );
    list.sort((a, b) =>
      sort === "size" ? b.size - a.size : a.name.localeCompare(b.name, "zh-CN"),
    );
    return list;
  }, [apps, query, sort]);

  const checkedSize = leftovers
    .filter((item) => checked.has(item.path))
    .reduce((total, item) => total + item.size, 0);
  const totalSize = apps.reduce((total, app) => total + app.size, 0);

  const requestUninstall = async () => {
    if (!active || checking) return;
    setChecking(true);
    try {
      const info = await api.appRunning(active.id);
      setRunningInfo(info.running ? info : null);
      setConfirmOpen(true);
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setChecking(false);
    }
  };

  const closeConfirm = () => {
    setConfirmOpen(false);
    setRunningInfo(null);
  };

  const uninstall = async () => {
    if (!active) return;
    const quitProcess = runningInfo !== null;
    setConfirmOpen(false);
    setRunningInfo(null);
    setRunning(true);
    try {
      const data = await api.uninstallApp(active.id, [...checked], permanent, quitProcess);
      setResult(data);
      toast(`${active.name} 已处理完成`, "success");
      invalidateCachePrefix("app-leftovers:");
      setActive(null);
      await refreshApps({ force: true });
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setRunning(false);
    }
  };

  const groupLeftovers = useMemo(() => {
    const map = new Map<string, Leftover[]>();
    for (const item of leftovers) {
      const key = item.kind;
      const list = map.get(key) ?? [];
      list.push(item);
      map.set(key, list);
    }
    return [...map.entries()];
  }, [leftovers]);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>应用卸载</h1>
          <p>
            移除应用本体，并清理可确认属于它的偏好、缓存与残留
            {apps.length > 0 ? ` · ${apps.length} 个应用 · ${formatBytes(totalSize)}` : ""}
          </p>
        </div>
        <Button
          icon={<RefreshCw size={16} />}
          onClick={() => refreshApps({ force: true })}
          loading={loading || refreshing}
        >
          刷新列表
        </Button>
      </header>

      <PermissionNotice />

      {error && <Card className="error-card">{error}</Card>}

      {loading && apps.length === 0 ? (
        <Spinner label="正在读取已安装应用…" />
      ) : apps.length === 0 ? (
        <EmptyState icon={<AppWindow size={24} />} title="没有找到应用" hint="请确认应用安装位置，或稍后重试。" />
      ) : (
        <div className="uninstall-layout">
          <Card className="app-list-card">
            <div className="list-tools">
              <div className="search">
                <Search size={15} />
                <input
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="搜索应用、发布者或 Bundle ID"
                />
              </div>
              <button
                className="sort-btn"
                onClick={() => setSort(sort === "size" ? "name" : "size")}
                title="切换排序"
              >
                {sort === "size" ? "按大小" : "按名称"}
              </button>
            </div>
            <div className="app-list">
              {filtered.map((app) => (
                <button
                  key={app.id}
                  className={`app-row ${active?.id === app.id ? "is-active" : ""}`}
                  onClick={() => setActive(app)}
                >
                  <span className="app-avatar">{app.name.slice(0, 1).toUpperCase()}</span>
                  <span className="app-row-main">
                    <strong>{app.name}</strong>
                    <span>
                      {app.version ? `v${app.version}` : "版本未知"}
                      {app.publisher ? ` · ${app.publisher}` : ""}
                    </span>
                  </span>
                  <em>{formatBytes(app.size)}</em>
                </button>
              ))}
              {filtered.length === 0 && <p className="muted list-empty">没有匹配的应用</p>}
            </div>
          </Card>

          <Card className="app-detail">
            {!active ? (
              <EmptyState
                icon={<Box size={24} />}
                title="选择一个应用"
                hint="左侧列表会显示应用本体与可清理的相关文件。"
              />
            ) : (
              <>
                <div className="detail-head">
                  <span className="app-avatar large">{active.name.slice(0, 1).toUpperCase()}</span>
                  <div>
                    <h2>{active.name}</h2>
                    <p>
                      {active.version ? `版本 ${active.version}` : "版本未知"}
                      {active.bundleId ? ` · ${active.bundleId}` : ""}
                    </p>
                    <span className="muted small">
                      {active.path || "未记录安装位置"} · 最近修改 {relativeTime(active.lastModified)}
                    </span>
                  </div>
                  <div className="detail-size">
                    <strong>{formatBytes(active.size)}</strong>
                    <span>应用本体</span>
                  </div>
                </div>

                <div className="detail-actions">
                  <Button
                    size="sm"
                    icon={<FolderSearch size={15} />}
                    disabled={!active.path}
                    onClick={() => active.path && api.reveal(active.path).catch(() => {})}
                  >
                    在文件夹中显示
                  </Button>
                  {active.uninstaller && (
                    <span className="hint-inline">将启动应用自带的卸载程序</span>
                  )}
                </div>

                <div className="detail-section">
                  <div className="detail-section-head">
                    <h3>相关文件</h3>
                    <span>{leftovers.length ? `${leftovers.length} 项 · ${formatBytes(leftovers.reduce((t, i) => t + i.size, 0))}` : ""}</span>
                  </div>
                  {leftoversLoading ? (
                    <Spinner label="正在查找残留…" />
                  ) : leftovers.length === 0 ? (
                    <p className="muted small">没有发现可确认归属的应用数据。</p>
                  ) : (
                    <div className="leftover-list">
                      {groupLeftovers.map(([kind, items]) => (
                        <div className="leftover-group" key={kind}>
                          <span className="leftover-kind">{KIND_LABEL[kind] ?? kind}</span>
                          {items.map((item) => (
                            <Checkbox
                              key={item.path}
                              checked={checked.has(item.path)}
                              onChange={(value) =>
                                setChecked((current) => {
                                  const next = new Set(current);
                                  if (value) next.add(item.path);
                                  else next.delete(item.path);
                                  return next;
                                })
                              }
                              className="leftover-item"
                              label={
                                <span className="leftover-label">
                                  <span>{item.label}</span>
                                  <em>{formatBytes(item.size)}</em>
                                </span>
                              }
                              hint={
                                <span className="path-text" title={item.path}>
                                  {item.path}
                                </span>
                              }
                            />
                          ))}
                        </div>
                      ))}
                    </div>
                  )}
                </div>

                <div className="detail-foot">
                  <Checkbox
                    checked={permanent}
                    onChange={setPermanent}
                    label="永久删除（不放入废纸篓）"
                    hint="默认移入废纸篓，可在系统里恢复。"
                  />
                  <div className="detail-foot-actions">
                    <span className="muted small">
                      将同时清理 {checked.size} 项数据 · {formatBytes(checkedSize)}
                    </span>
                    <Button
                      variant="danger"
                      icon={<Trash2 size={16} />}
                      loading={running || checking}
                      onClick={requestUninstall}
                    >
                      卸载 {active.name}
                    </Button>
                  </div>
                </div>
              </>
            )}
          </Card>
        </div>
      )}

      <Modal
        open={confirmOpen}
        title={
          active
            ? runningInfo
              ? `${active.name} 正在运行`
              : `卸载 ${active.name}？`
            : ""
        }
        description={
          runningInfo
            ? "应用进程仍在运行。继续操作会先结束该进程，未保存的内容可能会丢失。"
            : "应用本体与选中的相关文件会被移除。卸载后可能需要重新登录相关账号。"
        }
        tone="danger"
        onClose={closeConfirm}
        footer={
          <>
            <Button variant="quiet" onClick={closeConfirm}>
              取消
            </Button>
            <Button variant="danger" loading={running} onClick={uninstall}>
              {runningInfo ? "结束进程并卸载" : "确认卸载"}
            </Button>
          </>
        }
      >
        {runningInfo?.isSelf && (
          <p className="muted small">
            当前运行的就是 {active?.name}，卸载会结束它的进程。
          </p>
        )}
        {active && (
          <ul className="confirm-list">
            <li>
              <span>{active.path}</span>
              <em>{formatBytes(active.size)}</em>
            </li>
            {leftovers
              .filter((item) => checked.has(item.path))
              .slice(0, 8)
              .map((item) => (
                <li key={item.path}>
                  <span>{item.path}</span>
                  <em>{formatBytes(item.size)}</em>
                </li>
              ))}
          </ul>
        )}
        {active?.uninstaller && (
          <p className="muted small">
            <ExternalLink size={13} /> 会先启动应用自带卸载程序，再清理上面选中的数据。
          </p>
        )}
      </Modal>

      <Modal
        open={!!result}
        title="卸载处理完成"
        description={result ? `释放 ${formatBytes(result.freed)}，处理 ${result.files.toLocaleString()} 个文件。` : ""}
        onClose={() => setResult(null)}
        footer={
          <Button variant="primary" onClick={() => setResult(null)}>
            完成
          </Button>
        }
      >
        {result && result.errors.length > 0 && (
          <div className="result-errors">
            <strong>{result.errors.length} 项未能处理</strong>
            <ul>
              {result.errors.slice(0, 6).map((item, index) => (
                <li key={index}>{item}</li>
              ))}
            </ul>
          </div>
        )}
      </Modal>
    </div>
  );
}

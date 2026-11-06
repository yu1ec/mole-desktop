import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  FolderOpen,
  LockKeyhole,
  RefreshCw,
  RotateCcw,
  Search,
  ShieldCheck,
  ShieldX,
  Trash2,
} from "lucide-react";
import { api } from "../../api";
import { useCachedQuery } from "../../cache";
import { formatDate } from "../../format";
import type {
  PrivacyCleanResult,
  PrivacyLeftover,
  PrivacyScan,
  PrivacyTarget,
} from "../../types";
import {
  Button,
  Card,
  Checkbox,
  EmptyState,
  Modal,
  Spinner,
  useToast,
} from "../../components/ui";
import { usePathMenu } from "../../components/ContextMenu";
import { usePlatform } from "../../platform";

type Filter = "all" | "invalid" | "valid" | "user" | "system" | "trash";

const SCOPE_LABEL: Record<string, string> = {
  user: "用户级",
  system: "系统级",
};

function toTarget(app: PrivacyLeftover): PrivacyTarget {
  return { scope: app.scope, client: app.client, clientType: app.clientType };
}

function serviceNames(app: PrivacyLeftover) {
  return app.services.map((service) => service.label).join("、");
}

export function PrivacyPanel() {
  const toast = useToast();
  const pathMenu = usePathMenu();
  const platform = usePlatform();
  const {
    data: scan,
    loading,
    refreshing,
    error,
    refresh,
  } = useCachedQuery<PrivacyScan>("privacy-items", api.scanPrivacyLeftovers, {
    ttl: 60_000,
    enabled: platform === "macos",
  });

  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [pending, setPending] = useState<PrivacyLeftover[]>([]);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [result, setResult] = useState<PrivacyCleanResult | null>(null);
  const initialized = useRef(false);

  const apps = scan?.apps ?? [];

  // 首次自动选中失效记录；刷新后只保留仍然存在的项目。
  useEffect(() => {
    if (!scan) return;
    const ids = new Set(scan.apps.map((app) => app.id));
    if (!initialized.current) {
      initialized.current = true;
      setSelected(
        new Set(scan.apps.filter((app) => app.invalid).map((app) => app.id)),
      );
      return;
    }
    setSelected((current) => new Set([...current].filter((id) => ids.has(id))));
  }, [scan]);

  const invalidCount = apps.filter((app) => app.invalid).length;
  const trashCount = apps.filter((app) => app.inTrash).length;
  const userCount = apps.filter((app) => app.scope === "user").length;
  const systemCount = apps.filter((app) => app.scope === "system").length;

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return apps.filter((app) => {
      if (filter === "invalid" && !app.invalid) return false;
      if (filter === "valid" && app.invalid) return false;
      if (filter === "user" && app.scope !== "user") return false;
      if (filter === "system" && app.scope !== "system") return false;
      if (filter === "trash" && !app.inTrash) return false;
      if (!needle) return true;
      return (
        app.name.toLowerCase().includes(needle) ||
        app.client.toLowerCase().includes(needle) ||
        serviceNames(app).toLowerCase().includes(needle)
      );
    });
  }, [apps, filter, query]);

  const selectedApps = apps.filter((app) => selected.has(app.id));
  const allSelected = filtered.length > 0 && filtered.every((app) => selected.has(app.id));
  const pendingInvalid = pending.filter((app) => app.invalid).length;
  const pendingValid = pending.length - pendingInvalid;
  const pendingSystem = pending.filter((app) => app.scope === "system").length;
  const resetting = pendingValid > 0;
  const selectionResets = selectedApps.some((app) => !app.invalid);

  const toggleSelectAll = () => {
    setSelected((current) => {
      const next = new Set(current);
      if (allSelected) filtered.forEach((app) => next.delete(app.id));
      else filtered.forEach((app) => next.add(app.id));
      return next;
    });
  };

  const askClean = (list: PrivacyLeftover[]) => {
    if (list.length === 0) return;
    setPending(list);
    setConfirmOpen(true);
  };

  const runClean = async () => {
    if (pending.length === 0) return;
    setConfirmOpen(false);
    setCleaning(true);
    try {
      const data = await api.cleanPrivacyLeftovers(pending.map(toTarget));
      setResult(data);
      if (data.removedApps > 0) {
        toast(`已处理 ${data.removedApps} 个应用的权限记录`, "success");
      } else if (data.failed.length === 0 && !data.cancelled) {
        toast("没有需要处理的权限记录", "info");
      }
      setPending([]);
      await refresh({ force: true });
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setCleaning(false);
    }
  };

  if (platform !== "macos") {
    return (
      <EmptyState
        icon={<LockKeyhole size={24} />}
        title="当前平台没有 TCC 隐私数据库"
        hint="隐私权限记录只存在于 macOS，其他平台的权限跟随应用一起删除。"
      />
    );
  }

  return (
    <>
      {error && <Card className="error-card">{error}</Card>}

      {loading && !scan ? (
        <Spinner label="正在读取隐私权限数据库…" />
      ) : !scan || !scan.available ? (
        <EmptyState
          icon={<ShieldX size={24} />}
          title="无法读取隐私权限数据库"
          hint="系统级 TCC 数据库受保护，需要为 Mole Desktop 开启完全磁盘访问权限后重新扫描。"
          action={
            <div className="head-actions">
              <Button
                variant="primary"
                icon={<ShieldCheck size={16} />}
                onClick={() =>
                  api
                    .openPrivacySettings("full-disk-access")
                    .catch((err) => toast(String(err), "error"))
                }
              >
                打开完全磁盘访问设置
              </Button>
              <Button icon={<RefreshCw size={16} />} onClick={() => refresh({ force: true })}>
                重新扫描
              </Button>
            </div>
          }
        />
      ) : (
        <>
          <Card className="summary-strip manage-summary">
            <div className="summary-main">
              <span className="summary-icon">
                <ShieldX size={22} />
              </span>
              <div>
                <strong>
                  {invalidCount > 0
                    ? `${invalidCount} 个应用的权限记录已失效`
                    : "隐私权限记录正常"}
                </strong>
                <span>
                  共 {apps.length} 个应用 · {scan.totalEntries} 条记录 ·{" "}
                  {formatDate(scan.scannedAt)}
                </span>
              </div>
            </div>
            <div className="summary-side">
              <div>
                <span>已选择</span>
                <strong>{selectedApps.length}</strong>
              </div>
              <div>
                <span>在废纸篓</span>
                <strong>{trashCount}</strong>
              </div>
            </div>
            <div className="head-actions">
              <Button
                icon={<RefreshCw size={16} />}
                loading={loading || refreshing}
                disabled={cleaning}
                onClick={() => refresh({ force: true })}
              >
                重新扫描
              </Button>
              <Button
                variant={selectionResets ? "ghost" : "danger"}
                icon={selectionResets ? <RotateCcw size={15} /> : <Trash2 size={16} />}
                disabled={selectedApps.length === 0 || cleaning}
                loading={cleaning}
                onClick={() => askClean(selectedApps)}
              >
                {selectionResets ? "重置权限" : "清理记录"}
                {selectedApps.length > 0 ? ` ${selectedApps.length}` : ""}
              </Button>
            </div>
          </Card>

          {scan.warnings.length > 0 && (
            <Card className="clean-note">
              <AlertTriangle size={16} />
              <p>{scan.warnings.join("；")}</p>
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
                className={`chip ${filter === "invalid" ? "is-active" : ""}`}
                onClick={() => setFilter("invalid")}
              >
                已失效 {invalidCount}
              </button>
              <button
                className={`chip ${filter === "valid" ? "is-active" : ""}`}
                onClick={() => setFilter("valid")}
              >
                有效 {apps.length - invalidCount}
              </button>
              <button
                className={`chip ${filter === "user" ? "is-active" : ""}`}
                onClick={() => setFilter("user")}
              >
                用户级 {userCount}
              </button>
              <button
                className={`chip ${filter === "system" ? "is-active" : ""}`}
                onClick={() => setFilter("system")}
              >
                系统级 {systemCount}
              </button>
              {trashCount > 0 && (
                <button
                  className={`chip ${filter === "trash" ? "is-active" : ""}`}
                  onClick={() => setFilter("trash")}
                >
                  在废纸篓 {trashCount}
                </button>
              )}
            </div>
            <div className="search">
              <Search size={15} />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="搜索应用名称、Bundle ID 或权限"
              />
            </div>
          </div>

          {apps.length === 0 ? (
            <EmptyState
              icon={<ShieldCheck size={24} />}
              title="没有发现隐私权限记录"
              hint="当前没有第三方应用在隐私与安全性中留下授权记录。"
              action={
                <Button icon={<RefreshCw size={16} />} onClick={() => refresh({ force: true })}>
                  重新扫描
                </Button>
              }
            />
          ) : filtered.length === 0 ? (
            <EmptyState
              icon={<Search size={24} />}
              title="没有符合筛选条件的记录"
              hint="换个关键词，或切换到「全部」查看。"
            />
          ) : (
            <Card className="installer-list">
              <div className="installer-list-head">
                <button className="link-btn" onClick={toggleSelectAll}>
                  {allSelected ? "取消全选" : "全选当前列表"}
                </button>
                <span>共 {filtered.length} 项</span>
              </div>
              {filtered.map((app) => (
                <div
                  className={`installer-row ${app.invalid ? "is-invalid" : ""}`}
                  key={app.id}
                  onContextMenu={(event) => {
                    if (app.path) pathMenu(event, app.path, { isDir: true, canOpen: false });
                  }}
                >
                  <Checkbox
                    checked={selected.has(app.id)}
                    disabled={cleaning}
                    onChange={(value) =>
                      setSelected((current) => {
                        const next = new Set(current);
                        if (value) next.add(app.id);
                        else next.delete(app.id);
                        return next;
                      })
                    }
                    className="installer-check"
                    label={
                      <span className="installer-label">
                        <span className="installer-name">{app.name}</span>
                        {app.invalid ? (
                          <span className="badge badge-danger">已失效</span>
                        ) : (
                          <span className="badge badge-ok">有效</span>
                        )}
                        <span
                          className={`badge ${app.scope === "system" ? "badge-warn" : "badge-ok"}`}
                        >
                          {SCOPE_LABEL[app.scope] ?? app.scope}
                        </span>
                        {app.inTrash && <span className="badge badge-warn">在废纸篓</span>}
                      </span>
                    }
                    hint={
                      <span className="app-update-hint">
                        <span className="app-update-note">
                          {app.services.map((service) => service.label).join(" · ")}
                        </span>
                        <span className="path-text" title={app.client}>
                          {app.client}
                          {app.lastModified > 0
                            ? ` · 最后修改 ${formatDate(app.lastModified)}`
                            : ""}
                        </span>
                      </span>
                    }
                  />
                  <div className="app-update-actions">
                    {app.path && (
                      <button
                        className="icon-btn"
                        title="在文件夹中显示"
                        onClick={() => api.reveal(app.path!).catch(() => {})}
                      >
                        <FolderOpen size={16} />
                      </button>
                    )}
                    <button
                      className="icon-btn"
                      title={app.invalid ? "清理记录" : "重置权限"}
                      disabled={cleaning}
                      onClick={() => askClean([app])}
                    >
                      {app.invalid ? <Trash2 size={16} /> : <RotateCcw size={16} />}
                    </button>
                  </div>
                </div>
              ))}
            </Card>
          )}

          <Card className="clean-note">
            <LockKeyhole size={16} />
            <p>
              列表包含所有第三方应用在 TCC 数据库里的授权记录。标记为「已失效」的项目所对应的应用已经不存在，
              可以安全清理；重置仍安装应用的权限后，它下次使用相关功能时会重新请求授权。
              用户级和系统级记录都会通过系统权限服务一并重置，通常无需管理员密码。
            </p>
          </Card>
        </>
      )}

      <Modal
        open={confirmOpen}
        title={`处理 ${pending.length} 个应用的隐私权限？`}
        description={[
          pendingInvalid > 0
            ? `${pendingInvalid} 个应用已经不在系统中，会直接删除它们的权限记录。`
            : "",
          pendingValid > 0
            ? `${pendingValid} 个应用仍然安装着，重置后它们下次使用相关功能时会重新请求授权。`
            : "",
          pendingSystem > 0
            ? `其中 ${pendingSystem} 项位于系统级数据库，会一并重置。`
            : "",
        ]
          .filter(Boolean)
          .join(" ")}
        onClose={() => setConfirmOpen(false)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setConfirmOpen(false)}>
              取消
            </Button>
            <Button
              variant={resetting ? "primary" : "danger"}
              icon={resetting ? <RotateCcw size={15} /> : <Trash2 size={16} />}
              onClick={() => void runClean()}
            >
              {resetting ? "开始重置" : "开始清理"}
            </Button>
          </>
        }
      >
        <ul className="confirm-list">
          {pending.slice(0, 12).map((app) => (
            <li key={app.id}>
              <span>{app.name}</span>
              <em>{app.services.map((service) => service.label).join("、")}</em>
            </li>
          ))}
        </ul>
        {pending.length > 12 && (
          <p className="app-update-more">以及另外 {pending.length - 12} 项</p>
        )}
      </Modal>

      <Modal
        open={!!result}
        title={
          result?.cancelled
            ? "已取消"
            : result && result.removedApps > 0
              ? "处理完成"
              : "没有处理任何记录"
        }
        description={
          result
            ? result.cancelled
              ? "已取消管理员授权，未处理的项目保持原样。"
              : `已处理 ${result.removedApps} 个应用、${result.removedEntries} 条记录。`
            : ""
        }
        onClose={() => setResult(null)}
        footer={
          <Button variant="primary" onClick={() => setResult(null)}>
            完成
          </Button>
        }
      >
        {result && result.messages.length > 0 && (
          <ul className="confirm-list">
            {result.messages.map((message) => (
              <li key={message}>
                <span>{message}</span>
              </li>
            ))}
          </ul>
        )}
        {result && result.failed.length > 0 && (
          <div className="result-errors">
            <strong>{result.failed.length} 项未能处理</strong>
            <ul>
              {result.failed.map((failure) => (
                <li key={failure}>{failure}</li>
              ))}
            </ul>
          </div>
        )}
      </Modal>
    </>
  );
}

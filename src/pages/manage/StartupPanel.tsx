import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  FolderOpen,
  LockKeyhole,
  Power,
  PowerOff,
  RefreshCw,
  Rocket,
  Search,
  Settings2,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { api } from "../../api";
import { useCachedQuery } from "../../cache";
import { formatDate } from "../../format";
import type { StartupCleanResult, StartupLeftover, StartupScan } from "../../types";
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

type Filter = "all" | "invalid" | "valid" | "disabled" | "manual";
type KindFilter = "all" | "login" | "background";

const KIND_LABEL: Record<string, string> = {
  login: "登录时打开",
  background: "后台活动",
};

const KIND_GROUPS: { kind: "login" | "background"; label: string; hint: string }[] = [
  {
    kind: "login",
    label: "登录时打开",
    hint: "登录后自动启动的应用和项目",
  },
  {
    kind: "background",
    label: "App 后台活动",
    hint: "应用注册的辅助进程，由 launchd 在后台运行",
  },
];

const TYPE_LABEL: Record<string, string> = {
  app: "应用",
  "login-item": "登录项",
  agent: "LaunchAgent",
  daemon: "LaunchDaemon",
  importer: "导入器",
  plugin: "插件",
  "app-extension": "扩展",
  other: "其他",
};

function itemDetail(item: StartupLeftover) {
  return [item.developer, TYPE_LABEL[item.itemType] ?? item.itemType, item.bundleId]
    .filter(Boolean)
    .join(" · ");
}

export function StartupPanel() {
  const toast = useToast();
  const pathMenu = usePathMenu();
  const platform = usePlatform();
  const {
    data: scan,
    loading,
    refreshing,
    error,
    refresh,
  } = useCachedQuery<StartupScan>("startup-items", api.scanStartupLeftovers, {
    ttl: 60_000,
    enabled: platform === "macos",
  });

  const [filter, setFilter] = useState<Filter>("all");
  const [kindFilter, setKindFilter] = useState<KindFilter>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [pending, setPending] = useState<StartupLeftover[]>([]);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [toggling, setToggling] = useState<"enable" | "disable" | null>(null);
  const [result, setResult] = useState<StartupCleanResult | null>(null);
  const initialized = useRef(false);

  const items = scan?.items ?? [];

  // 首次自动选中可移除的失效项；刷新后只保留仍然存在的项目。
  useEffect(() => {
    if (!scan) return;
    const ids = new Set(scan.items.map((item) => item.id));
    if (!initialized.current) {
      initialized.current = true;
      setSelected(
        new Set(scan.items.filter((item) => item.invalid && item.removable).map((item) => item.id)),
      );
      return;
    }
    setSelected((current) => new Set([...current].filter((id) => ids.has(id))));
  }, [scan]);

  const invalidCount = items.filter((item) => item.invalid).length;
  const disabledCount = items.filter((item) => item.enabled === false).length;
  const manualCount = items.filter((item) => !item.canToggle && !item.removable).length;
  const loginCount = items.filter((item) => item.kind === "login").length;
  const backgroundCount = items.length - loginCount;

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return items.filter((item) => {
      if (kindFilter !== "all" && item.kind !== kindFilter) return false;
      if (filter === "invalid" && !item.invalid) return false;
      if (filter === "valid" && item.invalid) return false;
      if (filter === "disabled" && item.enabled !== false) return false;
      if (filter === "manual" && (item.canToggle || item.removable)) return false;
      if (!needle) return true;
      return (
        item.name.toLowerCase().includes(needle) ||
        (item.bundleId ?? "").toLowerCase().includes(needle) ||
        (item.developer ?? "").toLowerCase().includes(needle) ||
        (item.path ?? "").toLowerCase().includes(needle) ||
        (item.plistPath ?? "").toLowerCase().includes(needle)
      );
    });
  }, [items, filter, kindFilter, query]);

  const groups = KIND_GROUPS.map((group) => ({
    ...group,
    items: filtered.filter((item) => item.kind === group.kind),
  })).filter((group) => group.items.length > 0);

  const selectedItems = items.filter((item) => selected.has(item.id));
  const selectedRemovable = selectedItems.filter((item) => item.removable);
  const selectedDisable = selectedItems.filter(
    (item) => item.canToggle && item.enabled !== false,
  );
  const selectedEnable = selectedItems.filter(
    (item) => item.canToggle && item.enabled === false,
  );
  const allSelected = filtered.length > 0 && filtered.every((item) => selected.has(item.id));
  const adminCount = pending.filter((item) => item.requiresAdmin).length;
  const stillInstalled = pending.filter((item) => !item.invalid).length;

  const toggleSelectAll = () => {
    setSelected((current) => {
      const next = new Set(current);
      if (allSelected) filtered.forEach((item) => next.delete(item.id));
      else filtered.forEach((item) => next.add(item.id));
      return next;
    });
  };

  const askRemove = (list: StartupLeftover[]) => {
    if (list.length === 0) return;
    setPending(list);
    setConfirmOpen(true);
  };

  const runRemove = async () => {
    if (pending.length === 0) return;
    setConfirmOpen(false);
    setCleaning(true);
    try {
      const data = await api.cleanStartupLeftovers(pending.map((item) => ({ id: item.id })));
      setResult(data);
      if (data.removedItems > 0) {
        toast(`已移除 ${data.removedItems} 个启动项`, "success");
      } else if (data.failed.length === 0 && !data.cancelled) {
        toast("没有需要移除的项目", "info");
      }
      setPending([]);
      await refresh({ force: true });
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setCleaning(false);
    }
  };

  const setEnabled = async (list: StartupLeftover[], enabled: boolean) => {
    if (list.length === 0) return;
    setToggling(enabled ? "enable" : "disable");
    let done = 0;
    const failures: string[] = [];
    for (const item of list) {
      try {
        await api.setStartupItemEnabled(item.id, enabled);
        done += 1;
      } catch (err) {
        failures.push(`${item.name}：${err}`);
      }
    }
    if (done > 0) toast(`已${enabled ? "启用" : "禁用"} ${done} 项`, "success");
    if (failures.length > 0) {
      toast(`${failures.length} 项未能切换：${failures[0]}`, "error");
    }
    await refresh({ force: true });
    setToggling(null);
  };

  const openLoginItems = () =>
    api.openPrivacySettings("login-items").catch((err) => toast(String(err), "error"));

  if (platform !== "macos") {
    return (
      <EmptyState
        icon={<LockKeyhole size={24} />}
        title="当前平台没有登录项和后台活动"
        hint="这些是 macOS 的系统机制，其他平台的启动项跟随应用一起删除。"
      />
    );
  }

  return (
    <>
      {error && <Card className="error-card">{error}</Card>}

      {loading && !scan ? (
        <Spinner label="正在读取登录项和后台项目…" />
      ) : !scan || !scan.available ? (
        <EmptyState
          icon={<AlertTriangle size={24} />}
          title="无法读取登录项和后台项目"
          hint="系统数据库或 launchd 目录当前不可读，请稍后重试。"
          action={
            <Button icon={<RefreshCw size={16} />} onClick={() => refresh({ force: true })}>
              重新扫描
            </Button>
          }
        />
      ) : (
        <>
          <Card className="summary-strip manage-summary">
            <div className="summary-main">
              <span className="summary-icon">
                <Rocket size={22} />
              </span>
              <div>
                <strong>
                  {invalidCount > 0 ? `${invalidCount} 项启动项已失效` : "启动项状态正常"}
                </strong>
                <span>
                  共 {items.length} 项 · {loginCount} 个登录项 · {backgroundCount} 个后台活动 ·{" "}
                  {disabledCount} 项已禁用 · {formatDate(scan.scannedAt)}
                </span>
              </div>
            </div>
            <div className="summary-side">
              <div>
                <span>已选择</span>
                <strong>{selectedItems.length}</strong>
              </div>
              <div>
                <span>需系统设置</span>
                <strong>{manualCount}</strong>
              </div>
            </div>
            <div className="head-actions">
              <Button
                icon={<RefreshCw size={16} />}
                loading={loading || refreshing}
                disabled={cleaning || toggling !== null}
                onClick={() => refresh({ force: true })}
              >
                重新扫描
              </Button>
              {selectedDisable.length > 0 && (
                <Button
                  icon={<PowerOff size={15} />}
                  loading={toggling === "disable"}
                  disabled={cleaning || toggling !== null}
                  onClick={() => void setEnabled(selectedDisable, false)}
                >
                  禁用 {selectedDisable.length}
                </Button>
              )}
              {selectedEnable.length > 0 && (
                <Button
                  icon={<Power size={15} />}
                  loading={toggling === "enable"}
                  disabled={cleaning || toggling !== null}
                  onClick={() => void setEnabled(selectedEnable, true)}
                >
                  启用 {selectedEnable.length}
                </Button>
              )}
              <Button
                variant="danger"
                icon={<Trash2 size={16} />}
                disabled={selectedRemovable.length === 0 || cleaning || toggling !== null}
                loading={cleaning}
                onClick={() => askRemove(selectedRemovable)}
              >
                移除
                {selectedRemovable.length > 0 ? ` ${selectedRemovable.length}` : ""}
              </Button>
            </div>
          </Card>

          {scan.warnings.length > 0 && (
            <Card className="clean-note">
              <AlertTriangle size={16} />
              <p>{scan.warnings.join("；")}</p>
            </Card>
          )}

          <div className="startup-kind-row" role="group" aria-label="启动项类型">
            <button
              className={`chip ${kindFilter === "all" ? "is-active" : ""}`}
              onClick={() => setKindFilter("all")}
            >
              全部类型 {items.length}
            </button>
            <button
              className={`chip ${kindFilter === "login" ? "is-active" : ""}`}
              onClick={() => setKindFilter("login")}
            >
              登录时打开 {loginCount}
            </button>
            <button
              className={`chip ${kindFilter === "background" ? "is-active" : ""}`}
              onClick={() => setKindFilter("background")}
            >
              App 后台活动 {backgroundCount}
            </button>
          </div>

          <div className="app-update-toolbar">
            <div className="filter-row">
              <button
                className={`chip ${filter === "all" ? "is-active" : ""}`}
                onClick={() => setFilter("all")}
              >
                全部状态 {items.length}
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
                有效 {items.length - invalidCount}
              </button>
              {disabledCount > 0 && (
                <button
                  className={`chip ${filter === "disabled" ? "is-active" : ""}`}
                  onClick={() => setFilter("disabled")}
                >
                  已禁用 {disabledCount}
                </button>
              )}
              {manualCount > 0 && (
                <button
                  className={`chip ${filter === "manual" ? "is-active" : ""}`}
                  onClick={() => setFilter("manual")}
                >
                  需系统设置 {manualCount}
                </button>
              )}
            </div>
            <div className="search">
              <Search size={15} />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="搜索应用、Bundle ID 或路径"
              />
            </div>
          </div>

          {items.length === 0 ? (
            <EmptyState
              icon={<ShieldCheck size={24} />}
              title="没有发现登录项或后台活动"
              hint="当前系统里没有第三方应用注册登录项或后台活动。"
              action={
                <Button icon={<RefreshCw size={16} />} onClick={() => refresh({ force: true })}>
                  重新扫描
                </Button>
              }
            />
          ) : filtered.length === 0 ? (
            <EmptyState
              icon={<Search size={24} />}
              title="没有符合筛选条件的项目"
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
              {groups.map((group) => (
                <section className="startup-group" key={group.kind}>
                  <div className="startup-group-head">
                    <strong>{group.label}</strong>
                    <span className="startup-group-count">{group.items.length} 项</span>
                    <span className="startup-group-hint">{group.hint}</span>
                  </div>
                  {group.items.map((item) => {
                    const detail = itemDetail(item);
                    return (
                    <div
                      className={`installer-row ${item.invalid ? "is-invalid" : ""}`}
                      key={item.id}
                      onContextMenu={(event) => {
                        if (item.path) pathMenu(event, item.path, { canOpen: false });
                      }}
                    >
                      <Checkbox
                        checked={selected.has(item.id)}
                        disabled={cleaning || toggling !== null}
                        onChange={(value) =>
                          setSelected((current) => {
                            const next = new Set(current);
                            if (value) next.add(item.id);
                            else next.delete(item.id);
                            return next;
                          })
                        }
                        className="installer-check"
                        label={
                          <span className="installer-label">
                            <span className="installer-name">{item.name}</span>
                            {item.invalid ? (
                              <span className="badge badge-danger">已失效</span>
                            ) : (
                              <span className="badge badge-ok">有效</span>
                            )}
                            <span
                              className={`badge ${item.kind === "login" ? "badge-accent" : ""}`}
                            >
                              {KIND_LABEL[item.kind] ?? item.kind}
                            </span>
                            {item.enabled === false && (
                              <span className="badge badge-warn">已禁用</span>
                            )}
                            {item.requiresAdmin && <span className="badge badge-warn">需管理员</span>}
                            {!item.canToggle && !item.removable && (
                              <span className="badge badge-warn">需系统设置</span>
                            )}
                          </span>
                        }
                        hint={
                          <span className="app-update-hint">
                            <span className="app-update-note">{detail || "—"}</span>
                            {(item.path || item.plistPath) && (
                              <span
                                className="path-text"
                                title={item.plistPath ?? item.path ?? ""}
                              >
                                {item.plistPath ?? item.path}
                              </span>
                            )}
                            {item.note && <span className="app-update-note">{item.note}</span>}
                          </span>
                        }
                      />
                      <div className="app-update-actions">
                        {item.canToggle && (
                          <button
                            className="icon-btn"
                            title={item.enabled === false ? "启用" : "禁用"}
                            disabled={cleaning || toggling !== null}
                            onClick={() => void setEnabled([item], item.enabled === false)}
                          >
                            {item.enabled === false ? <Power size={16} /> : <PowerOff size={16} />}
                          </button>
                        )}
                        {item.path && (
                          <button
                            className="icon-btn"
                            title="在文件夹中显示"
                            onClick={() => api.reveal(item.path!).catch(() => {})}
                          >
                            <FolderOpen size={16} />
                          </button>
                        )}
                        {item.removable ? (
                          <button
                            className="icon-btn"
                            title="移除"
                            disabled={cleaning || toggling !== null}
                            onClick={() => askRemove([item])}
                          >
                            <Trash2 size={16} />
                          </button>
                        ) : (
                          !item.canToggle && (
                            <button
                              className="icon-btn"
                              title="在系统设置中管理"
                              onClick={() => void openLoginItems()}
                            >
                              <Settings2 size={16} />
                            </button>
                          )
                        )}
                      </div>
                    </div>
                    );
                  })}
                </section>
              ))}
            </Card>
          )}

          <Card className="clean-note">
            <Settings2 size={16} />
            <p>
              列表按「登录时打开」和「App 后台活动」分成两组：前者是登录后自动启动的应用，
              后者是应用注册的辅助进程。标记为「已失效」的项目所对应的应用已经不存在；
              有独立 launchd 文件的项目可以在本页禁用、启用或移除，其余项目需要在
              「系统设置 → 通用 → 登录项与扩展」中处理。系统级项目会要求输入管理员密码。
            </p>
            {manualCount > 0 && (
              <div className="head-actions">
                <Button icon={<Settings2 size={16} />} onClick={openLoginItems}>
                  打开登录项设置
                </Button>
              </div>
            )}
          </Card>
        </>
      )}

      <Modal
        open={confirmOpen}
        title={`移除 ${pending.length} 个启动项？`}
        description={
          stillInstalled > 0
            ? `其中 ${stillInstalled} 项仍对应已安装的应用，移除后该应用可能无法随系统启动${
                adminCount > 0 ? `；${adminCount} 项位于系统目录，会要求输入管理员密码` : ""
              }。`
            : adminCount > 0
              ? `其中 ${adminCount} 项位于系统目录，会要求输入管理员密码。`
              : "这些项目对应的应用已经不在系统中，删除后无法恢复。"
        }
        onClose={() => setConfirmOpen(false)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setConfirmOpen(false)}>
              取消
            </Button>
            <Button
              variant="danger"
              icon={<Trash2 size={16} />}
              onClick={() => void runRemove()}
            >
              开始移除
            </Button>
          </>
        }
      >
        <ul className="confirm-list">
          {pending.slice(0, 12).map((item) => (
            <li key={item.id}>
              <span>{item.name}</span>
              <em>{KIND_LABEL[item.kind] ?? item.kind}</em>
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
            : result && result.removedItems > 0
              ? "移除完成"
              : "没有移除任何项目"
        }
        description={
          result
            ? result.cancelled
              ? "取消管理员授权后没有删除任何文件。"
              : `已移除 ${result.removedItems} 个启动项。`
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
            <strong>{result.failed.length} 项未能移除</strong>
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

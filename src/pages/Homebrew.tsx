import { useMemo, useState } from "react";
import {
  ArrowUp,
  Beer,
  Download,
  ExternalLink,
  Plus,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";
import { api } from "../api";
import { useCachedQuery } from "../cache";
import { relativeTime } from "../format";
import type { BrewOverview, BrewPackage, BrewResult, BrewTap } from "../types";
import {
  Button,
  Card,
  Checkbox,
  EmptyState,
  Modal,
  Spinner,
  useToast,
} from "../components/ui";
import { usePathMenu } from "../components/ContextMenu";

type Tab = "installed" | "outdated" | "taps";

const TAP_NAME = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;
const BREW_SITE = "https://brew.sh";

function packageLabel(pkg: BrewPackage): string {
  return pkg.displayName && pkg.displayName !== pkg.name
    ? `${pkg.name} · ${pkg.displayName}`
    : pkg.name;
}

export function Homebrew() {
  const toast = useToast();
  const pathMenu = usePathMenu();
  const {
    data,
    loading,
    refreshing,
    error,
    refresh,
  } = useCachedQuery<BrewOverview>("brew", api.brewOverview, { ttl: 60_000 });

  const [tab, setTab] = useState<Tab>("installed");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [result, setResult] = useState<BrewResult | null>(null);
  const [uninstallTarget, setUninstallTarget] = useState<BrewPackage | null>(null);
  const [zap, setZap] = useState(false);
  const [upgradeAllOpen, setUpgradeAllOpen] = useState(false);
  const [untapTarget, setUntapTarget] = useState<BrewTap | null>(null);
  const [tapName, setTapName] = useState("");
  const [addingTap, setAddingTap] = useState(false);

  const switchTab = (next: Tab) => {
    setTab(next);
    setQuery("");
  };

  const packages = data?.packages ?? [];
  const taps = data?.taps ?? [];
  const outdatedCount = data?.outdatedCount ?? 0;
  const needle = query.trim().toLowerCase();

  const filteredPackages = useMemo(() => {
    const base = tab === "outdated" ? packages.filter((pkg) => pkg.outdated) : packages;
    if (!needle) return base;
    return base.filter(
      (pkg) =>
        pkg.name.toLowerCase().includes(needle) ||
        pkg.displayName.toLowerCase().includes(needle) ||
        pkg.description.toLowerCase().includes(needle) ||
        pkg.tap.toLowerCase().includes(needle),
    );
  }, [packages, tab, needle]);

  const filteredTaps = useMemo(() => {
    if (!needle) return taps;
    return taps.filter(
      (tap) => tap.name.toLowerCase().includes(needle) || tap.path.toLowerCase().includes(needle),
    );
  }, [taps, needle]);

  const run = async (
    label: string,
    action: () => Promise<BrewResult>,
    successMessage?: string,
  ) => {
    setBusy(label);
    try {
      const data = await action();
      setResult(data);
      if (data.failed.length === 0) {
        toast(successMessage ?? `${label}完成`, "success");
      } else {
        toast(`${label}完成，${data.failed.length} 项失败`, "error");
      }
      await refresh({ force: true });
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setBusy(null);
    }
  };

  const updateMetadata = () =>
    run("正在更新 Homebrew 元数据", api.brewUpdate, "软件源元数据已更新");

  const upgradePackage = (pkg: BrewPackage) =>
    run(
      `正在升级 ${pkg.name}`,
      () => api.brewUpgrade([{ name: pkg.name, kind: pkg.kind }]),
      `${pkg.name} 已升级`,
    );

  const upgradeAll = async () => {
    setUpgradeAllOpen(false);
    await run("正在升级全部可更新软件", api.brewUpgradeAll, "全部升级完成");
  };

  const uninstallPackage = async () => {
    const target = uninstallTarget;
    if (!target) return;
    setUninstallTarget(null);
    await run(
      `正在卸载 ${target.name}`,
      () => api.brewUninstall([{ name: target.name, kind: target.kind }], zap),
      `${target.name} 已卸载`,
    );
  };

  const addTap = async () => {
    const name = tapName.trim();
    if (!TAP_NAME.test(name)) {
      toast("请输入 user/repo 格式的软件源名称", "error");
      return;
    }
    setAddingTap(true);
    try {
      const data = await api.brewAddTap(name);
      setResult(data);
      if (data.failed.length === 0) {
        setTapName("");
        toast(`软件源 ${name} 已添加`, "success");
      } else {
        toast(`添加软件源失败`, "error");
      }
      await refresh({ force: true });
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setAddingTap(false);
    }
  };

  const removeTap = async () => {
    const target = untapTarget;
    if (!target) return;
    setUntapTarget(null);
    await run(
      `正在移除软件源 ${target.name}`,
      () => api.brewRemoveTap(target.name),
      `软件源 ${target.name} 已移除`,
    );
  };

  const header = (
    <header className="page-head">
      <div>
        <h1>Homebrew</h1>
        <p>
          管理通过 Homebrew 安装的 formula、cask 与软件源
          {data?.available && data.version ? ` · ${data.version}` : ""}
        </p>
      </div>
      {data?.available && (
        <div className="head-actions">
          <Button
            icon={<RefreshCw size={16} />}
            loading={loading || refreshing}
            disabled={!!busy}
            onClick={() => refresh({ force: true })}
          >
            刷新
          </Button>
          <Button
            icon={<Download size={16} />}
            disabled={!!busy}
            onClick={() => void updateMetadata()}
          >
            更新元数据
          </Button>
          {outdatedCount > 0 && (
            <Button
              variant="primary"
              icon={<ArrowUp size={16} />}
              disabled={!!busy}
              onClick={() => setUpgradeAllOpen(true)}
            >
              全部升级 ({outdatedCount})
            </Button>
          )}
        </div>
      )}
    </header>
  );

  return (
    <div className="page">
      {header}

      {error && <Card className="error-card">{error}</Card>}

      {loading && !data ? (
        <Spinner label="正在读取 Homebrew 数据…" />
      ) : !data?.available ? (
        <EmptyState
          icon={<Beer size={24} />}
          title="未检测到 Homebrew"
          hint="安装 Homebrew 后即可在这里管理 formula、cask 与 tap。如果已经安装，请确认 brew 位于 /opt/homebrew/bin 或 /usr/local/bin。"
          action={
            <Button icon={<ExternalLink size={15} />} onClick={() => api.openExternal(BREW_SITE).catch(() => {})}>
              打开 brew.sh
            </Button>
          }
        />
      ) : (
        <>
          <Card className="summary-strip">
            <div className="summary-main">
              <span className="summary-icon">
                <Beer size={22} />
              </span>
              <div>
                <strong>{packages.length}</strong>
                <span>已安装软件 · {data.prefix ?? "Homebrew"}</span>
              </div>
            </div>
            <div className="summary-side">
              <div>
                <span>Formula</span>
                <strong>{data.formulaCount}</strong>
              </div>
              <div>
                <span>Cask</span>
                <strong>{data.caskCount}</strong>
              </div>
              <div>
                <span>可更新</span>
                <strong className={outdatedCount ? "tone-warn" : ""}>{outdatedCount}</strong>
              </div>
              <div>
                <span>软件源</span>
                <strong>{taps.length}</strong>
              </div>
            </div>
          </Card>

          <div className="filter-row">
            <button
              className={`chip ${tab === "installed" ? "is-active" : ""}`}
              onClick={() => setTab("installed")}
            >
              已安装 {packages.length}
            </button>
            <button
              className={`chip ${tab === "outdated" ? "is-active" : ""}`}
              onClick={() => switchTab("outdated")}
            >
              可更新 {outdatedCount}
            </button>
            <button
              className={`chip ${tab === "taps" ? "is-active" : ""}`}
              onClick={() => setTab("taps")}
            >
              软件源 {taps.length}
            </button>
          </div>

          {tab !== "taps" && (
            <div className="brew-toolbar">
              <div className="search">
                <Search size={15} />
                <input
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="搜索软件名称、描述或 tap"
                />
              </div>
            </div>
          )}

          {tab === "taps" ? (
            <>
              <Card className="brew-tap-add">
                <div>
                  <strong>添加软件源</strong>
                  <p className="small muted">格式为 user/repo，例如 homebrew/cask-fonts。</p>
                </div>
                <div className="brew-tap-form">
                  <input
                    className="input"
                    value={tapName}
                    onChange={(event) => setTapName(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void addTap();
                    }}
                    placeholder="user/repo"
                  />
                  <Button
                    variant="primary"
                    icon={<Plus size={15} />}
                    loading={addingTap}
                    disabled={!tapName.trim() || !!busy}
                    onClick={() => void addTap()}
                  >
                    添加
                  </Button>
                </div>
              </Card>

              <Card className="brew-list">
                <div className="brew-list-head">
                  <span>{filteredTaps.length} 个软件源</span>
                  <span>brew tap</span>
                </div>
                {filteredTaps.length === 0 ? (
                  <p className="muted list-empty">
                    {taps.length === 0 ? "还没有添加额外的软件源。" : "没有匹配的软件源。"}
                  </p>
                ) : (
                  filteredTaps.map((tap) => (
                    <div
                      className="brew-row"
                      key={tap.name}
                      onContextMenu={(event) => pathMenu(event, tap.path, { isDir: true })}
                    >
                      <span className={`brew-kind ${tap.official ? "formula" : "tap"}`}>
                        {tap.official ? "官方" : "Tap"}
                      </span>
                      <div className="brew-main">
                        <div className="brew-title">
                          <strong>{tap.name}</strong>
                          {tap.official && <span className="badge">官方</span>}
                        </div>
                        <div className="brew-meta">
                          <span>{tap.formulaCount} 个 formula</span>
                          <span>{tap.caskCount} 个 cask</span>
                        </div>
                        <span className="path-text" title={tap.path}>
                          {tap.path}
                        </span>
                      </div>
                      <div className="brew-actions">
                        <Button
                          size="sm"
                          variant="quiet"
                          icon={<Trash2 size={14} />}
                          disabled={!!busy}
                          onClick={() => setUntapTarget(tap)}
                        >
                          移除
                        </Button>
                      </div>
                    </div>
                  ))
                )}
              </Card>
            </>
          ) : (
            <Card className="brew-list">
              <div className="brew-list-head">
                <span>{filteredPackages.length} 个软件</span>
                <span>{tab === "outdated" ? "仅显示可更新项" : "brew list"}</span>
              </div>
              {filteredPackages.length === 0 ? (
                <p className="muted list-empty">
                  {tab === "outdated"
                    ? "所有软件都是最新版本。"
                    : packages.length === 0
                      ? "还没有通过 Homebrew 安装软件。"
                      : "没有匹配的软件。"}
                </p>
              ) : (
                filteredPackages.map((pkg) => (
                  <div className={`brew-row ${pkg.outdated ? "is-outdated" : ""}`} key={`${pkg.kind}:${pkg.name}`}>
                    <span className={`brew-kind ${pkg.kind}`}>
                      {pkg.kind === "cask" ? "Cask" : "Formula"}
                    </span>
                    <div className="brew-main">
                      <div className="brew-title">
                        <strong>{pkg.name}</strong>
                        {pkg.displayName && pkg.displayName !== pkg.name && (
                          <span className="brew-display" title={packageLabel(pkg)}>
                            {pkg.displayName}
                          </span>
                        )}
                        {pkg.pinned && <span className="badge">已固定</span>}
                        {pkg.outdated && <span className="badge badge-warn">可更新</span>}
                      </div>
                      <p className="brew-desc" title={pkg.description}>
                        {pkg.description || "无描述"}
                      </p>
                      <div className="brew-meta">
                        <span>v{pkg.version || "未知"}</span>
                        {pkg.outdated && pkg.latestVersion && (
                          <span className="brew-latest">→ v{pkg.latestVersion}</span>
                        )}
                        {pkg.tap && <span>{pkg.tap}</span>}
                        {pkg.installedAt ? (
                          <span>安装于 {relativeTime(pkg.installedAt)}</span>
                        ) : null}
                        {pkg.kind === "formula" && !pkg.requested && (
                          <span>作为依赖安装</span>
                        )}
                      </div>
                    </div>
                    <div className="brew-actions">
                      {pkg.outdated && (
                        <Button
                          size="sm"
                          icon={<ArrowUp size={14} />}
                          disabled={!!busy}
                          onClick={() => void upgradePackage(pkg)}
                        >
                          升级
                        </Button>
                      )}
                      {pkg.homepage && (
                        <button
                          className="icon-btn"
                          title="打开主页"
                          onClick={() => api.openExternal(pkg.homepage).catch(() => {})}
                        >
                          <ExternalLink size={16} />
                        </button>
                      )}
                      <Button
                        size="sm"
                        variant="quiet"
                        icon={<Trash2 size={14} />}
                        disabled={!!busy}
                        onClick={() => {
                          setZap(false);
                          setUninstallTarget(pkg);
                        }}
                      >
                        卸载
                      </Button>
                    </div>
                  </div>
                ))
              )}
            </Card>
          )}

          <Card className="clean-note">
            <Beer size={16} />
            <p>
              升级与卸载会直接调用 <strong>brew</strong> 命令，可能需要数分钟。卸载 formula 时如果它仍被其他软件依赖，Homebrew 会拒绝执行。
            </p>
          </Card>
        </>
      )}

      <Modal
        open={!!busy}
        title={busy ?? "正在执行"}
        description="brew 命令可能需要数分钟，请保持窗口打开。"
        onClose={() => {}}
      >
        <Spinner label="正在执行 Homebrew 操作…" />
      </Modal>

      <Modal
        open={!!uninstallTarget}
        title={`卸载 ${uninstallTarget ? packageLabel(uninstallTarget) : ""}？`}
        description="卸载后需要重新安装才能恢复，配置文件与缓存是否保留取决于下方选项。"
        tone="danger"
        onClose={() => setUninstallTarget(null)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setUninstallTarget(null)}>
              取消
            </Button>
            <Button variant="danger" onClick={() => void uninstallPackage()}>
              确认卸载
            </Button>
          </>
        }
      >
        {uninstallTarget && (
          <>
            <ul className="confirm-list">
              <li>
                <span>{uninstallTarget.kind === "cask" ? "Cask" : "Formula"}</span>
                <em>v{uninstallTarget.version || "未知"}</em>
              </li>
              {uninstallTarget.tap && (
                <li>
                  <span>来源</span>
                  <em>{uninstallTarget.tap}</em>
                </li>
              )}
              {uninstallTarget.dependencies.length > 0 && (
                <li>
                  <span>依赖</span>
                  <em>{uninstallTarget.dependencies.slice(0, 4).join(", ")}</em>
                </li>
              )}
            </ul>
            {uninstallTarget.kind === "cask" && (
              <Checkbox
                checked={zap}
                onChange={setZap}
                label="同时删除配置与缓存（--zap）"
                hint="会一并清理该软件留在用户目录下的偏好设置与支持文件。"
              />
            )}
          </>
        )}
      </Modal>

      <Modal
        open={upgradeAllOpen}
        title={`升级 ${outdatedCount} 个可更新软件？`}
        description="将执行 brew upgrade，升级全部过期的 formula 与 cask，可能需要较长时间。"
        onClose={() => setUpgradeAllOpen(false)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setUpgradeAllOpen(false)}>
              取消
            </Button>
            <Button variant="primary" icon={<ArrowUp size={15} />} onClick={() => void upgradeAll()}>
              开始升级
            </Button>
          </>
        }
      >
        <ul className="confirm-list">
          {packages
            .filter((pkg) => pkg.outdated)
            .slice(0, 12)
            .map((pkg) => (
              <li key={`${pkg.kind}:${pkg.name}`}>
                <span>{pkg.name}</span>
                <em>
                  v{pkg.version} → v{pkg.latestVersion ?? "?"}
                </em>
              </li>
            ))}
        </ul>
        {outdatedCount > 12 && (
          <p className="muted small">另有 {outdatedCount - 12} 个软件未显示。</p>
        )}
      </Modal>

      <Modal
        open={!!untapTarget}
        title={`移除软件源 ${untapTarget?.name ?? ""}？`}
        description="只会移除本地的 tap 仓库；已安装的软件不会被卸载，但来自该 tap 的其他软件将无法再通过 brew 更新。"
        tone="danger"
        onClose={() => setUntapTarget(null)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setUntapTarget(null)}>
              取消
            </Button>
            <Button variant="danger" onClick={() => void removeTap()}>
              确认移除
            </Button>
          </>
        }
      />

      <Modal
        open={!!result}
        title="Homebrew 操作结果"
        description={
          result
            ? result.failed.length > 0
              ? `${result.ok.length} 项成功，${result.failed.length} 项失败`
              : result.ok.length > 0
                ? `${result.ok.length} 项处理成功`
                : "操作已完成"
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
            <strong>{result.failed.length} 项失败</strong>
            <ul>
              {result.failed.map((failure) => (
                <li key={failure.name} title={failure.message}>
                  {failure.name}：{failure.message}
                </li>
              ))}
            </ul>
          </div>
        )}
        {result?.output && <pre className="brew-output">{result.output}</pre>}
      </Modal>
    </div>
  );
}

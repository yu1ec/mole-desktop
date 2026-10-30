import { useEffect, useMemo, useState } from "react";
import {
  FileCog,
  FolderOpen,
  ListTree,
  RefreshCw,
  Search,
  Terminal,
  Trash2,
} from "lucide-react";
import { api } from "../api";
import { invalidateCachePrefix, useCachedQuery } from "../cache";
import { formatBytes, relativeTime } from "../format";
import type { ConfigPathInfo, ConfigSafety, ConfigTool, OperationResult } from "../types";
import {
  Button,
  Card,
  Checkbox,
  EmptyState,
  Modal,
  Spinner,
  useToast,
} from "../components/ui";
import { CleanDetailModal, clearCleanDetailCache } from "../components/CleanDetailModal";
import { usePathMenu } from "../components/ContextMenu";
import { revealLabel, trashName, usePlatform } from "../platform";

const EMPTY_TOOLS: ConfigTool[] = [];

const SAFETY_ORDER: ConfigSafety[] = ["safe", "caution", "keep"];

const SAFETY_META: Record<
  ConfigSafety,
  { label: string; hint: string; tone: string; title: string }
> = {
  safe: {
    label: "可自动重建",
    hint: "缓存与依赖，删除后会自动重建",
    tone: "safety-safe",
    title: "缓存、临时文件或可重新下载的依赖，删除后工具会自动重建。",
  },
  caution: {
    label: "会丢失配置",
    hint: "配置、凭证与历史，删除后需重新设置",
    tone: "safety-caution",
    title:
      "配置、登录凭证或历史记录，删除后需要重新配置或登录，部分数据无法恢复。",
  },
  keep: {
    label: "禁止删除",
    hint: "删除会破坏当前环境",
    tone: "safety-keep",
    title: "删除可能直接破坏当前环境，已禁止选择。如需清理请手动处理。",
  },
};

/** 删除成功后立刻从界面移除这些路径，后台扫描只用于校正剩余大小。 */
function withoutPaths(tools: ConfigTool[], removed: string[]): ConfigTool[] {
  const gone = new Set(removed);
  return tools
    .map((tool) => {
      const paths = tool.paths.filter((path) => !gone.has(path.path));
      if (paths.length === tool.paths.length) return tool;
      return {
        ...tool,
        paths,
        totalSize: paths.reduce((total, path) => total + path.size, 0),
        totalFiles: paths.reduce((total, path) => total + path.fileCount, 0),
      };
    })
    .filter((tool) => tool.paths.length > 0);
}

/** 优先看 CLI，其次看桌面应用；都没有才视为残留配置。 */
function toolStatus(tool: ConfigTool) {
  if (tool.commandAvailable) {
    return {
      label: "CLI 可用",
      tone: "badge-ok",
      title: "命令行工具仍可在 PATH 或常见包管理器目录中找到",
    };
  }
  if (tool.appInstalled) {
    return {
      label: "应用已安装",
      tone: "badge-ok",
      title: "在应用目录中找到该工具，这些配置可能仍在使用",
    };
  }
  if (tool.hasCli) {
    return {
      label: "未检测到 CLI",
      tone: "badge-warn",
      title: "未在 PATH 或常见包管理器目录中找到命令，这些目录可能只是残留",
    };
  }
  return {
    label: "仅残留数据",
    tone: "badge-warn",
    title: "未找到对应的桌面应用，这些目录可能只是残留",
  };
}

export function Configs() {
  const toast = useToast();
  const pathMenu = usePathMenu();
  const platform = usePlatform();
  const trash = trashName(platform);
  const {
    data: toolData,
    loading,
    refreshing,
    error,
    refresh,
    mutate,
  } = useCachedQuery<ConfigTool[]>("configs", api.scanConfigs, { ttl: 60_000 });
  const tools = toolData ?? EMPTY_TOOLS;
  const [query, setQuery] = useState("");
  const [activeId, setActiveId] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [detail, setDetail] = useState<{ tool: ConfigTool; path: string | null } | null>(null);
  const [detailDirty, setDetailDirty] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<OperationResult | null>(null);

  useEffect(() => {
    if (!toolData) return;
    setActiveId((current) =>
      current && toolData.some((tool) => tool.id === current)
        ? current
        : toolData[0]?.id ?? null,
    );
  }, [toolData]);

  const active = tools.find((tool) => tool.id === activeId) ?? null;

  useEffect(() => {
    setSelected(new Set());
  }, [activeId]);

  const selectablePaths = active
    ? active.paths.filter((path) => path.safety !== "keep")
    : [];
  const protectedCount = active ? active.paths.length - selectablePaths.length : 0;

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return tools;
    return tools.filter(
      (tool) =>
        tool.name.toLowerCase().includes(needle) ||
        tool.description.toLowerCase().includes(needle) ||
        tool.category.toLowerCase().includes(needle) ||
        tool.paths.some((path) => path.display.toLowerCase().includes(needle)),
    );
  }, [tools, query]);

  const selectedPaths = active
    ? active.paths.filter((path) => selected.has(path.path))
    : [];
  const selectedSize = selectedPaths.reduce((total, path) => total + path.size, 0);
  const totalSize = tools.reduce((total, tool) => total + tool.totalSize, 0);
  const protectedRoots = detail
    ? detail.tool.paths.filter((path) => path.safety === "keep").map((path) => path.path)
    : [];

  const togglePath = (path: ConfigPathInfo, checked: boolean) => {
    if (path.safety === "keep") return;
    setSelected((current) => {
      const next = new Set(current);
      if (checked) next.add(path.path);
      else next.delete(path.path);
      return next;
    });
  };

  const deleteSelected = async () => {
    setConfirmOpen(false);
    setRunning(true);
    try {
      const data = await api.deleteConfigPaths([...selected]);
      setResult(data);
      toast(
        `已移入${trash} ${data.files.toLocaleString()} 个文件，释放 ${formatBytes(data.freed)}`,
        data.errors.length ? "info" : "success",
      );
      setSelected(new Set());
      clearCleanDetailCache();
      if (data.removed.length > 0) {
        mutate((current) => withoutPaths(current, data.removed));
      }
      // 重新统计目录可能很慢，放到后台进行，不再阻塞按钮。
      invalidateCachePrefix("configs");
      void refresh({ force: true });
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>配置管理</h1>
          <p>
            AI 编程、编辑器、终端、版本控制与包管理器留下的配置、会话与缓存
            {tools.length > 0 ? ` · ${tools.length} 个工具 · ${formatBytes(totalSize)}` : ""}
          </p>
        </div>
        <Button
          icon={<RefreshCw size={16} />}
          loading={loading || refreshing}
          onClick={() => refresh({ force: true })}
        >
          重新扫描
        </Button>
      </header>

      {error && <Card className="error-card">{error}</Card>}

      {loading && tools.length === 0 ? (
        <Spinner label="正在扫描配置目录…" />
      ) : tools.length === 0 ? (
        <EmptyState
          icon={<FileCog size={24} />}
          title="没有发现开发工具配置"
          hint="扫描范围包括 AI 编程工具、编辑器、Shell、Git、Docker 与常见包管理器的配置目录。"
          action={
            <Button icon={<RefreshCw size={15} />} onClick={() => refresh({ force: true })}>
              重新扫描
            </Button>
          }
        />
      ) : (
        <div className="uninstall-layout">
          <Card className="app-list-card">
            <div className="list-tools">
              <div className="search">
                <Search size={15} />
                <input
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="搜索工具或路径"
                />
              </div>
            </div>
            <div className="app-list">
              {filtered.map((tool) => (
                <button
                  key={tool.id}
                  className={`app-row ${activeId === tool.id ? "is-active" : ""}`}
                  title={tool.description}
                  onClick={() => setActiveId(tool.id)}
                >
                  <span className="app-avatar">{tool.name.slice(0, 1).toUpperCase()}</span>
                  <span className="app-row-main">
                    <strong>{tool.name}</strong>
                    <span>
                      {tool.category}
                      {tool.commandAvailable || tool.appInstalled ? "" : " · 仅残留配置"}
                    </span>
                  </span>
                  <em>{formatBytes(tool.totalSize)}</em>
                </button>
              ))}
              {filtered.length === 0 && <p className="muted list-empty">没有匹配的工具</p>}
            </div>
          </Card>

          <Card className="app-detail">
            {!active ? (
              <EmptyState
                icon={<FileCog size={24} />}
                title="选择一个工具"
                hint="左侧会列出检测到配置文件的开发工具。"
              />
            ) : (
              <>
                <div className="detail-head">
                  <span className="app-avatar large">{active.name.slice(0, 1).toUpperCase()}</span>
                  <div>
                    <h2>{active.name}</h2>
                    <p>{active.description}</p>
                    <span className="muted small">
                      {active.category} · {active.paths.length} 个位置 · 最近修改{" "}
                      {relativeTime(active.modified)}
                    </span>
                  </div>
                  <div className="detail-size">
                    <strong>{formatBytes(active.totalSize)}</strong>
                    <span>{active.totalFiles.toLocaleString()} 个文件</span>
                  </div>
                </div>

                <div className="detail-actions">
                  <span
                    className={`badge ${toolStatus(active).tone}`}
                    title={toolStatus(active).title}
                  >
                    <Terminal size={12} />
                    {toolStatus(active).label}
                  </span>
                  <Button
                    size="sm"
                    icon={<ListTree size={15} />}
                    onClick={() => setDetail({ tool: active, path: null })}
                  >
                    查看全部明细
                  </Button>
                </div>

                <div className="detail-section">
                  <div className="detail-section-head">
                    <h3>配置位置</h3>
                    <span>
                      {selected.size > 0
                        ? `已选 ${selected.size} 项 · ${formatBytes(selectedSize)}`
                        : "默认不选中，避免误删"}
                    </span>
                  </div>
                  <div className="safety-legend">
                    {SAFETY_ORDER.map((key) => (
                      <span key={key} title={SAFETY_META[key].title}>
                        <i className={`config-safety ${SAFETY_META[key].tone}`}>
                          {SAFETY_META[key].label}
                        </i>
                        {SAFETY_META[key].hint}
                      </span>
                    ))}
                  </div>
                  <div className="config-path-list">
                    {active.paths.map((path) => (
                      <div
                        className={`config-path-row ${
                          path.safety === "keep" ? "is-protected" : ""
                        }`}
                        key={path.path}
                        onContextMenu={(event) =>
                          pathMenu(event, path.path, { isDir: path.isDir })
                        }
                      >
                        <Checkbox
                          className="config-path-check"
                          checked={selected.has(path.path)}
                          disabled={path.safety === "keep"}
                          onChange={(checked) => togglePath(path, checked)}
                          label={
                            <span className="config-path-label">
                              <span className="config-kind">{path.kind}</span>
                              <span
                                className={`config-safety ${SAFETY_META[path.safety].tone}`}
                                title={SAFETY_META[path.safety].title}
                              >
                                {SAFETY_META[path.safety].label}
                              </span>
                              <span className="config-path-desc">{path.description}</span>
                              <em>{formatBytes(path.size)}</em>
                            </span>
                          }
                          hint={
                            <span className="path-text" title={path.path}>
                              {path.display} · {path.fileCount.toLocaleString()} 个文件 ·{" "}
                              {relativeTime(path.modified)}
                            </span>
                          }
                        />
                        <div className="config-path-actions">
                          {path.isDir && (
                            <button
                              className="icon-btn"
                              title="查看目录明细"
                              onClick={() => setDetail({ tool: active, path: path.path })}
                            >
                              <ListTree size={16} />
                            </button>
                          )}
                          <button
                            className="icon-btn"
                            title={revealLabel(platform)}
                            onClick={() =>
                              api
                                .reveal(path.path)
                                .catch(() => toast("无法打开路径", "error"))
                            }
                          >
                            <FolderOpen size={16} />
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>

                <div className="detail-foot">
                  <Checkbox
                    checked={
                      selectablePaths.length > 0 &&
                      selectablePaths.every((path) => selected.has(path.path))
                    }
                    onChange={(checked) =>
                      setSelected(
                        checked
                          ? new Set(selectablePaths.map((path) => path.path))
                          : new Set(),
                      )
                    }
                    label="选择全部可删除位置"
                    hint={
                      protectedCount > 0
                        ? `${protectedCount} 个受保护位置不会选中；清理时会移入系统${trash}，可在${trash}中恢复。`
                        : `清理时会移入系统${trash}，可在${trash}中恢复。`
                    }
                  />
                  <div className="detail-foot-actions">
                    <span className="muted small">
                      将清理 {selected.size} 个位置 · {formatBytes(selectedSize)}
                    </span>
                    <Button
                      variant="danger"
                      icon={<Trash2 size={16} />}
                      disabled={selected.size === 0}
                      loading={running}
                      onClick={() => setConfirmOpen(true)}
                    >
                      移入{trash}
                    </Button>
                  </div>
                </div>
              </>
            )}
          </Card>
        </div>
      )}

      <Card className="clean-note config-note">
        <Trash2 size={16} />
        <p>
          这些目录可能仍被正在使用的工具读取。清理前请关闭对应工具，确认<strong>无需保留登录状态与历史会话</strong>；
          删除会移入{trash}，而不是永久删除。
        </p>
      </Card>

      <Modal
        open={confirmOpen}
        title={`清理 ${selected.size} 个配置位置？`}
        description={`共 ${formatBytes(selectedSize)}，将移入系统${trash}。清理后对应工具可能需要重新登录或重新配置。`}
        tone="danger"
        onClose={() => setConfirmOpen(false)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setConfirmOpen(false)}>
              取消
            </Button>
            <Button variant="danger" loading={running} onClick={deleteSelected}>
              确认移入{trash}
            </Button>
          </>
        }
      >
        <ul className="confirm-list">
          {selectedPaths.slice(0, 12).map((path) => (
            <li key={path.path}>
              <span title={path.path}>
                <i className={`config-safety ${SAFETY_META[path.safety].tone}`}>
                  {SAFETY_META[path.safety].label}
                </i>
                {path.display}
              </span>
              <em>{formatBytes(path.size)}</em>
            </li>
          ))}
        </ul>
        {selectedPaths.length > 12 && (
          <p className="muted small">另有 {selectedPaths.length - 12} 个位置未显示。</p>
        )}
      </Modal>

      <Modal
        open={!!result}
        title="配置清理完成"
        description={
          result
            ? `释放 ${formatBytes(result.freed)}，处理 ${result.files.toLocaleString()} 个文件。`
            : ""
        }
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
              {result.errors.slice(0, 6).map((message, index) => (
                <li key={index}>{message}</li>
              ))}
            </ul>
          </div>
        )}
      </Modal>

      {detail && (
        <CleanDetailModal
          key={`${detail.tool.id}:${detail.path ?? "all"}`}
          source={{
            key: `config:${detail.tool.id}`,
            title: `${detail.tool.name} 配置明细`,
            description: `点击文件夹可以继续查看下一级内容；右侧按钮可单独删除，删除后会移入${trash}。`,
            initialPath: detail.path,
            load: (path) => api.configDetail(detail.tool.id, path),
            remove: (path) => api.deleteConfigEntry(detail.tool.id, path),
            canRemove: (entry) =>
              !protectedRoots.some(
                (root) =>
                  entry.path === root ||
                  entry.path.startsWith(`${root}/`) ||
                  entry.path.startsWith(`${root}\\`),
              ),
            onChanged: () => setDetailDirty(true),
          }}
          onClose={() => {
            setDetail(null);
            if (detailDirty) {
              setDetailDirty(false);
              clearCleanDetailCache();
              invalidateCachePrefix("configs");
              void refresh({ force: true });
            }
          }}
        />
      )}
    </div>
  );
}

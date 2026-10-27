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
import { useCachedQuery } from "../cache";
import { formatBytes, relativeTime } from "../format";
import type { ConfigTool, OperationResult } from "../types";
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

const EMPTY_TOOLS: ConfigTool[] = [];

export function Configs() {
  const toast = useToast();
  const {
    data: toolData,
    loading,
    refreshing,
    error,
    refresh,
  } = useCachedQuery<ConfigTool[]>("configs", api.scanConfigs, { ttl: 60_000 });
  const tools = toolData ?? EMPTY_TOOLS;
  const [query, setQuery] = useState("");
  const [activeId, setActiveId] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [detail, setDetail] = useState<{ tool: ConfigTool; path: string | null } | null>(null);
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

  const togglePath = (path: string, checked: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      if (checked) next.add(path);
      else next.delete(path);
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
        `已移入废纸篓 ${data.files.toLocaleString()} 个文件，释放 ${formatBytes(data.freed)}`,
        data.errors.length ? "info" : "success",
      );
      setSelected(new Set());
      clearCleanDetailCache();
      await refresh({ force: true });
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
            查看 Codex、Pi、OpenCode 等开发工具留下的配置、会话与缓存
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
          hint="扫描范围包括 Codex、Claude Code、OpenCode、Pi、Gemini CLI 等常见目录。"
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
                  onClick={() => setActiveId(tool.id)}
                >
                  <span className="app-avatar">{tool.name.slice(0, 1).toUpperCase()}</span>
                  <span className="app-row-main">
                    <strong>{tool.name}</strong>
                    <span>
                      {tool.category}
                      {tool.commandAvailable ? "" : " · 仅残留配置"}
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
                    className={`badge ${active.commandAvailable ? "badge-ok" : "badge-warn"}`}
                    title={
                      active.commandAvailable
                        ? "命令行工具仍可在 PATH 中找到"
                        : "未在 PATH 中找到对应命令，这些目录可能只是残留"
                    }
                  >
                    <Terminal size={12} />
                    {active.commandAvailable ? "CLI 可用" : "未检测到 CLI"}
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
                  <div className="config-path-list">
                    {active.paths.map((path) => (
                      <div className="config-path-row" key={path.path}>
                        <Checkbox
                          className="config-path-check"
                          checked={selected.has(path.path)}
                          onChange={(checked) => togglePath(path.path, checked)}
                          label={
                            <span className="config-path-label">
                              <span className="config-kind">{path.kind}</span>
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
                            title="在 Finder 中显示"
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
                      active.paths.length > 0 && selected.size === active.paths.length
                    }
                    onChange={(checked) =>
                      setSelected(
                        checked
                          ? new Set(active.paths.map((path) => path.path))
                          : new Set(),
                      )
                    }
                    label="选择全部位置"
                    hint="清理时会移入系统废纸篓，可在废纸篓中恢复。"
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
                      移入废纸篓
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
          删除会移入废纸篓，而不是永久删除。
        </p>
      </Card>

      <Modal
        open={confirmOpen}
        title={`清理 ${selected.size} 个配置位置？`}
        description={`共 ${formatBytes(selectedSize)}，将移入系统废纸篓。清理后对应工具可能需要重新登录或重新配置。`}
        tone="danger"
        onClose={() => setConfirmOpen(false)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setConfirmOpen(false)}>
              取消
            </Button>
            <Button variant="danger" loading={running} onClick={deleteSelected}>
              确认移入废纸篓
            </Button>
          </>
        }
      >
        <ul className="confirm-list">
          {selectedPaths.slice(0, 12).map((path) => (
            <li key={path.path}>
              <span title={path.path}>{path.display}</span>
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
            description: "只在本地读取；点击文件夹可以继续查看下一级内容。",
            initialPath: detail.path,
            load: (path) => api.configDetail(detail.tool.id, path),
          }}
          onClose={() => setDetail(null)}
        />
      )}
    </div>
  );
}

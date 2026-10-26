import { useEffect, useMemo, useRef, useState } from "react";
import { FolderX, FolderSearch, RefreshCw, Trash2 } from "lucide-react";
import { api } from "../api";
import { useCachedQuery } from "../cache";
import { formatBytes, relativeTime } from "../format";
import type { OperationResult, ProjectArtifact } from "../types";
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
import { CleanDetailModal, clearCleanDetailCache } from "../components/CleanDetailModal";

const EMPTY_ARTIFACTS: ProjectArtifact[] = [];
const EMPTY_ROOTS: string[] = [];

export function Projects() {
  const toast = useToast();
  const {
    data,
    loading,
    refreshing,
    error,
    refresh,
  } = useCachedQuery(
    "projects",
    async () => {
      const [items, currentRoots] = await Promise.all([api.scanProjects(), api.purgeRoots()]);
      return { items, roots: currentRoots };
    },
    { ttl: 60_000 },
  );
  const artifacts = data?.items ?? EMPTY_ARTIFACTS;
  const roots = data?.roots ?? EMPTY_ROOTS;
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [scanned, setScanned] = useState(false);
  const [running, setRunning] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [result, setResult] = useState<OperationResult | null>(null);
  const [detailItem, setDetailItem] = useState<ProjectArtifact | null>(null);
  const initialized = useRef(false);

  // A new scan means the per-directory breakdowns may be stale.
  useEffect(() => {
    clearCleanDetailCache();
  }, [data]);

  // Default-select stable artifacts once; keep manual choices on refresh.
  useEffect(() => {
    if (!data) return;
    setScanned(true);
    if (!initialized.current) {
      initialized.current = true;
      setSelected(new Set(data.items.filter((item) => !item.recent).map((item) => item.path)));
      return;
    }
    setSelected((current) => {
      const paths = new Set(data.items.map((item) => item.path));
      return new Set([...current].filter((path) => paths.has(path)));
    });
  }, [data]);

  const scan = () => refresh({ force: true });

  const grouped = useMemo(() => {
    const map = new Map<string, ProjectArtifact[]>();
    for (const item of artifacts) {
      const list = map.get(item.project) ?? [];
      list.push(item);
      map.set(item.project, list);
    }
    return [...map.entries()];
  }, [artifacts]);

  const selectedItems = artifacts.filter((item) => selected.has(item.path));
  const selectedSize = selectedItems.reduce((total, item) => total + item.size, 0);
  const totalSize = artifacts.reduce((total, item) => total + item.size, 0);

  const execute = async () => {
    setConfirmOpen(false);
    setRunning(true);
    try {
      const data = await api.purgeProjects([...selected]);
      setResult(data);
      toast(`释放 ${formatBytes(data.freed)}`, data.errors.length ? "info" : "success");
      await scan();
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
          <h1>项目清理</h1>
          <p>找出 node_modules、target、dist 等可重新生成的构建产物</p>
        </div>
        <div className="head-actions">
          <Button icon={<RefreshCw size={16} />} onClick={scan} loading={loading || refreshing}>
            重新扫描
          </Button>
          <Button
            variant="primary"
            icon={<Trash2 size={16} />}
            disabled={selected.size === 0}
            loading={running}
            onClick={() => setConfirmOpen(true)}
          >
            清理 {selected.size} 项
          </Button>
        </div>
      </header>

      <PermissionNotice />

      <Card className="roots-card">
        <div>
          <strong>扫描目录</strong>
          <span>只在这些目录下查找构建产物</span>
        </div>
        <div className="root-chips">
          {roots.length === 0 ? (
            <span className="muted small">未配置扫描目录，可在设置中调整</span>
          ) : (
            roots.map((root) => (
              <span className="chip" key={root} title={root}>
                {root.replace(/^.*[\\/]/, "")}
              </span>
            ))
          )}
        </div>
      </Card>

      {error && <Card className="error-card">{error}</Card>}

      {loading && !scanned ? (
        <Spinner label="正在扫描项目目录…" />
      ) : artifacts.length === 0 ? (
        <EmptyState
          icon={<FolderX size={24} />}
          title={scanned ? "没有发现构建产物" : "还没有扫描"}
          hint="确认扫描目录后重新扫描，默认会检查 ~/Projects、~/dev、~/code 等位置。"
          action={<Button variant="primary" onClick={scan}>开始扫描</Button>}
        />
      ) : (
        <>
          <Card className="summary-strip">
            <div className="summary-main">
              <span className="summary-icon">
                <FolderSearch size={22} />
              </span>
              <div>
                <strong>{formatBytes(totalSize)}</strong>
                <span>
                  {artifacts.length} 个构建产物，分布在 {grouped.length} 个项目中
                </span>
              </div>
            </div>
            <div className="summary-side">
              <div>
                <span>已选择</span>
                <strong>{formatBytes(selectedSize)}</strong>
              </div>
            </div>
          </Card>

          {grouped.map(([project, items]) => {
            const projectSize = items.reduce((total, item) => total + item.size, 0);
            const allChecked = items.every((item) => selected.has(item.path));
            return (
              <section className="project-group" key={project}>
                <div className="project-head">
                  <div>
                    <strong>{project}</strong>
                    <span>
                      {items.length} 个产物 · {formatBytes(projectSize)}
                    </span>
                  </div>
                  <button
                    className="link-btn"
                    onClick={() =>
                      setSelected((current) => {
                        const next = new Set(current);
                        items.forEach((item) => (allChecked ? next.delete(item.path) : next.add(item.path)));
                        return next;
                      })
                    }
                  >
                    {allChecked ? "取消全选" : "全选"}
                  </button>
                </div>
                <Card className="artifact-list">
                  {items.map((item) => (
                    <div className="artifact-row-wrap" key={item.path}>
                      <Checkbox
                        checked={selected.has(item.path)}
                        onChange={(value) =>
                          setSelected((current) => {
                            const next = new Set(current);
                            if (value) next.add(item.path);
                            else next.delete(item.path);
                            return next;
                          })
                        }
                        className="artifact-row"
                        label={
                          <span className="artifact-label">
                            <span className="artifact-kind">{item.kind}</span>
                            <span className="artifact-path" title={item.path}>
                              {item.path}
                            </span>
                            {item.recent && <span className="badge badge-warn">7 天内修改</span>}
                            <em>{formatBytes(item.size)}</em>
                          </span>
                        }
                        hint={<span className="path-text">最后修改 {relativeTime(item.modified)}</span>}
                      />
                      {item.size > 0 && (
                        <button
                          className="clean-detail-link"
                          onClick={() => setDetailItem(item)}
                        >
                          查看明细
                        </button>
                      )}
                    </div>
                  ))}
                </Card>
              </section>
            );
          })}

          <Card className="clean-note">
            <Trash2 size={16} />
            <p>
              构建产物会被<strong>永久删除</strong>，不进入废纸篓。它们通常可以重新生成，但删除后首次构建会变慢。
              最近 7 天修改过的产物默认不勾选。点击每项右侧的<strong>查看明细</strong>
              可逐层查看将被删除的目录与文件。
            </p>
          </Card>
        </>
      )}

      <Modal
        open={confirmOpen}
        title={`永久删除 ${selected.size} 个构建产物？`}
        description={`将释放约 ${formatBytes(selectedSize)}，删除后无法从废纸篓恢复。`}
        tone="danger"
        onClose={() => setConfirmOpen(false)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setConfirmOpen(false)}>
              取消
            </Button>
            <Button variant="danger" loading={running} onClick={execute}>
              确认删除
            </Button>
          </>
        }
      >
        <ul className="confirm-list">
          {selectedItems.slice(0, 10).map((item) => (
            <li key={item.path}>
              <span>{item.path}</span>
              <em>{formatBytes(item.size)}</em>
            </li>
          ))}
        </ul>
      </Modal>

      {detailItem && (
        <CleanDetailModal
          key={detailItem.path}
          source={{
            key: `artifact:${detailItem.path}`,
            title: `${detailItem.kind} · ${formatBytes(detailItem.size)}`,
            description:
              "该构建产物会被整体永久删除，以下目录与文件都在删除范围内。",
            load: (path) => api.projectArtifactDetail(detailItem.path, path),
          }}
          onClose={() => setDetailItem(null)}
        />
      )}

      <Modal
        open={!!result}
        title="项目清理完成"
        description={result ? `释放 ${formatBytes(result.freed)}，删除 ${result.files.toLocaleString()} 个文件。` : ""}
        onClose={() => setResult(null)}
        footer={
          <Button variant="primary" onClick={() => setResult(null)}>
            完成
          </Button>
        }
      >
        {result && result.errors.length > 0 && (
          <div className="result-errors">
            <strong>{result.errors.length} 个条目未能删除</strong>
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

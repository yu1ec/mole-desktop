import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  ChevronRight,
  File,
  Folder,
  FolderOpen,
  Loader2,
  Trash2,
  Undo2,
} from "lucide-react";
import { api } from "../api";
import { formatBytes } from "../format";
import type { CleanDetail, CleanEntry, EntryTrashResult } from "../types";
import { Button, EmptyState, Modal, Spinner, useToast } from "./ui";
import { revealLabel, trashName, usePlatform } from "../platform";

// Browsing one directory level at a time keeps each request cheap; remembering
// visited levels makes walking back and forth instant.
const detailCache = new Map<string, CleanDetail>();

export function clearCleanDetailCache() {
  detailCache.clear();
}

function cacheKey(itemId: string, path: string | null) {
  return `${itemId}::${path ?? ""}`;
}

/** Drops every cached level of one source after its contents change. */
function invalidateSource(key: string) {
  const prefix = `${key}::`;
  for (const cached of [...detailCache.keys()]) {
    if (cached.startsWith(prefix)) detailCache.delete(cached);
  }
}

type RemovedEntry = {
  entry: CleanEntry;
  /** 能否通过界面恢复到原位置 */
  restorable: boolean;
  /** 删除时所在的目录；重新加载同一层时用它把划线条目放回来 */
  view: string | null;
};

export type DetailSource = {
  /** 缓存命名空间，同一来源的不同目录层级共享一份缓存 */
  key: string;
  title: string;
  description: string;
  /** 打开时直接进入的目录；留空表示先展示全部根目录 */
  initialPath?: string | null;
  load: (path: string | null) => Promise<CleanDetail>;
  /** 删除单个条目；留空表示该列表只读 */
  remove?: (path: string) => Promise<EntryTrashResult>;
  /** 返回 false 的条目不显示删除按钮（例如受保护的配置路径） */
  canRemove?: (entry: CleanEntry) => boolean;
  /** 删除成功后通知调用方刷新外层统计 */
  onChanged?: () => void;
};

export function CleanDetailModal({
  source,
  onClose,
}: {
  source: DetailSource;
  onClose: () => void;
}) {
  const toast = useToast();
  const platform = usePlatform();
  const trash = trashName(platform);
  const [path, setPath] = useState<string | null>(source.initialPath ?? null);
  const [detail, setDetail] = useState<CleanDetail | null>(null);
  const [removed, setRemoved] = useState<Record<string, RemovedEntry>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const requestRef = useRef(0);
  const busyRef = useRef(false);
  const { key, title, description, load, remove, canRemove, onChanged } = source;

  useEffect(() => {
    // Invalidate any in-flight scan before touching state, including cache hits.
    const request = ++requestRef.current;
    const cached = detailCache.get(cacheKey(key, path));
    if (cached) {
      setDetail(cached);
      setLoading(false);
      setError("");
      return;
    }
    setDetail(null);
    setLoading(true);
    setError("");
    load(path)
      .then((data) => {
        if (request !== requestRef.current) return;
        detailCache.set(cacheKey(key, path), data);
        setDetail(data);
      })
      .catch((err) => {
        if (request === requestRef.current) setError(String(err));
      })
      .finally(() => {
        if (request === requestRef.current) setLoading(false);
      });
  }, [key, path]);

  const removeEntry = async (entry: CleanEntry) => {
    if (!remove || busyRef.current) return;
    busyRef.current = true;
    setBusy(entry.path);
    try {
      const result = await remove(entry.path);
      invalidateSource(key);
      setRemoved((current) => ({
        ...current,
        [entry.path]: { entry, restorable: result.restorable, view: path },
      }));
      setDetail((current) =>
        current
          ? {
              ...current,
              entries: current.entries.filter((item) => item.path !== entry.path),
              totalSize: Math.max(0, current.totalSize - entry.size),
              fileCount: Math.max(0, current.fileCount - entry.fileCount),
            }
          : current,
      );
      onChanged?.();
      toast(result.restorable ? `已移入${trash}，可随时恢复` : "已删除", "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busyRef.current = false;
      setBusy(null);
    }
  };

  const restoreEntry = async (entry: CleanEntry) => {
    const record = removed[entry.path];
    if (!record?.restorable || busyRef.current) return;
    busyRef.current = true;
    setBusy(entry.path);
    try {
      await api.restoreTrashedEntry(entry.path);
      invalidateSource(key);
      setRemoved((current) => {
        const next = { ...current };
        delete next[entry.path];
        return next;
      });
      setDetail((current) => {
        if (!current) return current;
        const entries = current.entries.some((item) => item.path === entry.path)
          ? current.entries
          : [...current.entries, entry].sort((a, b) => b.size - a.size);
        return {
          ...current,
          entries,
          totalSize: current.totalSize + entry.size,
          fileCount: current.fileCount + entry.fileCount,
        };
      });
      toast("已恢复到原位置", "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busyRef.current = false;
      setBusy(null);
    }
  };

  // Deleted rows stay in the list (struck through) so the undo button remains
  // reachable, including after reloading the same directory from the backend.
  const rows = useMemo(() => {
    if (!detail) return [];
    const removedHere = Object.values(removed).filter((record) => record.view === detail.current);
    const deleted = new Set(removedHere.map((record) => record.entry.path));
    const entries = [
      ...detail.entries.filter((entry) => !deleted.has(entry.path)),
      ...removedHere.map((record) => record.entry),
    ];
    return entries
      .map((entry) => ({ entry, record: removed[entry.path] }))
      .sort((a, b) => b.entry.size - a.entry.size);
  }, [detail, removed]);

  const revealTarget = detail?.current ?? detail?.roots[0] ?? null;
  const total = detail?.totalSize ?? 0;

  return (
    <Modal
      open
      wide
      title={title}
      description={description}
      onClose={onClose}
      footer={
        <>
          <Button variant="quiet" onClick={onClose}>
            关闭
          </Button>
          {revealTarget && (
            <Button
              onClick={() =>
                api.reveal(revealTarget).catch(() => toast("无法打开目录", "error"))
              }
            >
              {revealLabel(platform)}
            </Button>
          )}
        </>
      }
    >
      <div className="clean-detail">
        {detail && (
          <div className="clean-detail-crumbs">
            {detail.crumbs.map((crumb, index) => (
              <span key={crumb.path ?? "all"} className="crumb-wrap">
                {index > 0 && <ChevronRight size={13} />}
                <button
                  onClick={() => setPath(crumb.path)}
                  disabled={index === detail.crumbs.length - 1}
                >
                  {crumb.label}
                </button>
              </span>
            ))}
          </div>
        )}

        {detail && detail.current === null && detail.roots.length > 0 && (
          <div className="clean-detail-roots">
            <span>清理根目录</span>
            {detail.roots.map((root, index) => (
              <button key={root} onClick={() => setPath(root)} title={root}>
                {detail.rootsDisplay[index] ?? root}
              </button>
            ))}
          </div>
        )}

        {loading ? (
          <Spinner label="正在扫描目录明细…" />
        ) : error ? (
          <div className="error-card">{error}</div>
        ) : !detail ? null : (
          <>
            <div className="clean-detail-summary">
              <div>
                <span>{detail.currentDisplay}</span>
                <strong>{formatBytes(detail.totalSize)}</strong>
              </div>
              <div>
                <span>文件数</span>
                <strong>{detail.fileCount.toLocaleString()}</strong>
              </div>
            </div>

            {rows.length === 0 ? (
              <EmptyState
                icon={<FolderOpen size={22} />}
                title="这里没有内容"
                hint="该目录为空，或当前用户没有读取权限。"
              />
            ) : (
              <div className="clean-detail-list">
                {rows.map(({ entry, record }) => {
                  const deleted = !!record;
                  const canDelete = !!remove && (canRemove?.(entry) ?? true);
                  const busyHere = busy === entry.path;
                  return (
                    <div
                      key={entry.path}
                      className={`clean-detail-row ${entry.isDir ? "is-dir" : ""} ${deleted ? "is-deleted" : ""}`}
                    >
                      <button
                        className="clean-detail-open"
                        disabled={deleted}
                        title={
                          deleted
                            ? record.restorable
                              ? `已移入${trash}`
                              : "已删除"
                            : entry.isDir
                              ? `查看 ${entry.path}`
                              : `${revealLabel(platform)} ${entry.path}`
                        }
                        onClick={() => {
                          if (deleted) return;
                          if (entry.isDir) setPath(entry.path);
                          else api.reveal(entry.path).catch(() => toast("无法打开文件", "error"));
                        }}
                      >
                        <span className={`size-icon ${entry.isDir ? "is-dir" : ""}`}>
                          {entry.isDir ? <Folder size={14} /> : <File size={14} />}
                        </span>
                        <span className="size-main">
                          <span className="size-name">{entry.name}</span>
                          <span className="size-bar">
                            <i
                              style={{
                                width: `${total ? Math.min(100, (entry.size / total) * 100) : 0}%`,
                              }}
                            />
                          </span>
                        </span>
                        {entry.isDir && (
                          <span className="clean-detail-count">
                            {entry.fileCount.toLocaleString()} 个文件
                          </span>
                        )}
                        <em>{formatBytes(entry.size)}</em>
                        {entry.isDir && (
                          <ChevronRight size={14} className="clean-detail-arrow" />
                        )}
                      </button>

                      {remove &&
                        (record ? (
                          record.restorable ? (
                            <button
                              className="clean-detail-action is-restore"
                              disabled={busy !== null}
                              onClick={() => restoreEntry(entry)}
                              title="恢复到原位置"
                              aria-label={`恢复 ${entry.name}`}
                            >
                              {busyHere ? <Loader2 size={14} className="spin" /> : <Undo2 size={14} />}
                            </button>
                          ) : (
                            <span className="clean-detail-flag">已删除</span>
                          )
                        ) : canDelete ? (
                          <button
                            className="clean-detail-action"
                            disabled={busy !== null}
                            onClick={() => removeEntry(entry)}
                            title={`删除（移入${trash}）`}
                            aria-label={`删除 ${entry.name}`}
                          >
                            {busyHere ? <Loader2 size={14} className="spin" /> : <Trash2 size={14} />}
                          </button>
                        ) : null)}
                    </div>
                  );
                })}
              </div>
            )}

            {detail.truncated && (
              <p className="muted small">
                条目较多，仅显示占用最大的 500 项；实际清理会包含全部内容。
              </p>
            )}
            {detail.errors > 0 && (
              <p className="muted small">
                <AlertTriangle size={12} />
                {detail.errors} 个条目无法读取（通常是权限限制），未计入大小统计。
              </p>
            )}
          </>
        )}
      </div>
    </Modal>
  );
}

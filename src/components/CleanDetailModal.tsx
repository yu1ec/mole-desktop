import { useEffect, useRef, useState } from "react";
import { AlertTriangle, ChevronRight, File, Folder, FolderOpen } from "lucide-react";
import { api } from "../api";
import { formatBytes } from "../format";
import type { CleanDetail } from "../types";
import { Button, EmptyState, Modal, Spinner, useToast } from "./ui";

// Browsing one directory level at a time keeps each request cheap; remembering
// visited levels makes walking back and forth instant.
const detailCache = new Map<string, CleanDetail>();

export function clearCleanDetailCache() {
  detailCache.clear();
}

function cacheKey(itemId: string, path: string | null) {
  return `${itemId}::${path ?? ""}`;
}

export type DetailSource = {
  /** 缓存命名空间，同一来源的不同目录层级共享一份缓存 */
  key: string;
  title: string;
  description: string;
  /** 打开时直接进入的目录；留空表示先展示全部根目录 */
  initialPath?: string | null;
  load: (path: string | null) => Promise<CleanDetail>;
};

export function CleanDetailModal({
  source,
  onClose,
}: {
  source: DetailSource;
  onClose: () => void;
}) {
  const toast = useToast();
  const [path, setPath] = useState<string | null>(source.initialPath ?? null);
  const [detail, setDetail] = useState<CleanDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const requestRef = useRef(0);
  const { key, title, description, load } = source;

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
              在 Finder 中显示
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

            {detail.entries.length === 0 ? (
              <EmptyState
                icon={<FolderOpen size={22} />}
                title="这里没有内容"
                hint="该目录为空，或当前用户没有读取权限。"
              />
            ) : (
              <div className="clean-detail-list">
                {detail.entries.map((entry) => (
                  <button
                    key={entry.path}
                    className={`clean-detail-row ${entry.isDir ? "is-dir" : ""}`}
                    title={
                      entry.isDir
                        ? `查看 ${entry.path}`
                        : `在 Finder 中显示 ${entry.path}`
                    }
                    onClick={() => {
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
                            width: `${total ? (entry.size / total) * 100 : 0}%`,
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
                ))}
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

import { useEffect, useState } from "react";
import {
  ArrowUp,
  ChevronRight,
  FileArchive,
  FolderOpen,
  HardDrive,
  RefreshCw,
  Search,
} from "lucide-react";
import { api } from "../api";
import { formatBytes } from "../format";
import type { AnalyzeReport } from "../types";
import { Button, Card, EmptyState, Spinner, useToast } from "../components/ui";
import { Treemap } from "../components/Treemap";

function breadcrumbs(path: string): { label: string; value: string }[] {
  const sep = path.includes("\\") ? "\\" : "/";
  const parts = path.split(sep).filter(Boolean);
  const out: { label: string; value: string }[] = [];
  if (sep === "/") out.push({ label: "根目录", value: "/" });
  let current = sep === "/" ? "" : "";
  parts.forEach((part, index) => {
    if (index === 0 && sep === "/") current = `/${part}`;
    else if (index === 0) current = `${part}\\`;
    else current = current.endsWith(sep) ? `${current}${part}` : `${current}${sep}${part}`;
    out.push({ label: part, value: current });
  });
  return out;
}

export function Analyze() {
  const toast = useToast();
  const [shortcuts, setShortcuts] = useState<[string, string][]>([]);
  const [path, setPath] = useState("");
  const [report, setReport] = useState<AnalyzeReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const analyze = async (target: string) => {
    if (!target.trim()) return;
    setLoading(true);
    setError("");
    try {
      const data = await api.analyze(target.trim());
      setReport(data);
      setPath(data.path);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    api
      .analyzeShortcuts()
      .then((items) => {
        setShortcuts(items);
        if (items.length > 0) analyze(items[0][1]);
      })
      .catch((err) => setError(String(err)));
  }, []);

  const crumbs = report ? breadcrumbs(report.path) : [];
  const total = report?.totalSize || 0;

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>磁盘分析</h1>
          <p>逐层查看目录占用，点击方块或列表即可下钻</p>
        </div>
        <div className="head-actions">
          <Button
            icon={<FolderOpen size={16} />}
            disabled={!report}
            onClick={() => report && api.reveal(report.path).catch(() => {})}
          >
            在文件夹中显示
          </Button>
          <Button
            variant="primary"
            icon={<RefreshCw size={16} />}
            loading={loading}
            onClick={() => analyze(path)}
          >
            重新扫描
          </Button>
        </div>
      </header>

      <Card className="analyze-bar">
        <div className="path-input">
          <Search size={15} />
          <input
            value={path}
            onChange={(event) => setPath(event.target.value)}
            onKeyDown={(event) => event.key === "Enter" && analyze(path)}
            placeholder="输入要分析的目录，例如 ~/Downloads"
          />
        </div>
        <Button variant="primary" onClick={() => analyze(path)} loading={loading}>
          分析
        </Button>
      </Card>

      <div className="shortcut-row">
        {shortcuts.map(([label, value]) => (
          <button
            key={value}
            className={`chip ${report?.path === value ? "is-active" : ""}`}
            onClick={() => analyze(value)}
          >
            {label}
          </button>
        ))}
      </div>

      {error && <Card className="error-card">{error}</Card>}

      {loading && !report ? (
        <Spinner label="正在计算目录大小，较大的目录可能需要一点时间…" />
      ) : !report ? null : (
        <>
          <Card className="analyze-summary">
            <div className="analyze-crumbs">
              {crumbs.map((crumb, index) => (
                <span key={crumb.value} className="crumb-wrap">
                  {index > 0 && <ChevronRight size={13} />}
                  <button onClick={() => analyze(crumb.value)}>{crumb.label}</button>
                </span>
              ))}
            </div>
            <div className="analyze-totals">
              <div>
                <span>{report.name}</span>
                <strong>{formatBytes(total)}</strong>
              </div>
              <div>
                <span>文件数</span>
                <strong>{report.fileCount.toLocaleString()}</strong>
              </div>
              <div>
                <span>可用空间</span>
                <strong>{formatBytes(report.freeSpace)}</strong>
              </div>
              {report.parent && (
                <Button size="sm" icon={<ArrowUp size={14} />} onClick={() => analyze(report.parent!)}>
                  上一级
                </Button>
              )}
            </div>
          </Card>

          {report.entries.length === 0 ? (
            <EmptyState
              icon={<HardDrive size={24} />}
              title="这个目录是空的"
              hint="或者当前用户没有读取权限。"
            />
          ) : (
            <div className="analyze-layout">
              <Card className="treemap-card">
                {loading ? (
                  <div className="treemap-loading">
                    <Spinner label="扫描中…" />
                  </div>
                ) : (
                  <Treemap entries={report.entries} onOpen={(entry) => analyze(entry.path)} />
                )}
              </Card>

              <Card className="size-list">
                <div className="size-list-head">
                  <strong>占用排行</strong>
                  <span>{report.entries.length} 项</span>
                </div>
                <div className="size-list-body">
                  {report.entries.slice(0, 40).map((entry) => (
                    <button
                      key={entry.path}
                      className="size-row"
                      onClick={() =>
                        entry.isDir ? analyze(entry.path) : api.reveal(entry.path).catch(() => {})
                      }
                      title={entry.path}
                    >
                      <span className={`size-icon ${entry.isDir ? "is-dir" : ""}`}>
                        {entry.isDir ? <FolderOpen size={14} /> : <FileArchive size={14} />}
                      </span>
                      <span className="size-main">
                        <span className="size-name">{entry.name}</span>
                        <span className="size-bar">
                          <i style={{ width: `${total ? (entry.size / total) * 100 : 0}%` }} />
                        </span>
                      </span>
                      <em>{formatBytes(entry.size)}</em>
                    </button>
                  ))}
                </div>
              </Card>
            </div>
          )}

          {report.largeFiles.length > 0 && (
            <Card className="large-files">
              <div className="size-list-head">
                <strong>本层大文件</strong>
                <span>直接点击可在文件夹中显示</span>
              </div>
              {report.largeFiles.slice(0, 10).map((file) => (
                <button
                  key={file.path}
                  className="large-file-row"
                  onClick={() => api.reveal(file.path).catch(() => toast("无法打开文件", "error"))}
                >
                  <span>{file.name}</span>
                  <em>{formatBytes(file.size)}</em>
                </button>
              ))}
            </Card>
          )}

          {report.errors > 0 && (
            <p className="muted small">
              有 {report.errors} 个条目无法读取（通常是权限限制），不影响其他目录的大小统计。
            </p>
          )}
        </>
      )}
    </div>
  );
}

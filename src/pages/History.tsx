import { useEffect, useMemo, useState } from "react";
import {
  AppWindow,
  FolderX,
  History as HistoryIcon,
  Package,
  ShieldCheck,
  Sparkles,
  Trash2,
} from "lucide-react";
import { api } from "../api";
import { formatBytes, formatDate } from "../format";
import type { HistoryEntry } from "../types";
import { Button, Card, EmptyState, Modal, Spinner, useToast } from "../components/ui";

const KINDS: Record<string, { label: string; icon: typeof Sparkles; tone: string }> = {
  clean: { label: "深度清理", icon: Sparkles, tone: "lime" },
  uninstall: { label: "应用卸载", icon: AppWindow, tone: "blue" },
  purge: { label: "项目清理", icon: FolderX, tone: "amber" },
  installer: { label: "安装包", icon: Package, tone: "teal" },
  optimize: { label: "系统优化", icon: ShieldCheck, tone: "violet" },
};

export function HistoryPage() {
  const toast = useToast();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<string>("all");
  const [confirmOpen, setConfirmOpen] = useState(false);

  const load = async () => {
    setLoading(true);
    try {
      setEntries(await api.history());
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
  }, []);

  const filtered = useMemo(
    () => (filter === "all" ? entries : entries.filter((entry) => entry.kind === filter)),
    [entries, filter],
  );

  const totalFreed = entries
    .filter((entry) => entry.kind !== "optimize")
    .reduce((total, entry) => total + entry.freed, 0);
  const totalFiles = entries.reduce((total, entry) => total + entry.files, 0);

  const clear = async () => {
    setConfirmOpen(false);
    try {
      await api.clearHistory();
      setEntries([]);
      toast("历史记录已清空", "success");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>操作历史</h1>
          <p>本地记录历次清理、卸载与优化，只保存在这台设备上</p>
        </div>
        <Button
          variant="danger"
          icon={<Trash2 size={16} />}
          disabled={entries.length === 0}
          onClick={() => setConfirmOpen(true)}
        >
          清空记录
        </Button>
      </header>

      {entries.length > 0 && (
        <div className="history-stats">
          <Card>
            <span>累计释放</span>
            <strong>{formatBytes(totalFreed)}</strong>
          </Card>
          <Card>
            <span>处理文件</span>
            <strong>{totalFiles.toLocaleString()}</strong>
          </Card>
          <Card>
            <span>操作次数</span>
            <strong>{entries.length}</strong>
          </Card>
        </div>
      )}

      <div className="filter-row">
        <button className={`chip ${filter === "all" ? "is-active" : ""}`} onClick={() => setFilter("all")}>
          全部
        </button>
        {Object.entries(KINDS).map(([key, meta]) => (
          <button
            key={key}
            className={`chip ${filter === key ? "is-active" : ""}`}
            onClick={() => setFilter(key)}
          >
            {meta.label}
          </button>
        ))}
      </div>

      {loading ? (
        <Spinner label="正在读取历史记录…" />
      ) : filtered.length === 0 ? (
        <EmptyState
          icon={<HistoryIcon size={24} />}
          title="还没有记录"
          hint="完成一次清理或优化后，这里会显示结果摘要。"
        />
      ) : (
        <Card className="history-list">
          {filtered.map((entry) => {
            const meta = KINDS[entry.kind] ?? {
              label: entry.kind,
              icon: HistoryIcon,
              tone: "lime",
            };
            const Icon = meta.icon;
            return (
              <div className="history-row" key={entry.id}>
                <span className={`history-icon tone-${meta.tone}`}>
                  <Icon size={16} />
                </span>
                <div className="history-main">
                  <div className="history-title">
                    <strong>{entry.title}</strong>
                    <span className="badge">{meta.label}</span>
                  </div>
                  {entry.detail && <p title={entry.detail}>{entry.detail}</p>}
                </div>
                <div className="history-meta">
                  <strong>{entry.freed > 0 ? formatBytes(entry.freed) : "—"}</strong>
                  <span>{formatDate(entry.timestamp)}</span>
                </div>
              </div>
            );
          })}
        </Card>
      )}

      <Modal
        open={confirmOpen}
        title="清空所有历史记录？"
        description="只会删除本地的操作日志，不影响已清理的文件或系统状态。"
        tone="danger"
        onClose={() => setConfirmOpen(false)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setConfirmOpen(false)}>
              取消
            </Button>
            <Button variant="danger" onClick={clear}>
              清空
            </Button>
          </>
        }
      />
    </div>
  );
}

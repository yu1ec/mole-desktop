import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  RefreshCw,
  Sparkles,
  Trash2,
} from "lucide-react";
import { api } from "../api";
import { useCachedQuery } from "../cache";
import { formatBytes, formatDate } from "../format";
import type { CleanItem, CleanScan, OperationResult } from "../types";
import {
  Button,
  Card,
  Checkbox,
  Modal,
  RiskBadge,
  SectionTitle,
  Spinner,
  useToast,
} from "../components/ui";
import { PermissionNotice } from "../components/PermissionNotice";
import { CleanDetailModal, clearCleanDetailCache } from "../components/CleanDetailModal";

export function Clean() {
  const toast = useToast();
  const {
    data: scan,
    loading,
    refreshing,
    error,
    refresh,
  } = useCachedQuery<CleanScan>("clean-scan", api.scanClean, { ttl: 30_000 });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [running, setRunning] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [result, setResult] = useState<OperationResult | null>(null);
  const [detailItem, setDetailItem] = useState<CleanItem | null>(null);
  const initialized = useRef(false);

  // A new scan means the per-directory breakdowns may be stale.
  useEffect(() => {
    clearCleanDetailCache();
  }, [scan]);

  // Default-select safe items once; later background refreshes keep the user's choices.
  useEffect(() => {
    if (!scan) return;
    const available = scan.items.filter((item) => item.available && item.size > 0);
    if (!initialized.current) {
      initialized.current = true;
      setSelected(
        new Set(available.filter((item) => item.risk === "safe").map((item) => item.id)),
      );
      return;
    }
    setSelected((current) => {
      const ids = new Set(available.map((item) => item.id));
      return new Set([...current].filter((id) => ids.has(id)));
    });
  }, [scan]);

  const runScan = () => refresh({ force: true });

  const groups = useMemo(() => {
    const map = new Map<string, CleanItem[]>();
    for (const item of scan?.items ?? []) {
      if (!item.available) continue;
      const list = map.get(item.category) ?? [];
      list.push(item);
      map.set(item.category, list);
    }
    return [...map.entries()];
  }, [scan]);

  const selectedItems = (scan?.items ?? []).filter((item) => selected.has(item.id));
  const selectedSize = selectedItems.reduce((total, item) => total + item.size, 0);
  const selectedFiles = selectedItems.reduce((total, item) => total + item.fileCount, 0);

  const toggle = (id: string, checked: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  const toggleGroup = (items: CleanItem[], checked: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      for (const item of items) {
        if (item.size === 0 && checked) continue;
        if (checked) next.add(item.id);
        else next.delete(item.id);
      }
      return next;
    });
  };

  const execute = async () => {
    setConfirmOpen(false);
    setRunning(true);
    try {
      const data = await api.runClean([...selected]);
      setResult(data);
      toast(`已释放 ${formatBytes(data.freed)}`, data.errors.length ? "info" : "success");
      await runScan();
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
          <h1>深度清理</h1>
          <p>扫描缓存、日志与临时文件，删除前逐项确认</p>
        </div>
        <div className="head-actions">
          <Button
            icon={<RefreshCw size={16} />}
            onClick={runScan}
            loading={loading || refreshing}
          >
            重新扫描
          </Button>
          <Button
            variant="primary"
            icon={<Trash2 size={16} />}
            disabled={selected.size === 0}
            loading={running}
            onClick={() => setConfirmOpen(true)}
          >
            清理选中项
          </Button>
        </div>
      </header>

      <PermissionNotice />

      {error && <Card className="error-card">{error}</Card>}

      {loading && !scan ? (
        <Spinner label="正在扫描可清理项目…" />
      ) : (
        <>
          <Card className="summary-strip">
            <div className="summary-main">
              <span className="summary-icon">
                <Sparkles size={22} />
              </span>
              <div>
                <strong>{formatBytes(scan?.totalSize ?? 0)}</strong>
                <span>
                  {scan?.fileCount.toLocaleString() ?? 0} 个文件可供清理 · 扫描于{" "}
                  {scan ? formatDate(scan.scannedAt) : "—"}
                </span>
              </div>
            </div>
            <div className="summary-side">
              <div>
                <span>磁盘可用</span>
                <strong>{formatBytes(scan?.freeSpace ?? 0)}</strong>
              </div>
              <div>
                <span>已选择</span>
                <strong>{formatBytes(selectedSize)}</strong>
              </div>
            </div>
          </Card>

          {groups.length === 0 && (
            <Card className="empty-card">
              <CheckCircle2 size={22} />
              <div>
                <strong>没有找到可清理的缓存</strong>
                <span>系统看起来很干净，稍后再来看看吧。</span>
              </div>
            </Card>
          )}

          {groups.map(([category, items]) => {
            const allChecked = items.every((item) => selected.has(item.id) || item.size === 0);
            const groupSize = items.reduce((total, item) => total + item.size, 0);
            return (
              <section className="clean-group" key={category}>
                <SectionTitle
                  title={category}
                  hint={`${items.length} 项 · ${formatBytes(groupSize)}`}
                  action={
                    <button
                      className="link-btn"
                      onClick={() => toggleGroup(items, !allChecked)}
                    >
                      {allChecked ? "取消全选" : "全选"}
                    </button>
                  }
                />
                <Card className="clean-items">
                  {items.map((item) => {
                    const disabled = item.size === 0;
                    return (
                      <div className="clean-row" key={item.id}>
                        <Checkbox
                          checked={selected.has(item.id)}
                          disabled={disabled}
                          onChange={(checked) => toggle(item.id, checked)}
                          className="clean-item"
                          label={
                            <span className="clean-label">
                              <span>{item.label}</span>
                              <RiskBadge risk={item.risk} admin={item.requiresAdmin} />
                              <em>{item.size > 0 ? formatBytes(item.size) : "空"}</em>
                            </span>
                          }
                          hint={
                            <span className="clean-hint">
                              <span>{item.description}</span>
                              <span className="paths">
                                {item.paths.slice(0, 2).join("  ·  ")}
                                {item.paths.length > 2 ? `  +${item.paths.length - 2}` : ""}
                              </span>
                            </span>
                          }
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
                    );
                  })}
                </Card>
              </section>
            );
          })}

          <Card className="clean-note">
            <AlertTriangle size={16} />
            <p>
              清理会永久删除缓存与日志（废纸篓除外）。点击每项右侧的<strong>查看明细</strong>
              可逐层查看将被删除的目录与文件。标记为<strong>谨慎</strong>的项目可能包含需要重新下载或恢复的数据，
              默认不勾选。需要管理员权限的项目会在执行时请求授权或跳过。
            </p>
          </Card>
        </>
      )}

      <Modal
        open={confirmOpen}
        title={`清理 ${selected.size} 个项目？`}
        description={`将释放约 ${formatBytes(selectedSize)}，共 ${selectedFiles.toLocaleString()} 个文件。缓存与日志删除后不可恢复。`}
        tone="danger"
        onClose={() => setConfirmOpen(false)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setConfirmOpen(false)}>
              取消
            </Button>
            <Button variant="danger" loading={running} onClick={execute}>
              确认清理
            </Button>
          </>
        }
      >
        <ul className="confirm-list">
          {selectedItems.map((item) => (
            <li key={item.id}>
              <span className="confirm-label">
                <span>{item.label}</span>
                <small>{item.paths.join("  ·  ")}</small>
              </span>
              <em>{formatBytes(item.size)}</em>
            </li>
          ))}
        </ul>
      </Modal>

      {detailItem && (
        <CleanDetailModal
          key={detailItem.id}
          source={{
            key: detailItem.id,
            title: `${detailItem.label} · ${formatBytes(detailItem.size)}`,
            description:
              "以下目录与文件会在执行清理时被删除，清理不会触碰该范围之外的路径。",
            load: (path) => api.cleanItemDetail(detailItem.id, path),
          }}
          onClose={() => setDetailItem(null)}
        />
      )}

      <Modal
        open={!!result}
        title="清理完成"
        description={result ? `释放了 ${formatBytes(result.freed)}，删除 ${result.files.toLocaleString()} 个文件。` : ""}
        onClose={() => setResult(null)}
        footer={
          <Button variant="primary" onClick={() => setResult(null)}>
            完成
          </Button>
        }
      >
        {result && result.errors.length > 0 && (
          <div className="result-errors">
            <strong>{result.errors.length} 个文件未能删除</strong>
            <ul>
              {result.errors.slice(0, 6).map((item, index) => (
                <li key={index}>{item}</li>
              ))}
            </ul>
          </div>
        )}
        {result && result.removed.length > 0 && (
          <p className="muted">已清理：{result.removed.join("、")}</p>
        )}
      </Modal>
    </div>
  );
}

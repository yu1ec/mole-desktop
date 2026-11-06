import { useCallback, useEffect, useMemo, useState } from "react";
import { Power } from "lucide-react";
import { api } from "../api";
import { formatBytes, formatSpeed } from "../format";
import type { ProcessInfo } from "../types";
import { Button, Modal, useToast } from "./ui";

export type ProcessSortKey = "cpu" | "memory" | "network";

const SORT_LABEL: Record<ProcessSortKey, string> = {
  cpu: "CPU",
  memory: "内存",
  network: "网络",
};

export function ProcessListModal({
  open,
  initialSort,
  onClose,
}: {
  open: boolean;
  initialSort: ProcessSortKey;
  onClose: () => void;
}) {
  const toast = useToast();
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [sortBy, setSortBy] = useState<ProcessSortKey>(initialSort);
  const [pendingKill, setPendingKill] = useState<number | null>(null);
  const [killing, setKilling] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      const list = await api.listProcesses();
      setProcesses(list);
      setError("");
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  // 每次打开都按入口卡片决定初始排序；打开期间每 2 秒刷新一次。
  useEffect(() => {
    if (!open) return;
    setSortBy(initialSort);
    setPendingKill(null);
    setLoading(true);
    load();
    const timer = window.setInterval(load, 2000);
    return () => window.clearInterval(timer);
  }, [open, initialSort, load]);

  // 进程已经消失时收起确认状态。
  useEffect(() => {
    if (pendingKill !== null && !processes.some((process) => process.pid === pendingKill)) {
      setPendingKill(null);
    }
  }, [processes, pendingKill]);

  const sorted = useMemo(() => {
    const list = [...processes];
    const value = (process: ProcessInfo) => {
      if (sortBy === "cpu") return process.cpu;
      if (sortBy === "memory") return process.memory;
      return process.received + process.transmitted;
    };
    const secondaryValue = (process: ProcessInfo) =>
      sortBy === "memory" ? process.cpu : process.memory;
    list.sort((a, b) => {
      const primary = value(b) - value(a);
      if (primary !== 0) return primary;
      const secondary = secondaryValue(b) - secondaryValue(a);
      return secondary !== 0 ? secondary : a.name.localeCompare(b.name);
    });
    return list;
  }, [processes, sortBy]);

  const kill = async (process: ProcessInfo) => {
    setKilling(process.pid);
    try {
      await api.killProcess(process.pid);
      toast(`已结束 ${process.name}`, "success");
      setPendingKill(null);
      await load();
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setKilling(null);
    }
  };

  return (
    <Modal
      open={open}
      wide
      title="进程列表"
      description={`按${SORT_LABEL[sortBy]}占用从高到低排序 · 每 2 秒自动刷新`}
      onClose={onClose}
      footer={<Button onClick={onClose}>关闭</Button>}
    >
      <div className="proc-manager">
        <div className="proc-toolbar">
          <div className="proc-sort">
            <button
              className={`proc-sort-btn ${sortBy === "cpu" ? "is-active" : ""}`}
              onClick={() => setSortBy("cpu")}
            >
              CPU 占用
            </button>
            <button
              className={`proc-sort-btn ${sortBy === "memory" ? "is-active" : ""}`}
              onClick={() => setSortBy("memory")}
            >
              内存占用
            </button>
            <button
              className={`proc-sort-btn ${sortBy === "network" ? "is-active" : ""}`}
              onClick={() => setSortBy("network")}
            >
              网络占用
            </button>
          </div>
          <span className="muted small">
            {loading && processes.length === 0 ? "正在读取…" : `共 ${processes.length} 个进程`}
          </span>
        </div>

        {error && <p className="proc-error">{error}</p>}

        <div className="proc-list">
          <div className="proc-row proc-row-head">
            <span>进程</span>
            <span>PID</span>
            <span>CPU</span>
            <span>内存</span>
            <span>网络</span>
            <span>操作</span>
          </div>
          {loading && processes.length === 0 && <div className="proc-empty">正在读取进程…</div>}
          {!loading && processes.length === 0 && !error && (
            <div className="proc-empty">没有读取到进程</div>
          )}
          {sorted.map((process) => {
            const protectedProcess = process.pid <= 1 || process.isSelf;
            return (
              <div className="proc-row" key={process.pid}>
                <span className="proc-name" title={process.exe || process.name}>
                  <i>{process.name.slice(0, 1).toUpperCase()}</i>
                  <span>{process.name}</span>
                </span>
                <span className="muted mono">{process.pid}</span>
                <span className="proc-cpu">
                  <i>
                    <b style={{ width: `${Math.min(100, process.cpu)}%` }} />
                  </i>
                  {process.cpu.toFixed(1)}%
                </span>
                <span>{formatBytes(process.memory)}</span>
                <span
                  className="proc-net"
                  title={`下行 ${formatSpeed(process.received)} · 上行 ${formatSpeed(process.transmitted)}`}
                >
                  {formatSpeed(process.received + process.transmitted)}
                </span>
                <span className="proc-action">
                  {pendingKill === process.pid ? (
                    <span className="proc-confirm">
                      <Button size="sm" variant="quiet" onClick={() => setPendingKill(null)}>
                        取消
                      </Button>
                      <Button
                        size="sm"
                        variant="danger"
                        loading={killing === process.pid}
                        onClick={() => kill(process)}
                      >
                        确认结束
                      </Button>
                    </span>
                  ) : protectedProcess ? (
                    <span className="proc-protected">{process.isSelf ? "本应用" : "系统进程"}</span>
                  ) : (
                    <Button
                      size="sm"
                      variant="quiet"
                      icon={<Power size={13} />}
                      title="结束该进程，未保存的数据可能丢失"
                      onClick={() => setPendingKill(process.pid)}
                    >
                      结束
                    </Button>
                  )}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </Modal>
  );
}

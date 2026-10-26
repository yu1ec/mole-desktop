import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Play,
  RefreshCw,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import { api } from "../api";
import { useCachedQuery } from "../cache";
import type { OptimizeResult, OptimizeTask } from "../types";
import {
  Button,
  Card,
  Checkbox,
  Modal,
  SectionTitle,
  Spinner,
  useToast,
} from "../components/ui";

const EMPTY_TASKS: OptimizeTask[] = [];

export function Optimize() {
  const toast = useToast();
  const {
    data: taskData,
    loading,
    refreshing,
    error,
    refresh,
  } = useCachedQuery<OptimizeTask[]>("optimize-tasks", api.optimizeTasks, {
    ttl: 5 * 60_000,
  });
  const tasks = taskData ?? EMPTY_TASKS;
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [running, setRunning] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [result, setResult] = useState<OptimizeResult | null>(null);
  const initialized = useRef(false);

  // Task definitions rarely change; default-select safe tasks only once.
  useEffect(() => {
    if (!taskData) return;
    if (!initialized.current) {
      initialized.current = true;
      setSelected(
        new Set(
          taskData.filter((task) => !task.danger && !task.requiresAdmin).map((task) => task.id),
        ),
      );
      return;
    }
    setSelected((current) => {
      const ids = new Set(taskData.map((task) => task.id));
      return new Set([...current].filter((id) => ids.has(id)));
    });
  }, [taskData]);

  const load = () => refresh({ force: true });

  const hasDanger = tasks.some((task) => selected.has(task.id) && task.danger);

  const execute = async () => {
    setConfirmOpen(false);
    setRunning(true);
    try {
      const data = await api.runOptimize([...selected]);
      setResult(data);
      toast(data.failed.length ? "部分任务未完成" : "优化任务已执行", data.failed.length ? "info" : "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setRunning(false);
    }
  };

  const grouped = tasks.reduce<Record<string, OptimizeTask[]>>((acc, task) => {
    acc[task.category] = [...(acc[task.category] ?? []), task];
    return acc;
  }, {});

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>系统优化</h1>
          <p>刷新缓存与服务，修复常见的系统小问题</p>
        </div>
        <div className="head-actions">
          <Button icon={<RefreshCw size={16} />} onClick={load} loading={loading || refreshing}>
            刷新
          </Button>
          <Button
            variant="primary"
            icon={<Play size={15} />}
            disabled={selected.size === 0}
            loading={running}
            onClick={() => (hasDanger ? setConfirmOpen(true) : execute())}
          >
            执行 {selected.size} 项
          </Button>
        </div>
      </header>

      {error && <Card className="error-card">{error}</Card>}

      {loading && tasks.length === 0 ? (
        <Spinner label="正在读取优化任务…" />
      ) : (
        Object.entries(grouped).map(([category, items]) => (
          <section key={category}>
            <SectionTitle title={category} hint={`${items.length} 个任务`} />
            <div className="task-grid">
              {items.map((task) => (
                <Card
                  key={task.id}
                  className={`task-card ${selected.has(task.id) ? "is-selected" : ""}`}
                >
                  <Checkbox
                    checked={selected.has(task.id)}
                    onChange={(value) =>
                      setSelected((current) => {
                        const next = new Set(current);
                        if (value) next.add(task.id);
                        else next.delete(task.id);
                        return next;
                      })
                    }
                    label={<span className="task-title">{task.title}</span>}
                    hint={<span className="task-desc">{task.description}</span>}
                  />
                  <div className="task-badges">
                    {task.requiresAdmin && (
                      <span className="badge badge-warn">
                        <ShieldCheck size={11} /> 需要管理员
                      </span>
                    )}
                    {task.danger && (
                      <span className="badge badge-danger">
                        <AlertTriangle size={11} /> 谨慎
                      </span>
                    )}
                    {!task.requiresAdmin && !task.danger && (
                      <span className="badge badge-ok">安全</span>
                    )}
                  </div>
                </Card>
              ))}
            </div>
          </section>
        ))
      )}

      <Card className="clean-note">
        <Terminal size={16} />
        <p>
          macOS 上需要管理员权限的任务会弹出系统授权窗口；Windows 上请以管理员身份重新启动 Mole Desktop
          后再执行。优化操作只运行系统自带命令，不会修改个人文件。
        </p>
      </Card>

      <Modal
        open={confirmOpen}
        title="执行包含谨慎项的任务？"
        description="其中一些任务会清空回收站或重启资源管理器，请确认没有未保存的工作。"
        tone="danger"
        onClose={() => setConfirmOpen(false)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setConfirmOpen(false)}>
              取消
            </Button>
            <Button variant="danger" loading={running} onClick={execute}>
              继续执行
            </Button>
          </>
        }
      />

      <Modal
        open={!!result}
        title="优化完成"
        description={result ? `成功 ${result.applied.length} 项${result.failed.length ? `，失败 ${result.failed.length} 项` : ""}` : ""}
        onClose={() => setResult(null)}
        footer={
          <Button variant="primary" onClick={() => setResult(null)}>
            完成
          </Button>
        }
      >
        {result && (
          <div className="result-messages">
            {result.messages.map((message, index) => (
              <div className="result-line" key={index}>
                {result.failed.some((title) => message.startsWith(title)) ? (
                  <AlertTriangle size={14} className="tone-danger" />
                ) : (
                  <CheckCircle2 size={14} className="tone-ok" />
                )}
                <p>{message}</p>
              </div>
            ))}
          </div>
        )}
      </Modal>
    </div>
  );
}

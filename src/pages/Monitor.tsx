import { useEffect, useRef, useState } from "react";
import {
  Activity,
  ArrowDown,
  ArrowUp,
  BatteryCharging,
  Cpu,
  HardDrive,
  MemoryStick,
  RefreshCw,
} from "lucide-react";
import { api } from "../api";
import { formatBytes, formatDuration, formatSpeed } from "../format";
import type { SystemSnapshot } from "../types";
import { Button, Card, ProgressBar, Ring, SectionTitle, Sparkline, Spinner } from "../components/ui";

const MAX_SAMPLES = 60;

export function Monitor() {
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [error, setError] = useState("");
  const [cpuHistory, setCpuHistory] = useState<number[]>([]);
  const [memHistory, setMemHistory] = useState<number[]>([]);
  const [downHistory, setDownHistory] = useState<number[]>([]);
  const [upHistory, setUpHistory] = useState<number[]>([]);
  const [paused, setPaused] = useState(false);
  const pausedRef = useRef(paused);
  pausedRef.current = paused;

  const load = async () => {
    try {
      const data = await api.snapshot();
      if (pausedRef.current) return;
      setSnapshot(data);
      setError("");
      const push = (list: number[], value: number) => [...list, value].slice(-MAX_SAMPLES);
      setCpuHistory((list) => push(list, data.cpuUsage));
      setMemHistory((list) => push(list, data.memoryPercent));
      setDownHistory((list) => push(list, data.network.receivedPerSec));
      setUpHistory((list) => push(list, data.network.transmittedPerSec));
    } catch (err) {
      setError(String(err));
    }
  };

  useEffect(() => {
    load();
    const timer = setInterval(load, 2000);
    return () => clearInterval(timer);
  }, []);

  if (!snapshot) {
    return (
      <div className="page">
        <header className="page-head">
          <div>
            <h1>系统监控</h1>
            <p>实时查看 CPU、内存、磁盘与网络</p>
          </div>
        </header>
        {error ? <Card className="error-card">{error}</Card> : <Spinner label="正在采样系统状态…" />}
      </div>
    );
  }

  const disk = snapshot.disks[0];
  const maxDown = Math.max(...downHistory, 1);
  const maxUp = Math.max(...upHistory, 1);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>系统监控</h1>
          <p>
            {snapshot.host} · 每 2 秒采样 · 已运行 {formatDuration(snapshot.uptime)}
          </p>
        </div>
        <div className="head-actions">
          <Button
            icon={<RefreshCw size={16} />}
            variant={paused ? "primary" : "ghost"}
            onClick={() => setPaused((value) => !value)}
          >
            {paused ? "继续采样" : "暂停采样"}
          </Button>
        </div>
      </header>

      {error && <Card className="error-card">{error}</Card>}

      <div className="monitor-grid">
        <Card className="monitor-health">
          <Ring value={snapshot.healthScore} label={`${snapshot.healthScore}`} sub="健康分" size={116} />
          <div>
            <span className="eyebrow">综合评分</span>
            <strong>
              {snapshot.healthScore >= 85 ? "状态优秀" : snapshot.healthScore >= 65 ? "运行良好" : "需要维护"}
            </strong>
            <p>
              CPU {snapshot.cpuUsage.toFixed(1)}% · 内存 {snapshot.memoryPercent.toFixed(1)}%
            </p>
          </div>
        </Card>

        <Card className="monitor-card">
          <div className="monitor-head">
            <span className="metric-icon tone-lime">
              <Cpu size={17} />
            </span>
            <div>
              <span>处理器</span>
              <strong>{snapshot.cpuUsage.toFixed(1)}%</strong>
            </div>
            <em>
              负载 {snapshot.loadAvg[0].toFixed(2)} / {snapshot.loadAvg[1].toFixed(2)} /{" "}
              {snapshot.loadAvg[2].toFixed(2)}
            </em>
          </div>
          <Sparkline data={cpuHistory} color="#c6f36b" height={54} />
          <div className="core-grid">
            {snapshot.cpuCoreUsage.map((usage, index) => (
              <div className="core" key={index}>
                <span>Core {index + 1}</span>
                <i>
                  <b
                    style={{
                      width: `${Math.min(100, usage)}%`,
                      background: usage > 85 ? "var(--danger)" : "var(--accent)",
                    }}
                  />
                </i>
                <em>{usage.toFixed(0)}%</em>
              </div>
            ))}
          </div>
        </Card>

        <Card className="monitor-card">
          <div className="monitor-head">
            <span className="metric-icon tone-blue">
              <MemoryStick size={17} />
            </span>
            <div>
              <span>内存</span>
              <strong>{snapshot.memoryPercent.toFixed(1)}%</strong>
            </div>
            <em>
              {formatBytes(snapshot.memoryUsed)} / {formatBytes(snapshot.memoryTotal)}
            </em>
          </div>
          <Sparkline data={memHistory} color="#7cc7f5" height={54} />
          <div className="kv-list">
            <div>
              <span>已使用</span>
              <strong>{formatBytes(snapshot.memoryUsed)}</strong>
            </div>
            <div>
              <span>可用</span>
              <strong>{formatBytes(snapshot.memoryTotal - snapshot.memoryUsed)}</strong>
            </div>
            <div>
              <span>交换空间</span>
              <strong>
                {formatBytes(snapshot.swapUsed)} / {formatBytes(snapshot.swapTotal)}
              </strong>
            </div>
          </div>
        </Card>

        <Card className="monitor-card">
          <div className="monitor-head">
            <span className="metric-icon tone-teal">
              <Activity size={17} />
            </span>
            <div>
              <span>网络</span>
              <strong>{formatSpeed(snapshot.network.receivedPerSec)}</strong>
            </div>
            <em>累计 {formatBytes(snapshot.network.totalReceived)}</em>
          </div>
          <div className="net-lines">
            <div>
              <span className="net-label down">
                <ArrowDown size={13} /> 下行 {formatSpeed(snapshot.network.receivedPerSec)}
              </span>
              <Sparkline data={downHistory} color="#8fe3c0" height={34} />
            </div>
            <div>
              <span className="net-label up">
                <ArrowUp size={13} /> 上行 {formatSpeed(snapshot.network.transmittedPerSec)}
              </span>
              <Sparkline data={upHistory} color="#f2b880" height={34} />
            </div>
          </div>
          <div className="net-scale">
            <span>峰值下行 {formatSpeed(maxDown)}</span>
            <span>峰值上行 {formatSpeed(maxUp)}</span>
          </div>
        </Card>

        <Card className="monitor-card">
          <div className="monitor-head">
            <span className="metric-icon tone-amber">
              <HardDrive size={17} />
            </span>
            <div>
              <span>磁盘</span>
              <strong>{disk ? `${disk.usedPercent.toFixed(0)}%` : "—"}</strong>
            </div>
            <em>{disk ? `${formatBytes(disk.free)} 可用` : ""}</em>
          </div>
          <div className="kv-list">
            {snapshot.disks.map((item) => (
              <div className="disk-kv" key={`${item.mount}-${item.total}`}>
                <span>
                  {item.mount}
                  <em>{item.fileSystem}</em>
                </span>
                <div className="disk-kv-bar">
                  <ProgressBar
                    value={item.usedPercent}
                    tone={item.usedPercent > 90 ? "danger" : item.usedPercent > 75 ? "warn" : "accent"}
                  />
                  <small>
                    {formatBytes(item.free)} / {formatBytes(item.total)}
                  </small>
                </div>
              </div>
            ))}
          </div>
        </Card>

        {snapshot.battery && (
          <Card className="monitor-card">
            <div className="monitor-head">
              <span className="metric-icon tone-teal">
                <BatteryCharging size={17} />
              </span>
              <div>
                <span>电池</span>
                <strong>{Math.round(snapshot.battery.percent)}%</strong>
              </div>
              <em>{snapshot.battery.status}</em>
            </div>
            <ProgressBar value={snapshot.battery.percent} tone="cool" />
            <p className="muted small">
              {snapshot.battery.status === "使用电池" ? "当前正在使用电池供电" : "已接通电源"}
            </p>
          </Card>
        )}
      </div>

      <section>
        <SectionTitle title="活跃进程" hint="按 CPU 占用排序，只读展示" />
        <Card className="process-table">
          <div className="process-head">
            <span>进程</span>
            <span>PID</span>
            <span>CPU</span>
            <span>内存</span>
          </div>
          {snapshot.processes.map((process) => (
            <div className="process-row" key={process.pid}>
              <span className="process-name">
                <i>{process.name.slice(0, 1).toUpperCase()}</i>
                <span>{process.name}</span>
              </span>
              <span className="muted">{process.pid}</span>
              <span className="process-cpu">
                <i>
                  <b style={{ width: `${Math.min(100, process.cpu)}%` }} />
                </i>
                {process.cpu.toFixed(1)}%
              </span>
              <span>{formatBytes(process.memory)}</span>
            </div>
          ))}
        </Card>
      </section>
    </div>
  );
}

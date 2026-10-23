import { useEffect, useState } from "react";
import {
  Activity,
  AppWindow,
  ArrowDown,
  ArrowUp,
  ChevronRight,
  Cpu,
  HardDrive,
  MemoryStick,
  ShieldCheck,
  Sparkles,
  Zap,
} from "lucide-react";
import { api } from "../api";
import { formatBytes, formatDate, formatDuration, formatSpeed, percent } from "../format";
import type { HistoryEntry, PageKey, SystemSnapshot } from "../types";
import { Button, Card, ProgressBar, Ring, SectionTitle, Spinner } from "../components/ui";

const QUICK: { key: PageKey; title: string; hint: string; icon: typeof Sparkles; tone: string }[] = [
  { key: "clean", title: "深度清理", hint: "缓存、日志与残留", icon: Sparkles, tone: "lime" },
  { key: "analyze", title: "磁盘分析", hint: "找出占用空间的大户", icon: HardDrive, tone: "blue" },
  { key: "optimize", title: "系统优化", hint: "刷新缓存与服务", icon: ShieldCheck, tone: "teal" },
  { key: "uninstall", title: "应用卸载", hint: "连残留一起清掉", icon: AppWindow, tone: "amber" },
];

export function Dashboard({ onNavigate }: { onNavigate: (page: PageKey) => void }) {
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    const load = async () => {
      try {
        const data = await api.snapshot();
        if (active) {
          setSnapshot(data);
          setError("");
        }
      } catch (err) {
        if (active) setError(String(err));
      }
    };
    load();
    const timer = setInterval(load, 3000);
    api.history().then((items) => active && setHistory(items)).catch(() => {});
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, []);

  if (!snapshot) {
    return (
      <div className="page">
        <header className="page-head">
          <div>
            <h1>仪表盘</h1>
            <p>实时了解这台设备的状态</p>
          </div>
        </header>
        {error ? <Card className="error-card">{error}</Card> : <Spinner label="正在读取系统状态…" />}
      </div>
    );
  }

  const disk = snapshot.disks[0];
  const totals = history.reduce(
    (acc, entry) => {
      if (entry.kind !== "optimize") acc.freed += entry.freed;
      return acc;
    },
    { freed: 0 },
  );

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>仪表盘</h1>
          <p>
            {snapshot.host} · {snapshot.osName} {snapshot.osVersion} · 已运行{" "}
            {formatDuration(snapshot.uptime)}
          </p>
        </div>
        <Button variant="primary" icon={<Activity size={16} />} onClick={() => onNavigate("monitor")}>
          查看实时监控
        </Button>
      </header>

      <div className="dash-grid">
        <Card className="hero-card">
          <div className="hero-top">
            <Ring value={snapshot.healthScore} label={`${snapshot.healthScore}`} sub="健康分" />
            <div className="hero-meta">
              <span className="eyebrow">系统状态</span>
              <h2>
                {snapshot.healthScore >= 85
                  ? "运行状态优秀"
                  : snapshot.healthScore >= 65
                    ? "状态良好，可以再轻一点"
                    : "建议做一些维护"}
              </h2>
              <p>
                {snapshot.cpuCores} 核 CPU · {formatBytes(snapshot.memoryTotal)} 内存
                {snapshot.battery ? ` · 电池 ${Math.round(snapshot.battery.percent)}%` : ""}
              </p>
              <div className="hero-pills">
                <span className="pill">
                  <Zap size={13} />
                  {snapshot.battery?.status ?? "接通电源"}
                </span>
                <span className="pill">
                  <HardDrive size={13} />
                  {disk ? `${formatBytes(disk.free)} 可用` : "无磁盘信息"}
                </span>
              </div>
            </div>
          </div>
          <div className="hero-foot">
            <div>
              <span>累计释放</span>
              <strong>{formatBytes(totals.freed)}</strong>
            </div>
            <div>
              <span>操作次数</span>
              <strong>{history.length}</strong>
            </div>
            <div>
              <span>负载</span>
              <strong>
                {snapshot.loadAvg[0].toFixed(2)} / {snapshot.loadAvg[1].toFixed(2)}
              </strong>
            </div>
          </div>
        </Card>

        <div className="metric-grid">
          <Card className="metric">
            <div className="metric-head">
              <span className="metric-icon tone-lime">
                <Cpu size={17} />
              </span>
              <span>处理器</span>
              <strong>{snapshot.cpuUsage.toFixed(1)}%</strong>
            </div>
            <ProgressBar value={snapshot.cpuUsage} tone={snapshot.cpuUsage > 80 ? "danger" : "accent"} />
            <small>{snapshot.cpuCores} 个逻辑核心</small>
          </Card>
          <Card className="metric">
            <div className="metric-head">
              <span className="metric-icon tone-blue">
                <MemoryStick size={17} />
              </span>
              <span>内存</span>
              <strong>{snapshot.memoryPercent.toFixed(1)}%</strong>
            </div>
            <ProgressBar
              value={snapshot.memoryPercent}
              tone={snapshot.memoryPercent > 85 ? "danger" : "cool"}
            />
            <small>
              {formatBytes(snapshot.memoryUsed)} / {formatBytes(snapshot.memoryTotal)}
            </small>
          </Card>
          <Card className="metric">
            <div className="metric-head">
              <span className="metric-icon tone-amber">
                <HardDrive size={17} />
              </span>
              <span>磁盘</span>
              <strong>{disk ? `${disk.usedPercent.toFixed(0)}%` : "—"}</strong>
            </div>
            <ProgressBar value={disk?.usedPercent ?? 0} tone={(disk?.usedPercent ?? 0) > 90 ? "danger" : "accent"} />
            <small>{disk ? `${formatBytes(disk.free)} 可用 / ${formatBytes(disk.total)}` : "—"}</small>
          </Card>
          <Card className="metric">
            <div className="metric-head">
              <span className="metric-icon tone-teal">
                <Activity size={17} />
              </span>
              <span>网络</span>
              <strong>{formatSpeed(snapshot.network.receivedPerSec)}</strong>
            </div>
            <div className="net-row">
              <span>
                <ArrowDown size={12} /> {formatSpeed(snapshot.network.receivedPerSec)}
              </span>
              <span>
                <ArrowUp size={12} /> {formatSpeed(snapshot.network.transmittedPerSec)}
              </span>
            </div>
            <small>累计下行 {formatBytes(snapshot.network.totalReceived)}</small>
          </Card>
        </div>
      </div>

      <div className="dash-lower">
        <section>
          <SectionTitle title="快捷操作" hint="常用维护入口" />
          <div className="quick-grid">
            {QUICK.map((item) => {
              const Icon = item.icon;
              return (
                <button className={`quick-card tone-${item.tone}`} key={item.key} onClick={() => onNavigate(item.key)}>
                  <span className="quick-icon">
                    <Icon size={20} />
                  </span>
                  <div>
                    <strong>{item.title}</strong>
                    <span>{item.hint}</span>
                  </div>
                  <ChevronRight size={16} className="quick-arrow" />
                </button>
              );
            })}
          </div>
        </section>

        <section className="activity">
          <SectionTitle
            title="最近操作"
            hint="清理与优化记录"
            action={
              <button className="link-btn" onClick={() => onNavigate("history")}>
                全部
              </button>
            }
          />
          <Card className="activity-card">
            {history.length === 0 ? (
              <div className="activity-empty">
                <Sparkles size={18} />
                <span>还没有操作记录，先扫描一次清理项吧</span>
              </div>
            ) : (
              history.slice(0, 5).map((entry) => (
                <div className="activity-row" key={entry.id}>
                  <span className={`activity-dot kind-${entry.kind}`} />
                  <div>
                    <strong>{entry.title}</strong>
                    <span>{formatDate(entry.timestamp)}</span>
                  </div>
                  <em>{entry.freed > 0 ? formatBytes(entry.freed) : "—"}</em>
                </div>
              ))
            )}
          </Card>
        </section>
      </div>

      <section>
        <SectionTitle title="磁盘与存储" hint="所有挂载的卷" />
        <Card className="disk-list">
          {snapshot.disks.length === 0 && <p className="muted">未读取到磁盘信息</p>}
          {snapshot.disks.map((diskInfo) => (
            <div className="disk-row" key={`${diskInfo.mount}-${diskInfo.total}`}>
              <span className="disk-icon">
                <HardDrive size={18} />
              </span>
              <div className="disk-main">
                <div className="disk-head">
                  <strong>{diskInfo.mount === "/" ? "启动磁盘" : diskInfo.name}</strong>
                  <span>
                    {formatBytes(diskInfo.total - diskInfo.free)} / {formatBytes(diskInfo.total)}
                  </span>
                </div>
                <ProgressBar
                  value={diskInfo.usedPercent}
                  tone={diskInfo.usedPercent > 90 ? "danger" : diskInfo.usedPercent > 75 ? "warn" : "accent"}
                />
                <span className="disk-sub">
                  {diskInfo.mount} · {diskInfo.fileSystem || "未知格式"} · 可用{" "}
                  {formatBytes(diskInfo.free)}（{percent(diskInfo.free, diskInfo.total).toFixed(0)}%）
                </span>
              </div>
            </div>
          ))}
        </Card>
      </section>
    </div>
  );
}

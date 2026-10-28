import {
  Activity,
  AppWindow,
  Beer,
  Brush,
  FileCog,
  FolderX,
  Gauge,
  History,
  LayoutDashboard,
  Package,
  Settings,
  ShieldCheck,
  Sparkles,
  HardDrive,
} from "lucide-react";
import type { PageKey, SystemSnapshot } from "../types";

const NAV: { group: string; items: { key: PageKey; label: string; icon: typeof LayoutDashboard }[] }[] = [
  {
    group: "总览",
    items: [{ key: "dashboard", label: "仪表盘", icon: LayoutDashboard }],
  },
  {
    group: "维护",
    items: [
      { key: "clean", label: "深度清理", icon: Sparkles },
      { key: "uninstall", label: "应用卸载", icon: AppWindow },
      { key: "configs", label: "配置管理", icon: FileCog },
      { key: "brew", label: "Homebrew", icon: Beer },
      { key: "optimize", label: "系统优化", icon: ShieldCheck },
      { key: "projects", label: "项目清理", icon: FolderX },
      { key: "installers", label: "安装包", icon: Package },
    ],
  },
  {
    group: "探索",
    items: [
      { key: "analyze", label: "磁盘分析", icon: HardDrive },
      { key: "monitor", label: "系统监控", icon: Activity },
    ],
  },
  {
    group: "其他",
    items: [
      { key: "history", label: "操作历史", icon: History },
      { key: "settings", label: "设置", icon: Settings },
    ],
  },
];

function Logo() {
  return (
    <svg width="30" height="30" viewBox="0 0 32 32" aria-hidden="true">
      <defs>
        <linearGradient id="logo-bg" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="#d8f89c" />
          <stop offset="100%" stopColor="#8fe3c0" />
        </linearGradient>
      </defs>
      <rect x="1" y="1" width="30" height="30" rx="9" fill="url(#logo-bg)" />
      <ellipse cx="16" cy="17.5" rx="8.4" ry="8.8" fill="#171a15" />
      <ellipse cx="16" cy="12.4" rx="6.4" ry="5.2" fill="#20241d" />
      <circle cx="12.7" cy="12.4" r="1.15" fill="#d8f89c" />
      <circle cx="19.3" cy="12.4" r="1.15" fill="#d8f89c" />
      <ellipse cx="16" cy="16.1" rx="1.5" ry="1.05" fill="#e98f8f" />
      <path d="M8.6 17.5h3M20.4 17.5h3" stroke="#8fe3c0" strokeWidth="0.9" strokeLinecap="round" />
    </svg>
  );
}

export function Sidebar({
  page,
  onNavigate,
  snapshot,
  updateAvailable = false,
}: {
  page: PageKey;
  onNavigate: (page: PageKey) => void;
  snapshot: SystemSnapshot | null;
  updateAvailable?: boolean;
}) {
  const health = snapshot?.healthScore ?? 0;
  const tone = health >= 80 ? "ok" : health >= 55 ? "warn" : "bad";
  return (
    <aside className="sidebar">
      <div className="brand">
        <Logo />
        <div>
          <strong>Mole Desktop</strong>
          <span>清理与系统工具箱</span>
        </div>
      </div>

      <nav>
        {NAV.map((group) => (
          <div className="nav-group" key={group.group}>
            <span className="nav-group-label">{group.group}</span>
            {group.items.map((item) => {
              const Icon = item.icon;
              return (
                <button
                  key={item.key}
                  className={`nav-item ${page === item.key ? "is-active" : ""}`}
                  onClick={() => onNavigate(item.key)}
                >
                  <Icon size={17} strokeWidth={1.9} />
                  <span>{item.label}</span>
                  {item.key === "settings" && updateAvailable && (
                    <span className="nav-badge">新版本</span>
                  )}
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      <div className="sidebar-foot">
        <button className="health-chip" onClick={() => onNavigate("monitor")}>
          <Gauge size={17} className={`tone-${tone}`} />
          <div>
            <span>系统健康</span>
            <strong>
              {snapshot ? `${health} 分` : "检测中"}
              <em className={`dot dot-${tone}`} />
            </strong>
          </div>
        </button>
        <div className="sidebar-meta">
          <Brush size={13} />
          <span>
            {snapshot
              ? `${snapshot.osName} ${snapshot.osVersion} · ${snapshot.arch}`
              : "Mole Desktop"}
          </span>
        </div>
      </div>
    </aside>
  );
}

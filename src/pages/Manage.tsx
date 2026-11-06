import { useState } from "react";
import { Rocket, ShieldCheck } from "lucide-react";
import { StartupPanel } from "./manage/StartupPanel";
import { PrivacyPanel } from "./manage/PrivacyPanel";

type Tab = "startup" | "privacy";

const TABS: { key: Tab; label: string; hint: string; icon: typeof Rocket }[] = [
  {
    key: "startup",
    label: "启动项",
    hint: "登录时打开、App 后台活动",
    icon: Rocket,
  },
  {
    key: "privacy",
    label: "隐私权限",
    hint: "相机、麦克风、辅助功能等授权记录",
    icon: ShieldCheck,
  },
];

export function Manage() {
  const [tab, setTab] = useState<Tab>("startup");

  return (
    <div className="page manage-page">
      <header className="page-head">
        <div>
          <h1>启动项与权限</h1>
          <p>查看并管理登录项、App 后台活动和隐私授权，清理已卸载应用留下的记录</p>
        </div>
      </header>

      <div className="manage-tabs" role="tablist">
        {TABS.map(({ key, label, hint, icon: Icon }) => (
          <button
            key={key}
            role="tab"
            aria-selected={tab === key}
            className={`manage-tab ${tab === key ? "is-active" : ""}`}
            onClick={() => setTab(key)}
          >
            <Icon size={16} />
            <span>{label}</span>
            <em>{hint}</em>
          </button>
        ))}
      </div>

      {tab === "startup" ? <StartupPanel /> : <PrivacyPanel />}
    </div>
  );
}

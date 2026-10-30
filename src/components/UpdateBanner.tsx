import { useState } from "react";
import { Download, ExternalLink, Sparkles, X } from "lucide-react";
import { api } from "../api";
import type { UpdateCheckResult } from "../types";
import { Button, ProgressBar, useToast } from "./ui";
import { usePlatform } from "../platform";
import { formatBytes, formatSpeed, percent } from "../format";
import { useUpdateInstaller } from "../update";

/** 顶部更新通知：点击后直接在通知条内下载、安装，不再跳转设置页。 */
export function UpdateBanner({
  update,
  onSkip,
  onDismiss,
}: {
  update: UpdateCheckResult;
  onSkip: (version: string | null) => void;
  onDismiss: (version: string | null) => void;
}) {
  const toast = useToast();
  const platform = usePlatform();
  const { installing, phase, progress, error, install } = useUpdateInstaller();
  const [launched, setLaunched] = useState(false);
  const version = update.latestVersion;

  const start = async () => {
    setLaunched(false);
    let accelerate = false;
    try {
      accelerate = (await api.settings()).accelerateDownload;
    } catch {
      // 读不到设置时按默认（直连）下载。
    }
    const outcome = await install(accelerate);
    if (!outcome) return;
    if (!outcome.ok) {
      toast("更新失败，可尝试手动下载安装包", "error");
      return;
    }
    if (outcome.result.mode === "installer") {
      setLaunched(true);
      toast(
        platform === "macos"
          ? "自动安装未完成，已打开安装包，请拖入「应用程序」完成更新"
          : "安装程序已启动，请按提示完成更新",
        "info",
      );
    }
  };

  const installingNow = phase === "installing";
  const progressValue = installingNow
    ? 100
    : progress?.total
      ? percent(progress.downloaded, progress.total)
      : 0;
  const progressMeta = installingNow
    ? "正在安装并重启…"
    : progress
      ? `${formatBytes(progress.downloaded)}${
          progress.total ? ` / ${formatBytes(progress.total)}` : ""
        } · ${formatSpeed(progress.bytesPerSec)}`
      : "正在连接下载源…";

  return (
    <div className={`update-banner ${installing ? "is-installing" : ""}`} role="status">
      <span className="update-banner-icon">
        {installing ? <Download size={16} /> : <Sparkles size={16} />}
      </span>
      <div className="update-banner-copy">
        <div className="update-banner-line">
          <strong>{launched ? "安装程序已启动" : `发现新版本 v${version}`}</strong>
          <span>
            {installing
              ? installingNow
                ? "正在安装并重启…"
                : "正在下载更新…"
              : launched
                ? "请按提示完成更新"
                : `当前版本 v${update.currentVersion}`}
          </span>
        </div>
        {installing && (
          <div className="update-banner-progress">
            <ProgressBar value={progressValue} />
            <span className="update-banner-meta">{progressMeta}</span>
          </div>
        )}
        {!installing && error && (
          <div className="update-banner-error">
            <span title={error}>{error}</span>
            <Button
              size="sm"
              variant="quiet"
              icon={<ExternalLink size={14} />}
              onClick={() => api.openExternal(update.releaseUrl).catch(() => {})}
            >
              手动下载
            </Button>
          </div>
        )}
      </div>
      {!installing && (
        <>
          {!launched && (
            <Button
              size="sm"
              variant="primary"
              icon={<Download size={14} />}
              onClick={() => void start()}
            >
              {error ? "重试更新" : "立即更新"}
            </Button>
          )}
          {!launched && (
            <Button size="sm" variant="quiet" onClick={() => onSkip(version)}>
              本次不更新
            </Button>
          )}
          <button
            className="update-banner-close"
            aria-label={launched ? "关闭" : "稍后提醒"}
            onClick={() => onDismiss(version)}
          >
            <X size={15} />
          </button>
        </>
      )}
    </div>
  );
}

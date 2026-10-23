import { useCallback, useEffect, useState } from "react";
import { FolderSearch, RotateCcw, ShieldAlert, ShieldCheck, X } from "lucide-react";
import { api } from "../api";
import type { DiskPermissionStatus } from "../types";
import { Button, Card, useToast } from "./ui";
import { PermissionRestartModal, usePermissionRestart } from "./permissionRestart";

/**
 * 磁盘扫描类页面的权限提示。
 * 只在 macOS 且缺少“完全磁盘访问”时出现，不会自动触发系统弹窗；
 * 用户点击“允许文件访问”后才会访问桌面/文稿/下载并让系统询问。
 */
export function PermissionNotice() {
  const toast = useToast();
  const [status, setStatus] = useState<DiskPermissionStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [requesting, setRequesting] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const {
    open: restartPromptOpen,
    close: closeRestartPrompt,
    restart,
    restarting,
    expectRestart,
  } = usePermissionRestart();

  const load = useCallback(async (silent = false) => {
    if (!silent) setChecking(true);
    try {
      setStatus(await api.diskPermission());
    } catch {
      if (!silent) setStatus(null);
    } finally {
      if (!silent) setChecking(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // 从系统设置切回窗口时自动重新检测，授权后无需手动点“重新检查”。
  useEffect(() => {
    const recheck = () => {
      if (document.visibilityState === "visible") void load(true);
    };
    window.addEventListener("focus", recheck);
    document.addEventListener("visibilitychange", recheck);
    return () => {
      window.removeEventListener("focus", recheck);
      document.removeEventListener("visibilitychange", recheck);
    };
  }, [load]);

  const restartModal = (
    <PermissionRestartModal
      open={restartPromptOpen}
      onClose={closeRestartPrompt}
      onRestart={() => void restart()}
      restarting={restarting}
    />
  );

  if (!status?.supported || status.fullDiskAccess) return null;
  if (dismissed) return restartModal;

  const requestFolders = async () => {
    setRequesting(true);
    try {
      const folders = await api.requestFileAccess();
      const denied = folders.filter((folder) => folder.granted === false);
      const granted = folders.filter((folder) => folder.granted === true);
      if (denied.length > 0) {
        toast(
          `未获得 ${denied.map((f) => f.label).join("、")} 的访问权限，可在系统设置中开启`,
          "error",
        );
      } else if (granted.length > 0) {
        toast(`已获得 ${granted.map((f) => f.label).join("、")} 的访问权限`, "success");
      }
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setRequesting(false);
    }
  };

  const openSettings = async () => {
    try {
      expectRestart();
      await api.openPrivacySettings("full-disk-access");
      toast("请在系统设置中将 Mole Desktop 加入完全磁盘访问列表，返回应用后会提示重启", "info");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  return (
    <>
      {restartModal}
      <Card className="permission-card">
      <span className="permission-icon">
        <ShieldAlert size={19} />
      </span>
      <div className="permission-main">
        <strong>磁盘访问权限受限</strong>
        <p>
          macOS 会阻止读取下载、桌面、文稿以及部分系统目录，扫描结果可能不完整。
          可以先允许文件访问，或在系统设置中开启完全磁盘访问；授权后需重启应用才会生效。
        </p>
        {!status.bundled && (
          <p className="permission-hint">
            当前为开发模式（未打包），macOS 会把权限记在启动本应用的终端/IDE 名下。
            请为对应程序授权，或使用打包后的 .app 验证。
          </p>
        )}
        <div className="permission-actions">
          <Button
            size="sm"
            variant="primary"
            icon={<FolderSearch size={14} />}
            loading={requesting}
            onClick={() => void requestFolders()}
          >
            允许文件访问
          </Button>
          <Button size="sm" icon={<ShieldCheck size={14} />} onClick={() => void openSettings()}>
            开启完全磁盘访问
          </Button>
          <Button
            size="sm"
            icon={<RotateCcw size={14} />}
            loading={restarting}
            onClick={() => void restart()}
          >
            重启应用
          </Button>
          <Button
            size="sm"
            variant="quiet"
            icon={<ShieldAlert size={14} />}
            loading={checking}
            onClick={() => void load()}
          >
            重新检查
          </Button>
        </div>
      </div>
      <button className="icon-btn" aria-label="忽略" onClick={() => setDismissed(true)}>
        <X size={16} />
      </button>
    </Card>
    </>
  );
}

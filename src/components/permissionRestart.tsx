import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { Button, Modal } from "./ui";

/**
 * 打开“完全磁盘访问”设置后，用户切回窗口时提示重启。
 * macOS 只在进程启动时读取 TCC 授权，运行中勾选的权限必须重启应用才生效。
 */
export function usePermissionRestart() {
  const [open, setOpen] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const expecting = useRef(false);

  /** 在跳转系统设置的“完全磁盘访问”面板前调用。 */
  const expectRestart = useCallback(() => {
    expecting.current = true;
  }, []);

  useEffect(() => {
    const check = () => {
      if (document.visibilityState !== "visible" || !expecting.current) return;
      expecting.current = false;
      setOpen(true);
    };
    window.addEventListener("focus", check);
    document.addEventListener("visibilitychange", check);
    return () => {
      window.removeEventListener("focus", check);
      document.removeEventListener("visibilitychange", check);
    };
  }, []);

  const restart = useCallback(async () => {
    setRestarting(true);
    try {
      await api.restartApp();
    } catch {
      setRestarting(false);
    }
  }, []);

  const close = useCallback(() => setOpen(false), []);

  return { open, close, restart, restarting, expectRestart };
}

export function PermissionRestartModal({
  open,
  onClose,
  onRestart,
  restarting,
}: {
  open: boolean;
  onClose: () => void;
  onRestart: () => void;
  restarting: boolean;
}) {
  return (
    <Modal
      open={open}
      title="重启应用以启用权限"
      description="如果已在系统设置中开启完全磁盘访问，需要重启应用后才会生效。"
      onClose={onClose}
      footer={
        <>
          <Button variant="quiet" onClick={onClose}>
            稍后
          </Button>
          <Button variant="primary" loading={restarting} onClick={onRestart}>
            立即重启
          </Button>
        </>
      }
    />
  );
}

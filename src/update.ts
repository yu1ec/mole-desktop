import { useCallback, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import type { UpdateDownloadProgress, UpdateInstallResult } from "./types";

export type UpdateInstallPhase = "downloading" | "installing";

export type UpdateInstallOutcome =
  | { ok: true; result: UpdateInstallResult }
  | { ok: false; error: string };

/** 下载并安装更新，同时把下载进度事件转换成可渲染的状态。 */
export function useUpdateInstaller() {
  const [installing, setInstalling] = useState(false);
  const [phase, setPhase] = useState<UpdateInstallPhase | null>(null);
  const [progress, setProgress] = useState<UpdateDownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const busy = useRef(false);

  const install = useCallback(
    async (accelerate: boolean): Promise<UpdateInstallOutcome | null> => {
      if (busy.current) return null;
      busy.current = true;
      setInstalling(true);
      setPhase("downloading");
      setProgress(null);
      setError(null);
      let unlisten: (() => void) | undefined;
      let unlistenInstalling: (() => void) | undefined;
      try {
        unlisten = await listen<UpdateDownloadProgress>("update-download-progress", (event) => {
          setProgress(event.payload);
        });
        unlistenInstalling = await listen("update-installing", () => {
          setPhase("installing");
        });
        const result = await api.downloadAndInstallUpdate(accelerate);
        return { ok: true, result };
      } catch (err) {
        const message = String(err);
        setError(message);
        return { ok: false, error: message };
      } finally {
        unlisten?.();
        unlistenInstalling?.();
        busy.current = false;
        setInstalling(false);
        setPhase(null);
        setProgress(null);
      }
    },
    [],
  );

  return { installing, phase, progress, error, install };
}

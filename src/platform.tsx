import { createContext, useContext, type ReactNode } from "react";
import type { Platform } from "./types";

const PlatformContext = createContext<Platform>("macos");

export function PlatformProvider({
  platform,
  children,
}: {
  platform: Platform;
  children: ReactNode;
}) {
  return <PlatformContext.Provider value={platform}>{children}</PlatformContext.Provider>;
}

export function usePlatform(): Platform {
  return useContext(PlatformContext);
}

/**
 * 首次渲染时后端快照还没回来，先用 WebView 的 userAgent 判断平台，
 * 避免不适用于当前系统的菜单闪一下。
 */
export function detectPlatform(): Platform {
  if (typeof navigator === "undefined") return "linux";
  const ua = navigator.userAgent;
  if (/windows/i.test(ua)) return "windows";
  if (/macintosh|mac os x/i.test(ua)) return "macos";
  return "linux";
}

/** Windows 叫回收站，macOS 叫废纸篓。 */
export function trashName(platform: Platform): string {
  return platform === "windows" ? "回收站" : "废纸篓";
}

export function revealLabel(platform: Platform): string {
  if (platform === "windows") return "在资源管理器中显示";
  if (platform === "macos") return "在 Finder 中显示";
  return "在文件管理器中显示";
}

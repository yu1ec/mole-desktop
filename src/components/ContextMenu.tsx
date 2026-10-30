import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { Copy, FileText, FolderSearch } from "lucide-react";
import { api } from "../api";
import { revealLabel, usePlatform } from "../platform";
import { useToast } from "./ui";

export type ContextMenuItem =
  | {
      kind?: "item";
      label: string;
      icon?: ReactNode;
      onSelect: () => void;
      disabled?: boolean;
      danger?: boolean;
    }
  | { kind: "separator" }
  | { kind: "label"; label: string };

type OpenMenu = (x: number, y: number, items: ContextMenuItem[]) => void;

const ContextMenuContext = createContext<OpenMenu>(() => {});

/** Opens a custom context menu at the pointer position. */
export function useContextMenu(): OpenMenu {
  return useContext(ContextMenuContext);
}

type MenuState = { x: number; y: number; items: ContextMenuItem[] };

export function ContextMenuProvider({ children }: { children: ReactNode }) {
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const menuRef = useRef<HTMLDivElement>(null);

  const open = useCallback<OpenMenu>((x, y, items) => {
    if (items.length === 0) return;
    setPosition({ x, y });
    setMenu({ x, y, items });
  }, []);

  const close = useCallback(() => setMenu(null), []);

  useEffect(() => {
    if (!menu) return;
    const onPointerDown = (event: MouseEvent) => {
      if (menuRef.current?.contains(event.target as Node)) return;
      close();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("mousedown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    window.addEventListener("scroll", close, true);
    return () => {
      window.removeEventListener("mousedown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      window.removeEventListener("scroll", close, true);
    };
  }, [menu, close]);

  // Keep the menu inside the window without letting it flash at the edge.
  useLayoutEffect(() => {
    if (!menu) return;
    const element = menuRef.current;
    if (!element) return;
    const rect = element.getBoundingClientRect();
    const padding = 8;
    setPosition({
      x: Math.max(padding, Math.min(menu.x, window.innerWidth - rect.width - padding)),
      y: Math.max(padding, Math.min(menu.y, window.innerHeight - rect.height - padding)),
    });
  }, [menu]);

  return (
    <ContextMenuContext.Provider value={open}>
      {children}
      {menu &&
        createPortal(
          <div
            ref={menuRef}
            className="context-menu"
            style={{ left: position.x, top: position.y }}
            role="menu"
            onContextMenu={(event) => event.preventDefault()}
          >
            {menu.items.map((item, index) => {
              if (item.kind === "separator") {
                return <div key={`separator-${index}`} className="context-menu-separator" />;
              }
              if (item.kind === "label") {
                return (
                  <div key={`label-${index}`} className="context-menu-label">
                    {item.label}
                  </div>
                );
              }
              return (
                <button
                  key={`${item.label}-${index}`}
                  type="button"
                  role="menuitem"
                  className={`context-menu-item ${item.danger ? "is-danger" : ""}`}
                  disabled={item.disabled}
                  onClick={() => {
                    if (item.disabled) return;
                    close();
                    item.onSelect();
                  }}
                >
                  <span className="context-menu-icon">{item.icon}</span>
                  <span className="context-menu-text">{item.label}</span>
                </button>
              );
            })}
          </div>,
          document.body,
        )}
    </ContextMenuContext.Provider>
  );
}

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    // WebViews without clipboard permission fall back to the legacy path.
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  const copied = document.execCommand("copy");
  document.body.removeChild(area);
  if (!copied) throw new Error("copy failed");
}

type PathMenuOptions = { isDir?: boolean; canOpen?: boolean };

/**
 * Right-click menu for a file or directory: open files, reveal the entry in the
 * system file manager, and copy the path. Pass several paths for lists that
 * group them.
 */
export function usePathMenu() {
  const openMenu = useContextMenu();
  const platform = usePlatform();
  const toast = useToast();

  const openOne = useCallback(
    (event: React.MouseEvent, path: string, options: PathMenuOptions = {}) => {
      // Stop the native menu before any async work, otherwise it would pop up
      // while we are still resolving whether the path is a directory.
      event.preventDefault();
      event.stopPropagation();
      const x = event.clientX;
      const y = event.clientY;

      const reveal = () =>
        api.reveal(path).catch((err) => toast(String(err), "error"));
      const open = () => api.openPath(path).catch((err) => toast(String(err), "error"));
      const copy = () =>
        copyText(path)
          .then(() => toast("路径已复制", "success"))
          .catch(() => toast("复制失败", "error"));

      const show = (isDir: boolean | null) => {
        const items: ContextMenuItem[] = [];
        // "打开" means opening the file with its default app, so directories
        // only offer the file manager entry below.
        if (isDir === false && options.canOpen !== false) {
          items.push({ label: "打开", icon: <FileText size={14} />, onSelect: open });
        }
        items.push({
          label: revealLabel(platform),
          icon: <FolderSearch size={14} />,
          onSelect: reveal,
        });
        items.push({ kind: "separator" });
        items.push({ label: "复制路径", icon: <Copy size={14} />, onSelect: copy });
        openMenu(x, y, items);
      };

      if (options.isDir !== undefined) {
        show(options.isDir);
        return;
      }
      // Some lists (app leftovers, cleanup rules) mix files and directories, so
      // ask the backend instead of guessing from the entry.
      api
        .pathIsDir(path)
        .then((isDir) => show(isDir ?? null))
        .catch(() => show(null));
    },
    [openMenu, platform, toast],
  );

  return useCallback(
    (event: React.MouseEvent, path: string | string[], options: PathMenuOptions = {}) => {
      if (typeof path === "string") {
        const value = path.trim();
        if (value) openOne(event, value, options);
        return;
      }
      const paths = path.map((item) => item.trim()).filter(Boolean);
      if (paths.length === 0) return;
      if (paths.length === 1) {
        openOne(event, paths[0], options);
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      openMenu(event.clientX, event.clientY, [
        { kind: "label", label: revealLabel(platform) },
        ...paths.slice(0, 8).map((item) => ({
          label: item,
          icon: <FolderSearch size={14} />,
          onSelect: () => api.reveal(item).catch((err) => toast(String(err), "error")),
        })),
        { kind: "separator" as const },
        {
          label: "复制全部路径",
          icon: <Copy size={14} />,
          onSelect: () =>
            copyText(paths.join("\n"))
              .then(() => toast("路径已复制", "success"))
              .catch(() => toast("复制失败", "error")),
        },
      ]);
    },
    [openMenu, openOne, platform, toast],
  );
}

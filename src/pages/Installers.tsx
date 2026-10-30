import { useEffect, useRef, useState } from "react";
import { FolderOpen, Package, RefreshCw, Trash2 } from "lucide-react";
import { api } from "../api";
import { useCachedQuery } from "../cache";
import { formatBytes, relativeTime } from "../format";
import type { InstallerFile, OperationResult } from "../types";
import {
  Button,
  Card,
  Checkbox,
  EmptyState,
  Modal,
  Spinner,
  useToast,
} from "../components/ui";
import { PermissionNotice } from "../components/PermissionNotice";
import { usePathMenu } from "../components/ContextMenu";
import { trashName, usePlatform } from "../platform";

const EMPTY_FILES: InstallerFile[] = [];

export function Installers() {
  const toast = useToast();
  const pathMenu = usePathMenu();
  const trash = trashName(usePlatform());
  const {
    data: fileData,
    loading,
    refreshing,
    error,
    refresh,
  } = useCachedQuery<InstallerFile[]>("installers", api.listInstallers, { ttl: 30_000 });
  const files = fileData ?? EMPTY_FILES;
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [running, setRunning] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [result, setResult] = useState<OperationResult | null>(null);
  const initialized = useRef(false);

  // Select everything once; keep the user's edits across background refreshes.
  useEffect(() => {
    if (!fileData) return;
    if (!initialized.current) {
      initialized.current = true;
      setSelected(new Set(fileData.map((file) => file.path)));
      return;
    }
    setSelected((current) => {
      const paths = new Set(fileData.map((file) => file.path));
      return new Set([...current].filter((path) => paths.has(path)));
    });
  }, [fileData]);

  const load = () => refresh({ force: true });

  const selectedItems = files.filter((file) => selected.has(file.path));
  const selectedSize = selectedItems.reduce((total, file) => total + file.size, 0);
  const totalSize = files.reduce((total, file) => total + file.size, 0);

  const execute = async () => {
    setConfirmOpen(false);
    setRunning(true);
    try {
      const data = await api.deleteInstallers([...selected]);
      setResult(data);
      toast(`释放 ${formatBytes(data.freed)}`, data.errors.length ? "info" : "success");
      await load();
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>安装包</h1>
          <p>找出下载目录、桌面与缓存中的 DMG、PKG、MSI 等安装文件</p>
        </div>
        <div className="head-actions">
          <Button
            icon={<RefreshCw size={16} />}
            onClick={load}
            loading={loading || refreshing}
          >
            重新扫描
          </Button>
          <Button
            variant="primary"
            icon={<Trash2 size={16} />}
            disabled={selected.size === 0}
            loading={running}
            onClick={() => setConfirmOpen(true)}
          >
            删除选中项
          </Button>
        </div>
      </header>

      <PermissionNotice />

      {error && <Card className="error-card">{error}</Card>}

      {loading && files.length === 0 ? (
        <Spinner label="正在查找安装包…" />
      ) : files.length === 0 ? (
        <EmptyState
          icon={<Package size={24} />}
          title="没有发现安装包"
          hint="下载目录和桌面很干净，或者安装包已被清理。"
          action={<Button onClick={load}>重新扫描</Button>}
        />
      ) : (
        <>
          <Card className="summary-strip">
            <div className="summary-main">
              <span className="summary-icon">
                <Package size={22} />
              </span>
              <div>
                <strong>{formatBytes(totalSize)}</strong>
                <span>{files.length} 个安装包文件</span>
              </div>
            </div>
            <div className="summary-side">
              <div>
                <span>已选择</span>
                <strong>{formatBytes(selectedSize)}</strong>
              </div>
            </div>
          </Card>

          <Card className="installer-list">
            <div className="installer-list-head">
              <button
                className="link-btn"
                onClick={() =>
                  setSelected(
                    selected.size === files.length ? new Set() : new Set(files.map((file) => file.path)),
                  )
                }
              >
                {selected.size === files.length ? "取消全选" : "全选"}
              </button>
              <span>{files.length} 个文件</span>
            </div>
            {files.map((file) => (
              <div
                className="installer-row"
                key={file.path}
                onContextMenu={(event) => pathMenu(event, file.path, { isDir: false })}
              >
                <Checkbox
                  checked={selected.has(file.path)}
                  onChange={(value) =>
                    setSelected((current) => {
                      const next = new Set(current);
                      if (value) next.add(file.path);
                      else next.delete(file.path);
                      return next;
                    })
                  }
                  className="installer-check"
                  label={
                    <span className="installer-label">
                      <span className="installer-name">{file.name}</span>
                      <span className="badge badge-ok">{file.source}</span>
                    </span>
                  }
                  hint={
                    <span className="path-text" title={file.path}>
                      {file.path} · {relativeTime(file.modified)}
                    </span>
                  }
                />
                <span className="installer-size">{formatBytes(file.size)}</span>
                <button
                  className="icon-btn"
                  title="在文件夹中显示"
                  onClick={() => api.reveal(file.path).catch(() => {})}
                >
                  <FolderOpen size={16} />
                </button>
              </div>
            ))}
          </Card>

          <Card className="clean-note">
            <Trash2 size={16} />
            <p>
              安装包会被<strong>永久删除</strong>，不会进入{trash}。删除前请确认对应软件已经安装完成，且不再需要离线安装。
            </p>
          </Card>
        </>
      )}

      <Modal
        open={confirmOpen}
        title={`删除 ${selected.size} 个安装包？`}
        description={`将释放约 ${formatBytes(selectedSize)}，删除后不可恢复。`}
        tone="danger"
        onClose={() => setConfirmOpen(false)}
        footer={
          <>
            <Button variant="quiet" onClick={() => setConfirmOpen(false)}>
              取消
            </Button>
            <Button variant="danger" loading={running} onClick={execute}>
              确认删除
            </Button>
          </>
        }
      >
        <ul className="confirm-list">
          {selectedItems.slice(0, 12).map((file) => (
            <li key={file.path}>
              <span>{file.name}</span>
              <em>{formatBytes(file.size)}</em>
            </li>
          ))}
        </ul>
      </Modal>

      <Modal
        open={!!result}
        title="安装包清理完成"
        description={result ? `释放 ${formatBytes(result.freed)}，删除 ${result.files} 个文件。` : ""}
        onClose={() => setResult(null)}
        footer={
          <Button variant="primary" onClick={() => setResult(null)}>
            完成
          </Button>
        }
      >
        {result && result.errors.length > 0 && (
          <div className="result-errors">
            <strong>{result.errors.length} 个文件未能删除</strong>
            <ul>
              {result.errors.slice(0, 6).map((item, index) => (
                <li key={index}>{item}</li>
              ))}
            </ul>
          </div>
        )}
      </Modal>
    </div>
  );
}

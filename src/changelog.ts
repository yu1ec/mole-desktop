/**
 * 打包进应用的发布说明。release-notes/*.md 在构建时以原始文本内联，
 * 因此更新后首次启动、甚至离线状态下也能展示变更记录。
 */
const modules = import.meta.glob("../release-notes/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

export interface ReleaseNotes {
  version: string;
  body: string;
}

function parseVersion(version: string): number[] {
  return version
    .replace(/^v/, "")
    .split(/[.+-]/)
    .slice(0, 3)
    .map((part) => Number.parseInt(part, 10) || 0);
}

function compareVersions(a: string, b: string): number {
  const left = parseVersion(a);
  const right = parseVersion(b);
  for (let index = 0; index < 3; index += 1) {
    const diff = (left[index] ?? 0) - (right[index] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

/** 去掉发布说明开头的版本标题和末尾的下载说明，只保留应用内可见内容。 */
function trimBody(raw: string): string {
  let body = raw.replace(/^##\s+Mole Desktop[^\n]*\n+/, "").trim();
  const download = body.search(/\n---\s*\n\s*###\s*下载\s*\n/);
  if (download >= 0) body = body.slice(0, download);
  return body.replace(/\n+---\s*$/, "").trim();
}

export const releaseNotes: ReleaseNotes[] = Object.entries(modules)
  .map(([path, raw]) => {
    const filename = path.split("/").pop() ?? "";
    const version = filename.replace(/^v/, "").replace(/\.md$/, "");
    return { version, body: trimBody(raw) };
  })
  .filter((entry) => entry.version)
  .sort((a, b) => compareVersions(b.version, a.version));

export function notesFor(version: string): ReleaseNotes | undefined {
  const target = version.trim().replace(/^v/, "");
  return releaseNotes.find((entry) => entry.version === target);
}

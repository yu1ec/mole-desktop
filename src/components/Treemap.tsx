import { useEffect, useRef, useState } from "react";
import type { AnalyzeEntry } from "../types";
import { formatBytes } from "../format";

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Cell extends Rect {
  entry: AnalyzeEntry;
  index: number;
}

const COLORS = [
  "#c6f36b",
  "#8fe3c0",
  "#7cc7f5",
  "#b8a7f7",
  "#f2b880",
  "#f58fa8",
  "#a3d977",
  "#67c9c0",
  "#8fa9f5",
  "#d9c26f",
];

function sum(entries: AnalyzeEntry[]): number {
  return entries.reduce((total, entry) => total + entry.size, 0);
}

function layout(entries: AnalyzeEntry[], rect: Rect, offset = 0, out: Cell[] = []): Cell[] {
  if (entries.length === 0) return out;
  if (entries.length === 1) {
    out.push({ ...rect, entry: entries[0], index: offset });
    return out;
  }
  const total = sum(entries);
  let acc = 0;
  let split = 1;
  let best = Number.POSITIVE_INFINITY;
  for (let index = 1; index < entries.length; index += 1) {
    acc += entries[index - 1].size;
    const diff = Math.abs(acc - total / 2);
    if (diff < best) {
      best = diff;
      split = index;
    }
  }
  const first = entries.slice(0, split);
  const second = entries.slice(split);
  const ratio = sum(first) / total;
  if (rect.w >= rect.h) {
    const w1 = rect.w * ratio;
    layout(first, { x: rect.x, y: rect.y, w: w1, h: rect.h }, offset, out);
    layout(
      second,
      { x: rect.x + w1, y: rect.y, w: Math.max(0, rect.w - w1), h: rect.h },
      offset + first.length,
      out,
    );
  } else {
    const h1 = rect.h * ratio;
    layout(first, { x: rect.x, y: rect.y, w: rect.w, h: h1 }, offset, out);
    layout(
      second,
      { x: rect.x, y: rect.y + h1, w: rect.w, h: Math.max(0, rect.h - h1) },
      offset + first.length,
      out,
    );
  }
  return out;
}

export function Treemap({
  entries,
  onOpen,
  onContextMenu,
}: {
  entries: AnalyzeEntry[];
  onOpen: (entry: AnalyzeEntry) => void;
  onContextMenu?: (event: React.MouseEvent, entry: AnalyzeEntry) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(720);
  const [hover, setHover] = useState<Cell | null>(null);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const observer = new ResizeObserver((records) => {
      const next = records[0]?.contentRect.width;
      if (next) setWidth(next);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const height = 430;
  const cells = layout(entries, { x: 0, y: 0, w: width, h: height });

  return (
    <div className="treemap" ref={ref}>
      <svg width={width} height={height}>
        {cells.map((cell) => {
          const color = COLORS[cell.index % COLORS.length];
          const showLabel = cell.w > 86 && cell.h > 38;
          const showSize = cell.w > 110 && cell.h > 58;
          return (
            <g
              key={cell.entry.path}
              onClick={() => cell.entry.isDir && onOpen(cell.entry)}
              onContextMenu={(event) => onContextMenu?.(event, cell.entry)}
              onMouseEnter={() => setHover(cell)}
              onMouseLeave={() => setHover(null)}
              className={cell.entry.isDir ? "treemap-cell is-dir" : "treemap-cell"}
            >
              <rect
                x={cell.x + 1.5}
                y={cell.y + 1.5}
                width={Math.max(0, cell.w - 3)}
                height={Math.max(0, cell.h - 3)}
                rx={5}
                fill={color}
                fillOpacity={hover?.entry.path === cell.entry.path ? 0.95 : 0.72}
              />
              {showLabel && (
                <text
                  x={cell.x + 10}
                  y={cell.y + 21}
                  className="treemap-label"
                  fill="#10130f"
                >
                  {cell.entry.name.length > Math.floor(cell.w / 7)
                    ? `${cell.entry.name.slice(0, Math.max(6, Math.floor(cell.w / 7)))}…`
                    : cell.entry.name}
                </text>
              )}
              {showSize && (
                <text x={cell.x + 10} y={cell.y + 38} className="treemap-size" fill="#10130f">
                  {formatBytes(cell.entry.size)}
                </text>
              )}
            </g>
          );
        })}
      </svg>
      {hover && (
        <div className="treemap-tip">
          <strong>{hover.entry.name}</strong>
          <span>
            {formatBytes(hover.entry.size)} · {hover.entry.fileCount.toLocaleString()} 个文件
            {hover.entry.isDir ? " · 点击进入" : ""}
          </span>
        </div>
      )}
    </div>
  );
}

import type { ReactNode } from "react";

/* 发布说明只用到标题、列表、引用和少量行内格式，这里实现一个
   足够小且无额外依赖的 Markdown 渲染器。 */

type Block =
  | { kind: "heading"; level: number; text: string }
  | { kind: "list"; items: string[] }
  | { kind: "quote"; text: string }
  | { kind: "paragraph"; text: string }
  | { kind: "rule" };

const INLINE_PATTERN = /(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))/g;

function renderInline(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  let cursor = 0;
  let key = 0;

  for (const match of text.matchAll(INLINE_PATTERN)) {
    const index = match.index ?? 0;
    if (index > cursor) nodes.push(text.slice(cursor, index));
    const token = match[0];
    if (token.startsWith("`")) {
      nodes.push(<code key={key++}>{token.slice(1, -1)}</code>);
    } else if (token.startsWith("**")) {
      nodes.push(<strong key={key++}>{renderInline(token.slice(2, -2))}</strong>);
    } else {
      const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(token);
      nodes.push(<span key={key++}>{link ? link[1] : token}</span>);
    }
    cursor = index + token.length;
  }

  if (cursor < text.length) nodes.push(text.slice(cursor));
  return nodes;
}

function parse(markdown: string): Block[] {
  const blocks: Block[] = [];
  let list: string[] = [];
  let quote: string[] = [];
  let paragraph: string[] = [];

  const flushList = () => {
    if (list.length > 0) {
      blocks.push({ kind: "list", items: list });
      list = [];
    }
  };
  const flushQuote = () => {
    if (quote.length > 0) {
      blocks.push({ kind: "quote", text: quote.join(" ") });
      quote = [];
    }
  };
  const flushParagraph = () => {
    if (paragraph.length > 0) {
      blocks.push({ kind: "paragraph", text: paragraph.join(" ") });
      paragraph = [];
    }
  };
  const flushAll = () => {
    flushList();
    flushQuote();
    flushParagraph();
  };

  for (const rawLine of markdown.split("\n")) {
    const line = rawLine.trimEnd();
    const trimmed = line.trim();

    if (trimmed === "") {
      flushAll();
      continue;
    }

    const heading = /^(#{1,6})\s+(.*)$/.exec(trimmed);
    if (heading) {
      flushAll();
      blocks.push({ kind: "heading", level: heading[1].length, text: heading[2] });
      continue;
    }

    if (/^---+$/.test(trimmed)) {
      flushAll();
      blocks.push({ kind: "rule" });
      continue;
    }

    if (/^[-*]\s+/.test(trimmed)) {
      flushQuote();
      flushParagraph();
      list.push(trimmed.replace(/^[-*]\s+/, ""));
      continue;
    }

    if (trimmed.startsWith(">")) {
      flushList();
      flushParagraph();
      quote.push(trimmed.replace(/^>\s?/, ""));
      continue;
    }

    flushList();
    flushQuote();
    paragraph.push(trimmed);
  }

  flushAll();
  return blocks;
}

function renderBlock(block: Block, index: number): ReactNode {
  switch (block.kind) {
    case "heading": {
      const level = block.level <= 2 ? 3 : Math.min(6, block.level);
      const Tag = `h${level}` as "h3" | "h4" | "h5" | "h6";
      return <Tag key={index}>{renderInline(block.text)}</Tag>;
    }
    case "list":
      return (
        <ul key={index}>
          {block.items.map((item, itemIndex) => (
            <li key={itemIndex}>{renderInline(item)}</li>
          ))}
        </ul>
      );
    case "quote":
      return (
        <blockquote key={index}>
          <p>{renderInline(block.text)}</p>
        </blockquote>
      );
    case "paragraph":
      return <p key={index}>{renderInline(block.text)}</p>;
    case "rule":
      return <hr key={index} />;
  }
}

export function MarkdownView({ source }: { source: string }) {
  return <div className="changelog-md">{parse(source).map(renderBlock)}</div>;
}

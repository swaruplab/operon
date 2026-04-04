import { useMemo } from 'react';

/**
 * Lightweight Markdown renderer for chat messages.
 * Handles: headers, bold, italic, inline code, code blocks, links, lists.
 * No external dependencies — regex-based conversion to sanitized HTML.
 */

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function markdownToHtml(md: string): string {
  // Split into lines for block-level processing
  const lines = md.split('\n');
  const htmlLines: string[] = [];
  let inCodeBlock = false;
  let codeBlockLang = '';
  let codeBlockContent: string[] = [];
  let inList = false;
  let listType: 'ul' | 'ol' = 'ul';

  const closeList = () => {
    if (inList) {
      htmlLines.push(listType === 'ul' ? '</ul>' : '</ol>');
      inList = false;
    }
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // Code blocks (fenced with ```)
    if (line.trimStart().startsWith('```')) {
      if (!inCodeBlock) {
        closeList();
        inCodeBlock = true;
        codeBlockLang = line.trimStart().slice(3).trim();
        codeBlockContent = [];
      } else {
        const escaped = escapeHtml(codeBlockContent.join('\n'));
        htmlLines.push(
          `<pre class="my-1.5 p-2.5 bg-zinc-950/80 rounded-lg border border-zinc-800/50 overflow-x-auto"><code class="text-[12px] font-mono text-emerald-300/90 leading-relaxed">${escaped}</code></pre>`
        );
        inCodeBlock = false;
        codeBlockLang = '';
      }
      continue;
    }

    if (inCodeBlock) {
      codeBlockContent.push(line);
      continue;
    }

    // Empty line — close lists
    if (line.trim() === '') {
      closeList();
      htmlLines.push('');
      continue;
    }

    // Headers
    const headerMatch = line.match(/^(#{1,6})\s+(.+)/);
    if (headerMatch) {
      closeList();
      const level = headerMatch[1].length;
      const text = processInline(headerMatch[2]);
      const sizes: Record<number, string> = {
        1: 'text-base font-semibold mt-3 mb-1.5',
        2: 'text-[14px] font-semibold mt-2.5 mb-1',
        3: 'text-[13px] font-semibold mt-2 mb-1',
        4: 'text-[13px] font-medium mt-1.5 mb-0.5',
        5: 'text-[12px] font-medium mt-1 mb-0.5',
        6: 'text-[12px] font-medium mt-1 mb-0.5 text-zinc-400',
      };
      htmlLines.push(`<div class="${sizes[level] || sizes[3]}">${text}</div>`);
      continue;
    }

    // Unordered list items
    const ulMatch = line.match(/^(\s*)[-*+]\s+(.+)/);
    if (ulMatch) {
      if (!inList || listType !== 'ul') {
        closeList();
        inList = true;
        listType = 'ul';
        htmlLines.push('<ul class="my-1 ml-4 space-y-0.5 list-disc list-outside">');
      }
      htmlLines.push(`<li class="text-[13px] leading-relaxed">${processInline(ulMatch[2])}</li>`);
      continue;
    }

    // Ordered list items
    const olMatch = line.match(/^(\s*)\d+[.)]\s+(.+)/);
    if (olMatch) {
      if (!inList || listType !== 'ol') {
        closeList();
        inList = true;
        listType = 'ol';
        htmlLines.push('<ol class="my-1 ml-4 space-y-0.5 list-decimal list-outside">');
      }
      htmlLines.push(`<li class="text-[13px] leading-relaxed">${processInline(olMatch[2])}</li>`);
      continue;
    }

    // Horizontal rule
    if (/^[-*_]{3,}\s*$/.test(line.trim())) {
      closeList();
      htmlLines.push('<hr class="my-2 border-zinc-700/50" />');
      continue;
    }

    // Regular paragraph
    closeList();
    htmlLines.push(`<p class="my-0.5 leading-relaxed">${processInline(line)}</p>`);
  }

  // Close any open blocks
  closeList();
  if (inCodeBlock) {
    const escaped = escapeHtml(codeBlockContent.join('\n'));
    htmlLines.push(
      `<pre class="my-1.5 p-2.5 bg-zinc-950/80 rounded-lg border border-zinc-800/50 overflow-x-auto"><code class="text-[12px] font-mono text-emerald-300/90 leading-relaxed">${escaped}</code></pre>`
    );
  }

  return htmlLines.join('\n');
}

/** Process inline markdown: bold, italic, code, links, strikethrough */
function processInline(text: string): string {
  let result = escapeHtml(text);

  // Inline code (must come before bold/italic to avoid conflicts)
  result = result.replace(/`([^`]+)`/g, '<code class="px-1 py-0.5 bg-zinc-900/80 rounded text-[12px] font-mono text-amber-300/90">$1</code>');

  // Bold + italic
  result = result.replace(/\*\*\*(.+?)\*\*\*/g, '<strong><em>$1</em></strong>');

  // Bold
  result = result.replace(/\*\*(.+?)\*\*/g, '<strong class="font-semibold text-zinc-100">$1</strong>');
  result = result.replace(/__(.+?)__/g, '<strong class="font-semibold text-zinc-100">$1</strong>');

  // Italic
  result = result.replace(/\*(.+?)\*/g, '<em>$1</em>');
  result = result.replace(/_(.+?)_/g, '<em>$1</em>');

  // Strikethrough
  result = result.replace(/~~(.+?)~~/g, '<del class="text-zinc-500">$1</del>');

  // Links [text](url)
  result = result.replace(
    /\[([^\]]+)\]\(([^)]+)\)/g,
    '<a href="$2" target="_blank" rel="noopener noreferrer" class="text-blue-400 hover:text-blue-300 underline underline-offset-2">$1</a>'
  );

  // Bare URLs (simple pattern — only outside of already-processed tags)
  result = result.replace(
    /(?<!")(https?:\/\/[^\s<]+)/g,
    '<a href="$1" target="_blank" rel="noopener noreferrer" class="text-blue-400 hover:text-blue-300 underline underline-offset-2">$1</a>'
  );

  return result;
}

interface MarkdownRendererProps {
  text: string;
  className?: string;
}

export function MarkdownRenderer({ text, className = '' }: MarkdownRendererProps) {
  const html = useMemo(() => markdownToHtml(text), [text]);

  return (
    <div
      className={`markdown-content ${className}`}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

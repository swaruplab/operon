import { useState, useEffect, useRef, useMemo } from "react";

interface Command {
  id: string;
  label: string;
  shortcut?: string;
  action: () => void;
}

interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  commands: Command[];
}

export function CommandPalette({ isOpen, onClose, commands }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    if (!query) return commands;
    const lower = query.toLowerCase();
    return commands
      .filter((cmd) => cmd.label.toLowerCase().includes(lower))
      .sort((a, b) => {
        const aIdx = a.label.toLowerCase().indexOf(lower);
        const bIdx = b.label.toLowerCase().indexOf(lower);
        return aIdx - bIdx;
      });
  }, [query, commands]);

  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  // Scroll selected item into view
  useEffect(() => {
    if (listRef.current) {
      const selected = listRef.current.children[selectedIndex] as HTMLElement;
      selected?.scrollIntoView({ block: "nearest" });
    }
  }, [selectedIndex]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (filtered[selectedIndex]) {
        filtered[selectedIndex].action();
        onClose();
      }
    } else if (e.key === "Escape") {
      onClose();
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[12vh]">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onClose}
      />

      {/* Palette */}
      <div className="relative w-[560px] bg-panel rounded-xl border border-border-strong shadow-2xl shadow-black/50 overflow-hidden">
        {/* Search input */}
        <div className="flex items-center px-4 border-b border-border-default">
          <span className="text-muted text-sm mr-2">&gt;</span>
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Type a command..."
            className="flex-1 py-3 bg-transparent text-primary text-sm outline-none placeholder:text-subtle"
          />
        </div>

        {/* Results */}
        <div ref={listRef} className="max-h-[320px] overflow-y-auto py-1">
          {filtered.map((cmd, i) => (
            <button
              key={cmd.id}
              onClick={() => {
                cmd.action();
                onClose();
              }}
              className={`
                w-full flex items-center justify-between px-4 py-2 text-sm transition-colors
                ${i === selectedIndex
                  ? "bg-surface text-primary"
                  : "text-secondary hover:bg-hover/50"
                }
              `}
            >
              <span>{cmd.label}</span>
              {cmd.shortcut && (
                <span className="text-[11px] text-muted font-mono bg-surface px-1.5 py-0.5 rounded">
                  {cmd.shortcut}
                </span>
              )}
            </button>
          ))}

          {filtered.length === 0 && (
            <div className="px-4 py-8 text-center text-subtle text-sm">
              No matching commands
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

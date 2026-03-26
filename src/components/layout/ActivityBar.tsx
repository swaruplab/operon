import { Files, Search, MonitorSmartphone, Settings, BookOpen, HelpCircle, GitBranch, Blocks } from "lucide-react";

interface ActivityBarProps {
  activeView: string;
  onViewChange: (view: string) => void;
}

const items = [
  { id: "files", icon: Files, label: "Explorer" },
  { id: "search", icon: Search, label: "Search" },
  { id: "git", icon: GitBranch, label: "Git & GitHub" },
  { id: "ssh", icon: MonitorSmartphone, label: "Remote SSH" },
  { id: "extensions", icon: Blocks, label: "Extensions" },
  { id: "protocols", icon: BookOpen, label: "Protocols" },
  { id: "help", icon: HelpCircle, label: "Help" },
  { id: "settings", icon: Settings, label: "Settings" },
];

export function ActivityBar({ activeView, onViewChange }: ActivityBarProps) {
  return (
    <div className="w-12 flex flex-col items-center py-2 gap-0.5 bg-zinc-900 border-r border-zinc-800 shrink-0 relative z-10">
      {items.map((item) => {
        const Icon = item.icon;
        const isActive = activeView === item.id;

        return (
          <button
            key={item.id}
            onClick={() => onViewChange(item.id)}
            className={`
              relative w-10 h-10 flex items-center justify-center rounded-md transition-colors
              ${isActive
                ? "text-zinc-50"
                : "text-zinc-500 hover:text-zinc-300"
              }
            `}
            title={item.label}
          >
            {isActive && (
              <div className="absolute left-0 top-2 bottom-2 w-[2px] bg-blue-500 rounded-r" />
            )}
            <Icon className="w-5 h-5" strokeWidth={1.5} />
          </button>
        );
      })}
    </div>
  );
}

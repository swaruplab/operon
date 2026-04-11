import { Files, Search, MonitorSmartphone, Settings, BookOpen, HelpCircle, GitBranch, Blocks } from "lucide-react";
import { Tooltip } from "../ui/Tooltip";

interface ActivityBarProps {
  activeView: string;
  onViewChange: (view: string) => void;
}

const items = [
  { id: "files", icon: Files, label: "Explorer", shortcut: undefined, description: "Browse project files" },
  { id: "search", icon: Search, label: "Search", shortcut: undefined, description: "Search across files" },
  { id: "git", icon: GitBranch, label: "Git & GitHub", shortcut: undefined, description: "Version control and GitHub" },
  { id: "ssh", icon: MonitorSmartphone, label: "Remote SSH", shortcut: undefined, description: "Connect to remote servers" },
  { id: "extensions", icon: Blocks, label: "Extensions", shortcut: undefined, description: "Manage extensions" },
  { id: "protocols", icon: BookOpen, label: "Protocols", shortcut: undefined, description: "Analysis protocols" },
  { id: "help", icon: HelpCircle, label: "Help", shortcut: undefined, description: "Documentation and guides" },
  { id: "settings", icon: Settings, label: "Settings", shortcut: "\u2318,", description: "App preferences" },
];

export function ActivityBar({ activeView, onViewChange }: ActivityBarProps) {
  return (
    <div className="w-12 flex flex-col items-center py-2 gap-0.5 bg-zinc-900 border-r border-zinc-800 shrink-0 relative z-10">
      {items.map((item) => {
        const Icon = item.icon;
        const isActive = activeView === item.id;

        return (
          <Tooltip
            key={item.id}
            label={item.description}
            shortcut={item.shortcut}
            position="right"
            delay={80}
          >
            <button
              onClick={() => onViewChange(item.id)}
              className={`
                relative w-10 h-10 flex items-center justify-center rounded-md transition-colors
                ${isActive
                  ? "text-zinc-50"
                  : "text-zinc-500 hover:text-zinc-300"
                }
              `}
            >
              {isActive && (
                <div className="absolute left-0 top-2 bottom-2 w-[2px] bg-blue-500 rounded-r" />
              )}
              <Icon className="w-5 h-5" strokeWidth={1.5} />
            </button>
          </Tooltip>
        );
      })}
    </div>
  );
}

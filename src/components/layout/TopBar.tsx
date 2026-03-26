import { Hammer, Settings, Wifi, HelpCircle } from "lucide-react";

interface TopBarProps {
  onToggleSidebar: () => void;
  onToggleChat: () => void;
  onOpenSettings?: () => void;
  onOpenHelp?: () => void;
}

export function TopBar({ onToggleSidebar, onToggleChat, onOpenSettings, onOpenHelp }: TopBarProps) {
  return (
    <div className="h-10 flex items-center bg-zinc-900 border-b border-zinc-800 shrink-0">
      {/* macOS traffic light spacer (when using transparent titlebar) */}
      <div className="w-[78px] shrink-0" />

      {/* App branding */}
      <div className="flex items-center gap-2 px-3">
        <Hammer className="w-4 h-4 text-blue-400" />
        <span className="text-sm font-semibold text-zinc-200 tracking-tight">Operon</span>
        {__APP_VERSION__ === 'dev' ? (
          <span className="text-[10px] font-mono font-semibold px-1.5 py-0.5 rounded bg-orange-500/20 text-orange-400 border border-orange-500/30">DEV</span>
        ) : (
          <span className="text-[10px] text-zinc-500 font-mono">v{__APP_VERSION__}</span>
        )}
      </div>

      {/* Center: project name */}
      <div className="flex-1 flex justify-center">
        <button className="flex items-center gap-1.5 px-3 py-1 rounded-md hover:bg-zinc-800 transition-colors text-xs text-zinc-400">
          <span>~/projects/my-app</span>
          <span className="text-zinc-600">▼</span>
        </button>
      </div>

      {/* Right: auth status + settings */}
      <div className="flex items-center gap-2 px-3">
        {/* Connection status */}
        <div className="flex items-center gap-1.5 px-2 py-1 rounded text-xs">
          <div className="w-1.5 h-1.5 rounded-full bg-green-500" />
          <span className="text-zinc-400">Connected</span>
        </div>

        <button
          onClick={onToggleChat}
          className="p-1.5 rounded hover:bg-zinc-800 transition-colors text-zinc-400 hover:text-zinc-200"
          title="Toggle Chat"
        >
          <Wifi className="w-4 h-4" />
        </button>

        <button
          onClick={onOpenHelp}
          className="p-1.5 rounded hover:bg-zinc-800 transition-colors text-zinc-400 hover:text-zinc-200"
          title="Help"
        >
          <HelpCircle className="w-4 h-4" />
        </button>

        <button
          onClick={onOpenSettings}
          className="p-1.5 rounded hover:bg-zinc-800 transition-colors text-zinc-400 hover:text-zinc-200"
          title="Settings"
        >
          <Settings className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}

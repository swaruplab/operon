import { Hammer, Settings, Wifi, HelpCircle, Sun, Moon } from "lucide-react";
import { isMac } from "../../lib/platform";
import { UpdateChecker } from "./UpdateChecker";
import { useTheme } from "../../context/ThemeContext";
import { useProject } from "../../context/ProjectContext";
import { basename } from "../../lib/path";

interface TopBarProps {
  onToggleSidebar: () => void;
  onToggleChat: () => void;
  onOpenSettings?: () => void;
  onOpenHelp?: () => void;
}

export function TopBar({ onToggleSidebar, onToggleChat, onOpenSettings, onOpenHelp }: TopBarProps) {
  const { resolved, toggle } = useTheme();
  const { projectPath } = useProject();
  return (
    <div className="h-10 flex items-center bg-panel border-b border-border-default shrink-0">
      {/* macOS traffic light spacer (when using transparent titlebar) */}
      {isMac && <div className="w-[78px] shrink-0" />}

      {/* App branding */}
      <div className="flex items-center gap-2 px-3">
        <Hammer className="w-4 h-4 text-blue-600 dark:text-blue-400" />
        <span className="text-sm font-semibold text-secondary tracking-tight">Operon</span>
        {__APP_VERSION__ === 'dev' ? (
          <span className="text-[10px] font-mono font-semibold px-1.5 py-0.5 rounded bg-orange-500/20 text-orange-600 dark:text-orange-400 border border-orange-500/30">DEV</span>
        ) : (
          <span className="text-[10px] text-muted font-mono">v{__APP_VERSION__}</span>
        )}
      </div>

      {/* Center: project name */}
      <div className="flex-1 flex justify-center">
        <button
          className="flex items-center gap-1.5 px-3 py-1 rounded-md hover:bg-hover transition-colors text-xs text-secondary max-w-[40vw]"
          title={projectPath ?? undefined}
        >
          <span className="truncate">{projectPath ? basename(projectPath) : 'No folder open'}</span>
          <span className="text-subtle shrink-0">▼</span>
        </button>
      </div>

      {/* Right: update status + auth + settings */}
      <div className="flex items-center gap-2 px-3">
        {/* Update checker */}
        <UpdateChecker />

        {/* Connection status */}
        <div className="flex items-center gap-1.5 px-2 py-1 rounded text-xs">
          <div className="w-1.5 h-1.5 rounded-full bg-green-500" />
          <span className="text-secondary">Connected</span>
        </div>

        <button
          onClick={toggle}
          className="p-1.5 rounded hover:bg-hover transition-colors text-secondary hover:text-primary"
          title={resolved === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
        >
          {resolved === 'dark'
            ? <Sun className="w-4 h-4 pointer-events-none" />
            : <Moon className="w-4 h-4 pointer-events-none" />}
        </button>

        <button
          onClick={onToggleChat}
          className="p-1.5 rounded hover:bg-hover transition-colors text-secondary hover:text-primary"
          title="Toggle Chat"
        >
          <Wifi className="w-4 h-4" />
        </button>

        <button
          onClick={onOpenHelp}
          className="p-1.5 rounded hover:bg-hover transition-colors text-secondary hover:text-primary"
          title="Help"
        >
          <HelpCircle className="w-4 h-4" />
        </button>

        <button
          onClick={onOpenSettings}
          className="p-1.5 rounded hover:bg-hover transition-colors text-secondary hover:text-primary"
          title="Settings"
        >
          <Settings className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}

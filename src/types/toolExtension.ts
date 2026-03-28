import type { LucideIcon } from 'lucide-react';

export interface CommandDefinition {
  id: string;
  label: string;
  handler: () => void | Promise<void>;
}

export interface ToolExtension {
  id: string;
  name: string;
  icon: LucideIcon;
  description: string;
  checkInstalled(): Promise<boolean>;
  SidebarPanel: React.FC;
  StatusBarItem?: React.FC;
  commands?: CommandDefinition[];
}

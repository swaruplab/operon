import { invoke } from '@tauri-apps/api/core';

export interface PlanHistoryEntry {
  filename: string;
  timestamp: string;
  title: string;
  lines: number;
  path: string;
}

/** List all archived plans from .operon/plan_history/, newest first. */
export async function listPlanHistory(projectPath: string): Promise<PlanHistoryEntry[]> {
  return invoke('list_plan_history', { projectPath });
}

/** Read the content of a specific archived plan. */
export async function readPlanHistoryEntry(path: string): Promise<string> {
  return invoke('read_plan_history_entry', { path });
}

import { invoke } from '@tauri-apps/api/core';
import type { ProjectScan, MethodsInfo, ReportConfig, FilePreview } from '../types/report';

export async function scanProjectFiles(path: string, showHidden?: boolean): Promise<ProjectScan> {
  return invoke('scan_project_files', { path, showHidden });
}

export async function scanRemoteProjectFiles(profileId: string, path: string): Promise<ProjectScan> {
  return invoke('scan_remote_project_files', { profileId, path });
}

export async function extractMethodsInfo(path: string): Promise<MethodsInfo> {
  return invoke('extract_methods_info', { path });
}

export async function readCsvForReport(path: string, maxRows?: number): Promise<[string[], string[][]]> {
  return invoke('read_csv_for_report', { path, maxRows });
}

export async function generateReportPdf(config: ReportConfig): Promise<string> {
  return invoke('generate_report_pdf', { config });
}

export async function batchReadFilePreviews(paths: string[]): Promise<FilePreview[]> {
  return invoke('batch_read_file_previews', { paths });
}

export async function batchReadRemoteFilePreviews(profileId: string, paths: string[]): Promise<FilePreview[]> {
  return invoke('batch_read_remote_file_previews', { profileId, paths });
}

/** Generate a timestamped report filename */
export function generateReportFilename(): string {
  const now = new Date();
  const y = now.getFullYear();
  const mo = String(now.getMonth() + 1).padStart(2, '0');
  const d = String(now.getDate()).padStart(2, '0');
  const h = String(now.getHours()).padStart(2, '0');
  const mi = String(now.getMinutes()).padStart(2, '0');
  return `report_${y}-${mo}-${d}_${h}${mi}.pdf`;
}

/** Format bytes to human readable */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

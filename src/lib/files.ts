import { invoke } from '@tauri-apps/api/core';

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  extension: string | null;
}

export async function listDirectory(path: string, showHidden?: boolean): Promise<FileEntry[]> {
  return invoke('list_directory', { path, showHidden });
}

export async function readFile(path: string): Promise<string> {
  return invoke('read_file', { path });
}

export async function readFileBase64(path: string): Promise<string> {
  return invoke('read_file_base64', { path });
}

export async function writeFile(path: string, content: string): Promise<void> {
  return invoke('write_file', { path, content });
}

export async function getHomeDir(): Promise<string> {
  return invoke('get_home_dir');
}

export async function createFile(path: string): Promise<void> {
  return invoke('create_file', { path });
}

export async function createDirectory(path: string): Promise<void> {
  return invoke('create_directory', { path });
}

export async function deletePath(path: string): Promise<void> {
  return invoke('delete_path', { path });
}

export async function batchDeleteFiles(paths: string[]): Promise<number> {
  return invoke('batch_delete_files', { paths });
}

export async function renamePath(oldPath: string, newPath: string): Promise<void> {
  return invoke('rename_path', { oldPath, newPath });
}

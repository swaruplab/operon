import { invoke } from '@tauri-apps/api/core';
import type { MCPCatalogEntry, MCPServerConfig, MCPServerStatus, DependencyStatus } from '../types/mcp';

export async function getMCPCatalog(): Promise<MCPCatalogEntry[]> {
  return invoke('get_mcp_catalog');
}

export async function listMCPServers(): Promise<MCPServerStatus[]> {
  return invoke('list_mcp_servers');
}

export async function addMCPServer(config: MCPServerConfig): Promise<void> {
  return invoke('add_mcp_server', { config });
}

export async function removeMCPServer(name: string): Promise<void> {
  return invoke('remove_mcp_server', { name });
}

export async function enableMCPServer(name: string): Promise<void> {
  return invoke('enable_mcp_server', { name });
}

export async function disableMCPServer(name: string): Promise<void> {
  return invoke('disable_mcp_server', { name });
}

export async function installMCPServer(catalogId: string): Promise<void> {
  return invoke('install_mcp_server', { catalogId });
}

export async function checkMCPDependencies(serverName: string): Promise<DependencyStatus> {
  return invoke('check_mcp_dependencies', { serverName });
}

export async function checkRemoteMCPDependencies(sshProfile: string, serverName: string): Promise<DependencyStatus> {
  return invoke('check_remote_mcp_dependencies', { sshProfile, serverName });
}

export async function installRemoteMCPServer(sshProfile: string, catalogId: string): Promise<void> {
  return invoke('install_remote_mcp_server', { sshProfile, catalogId });
}

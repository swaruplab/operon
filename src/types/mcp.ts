export interface MCPServerConfig {
  name: string;
  enabled: boolean;
  command: string;
  args: string[];
  env: Record<string, string>;
  catalog_id: string | null;
  description: string | null;
}

export interface MCPCatalogEntry {
  id: string;
  name: string;
  description: string;
  category: string;
  tools_count: number;
  tools_summary: string[];
  databases: string[];
  runtime: string;
  install_command: string;
  config: MCPServerConfig;
  homepage: string;
  license: string;
}

export interface DependencyStatus {
  satisfied: boolean;
  runtime: string;
  runtime_found: boolean;
  runtime_version: string | null;
  min_version: string;
  install_hint: string;
  package_installed: boolean;
}

export interface MCPServerStatus {
  config: MCPServerConfig;
  from_catalog: boolean;
  catalog_entry: MCPCatalogEntry | null;
}

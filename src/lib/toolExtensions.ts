import type { ToolExtension } from '../types/toolExtension';

// Import tool extensions (defined in their respective components)
// These will be populated as we create the components
import { dockerExtension } from '../components/sidebar/DockerPanel';
import { singularityExtension } from '../components/sidebar/SingularityPanel';

const ALL_TOOL_EXTENSIONS: ToolExtension[] = [
  dockerExtension,
  singularityExtension,
];

/**
 * Get all registered tool extensions
 */
export function getToolExtensions(): ToolExtension[] {
  return ALL_TOOL_EXTENSIONS;
}

/**
 * Get only the available tool extensions (where the tool is installed)
 */
export async function getAvailableToolExtensions(): Promise<ToolExtension[]> {
  const available: ToolExtension[] = [];

  for (const ext of ALL_TOOL_EXTENSIONS) {
    try {
      const installed = await ext.checkInstalled();
      if (installed) {
        available.push(ext);
      }
    } catch (err) {
      console.warn(`Failed to check if ${ext.id} is installed:`, err);
    }
  }

  return available;
}

/**
 * Get a specific tool extension by ID
 */
export function getToolExtensionById(id: string): ToolExtension | undefined {
  return ALL_TOOL_EXTENSIONS.find((ext) => ext.id === id);
}

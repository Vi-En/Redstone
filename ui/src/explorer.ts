import { invoke, isTauri } from '@tauri-apps/api/core';

export interface FileEntry {
  name: string;
  relative_path: string;
  is_dir: boolean;
  size_bytes: number;
  modified_ms: number;
}

const previewFilesKey = (instanceName: string) => `redstone.preview.files.${instanceName}`;

function defaultPreviewFiles(): Record<string, string> {
  return {
    'server.properties': '# Minecraft server properties\nserver-port=25565\nmotd=A Redstone Server\ndifficulty=easy\npvp=true\n',
    'eula.txt': '# By changing the setting below to TRUE you are indicating your agreement to our EULA\neula=true\n',
    'config.toml': '# Server configuration\nname = "Redstone Server"\n',
    'logs/latest.log': '[12:00:00] [INFO]: Starting minecraft server version 1.21.4\n[12:00:02] [INFO]: Done (2.1s)! For help, type "help"\n'
  };
}

function getPreviewFiles(instanceName: string): Record<string, string> {
  const stored = localStorage.getItem(previewFilesKey(instanceName));
  if (stored) {
    try {
      return JSON.parse(stored) as Record<string, string>;
    } catch {
      // fallback
    }
  }
  const defaults = defaultPreviewFiles();
  localStorage.setItem(previewFilesKey(instanceName), JSON.stringify(defaults));
  return defaults;
}

function setPreviewFiles(instanceName: string, files: Record<string, string>): void {
  localStorage.setItem(previewFilesKey(instanceName), JSON.stringify(files));
}

export async function listDirectory(instanceName: string, relativePath = ''): Promise<FileEntry[]> {
  if (isTauri()) {
    return invoke<FileEntry[]>('explorer_list_directory', { instanceName, relativePath });
  }

  const files = getPreviewFiles(instanceName);
  const normalizedRel = relativePath.trim().replace(/^\/+|\/+$/g, '');
  const prefix = normalizedRel ? `${normalizedRel}/` : '';
  const entriesMap = new Map<string, FileEntry>();

  for (const [filePath, content] of Object.entries(files)) {
    if (!normalizedRel || filePath.startsWith(prefix)) {
      const rest = normalizedRel ? filePath.slice(prefix.length) : filePath;
      const parts = rest.split('/');
      const topName = parts[0];
      const isDir = parts.length > 1;
      const entryRelPath = normalizedRel ? `${normalizedRel}/${topName}` : topName;

      if (!entriesMap.has(topName)) {
        entriesMap.set(topName, {
          name: topName,
          relative_path: entryRelPath,
          is_dir: isDir,
          size_bytes: isDir ? 0 : content.length,
          modified_ms: 1700000000000
        });
      }
    }
  }

  const result = Array.from(entriesMap.values());
  result.sort((a, b) => {
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
  return result;
}

export async function readFile(instanceName: string, relativePath: string): Promise<string> {
  if (isTauri()) {
    return invoke<string>('explorer_read_file', { instanceName, relativePath });
  }
  const files = getPreviewFiles(instanceName);
  const content = files[relativePath];
  if (content === undefined) {
    throw new Error(`File not found: ${relativePath}`);
  }
  return content;
}

export async function writeFile(instanceName: string, relativePath: string, contents: string): Promise<void> {
  if (isTauri()) {
    await invoke<void>('explorer_write_file', { instanceName, relativePath, contents });
    return;
  }
  const files = getPreviewFiles(instanceName);
  files[relativePath] = contents;
  setPreviewFiles(instanceName, files);
}

export async function deleteFile(instanceName: string, relativePath: string, confirmed: boolean): Promise<void> {
  if (!confirmed) {
    throw new Error('Delete requires confirmation.');
  }
  if (isTauri()) {
    await invoke<void>('explorer_delete_file', { instanceName, relativePath, confirmed });
    return;
  }
  const files = getPreviewFiles(instanceName);
  delete files[relativePath];
  setPreviewFiles(instanceName, files);
}

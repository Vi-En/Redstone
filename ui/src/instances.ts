import { invoke, isTauri } from '@tauri-apps/api/core';

export interface CreateInstanceInput {
  name: string;
  minecraftVersion: string;
  loader: string;
}

export interface CreatedInstance {
  id: number;
  name: string;
  minecraft_version: string;
  loader: string;
  previewOnly: boolean;
}

interface NativeInstance extends Omit<CreatedInstance, 'previewOnly'> {
  path: string;
  world: string;
}

const previewStorageKey = 'redstone.preview.instances';

export async function createInstance(input: CreateInstanceInput): Promise<CreatedInstance> {
  if (isTauri()) {
    const instance = await invoke<NativeInstance>('create_instance', {
      name: input.name,
      minecraftVersion: input.minecraftVersion,
      loader: input.loader
    });
    return { ...instance, previewOnly: false };
  }

  const name = input.name.trim();
  if (!name || name.length > 48 || !/^[A-Za-z0-9 _-]+$/.test(name)) {
    throw new Error("Name must be 1-48 letters, numbers, spaces, '_' or '-'.");
  }

  const instances = JSON.parse(localStorage.getItem(previewStorageKey) ?? '[]') as CreatedInstance[];
  if (instances.some((instance) => instance.name === name)) {
    throw new Error(`An instance named ${name} already exists.`);
  }

  const instance: CreatedInstance = {
    id: Date.now(),
    name,
    minecraft_version: input.minecraftVersion,
    loader: input.loader,
    previewOnly: true
  };
  instances.push(instance);
  localStorage.setItem(previewStorageKey, JSON.stringify(instances));
  return instance;
}

export async function listInstances(): Promise<CreatedInstance[]> {
  if (isTauri()) {
    const instances = await invoke<NativeInstance[]>('list_instances');
    return instances.map((instance) => ({ ...instance, previewOnly: false }));
  }

  return JSON.parse(localStorage.getItem(previewStorageKey) ?? '[]') as CreatedInstance[];
}

export async function duplicateInstance(sourceName: string, newName: string): Promise<CreatedInstance> {
  if (isTauri()) {
    const instance = await invoke<NativeInstance>('duplicate_instance_by_name', {
      sourceName,
      newName
    });
    return { ...instance, previewOnly: false };
  }

  const trimmed = newName.trim();
  if (!trimmed || trimmed.length > 48 || !/^[A-Za-z0-9 _-]+$/.test(trimmed)) {
    throw new Error("Name must be 1-48 letters, numbers, spaces, '_' or '-'.");
  }

  const instances = JSON.parse(localStorage.getItem(previewStorageKey) ?? '[]') as CreatedInstance[];
  const source = instances.find((instance) => instance.name === sourceName);
  if (!source) {
    throw new Error(`Instance does not exist: ${sourceName}`);
  }
  if (instances.some((instance) => instance.name === trimmed)) {
    throw new Error(`An instance named ${trimmed} already exists.`);
  }

  const copy: CreatedInstance = {
    id: Date.now(),
    name: trimmed,
    minecraft_version: source.minecraft_version,
    loader: source.loader,
    previewOnly: true
  };
  instances.push(copy);
  localStorage.setItem(previewStorageKey, JSON.stringify(instances));
  return copy;
}

export async function deleteInstance(name: string, confirmed: boolean): Promise<void> {
  if (!confirmed) {
    throw new Error('Deleting an instance requires confirmation.');
  }

  if (isTauri()) {
    await invoke<void>('delete_instance_by_name', { name, confirmed });
    return;
  }

  const instances = JSON.parse(localStorage.getItem(previewStorageKey) ?? '[]') as CreatedInstance[];
  const next = instances.filter((instance) => instance.name !== name);
  localStorage.setItem(previewStorageKey, JSON.stringify(next));
}

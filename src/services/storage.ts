import type { StorageSnapshot, StorageStats, StorageTableStats } from "../contracts/types";
import { invokeCommand } from "../api/tauri";

export const StorageService = {
  getStats(): Promise<StorageStats> {
    return invokeCommand<StorageStats>("get_storage_stats");
  },
  getTableStats(): Promise<StorageTableStats> {
    return invokeCommand<StorageTableStats>("get_table_stats");
  },
  getSnapshot(): Promise<StorageSnapshot> {
    return invokeCommand<StorageSnapshot>("get_storage_snapshot");
  },
  setCustomPath(path: string, overwrite = false): Promise<StorageStats> {
    return invokeCommand<StorageStats>("set_custom_storage_path", { path, overwrite });
  },
  resetPath(overwrite = false): Promise<StorageStats> {
    return invokeCommand<StorageStats>("reset_storage_path", { overwrite });
  },
  clearTable(table: string): Promise<StorageTableStats> {
    return invokeCommand<StorageTableStats>("clear_table", { table });
  },
};

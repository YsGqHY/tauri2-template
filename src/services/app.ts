import type { AppInfo } from "../contracts/types";
import { invokeCommand } from "../api/tauri";

export const AppService = {
  greet(name: string): Promise<string> {
    return invokeCommand<string>("greet", { name });
  },
  getInfo(): Promise<AppInfo> {
    return invokeCommand<AppInfo>("get_app_info");
  },
};

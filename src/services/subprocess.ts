import type { AvailableCommand, Process, SubprocessRequest, SubprocessSnapshot } from "../contracts/types";
import { invokeCommand } from "../api/tauri";

export const SubprocessService = {
  availableCommands(): Promise<AvailableCommand[]> {
    return invokeCommand<AvailableCommand[]>("get_available_commands");
  },
  run(request: SubprocessRequest): Promise<Process> {
    return invokeCommand<Process>("run_subprocess", { request });
  },
  stop(id: string): Promise<void> {
    return invokeCommand<void>("stop_subprocess", { id });
  },
  list(): Promise<Process[]> {
    return invokeCommand<Process[]>("list_subprocesses");
  },
  output(id: string): Promise<SubprocessSnapshot> {
    return invokeCommand<SubprocessSnapshot>("get_subprocess_output", { id });
  },
};

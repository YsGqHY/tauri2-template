import type { AvailableCommand, Cleanup, Process } from "../../contracts/types";
import type { Translate } from "../../i18n";
import { safeListen, type Unlisten } from "../../api/events";
import { toDisplayError } from "../../api/tauri";
import { SubprocessService } from "../../services";
import { AppEvents } from "../../shared/events";
import { escapeHtml, on, qs } from "../../shared/dom";

export interface SubprocessPageProps {
  t: Translate;
  onToast: (message: string, kind?: "info" | "success" | "error") => void;
}

const MAX_LOG_LINES = 200;

export const mountSubprocessPage = (container: HTMLElement, props: SubprocessPageProps): Cleanup => {
  let commands: AvailableCommand[] = [];
  let processes: Process[] = [];
  let output: string[] = [];
  let disposed = false;
  let refreshBusy = false;
  const processListeners = new Map<string, Unlisten[]>();
  const processLogs = new Map<string, string[]>();

  const errorMessage = (error: unknown): string => {
    const payload = toDisplayError(error, props.t);
    return payload.nextStep ? `${payload.message} ${payload.nextStep}` : payload.message;
  };
  const statusLabel = (process: Process): string => process.running ? props.t("common.processRunning") : props.t("common.processExited");
  const parseArgs = (value: string): string[] => value.trim() ? value.trim().split(/\s+/) : [];

  const render = (): void => {
    if (disposed) {
      return;
    }
    container.innerHTML = `<section class="page page--subprocess"><div class="page-header"><span class="eyebrow">${props.t("subprocess.eyebrow")}</span><h1>${props.t("subprocess.title")}</h1><p>${props.t("subprocess.description")}</p></div>
      <section class="card"><div class="section-heading"><h2>${props.t("subprocess.available")}</h2><button class="button" data-refresh ${refreshBusy ? "disabled" : ""}>${props.t("subprocess.refresh")}</button></div>
        ${commands.length ? `<div class="command-list">${commands.map((command) => `<div class="command-row"><div><strong>${escapeHtml(command.id)}</strong><p>${props.t("subprocess.executable")}: ${escapeHtml(command.executable)} · ${props.t("subprocess.maxArgs")}: ${command.maxArgs}</p><label class="field"><span>${props.t("subprocess.args")}</span><input data-command-args="${escapeHtml(command.id)}" placeholder="${escapeHtml(props.t("subprocess.argsPlaceholder"))}"/><span>${props.t("subprocess.cwd")}</span><input data-command-cwd="${escapeHtml(command.id)}" placeholder="${escapeHtml(command.cwdRoot ?? props.t("subprocess.cwdPlaceholder"))}" /></label></div><button class="button button--primary" data-run-command="${escapeHtml(command.id)}">${props.t("subprocess.run")}</button></div>`).join("")}</div>` : `<div class="empty-state">${props.t("subprocess.empty")}</div>`}
      </section>
      <section class="card"><div class="section-heading"><h2>${props.t("subprocess.processes")}</h2></div>${processes.length ? `<div class="process-list">${processes.map((process) => `<div class="process-row"><div><strong>${escapeHtml(process.command)}</strong><span>${statusLabel(process)}${process.cwd ? ` · ${escapeHtml(process.cwd)}` : ""}</span></div><button class="button button--danger button--small" data-stop="${escapeHtml(process.id)}" ${process.running ? "" : "disabled"}>${props.t("subprocess.stop")}</button></div>`).join("")}</div>` : `<div class="empty-state">${props.t("subprocess.noProcesses")}</div>`}</section>
      <section class="card output-card"><div class="section-heading"><h2>${props.t("subprocess.output")}</h2></div><pre data-output data-scroll-container="subprocess-output">${output.length ? escapeHtml(output.join("\n")) : props.t("common.noData")}</pre></section></section>`;
  };

  const snapshotLoads = new Set<string>();
  const snapshotPending = new Set<string>();

  const restoreOutput = async (id: string): Promise<void> => {
    if (disposed) return;
    if (snapshotLoads.has(id)) {
      snapshotPending.add(id);
      return;
    }
    snapshotLoads.add(id);
    try {
      do {
        snapshotPending.delete(id);
        const snapshot = await SubprocessService.output(id);
        if (disposed) return;
        const lines = [
          ...snapshot.stdout,
          ...snapshot.stderr.map((line) => `${props.t("subprocess.stderr")}: ${line}`),
        ];
        if (snapshot.exit) {
          lines.push(`${props.t("subprocess.exit")}: ${snapshot.exit.success ? props.t("common.success") : props.t("common.failed")} (${snapshot.exit.code ?? props.t("common.noData")})`);
        }
        processLogs.set(id, lines.slice(-MAX_LOG_LINES));
        output = Array.from(processLogs.values()).flat().slice(-MAX_LOG_LINES);
        processes = processes.map((process) => process.id === id ? snapshot.info : process);
        render();
      } while (!disposed && snapshotPending.has(id));
    } catch (error: unknown) {
      if (!disposed) props.onToast(errorMessage(error), "error");
    } finally {
      snapshotLoads.delete(id);
    }
  };

  const attachListeners = async (id: string): Promise<void> => {
    if (disposed || processListeners.has(id)) return;
    const unlisteners: Unlisten[] = [];
    processListeners.set(id, unlisteners);
    await Promise.all([
      AppEvents.subprocessStdout(id),
      AppEvents.subprocessStderr(id),
      AppEvents.subprocessExit(id),
    ].map(async (eventName) => {
      const unlisten = await safeListen<unknown>(eventName, () => { void restoreOutput(id); });
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    }));
    // Subscribe first, then restore the bounded backend snapshot: a process that
    // exited before command resolution still has its output and terminal state.
    await restoreOutput(id);
  };

  const refresh = (): void => {
    if (disposed || refreshBusy) {
      return;
    }
    refreshBusy = true;
    Promise.all([SubprocessService.availableCommands(), SubprocessService.list()])
      .then(([nextCommands, nextProcesses]) => {
        if (disposed) {
          return;
        }
        commands = nextCommands;
        processes = nextProcesses;
        nextProcesses.forEach((process) => attachListeners(process.id));
        render();
      })
      .catch((error: unknown) => {
        if (!disposed) {
          props.onToast(errorMessage(error), "error");
          commands = [];
          processes = [];
          render();
        }
      })
      .finally(() => {
        refreshBusy = false;
        if (!disposed) {
          render();
        }
      });
  };

  render();
  refresh();
  const cleanupClick = on(container, "click", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement) || disposed) {
      return;
    }
    if (target.closest("[data-refresh]")) {
      refresh();
      return;
    }
    const commandId = target.closest<HTMLElement>("[data-run-command]")?.dataset.runCommand;
    if (commandId) {
      const command = commands.find((candidate) => candidate.id === commandId);
      if (!command || target.closest("[data-run-command]")?.classList.contains("is-busy")) {
        return;
      }
      const args = parseArgs(qs<HTMLInputElement>(container, `[data-command-args="${commandId}"]`)?.value ?? "");
      const cwd = qs<HTMLInputElement>(container, `[data-command-cwd="${commandId}"]`)?.value.trim() || undefined;
      const button = target.closest<HTMLElement>("[data-run-command]");
      button?.classList.add("is-busy");
      button?.setAttribute("disabled", "true");
      SubprocessService.run({ command: command.id, args, cwd })
        .then((process) => {
          if (disposed) {
            return;
          }
          processes = [process, ...processes.filter((candidate) => candidate.id !== process.id)];
          attachListeners(process.id);
          render();
          props.onToast(props.t("common.success"), "success");
        })
        .catch((error: unknown) => { if (!disposed) props.onToast(errorMessage(error), "error"); })
        .finally(() => { if (!disposed) render(); });
      return;
    }
    const id = target.closest<HTMLElement>("[data-stop]")?.dataset.stop;
    if (id) {
      SubprocessService.stop(id)
        .then(() => refresh())
        .catch((error: unknown) => { if (!disposed) props.onToast(errorMessage(error), "error"); });
    }
  });

  return () => {
    disposed = true;
    cleanupClick();
    processListeners.forEach((unlisteners) => unlisteners.splice(0).forEach((unlisten) => unlisten()));
    processListeners.clear();
    container.replaceChildren();
  };
};

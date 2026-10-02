export const AppEvents = {
  time: "app:time",
  childResult: (id: string): string => `child:result:${id}`,
  childSend: (id: string): string => `child:send:${id}`,
  childBroadcast: "child:broadcast",
  childClosed: (id: string): string => `child:closed:${id}`,
  subprocessStdout: (id: string): string => `subprocess:stdout:${id}`,
  subprocessStderr: (id: string): string => `subprocess:stderr:${id}`,
  subprocessReady: (id: string): string => `subprocess:ready:${id}`,
  subprocessExit: (id: string): string => `subprocess:exit:${id}`,
  trayAction: "tray:action",
} as const;

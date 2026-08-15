import { invoke } from "@tauri-apps/api/core";

export interface StatusPayload {
  status: string;
  pid: number | null;
  port: number;
  installedVersion: string | null;
  uptimeMs: number | null;
  lastError: string | null;
  controlPort: number | null;
  autostart: boolean;
  nodePresent: boolean;
  nodeVersion: string | null;
  crashRestart: boolean;
}

export interface RuntimeInfo {
  nodePresent: boolean;
  nodeVersion: string | null;
  installedVersion: string | null;
  runtimeDir: string;
}

export interface RuntimeInstallResult {
  version: string;
  changed: boolean;
}

export interface UpdateCheck {
  installed: string | null;
  latest: string;
  updateAvailable: boolean;
}

export interface Settings {
  port: number;
  /** 回调控制端口；null = 自动（DSH 端口 + 1） */
  controlPort: number | null;
  dshVersion: string;
  crashRestart: boolean;
  quitStopsDsh: boolean;
  registerCli: boolean;
  /** 界面语言：zh / en */
  language: string;
}

export interface LogLine {
  ts: string;
  level: string;
  msg: string;
}

export interface CallbackInfo {
  httpUrl: string;
  httpPort: number | null;
  cliCmd: string;
}

export const api = {
  getStatus: () => invoke<StatusPayload>("get_status"),
  startDsh: () => invoke<void>("start_dsh"),
  stopDsh: () => invoke<void>("stop_dsh"),
  restartDsh: (reason?: string) => invoke<void>("restart_dsh", { reason }),
  forceStopExternal: () => invoke<void>("force_stop_external"),
  ensureRuntime: (version?: string) =>
    invoke<RuntimeInstallResult>("ensure_runtime", { version }),
  getRuntimeInfo: () => invoke<RuntimeInfo>("get_runtime_info"),
  installNodeGuided: () => invoke<string>("install_node_guided"),
  updateDsh: () => invoke<string>("update_dsh"),
  checkUpdate: () => invoke<UpdateCheck>("check_update"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) =>
    invoke<void>("save_settings", { settings }),
  setAutostart: (enabled: boolean) =>
    invoke<boolean>("set_autostart", { enabled }),
  getAutostart: () => invoke<boolean>("get_autostart"),
  getLogs: (limit?: number) => invoke<LogLine[]>("get_logs", { limit }),
  getCallbackInfo: () => invoke<CallbackInfo>("get_callback_info"),
  openLogFile: () => invoke<void>("open_log_file"),
};

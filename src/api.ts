import { invoke } from "@tauri-apps/api/core";

export interface StatusPayload {
  status: string;
  pid: number | null;
  port: number;
  installedVersion: string | null;
  /** 系统层面（PATH 全局安装 / npx 缓存）存在的 dsh 版本 */
  systemDshVersion: string | null;
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
  systemDshVersion: string | null;
  systemDshLocation: string | null;
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
  /** 首次向导已跳过/完成（持久化） */
  wizardDismissed: boolean;
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

/** DSH 配置中的一个 API(供应商路由)概要;Key 只含脱敏形式 */
export interface ProviderSummary {
  id: string;
  displayName: string;
  /** 线缆协议:openai-completions / openai-responses / anthropic-messages;无法确定时为 null */
  protocol: string | null;
  baseUrl: string | null;
  /** 凭据环境变量名(settings.yaml 的 apiKeyEnv,目录路由用内置缺省值) */
  apiKeyEnv: string;
  keyConfigured: boolean;
  /** 脱敏 Key(未配置时为 null) */
  maskedKey: string | null;
}

/** 额度页数据源:DSH 配置根目录 + API 列表 */
export interface ProviderList {
  home: string;
  providers: ProviderSummary[];
}

export type ConfigErrorCode = "not_found" | "parse_failed";

export interface ConfigError {
  code: ConfigErrorCode;
  message: string;
}

/** 单个币种的余额信息;字段与 DeepSeek 官方 /user/balance 响应保持一致,金额为字符串精度 */
export interface DeepSeekBalanceInfo {
  currency: string;
  total_balance: string;
  granted_balance: string;
  topped_up_balance: string;
}

/** 查询结果:按供应商类型分别承载不同的信息 */
export type QuotaResult =
  | { kind: "deep_seek"; is_available: boolean; balance_infos: DeepSeekBalanceInfo[] }
  | { kind: "open_router"; total_credits: number; total_usage: number; remaining: number }
  | { kind: "simple"; balance: number; unit: string | null };

export type QuotaErrorCode =
  | "invalid_input"
  | "no_key"
  | "unsupported"
  | "config"
  | "auth_failed"
  | "rate_limited"
  | "network"
  | "server"
  | "bad_response"
  /** 前端兜底:无法识别的拒绝值 */
  | "unknown";

export interface QuotaError {
  code: QuotaErrorCode;
  message: string;
}

const QUOTA_ERROR_CODES: readonly string[] = [
  "invalid_input",
  "no_key",
  "unsupported",
  "config",
  "auth_failed",
  "rate_limited",
  "network",
  "server",
  "bad_response",
];

/** 把 invoke 的拒绝值归一为结构化错误(后端崩溃串、意外对象等兜底为 unknown) */
export function toQuotaError(e: unknown): QuotaError {
  if (e && typeof e === "object" && "code" in e) {
    const code = (e as { code: unknown }).code;
    if (typeof code === "string" && QUOTA_ERROR_CODES.includes(code)) {
      const message = (e as { message?: unknown }).message;
      return {
        code: code as QuotaErrorCode,
        message: typeof message === "string" ? message : "",
      };
    }
  }
  return { code: "unknown", message: String(e) };
}

/** 把 invoke 的拒绝值归一为配置错误(意外对象兜底为 parse_failed) */
export function toConfigError(e: unknown): ConfigError {
  if (e && typeof e === "object" && "code" in e) {
    const code = (e as { code: unknown }).code;
    const message = (e as { message?: unknown }).message;
    if (code === "not_found" || code === "parse_failed") {
      return { code, message: typeof message === "string" ? message : "" };
    }
  }
  return { code: "parse_failed", message: String(e) };
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
  upgradeSystemDsh: (version?: string) =>
    invoke<string>("upgrade_system_dsh", { version }),
  checkUpdate: () => invoke<UpdateCheck>("check_update"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) =>
    invoke<void>("save_settings", { settings }),
  dismissWizard: () => invoke<void>("dismiss_wizard"),
  setAutostart: (enabled: boolean) =>
    invoke<boolean>("set_autostart", { enabled }),
  getAutostart: () => invoke<boolean>("get_autostart"),
  getLogs: (limit?: number) => invoke<LogLine[]>("get_logs", { limit }),
  getCallbackInfo: () => invoke<CallbackInfo>("get_callback_info"),
  openLogFile: () => invoke<void>("open_log_file"),
  openDir: (path: string) => invoke<void>("open_dir", { path }),
  listQuotaProviders: () => invoke<ProviderList>("list_quota_providers"),
  queryProviderQuota: (providerId: string) =>
    invoke<QuotaResult>("query_provider_quota", { providerId }),
};

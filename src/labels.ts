import { t, type MsgKey } from "./i18n";

const STATUS_KEYS = [
  "node-missing",
  "installing",
  "installed-idle",
  "starting",
  "running",
  "external",
  "stopped",
  "crashed",
  "port-in-use",
  "error",
] as const;

export function statusText(s: string): string {
  if ((STATUS_KEYS as readonly string[]).includes(s)) {
    return t(`status.${s}` as MsgKey);
  }
  return s;
}

/** 02:41:07 格式的运行时长（指标卡用） */
export function fmtClock(ms: number | null): string {
  if (ms === null || ms === undefined) return "—";
  const total = Math.floor(ms / 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(Math.floor(total / 3600))}:${p(Math.floor((total % 3600) / 60))}:${p(total % 60)}`;
}

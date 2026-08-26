import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { reactive } from "vue";
import type { LogLine, StatusPayload } from "./api";

export const store = reactive({
  status: null as StatusPayload | null,
  logs: [] as LogLine[],
  installProgress: [] as string[],
  wizardDismissed: false,
  /** 升级任务进行中（全局记忆：切换标签页组件销毁重建后仍保持锁定与进度展示） */
  updateBusy: false,
  /** 本次升级的目标版本号（点击时固定；避免切页回来 latest 尚未刷新导致空版本号文案） */
  updateTarget: null as string | null,
  /** 最后一次状态快照里的 uptimeMs 及其本地接收时刻（用于本地推算） */
  uptimeBase: null as number | null,
  uptimeAt: 0,
});

export function setStatus(p: StatusPayload) {
  store.status = p;
  store.uptimeBase = p.uptimeMs;
  store.uptimeAt = Date.now();
}

/** 实时运行时长：后端只在状态变化时推送快照，这里加上本地经过的时间推算 */
export function liveUptimeMs(): number | null {
  if (store.uptimeBase === null) return null;
  return store.uptimeBase + (Date.now() - store.uptimeAt);
}

export async function bindEvents(): Promise<UnlistenFn> {
  const offs: UnlistenFn[] = [];
  offs.push(
    await listen<StatusPayload>("dsh-status", (e) => {
      setStatus(e.payload);
    }),
  );
  offs.push(
    await listen<LogLine>("dsh-log", (e) => {
      store.logs.push(e.payload);
      if (store.logs.length > 2000) {
        store.logs.splice(0, store.logs.length - 2000);
      }
    }),
  );
  offs.push(
    await listen<string>("install-progress", (e) => {
      store.installProgress.push(e.payload);
      if (store.installProgress.length > 800) {
        store.installProgress.shift();
      }
    }),
  );
  return () => offs.forEach((u) => u());
}

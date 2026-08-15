import { reactive } from "vue";
import { getVersion } from "@tauri-apps/api/app";

/** 应用仓库（DSH-start 自身的 GitHub 仓库） */
export const REPO = "1x1-lab/dsh-start";
export const REPO_URL = `https://github.com/${REPO}`;
export const RELEASES_URL = `${REPO_URL}/releases`;

export const appUpdate = reactive({
  current: "",
  latest: null as string | null,
  checked: false,
  checking: false,
  failed: false,
});

/** 简单版本比较：按 . / - 分段数字比较；a>b 返回 1，相等 0，a<b -1 */
function cmpVersion(a: string, b: string): number {
  const pa = a.split(/[.-]/).map(Number);
  const pb = b.split(/[.-]/).map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const x = pa[i] ?? 0;
    const y = pb[i] ?? 0;
    if (x !== y) return x < y ? -1 : 1;
  }
  return 0;
}

export function updateAvailable(): boolean {
  return (
    appUpdate.checked &&
    appUpdate.latest !== null &&
    appUpdate.current !== "" &&
    cmpVersion(appUpdate.latest, appUpdate.current) > 0
  );
}

/**
 * 查询 GitHub 最新 release（tag_name）并与本应用版本比对。
 * - 仓库尚无 release（404）→ 视为已是最新，checked=true
 * - 网络/限流等其他错误 → failed=true，UI 提示可重试
 */
export async function checkAppUpdate(force = false): Promise<void> {
  if (appUpdate.checking || (appUpdate.checked && !force)) return;
  appUpdate.checking = true;
  appUpdate.failed = false;
  try {
    if (!appUpdate.current) {
      appUpdate.current = await getVersion();
    }
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (res.status === 404) {
      // 仓库还没有发布任何 release → 无从更新
      appUpdate.checked = true;
      appUpdate.latest = null;
      return;
    }
    if (!res.ok) throw new Error(`GitHub API ${res.status}`);
    const data = (await res.json()) as { tag_name?: string };
    appUpdate.latest = (data.tag_name ?? "").replace(/^v/i, "") || null;
    appUpdate.checked = true;
  } catch {
    appUpdate.failed = true;
  } finally {
    appUpdate.checking = false;
  }
}

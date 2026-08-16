import { reactive } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "./i18n";
import { showToast } from "./toast";

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

/** 检查完成后按结果弹居中 toast（notify=true 时）：新版本可点击直达 Release 页 */
function toastResult() {
  if (appUpdate.failed) {
    showToast(t("about.checkFailed"));
  } else if (updateAvailable()) {
    showToast(
      t("about.newVersion", { v: appUpdate.latest ?? "" }),
      6000,
      () => openUrl(RELEASES_URL),
    );
  } else {
    showToast(t("about.upToDate"));
  }
}

/**
 * 查询 GitHub 最新版本并与本应用版本比对。
 * - 优先取最新 release 的 tag_name（草稿 release 对 API 不可见）
 * - 无 release（404）→ 回退到 tags（公开即时，打 tag 即可被检测到）
 * - 网络/限流等其他错误 → failed=true，UI 提示可重试
 * @param force 忽略已检查过的缓存，强制重新查询
 * @param notify 完成后用居中 toast 反馈结果
 */
export async function checkAppUpdate(force = false, notify = false): Promise<void> {
  if (appUpdate.checking || (appUpdate.checked && !force)) return;
  appUpdate.checking = true;
  appUpdate.failed = false;
  try {
    if (!appUpdate.current) {
      appUpdate.current = await getVersion();
    }
    const headers = { Accept: "application/vnd.github+json" };
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers });
    if (res.status === 404) {
      // 无公开 release（可能从未发布或仍是草稿）→ 回退到最新 tag
      const tagRes = await fetch(`https://api.github.com/repos/${REPO}/tags?per_page=1`, {
        headers,
      });
      if (tagRes.ok) {
        const tags = (await tagRes.json()) as { name?: string }[];
        const tag = tags[0]?.name ?? "";
        appUpdate.latest = tag.replace(/^v/i, "") || null;
      } else {
        appUpdate.latest = null;
      }
      appUpdate.checked = true;
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
    if (notify) toastResult();
  }
}

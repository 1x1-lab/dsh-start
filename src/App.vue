<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "./api";
import logoUrl from "./assets/deepseek.svg";
import { bindEvents, setStatus, store } from "./events";
import { setLocale, t } from "./i18n";
import { statusText } from "./labels";
import { toast } from "./toast";
import { RELEASES_URL, appUpdate, checkAppUpdate, updateAvailable } from "./update";
import Dashboard from "./views/Dashboard.vue";
import LogsView from "./views/LogsView.vue";
import SettingsView from "./views/SettingsView.vue";
import SetupWizard from "./views/SetupWizard.vue";

const appWindow = getCurrentWindow();

// 手动实现标题栏拖拽 / 双击最大化（data-tauri-drag-region 在透明窗口上不稳定）
function inWinControls(e: MouseEvent): boolean {
  return (e.target as HTMLElement).closest(".win-controls") !== null;
}
function onTitlebarDown(e: MouseEvent) {
  if (e.button !== 0 || inWinControls(e)) return;
  // 双击的第二次按下 detail=2：直接最大化/还原，不再进入拖拽（否则 dblclick 被拖拽吞掉）
  if (e.detail === 2) {
    void appWindow.toggleMaximize();
    return;
  }
  void appWindow.startDragging();
}

type Tab = "console" | "logs" | "settings";
const tab = ref<Tab>("console");

const showWizard = computed(
  () =>
    !store.wizardDismissed &&
    !!store.status &&
    (!store.status.nodePresent || !store.status.installedVersion),
);

const pill = computed(() => {
  const s = store.status?.status ?? "unknown";
  return { cls: s, text: statusText(s) };
});

const navGroups = computed(
  (): { label: string; items: { id: Tab; label: string; icon: string }[] }[] => [
    {
      label: t("nav.group.hosting"),
      items: [
        { id: "console", label: t("nav.console"), icon: "◧" },
        { id: "logs", label: t("nav.logs"), icon: "≡" },
      ],
    },
    {
      label: t("nav.group.app"),
      items: [{ id: "settings", label: t("nav.settings"), icon: "⚙" }],
    },
  ],
);

const head = computed(
  () =>
    ({
      console: { t: t("head.console.t"), s: t("head.console.s") },
      logs: { t: t("head.logs.t"), s: t("head.logs.s") },
      settings: { t: t("head.settings.t"), s: t("head.settings.s") },
    })[tab.value],
);

const appVersion = ref("");

onMounted(async () => {
  await bindEvents();
  try {
    setLocale((await api.getSettings()).language);
  } catch {
    /* 默认中文 */
  }
  try {
    appVersion.value = await getVersion();
  } catch {
    /* 版本号取不到就留空 */
  }
  void checkAppUpdate(); // 应用自身版本更新检查（GitHub latest release）
  try {
    setStatus(await api.getStatus());
  } catch {
    /* IPC not ready yet; the status event will fill it in */
  }
  try {
    const logs = await api.getLogs(500);
    if (store.logs.length === 0) store.logs = logs;
  } catch {
    /* ignore */
  }
});
</script>

<template>
  <div class="shell">
    <!-- 自定义标题栏：拖拽区 + 窗口控制 -->
    <div class="titlebar" @mousedown="onTitlebarDown">
      <div class="win-controls">
        <button class="wc" title="最小化" @click="appWindow.minimize()">
          <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" stroke-width="1"/></svg>
        </button>
        <button class="wc" title="最大化 / 还原" @click="appWindow.toggleMaximize()">
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor"/></svg>
        </button>
        <button class="wc close" title="关闭（最小化到托盘）" @click="appWindow.close()">
          <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1"/></svg>
        </button>
      </div>
    </div>

    <div class="main-row">
      <!-- 第 1 层：菜单（无背景，直接浮在毛玻璃上） -->
      <aside class="sidebar">
        <div class="ws">
          <img class="avatar" :src="logoUrl" alt="DSH" />
        </div>
        <template v-for="group in navGroups" :key="group.label">
          <div class="nav-label">{{ group.label }}</div>
          <div
            v-for="item in group.items"
            :key="item.id"
            class="nav-item"
            :class="{ active: tab === item.id }"
            @click="tab = item.id"
          >
            <span class="ic">{{ item.icon }}</span>{{ item.label }}
          </div>
        </template>
        <div v-if="appVersion" class="ver">
          <span>v{{ appVersion }}</span>
          <button
            v-if="updateAvailable()"
            class="up-badge"
            :title="`New version v${appUpdate.latest ?? ''}`"
            @click="openUrl(RELEASES_URL)"
          >
            ⬆ v{{ appUpdate.latest }}
          </button>
        </div>
      </aside>

      <!-- 第 2 层：模块层 -->
      <div class="layer2">
        <div class="page-head">
          <div>
            <h1>{{ head.t }}</h1>
            <div class="sub">{{ head.s }}</div>
          </div>
          <div class="right">
            <span class="status-pill" :class="pill.cls">
              <span class="d"></span>{{ pill.text }}
            </span>
          </div>
        </div>

        <div class="canvas">
          <Dashboard v-if="tab === 'console'" />
          <LogsView v-else-if="tab === 'logs'" />
          <SettingsView v-else />
        </div>
      </div>
    </div>

    <SetupWizard v-if="showWizard" />

    <!-- 全局轻提示 -->
    <div v-if="toast.text" class="toast">{{ toast.text }}</div>
  </div>
</template>

<style scoped>
.shell {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
  /* 60% 不透明度：Acrylic 提供模糊质感，实际透度由这里精确控制 */
  background: rgba(237, 239, 244, 0.6);
}

/* ===== 自定义标题栏 ===== */
.titlebar {
  flex: none;
  height: 38px;
  display: flex;
  align-items: center;
  padding: 0 4px 0 16px;
  /* 同上：保证拖拽区可被命中 */
  background: rgba(255, 255, 255, 0.01);
}
.win-controls {
  margin-left: auto;
  display: flex;
  gap: 2px;
}
.wc {
  width: 40px;
  height: 30px;
  display: grid;
  place-items: center;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-dim);
  transition: all 0.12s;
}
.wc:hover {
  background: rgba(0, 0, 0, 0.07);
  color: var(--text);
}
.wc.close:hover {
  background: #e81123;
  color: #fff;
}

.main-row {
  flex: 1;
  min-height: 0;
  display: flex;
  position: relative;
}

/* ===== 第 1 层：菜单 ===== */
.sidebar {
  position: relative;
  z-index: 2;
  width: 120px;
  flex: none;
  margin: 0 0 8px 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 2px 12px 12px;
}
.ws {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 6px 8px 10px;
  border-radius: 8px;
}
.ws .avatar {
  width: 26px;
  height: 26px;
  display: block;
}
.nav-label {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-dim);
  padding: 10px 10px 5px;
}
.nav-item {
  display: flex;
  align-items: center;
  gap: 9px;
  height: 29px;
  padding: 0 10px;
  border-radius: 7px;
  font-size: 13px;
  color: #4b5158;
  cursor: pointer;
  transition: all 0.12s;
}
.nav-item .ic {
  width: 16px;
  text-align: center;
  font-size: 13px;
  opacity: 0.75;
}
.nav-item:hover {
  background: rgba(0, 0, 0, 0.05);
  color: var(--text);
}
.nav-item.active {
  background: rgba(255, 255, 255, 0.75);
  color: var(--text);
  box-shadow: 0 2px 8px rgba(30, 38, 60, 0.08);
}
.nav-item.active .ic {
  color: var(--accent);
  opacity: 1;
}
.ver {
  margin-top: auto;
  padding-top: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-faint);
  font-variant-numeric: tabular-nums;
}
.up-badge {
  background: linear-gradient(135deg, var(--accent), var(--accent-2));
  color: #fff;
  border: none;
  border-radius: 999px;
  padding: 2px 8px;
  font-size: 10.5px;
  cursor: pointer;
  transition: filter 0.12s;
}
.up-badge:hover {
  filter: brightness(1.12);
}

/* ===== 第 2 层：模块层 ===== */
.layer2 {
  position: relative;
  z-index: 1;
  flex: 1;
  margin: 0 8px 8px 8px;
  border-radius: 14px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  background: linear-gradient(
    180deg,
    rgba(255, 255, 255, 0.78),
    rgba(255, 255, 255, 0.62)
  );
  border: 1px solid rgba(255, 255, 255, 0.7);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.6);
}
.page-head {
  flex: none;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px 20px 14px;
  border-bottom: 1px solid var(--border);
}
.page-head h1 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  letter-spacing: -0.01em;
}
.page-head .sub {
  font-size: 12px;
  color: var(--text-dim);
  margin-top: 2px;
}
.page-head .right {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 8px;
}

.status-pill {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11.5px;
  font-weight: 550;
  color: var(--text-dim);
  background: rgba(0, 0, 0, 0.04);
  border: 1px solid rgba(0, 0, 0, 0.08);
  border-radius: 999px;
  padding: 5px 12px;
  font-variant-numeric: tabular-nums;
}
.status-pill .d {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-faint);
}
.status-pill.running,
.status-pill.external {
  color: var(--green);
  background: rgba(47, 163, 107, 0.1);
  border-color: rgba(47, 163, 107, 0.25);
}
.status-pill.running .d,
.status-pill.external .d {
  background: var(--green);
}
.status-pill.starting,
.status-pill.installing {
  color: var(--yellow);
  background: rgba(192, 138, 0, 0.1);
  border-color: rgba(192, 138, 0, 0.25);
}
.status-pill.starting .d,
.status-pill.installing .d {
  background: var(--yellow);
  animation: pulse 1s infinite;
}
.status-pill.crashed,
.status-pill.error,
.status-pill.node-missing {
  color: var(--red);
  background: rgba(214, 69, 69, 0.08);
  border-color: rgba(214, 69, 69, 0.25);
}
.status-pill.crashed .d,
.status-pill.error .d,
.status-pill.node-missing .d {
  background: var(--red);
}
.status-pill.port-in-use {
  color: var(--orange);
  background: rgba(224, 123, 57, 0.1);
  border-color: rgba(224, 123, 57, 0.28);
}
.status-pill.port-in-use .d {
  background: var(--orange);
}
@keyframes pulse {
  50% {
    opacity: 0.35;
  }
}

.canvas {
  flex: 1;
  overflow-y: auto;
  padding: 16px 20px 20px;
}

/* ===== 全局轻提示 ===== */
.toast {
  position: fixed;
  top: 48px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 100;
  background: rgba(28, 30, 36, 0.92);
  color: #fff;
  font-size: 12px;
  font-weight: 550;
  padding: 8px 18px;
  border-radius: 999px;
  box-shadow: 0 8px 24px rgba(20, 24, 40, 0.25);
  animation: toast-in 0.18s ease;
  pointer-events: none;
}
@keyframes toast-in {
  from {
    opacity: 0;
    transform: translateX(-50%) translateY(-6px);
  }
}
</style>


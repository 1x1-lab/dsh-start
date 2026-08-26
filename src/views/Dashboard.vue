<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import { liveUptimeMs, store } from "../events";
import { t } from "../i18n";
import { showToast } from "../toast";
import CallbackCard from "../components/CallbackCard.vue";
import StatusCard from "../components/StatusCard.vue";

const busy = ref<string | null>(null);
const message = ref("");
const latest = ref<string | null>(null);
const armedStop = ref(false);
let armTimer = 0;
let ticker = 0;

const running = computed(
  () => store.status?.status === "running" || store.status?.status === "starting",
);
const external = computed(() => store.status?.status === "external");
/** 是否有托管副本（决定升级方式：托管=更新，非托管=安装托管副本） */
const managed = computed(() => !!store.status?.installedVersion);
/** 当前展示的版本：托管版本优先，否则系统版本 */
const curVersion = computed(
  () => store.status?.installedVersion ?? store.status?.systemDshVersion ?? null,
);
// 仅在 registry 上确有更新版本时才展示升级入口（查询失败则保持隐藏）
const hasUpdate = computed(
  () => latest.value !== null && !!curVersion.value && latest.value !== curVersion.value,
);
/** 升级入口文案：托管 → 更新到 vX；非托管 → 升级系统 DSH 到 vX */
const updateLabel = computed(() =>
  t(managed.value ? "dash.updateTo" : "dash.upgradeSystem", { v: latest.value ?? "" }),
);
const uptime = ref(liveUptimeMs());

/** 升级进行中：本地点击或切页返回后（状态存全局 store，跨标签页记忆） */
const updating = computed(() => busy.value === "update" || store.updateBusy);

function tick() {
  uptime.value = liveUptimeMs();
}
ticker = window.setInterval(tick, 1000);
onUnmounted(() => window.clearInterval(ticker));

async function checkUpdate() {
  try {
    latest.value = (await api.checkUpdate()).latest;
  } catch {
    /* 离线 / npm 不可用 → 保持隐藏 */
  }
}
onMounted(checkUpdate);

// 升级需区分方式：
// 托管副本 → update_dsh（停 → 装 → 恢复运行）；
// 系统安装（非托管）→ 按原始方式原地升级（全局 npm install -g / npx 刷新），不转为托管。
// 进行中状态记入全局 store：切换标签页销毁本组件后仍保持锁定与「正在升级」提示
async function doUpdate() {
  if (busy.value || store.updateBusy) return;
  busy.value = "update";
  message.value = "";
  store.updateBusy = true;
  store.updateTarget = latest.value;
  try {
    if (managed.value) {
      const v = await api.updateDsh();
      latest.value = v; // 已更新到该版本 → 入口隐藏
    } else {
      const v = await api.upgradeSystemDsh(latest.value ?? undefined);
      latest.value = v;
      showToast(t("dash.systemUpgraded", { v }));
    }
  } catch (e) {
    message.value = String(e);
  } finally {
    busy.value = null;
    store.updateBusy = false;
    store.updateTarget = null;
  }
}

async function run(action: () => Promise<unknown>, key: string) {
  if (busy.value) return;
  busy.value = key;
  message.value = "";
  try {
    await action();
  } catch (e) {
    message.value = String(e);
  } finally {
    busy.value = null;
  }
}

async function openConsole() {
  const port = store.status?.port ?? 3080;
  await openUrl(`http://127.0.0.1:${port}`);
}

// 强制停止外部实例：第一次点击进入「确认」状态（3s 内再点才执行）
async function onForceStop() {
  if (!armedStop.value) {
    armedStop.value = true;
    window.clearTimeout(armTimer);
    armTimer = window.setTimeout(() => (armedStop.value = false), 3000);
    return;
  }
  armedStop.value = false;
  window.clearTimeout(armTimer);
  await run(() => api.forceStopExternal(), "stop");
  if (!message.value) showToast(t("dash.forceStopDone"));
}
</script>

<template>
  <div class="dashboard">
    <!-- 操作：独立悬浮按钮，无容器 -->
    <div class="action-row">
      <template v-if="external">
        <button class="btn primary" @click="openConsole">{{ t("dash.openConsole") }}</button>
        <button
          class="btn danger"
          :class="{ armed: armedStop }"
          :disabled="busy !== null || store.updateBusy"
          @click="onForceStop"
        >
          {{ armedStop ? t("dash.confirmStop") : t("dash.forceStop") }}
        </button>
      </template>
      <template v-else-if="running">
        <button class="btn primary" @click="openConsole">{{ t("dash.openConsole") }}</button>
        <button
          class="btn"
          :disabled="busy !== null || store.updateBusy"
          @click="run(() => api.restartDsh('ui'), 'restart')"
        >
          {{ busy === "restart" ? t("dash.restarting") : t("dash.restart") }}
        </button>
        <button
          class="btn danger"
          :disabled="busy !== null || store.updateBusy"
          @click="run(() => api.stopDsh(), 'stop')"
        >
          {{ busy === "stop" ? t("dash.stopping") : t("dash.stop") }}
        </button>
      </template>
      <button
        v-else
        class="btn primary"
        :disabled="busy !== null || store.updateBusy"
        @click="run(() => api.startDsh(), 'start')"
      >
        {{ busy === "start" ? t("dash.starting") : t("dash.start") }}
      </button>
      <button
        v-if="hasUpdate"
        class="btn"
        :disabled="busy !== null || store.updateBusy"
        @click="doUpdate"
      >
        {{ updating ? t("dash.updating") : updateLabel }}
      </button>
    </div>
    <!-- 升级进行中（跨标签页记忆；npm 输出不在此展示，完整日志见「日志」页） -->
    <div v-if="store.updateBusy" class="upgrade-hint">
      <span class="spin" aria-hidden="true"></span>
      <span>{{ t("dash.upgradeInProgress", { v: store.updateTarget ?? latest ?? "" }) }}</span>
    </div>
    <p v-if="message" class="msg">{{ message }}</p>

    <div class="grid-12">
      <StatusCard
        :status="store.status"
        :uptime="uptime"
        :update-available="hasUpdate"
        :update-to="latest"
        :update-label="updateLabel"
        :update-busy="store.updateBusy"
        @update-click="doUpdate"
      />
      <CallbackCard class="s12" />
    </div>
  </div>
</template>

<style scoped>
.dashboard {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.action-row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.msg {
  margin: 0;
  color: var(--red);
  font-size: 12px;
}
/* 升级进行中的轻提示（无输出面板，日志见「日志」页） */
.upgrade-hint {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-dim);
  margin: 0;
}
.spin {
  width: 13px;
  height: 13px;
  flex: none;
  border-radius: 50%;
  border: 2px solid var(--border);
  border-top-color: var(--accent);
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.btn.danger.armed {
  background: var(--red);
  border-color: transparent;
  color: #fff;
}
</style>


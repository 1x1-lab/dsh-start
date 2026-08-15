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
// 仅在 registry 上确有更新版本时才展示更新按钮（查询失败则保持隐藏）
const hasUpdate = computed(() => {
  const inst = store.status?.installedVersion;
  return latest.value !== null && !!inst && latest.value !== inst;
});
const uptime = ref(liveUptimeMs());

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

async function doUpdate() {
  await run(async () => {
    const v = await api.updateDsh();
    latest.value = v; // 已更新到该版本 → 按钮隐藏
  }, "update");
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
          :disabled="busy !== null"
          @click="onForceStop"
        >
          {{ armedStop ? t("dash.confirmStop") : t("dash.forceStop") }}
        </button>
      </template>
      <template v-else-if="running">
        <button class="btn primary" @click="openConsole">{{ t("dash.openConsole") }}</button>
        <button
          class="btn"
          :disabled="busy !== null"
          @click="run(() => api.restartDsh('ui'), 'restart')"
        >
          {{ busy === "restart" ? t("dash.restarting") : t("dash.restart") }}
        </button>
        <button
          class="btn danger"
          :disabled="busy !== null"
          @click="run(() => api.stopDsh(), 'stop')"
        >
          {{ busy === "stop" ? t("dash.stopping") : t("dash.stop") }}
        </button>
      </template>
      <button
        v-else
        class="btn primary"
        :disabled="busy !== null"
        @click="run(() => api.startDsh(), 'start')"
      >
        {{ busy === "start" ? t("dash.starting") : t("dash.start") }}
      </button>
      <button
        v-if="hasUpdate"
        class="btn"
        :disabled="busy !== null"
        @click="doUpdate"
      >
        {{ busy === "update" ? t("dash.updating") : t("dash.updateTo", { v: latest ?? "" }) }}
      </button>
    </div>
    <p v-if="external" class="hint">
      {{ t("dash.externalHint", { port: store.status?.port ?? "" }) }}
    </p>
    <p v-if="message" class="msg">{{ message }}</p>

    <div class="grid-12">
      <StatusCard
        :status="store.status"
        :uptime="uptime"
        :update-available="hasUpdate"
        :update-to="latest"
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
.btn.danger.armed {
  background: var(--red);
  border-color: transparent;
  color: #fff;
}
.hint {
  margin: 0;
  color: var(--text-dim);
  font-size: 12px;
  line-height: 1.6;
}
</style>


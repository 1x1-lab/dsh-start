<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api, type CallbackInfo } from "../api";
import { store } from "../events";
import { t } from "../i18n";

const info = ref<CallbackInfo | null>(null);
const copied = ref("");

onMounted(async () => {
  try {
    info.value = await api.getCallbackInfo();
  } catch {
    /* ignore */
  }
});

// 实际绑定端口可能因设置修改 / 冲突回退而变化，跟随状态事件实时更新
const httpUrl = computed(() => {
  const p = store.status?.controlPort ?? info.value?.httpPort ?? null;
  return p ? `http://127.0.0.1:${p}/api/restart` : null;
});

async function copy(text: string, key: string) {
  try {
    await navigator.clipboard.writeText(text);
    copied.value = key;
    setTimeout(() => (copied.value = ""), 1500);
  } catch {
    /* clipboard unavailable */
  }
}
</script>

<template>
  <div class="card callback">
    <h3 class="card-title">{{ t("cb.title") }}<span class="r">{{ t("cb.throttle") }}</span></h3>
    <template v-if="info">
      <div v-if="httpUrl" class="cmd">
        <span class="k">HTTP</span>
        <code>POST {{ httpUrl }}</code>
        <span class="cp" @click="copy(`POST ${httpUrl}`, 'http')">
          {{ copied === "http" ? t("cb.copied") : t("cb.copy") }}
        </span>
      </div>
      <div v-else class="cmd">
        <span class="k">HTTP</span>
        <code>{{ t("cb.unavailable") }}</code>
      </div>
      <div class="cmd">
        <span class="k">CLI</span>
        <code>{{ info.cliCmd }}</code>
        <span class="cp" @click="copy(info.cliCmd, 'cli')">
          {{ copied === "cli" ? t("cb.copied") : t("cb.copy") }}
        </span>
      </div>
      <div class="line">
        <span class="note">{{ t("cb.note") }}</span>
      </div>
    </template>
    <div v-else class="muted">{{ t("cb.noInfo") }}</div>
  </div>
</template>

<style scoped>
.cmd {
  display: flex;
  align-items: center;
  gap: 10px;
  background: var(--bg-soft);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px 12px;
  font-size: 12px;
}
.cmd + .cmd {
  margin-top: 8px;
}
.cmd .k {
  font-size: 10.5px;
  font-weight: 600;
  color: var(--accent);
  width: 38px;
  flex: none;
}
.cmd code {
  font-family: ui-monospace, "SF Mono", Consolas, monospace;
  font-size: 11px;
  color: #3a3f46;
  word-break: break-all;
  user-select: text;
}
.cmd .cp {
  margin-left: auto;
  font-size: 11px;
  color: var(--text-faint);
  cursor: pointer;
  flex: none;
}
.cmd .cp:hover {
  color: var(--accent);
}
.line {
  margin-top: 10px;
}
</style>

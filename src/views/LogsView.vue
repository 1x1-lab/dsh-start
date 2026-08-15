<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from "vue";
import { api } from "../api";
import { store } from "../events";
import { t } from "../i18n";

const box = ref<HTMLElement | null>(null);

async function scrollToBottom() {
  await nextTick();
  if (box.value) box.value.scrollTop = box.value.scrollHeight;
}

watch(
  () => store.logs.length,
  () => scrollToBottom(),
);

onMounted(async () => {
  try {
    const logs = await api.getLogs(500);
    if (store.logs.length === 0) store.logs = logs;
  } catch {
    /* ignore */
  }
  scrollToBottom();
});

function levelClass(level: string): string {
  const l = level.toLowerCase();
  if (l === "error" || l === "warn") return l;
  return "info";
}
</script>

<template>
  <div class="logs">
    <div class="card log-card">
      <div class="log-head">
        {{ t("logs.live", { n: store.logs.length }) }}
        <div class="tools">
          <button class="fpill" @click="api.openLogFile()">{{ t("logs.open") }}</button>
          <button class="fpill" @click="store.logs = []">{{ t("logs.clear") }}</button>
        </div>
      </div>
      <div ref="box" class="log-body">
        <div
          v-for="(l, i) in store.logs"
          :key="i"
          class="log-line"
          :class="levelClass(l.level)"
        >
          <span class="ts">{{ l.ts }}</span>
          <span class="lv">{{ l.level.toUpperCase() }}</span>
          <span class="msg">{{ l.msg }}</span>
        </div>
        <div v-if="store.logs.length === 0" class="muted empty">{{ t("logs.empty") }}</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.logs {
  height: 100%;
  display: flex;
  flex-direction: column;
}
.log-card {
  flex: 1;
  display: flex;
  flex-direction: column;
  padding: 0;
  overflow: hidden;
}
.log-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 11px 16px;
  border-bottom: 1px solid var(--border);
  font-size: 11.5px;
  font-weight: 550;
  color: var(--text-dim);
  flex: none;
}
.tools {
  margin-left: auto;
  display: flex;
  gap: 6px;
}
.fpill {
  font-size: 10.5px;
  font-family: inherit;
  padding: 3px 10px;
  border-radius: 999px;
  color: var(--text-dim);
  background: var(--bg-soft);
  border: 1px solid rgba(0, 0, 0, 0.05);
  cursor: pointer;
  transition: all 0.12s;
}
.fpill:hover {
  color: var(--accent);
  border-color: rgba(94, 106, 210, 0.35);
}
.log-body {
  flex: 1;
  overflow-y: auto;
  padding: 10px 16px 14px;
  font-family: ui-monospace, "SF Mono", Consolas, monospace;
  font-size: 11.5px;
  line-height: 1.9;
  color: #4b5158;
  user-select: text;
}
.log-line {
  display: flex;
  gap: 10px;
  white-space: pre-wrap;
  word-break: break-all;
}
.ts {
  color: var(--text-faint);
  flex: none;
}
.lv {
  flex: none;
  width: 46px;
  font-weight: 600;
}
.log-line.info .lv {
  color: var(--green);
}
.log-line.warn .lv {
  color: var(--yellow);
}
.log-line.error .lv {
  color: var(--red);
}
.log-line.error .msg {
  color: var(--red);
}
.empty {
  padding: 20px;
  text-align: center;
}
</style>

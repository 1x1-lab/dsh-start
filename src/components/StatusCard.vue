<script setup lang="ts">
import { computed } from "vue";
import type { StatusPayload } from "../api";
import { t } from "../i18n";
import { fmtClock } from "../labels";

const props = defineProps<{
  status: StatusPayload | null;
  uptime: number | null;
  /** 有可用新版本（registry 版本 > 已装/系统版本）时显示升级箭头 */
  updateAvailable?: boolean;
  updateTo?: string | null;
  /** 升级入口文案（托管=更新到 vX；非托管=安装托管副本 vX） */
  updateLabel?: string;
}>();
const emit = defineEmits<{ (e: "update-click"): void }>();

const d = computed(() => {
  const s = props.status;
  const external = s?.status === "external";
  const running = s?.status === "running" || external;
  return {
    running,
    external,
    port: s?.port ?? 3080,
    controlPort: s?.controlPort ?? null,
    version: s?.installedVersion
      ? `v${s.installedVersion}`
      : s?.systemDshVersion
        ? `v${s.systemDshVersion}`
        : t("card.notInstalled"),
    node: s?.nodeVersion ?? null,
    pid: s?.pid ?? null,
    lastError: s?.lastError ?? null,
  };
});
</script>

<template>
  <div class="card stat s3">
    <label>{{ t("card.port") }}</label>
    <b>{{ d.port }}</b>
    <em :class="{ off: !d.running }">
      {{
        d.external
          ? t("card.port.external")
          : d.running
            ? t("card.port.listening")
            : t("card.port.off")
      }}
    </em>
  </div>
  <div class="card stat s3">
    <label>{{ t("card.controlPort") }}</label>
    <b>{{ d.controlPort ?? "—" }}</b>
    <em :class="d.controlPort ? 'ind' : 'off'">
      {{ d.controlPort ? t("card.cp.localhost") : t("card.cp.unbound") }}
    </em>
  </div>
  <div class="card stat s3">
    <label>{{ t("card.version") }}</label>
    <div class="ver-row">
      <b class="ver">{{ d.version }}</b>
      <button
        v-if="updateAvailable"
        class="up-arrow"
        :title="updateLabel ?? t('dash.updateTo', { v: updateTo ?? '' })"
        @click="emit('update-click')"
      >
        <!-- 字符「⬆」字形基线不稳，用 SVG 箭头保证在外圆中精确居中 -->
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path
            d="M5 8.5v-7M1.5 5L5 1.5 8.5 5"
            fill="none"
            stroke="currentColor"
            stroke-width="1.4"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
    </div>
    <em class="ind">{{ d.node ? `Node ${d.node}` : t("card.noNode") }}</em>
  </div>
  <div class="card stat s3">
    <label>{{ t("card.uptime") }}</label>
    <b>{{ d.running ? fmtClock(uptime) : "—" }}</b>
    <em class="ind">{{ d.external ? t("card.externalProcess") : `PID ${d.pid ?? "—"}` }}</em>
  </div>
  <div v-if="d.lastError" class="card err s12">{{ d.lastError }}</div>
</template>

<style scoped>
.stat label {
  font-size: 11px;
  color: var(--text-dim);
  font-weight: 550;
}
.stat b {
  display: block;
  margin-top: 7px;
  font-size: 17px;
  font-weight: 600;
  letter-spacing: -0.01em;
  font-variant-numeric: tabular-nums;
}
/* 版本值（如「系统 v0.1.0-rc.6」）较长，用小号字与其他信息协调 */
.stat b.ver {
  font-size: 13px;
  font-weight: 600;
  margin-top: 0;
}
.ver-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 7px;
  min-height: 20px;
}
.up-arrow {
  flex: none;
  width: 20px;
  height: 20px;
  padding: 0; /* 清除浏览器默认按钮内边距，避免内容区不对称导致偏移 */
  display: grid;
  place-items: center;
  border: none;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--accent), var(--accent-2));
  color: #fff;
  cursor: pointer;
  transition: filter 0.12s;
}
.up-arrow:hover {
  filter: brightness(1.15);
}
.stat em {
  display: block;
  font-style: normal;
  font-size: 11px;
  margin-top: 4px;
  color: var(--green);
}
.stat em.ind {
  color: var(--accent);
}
.stat em.off {
  color: var(--text-faint);
}
.err {
  color: var(--red);
  font-size: 12.5px;
  background: rgba(214, 69, 69, 0.05);
  border-color: rgba(214, 69, 69, 0.2);
}
</style>

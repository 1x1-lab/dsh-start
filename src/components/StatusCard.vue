<script setup lang="ts">
import { computed } from "vue";
import type { StatusPayload } from "../api";
import { t } from "../i18n";
import { fmtClock } from "../labels";

const props = defineProps<{
  status: StatusPayload | null;
  uptime: number | null;
}>();

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
        ? t("card.systemVersion", { v: s.systemDshVersion })
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
    <b class="ver">{{ d.version }}</b>
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
/* 版本值（如「系统 v0.1.0-rc.6（未托管）」）较长，用小号字与其他信息协调 */
.stat b.ver {
  font-size: 13px;
  font-weight: 600;
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

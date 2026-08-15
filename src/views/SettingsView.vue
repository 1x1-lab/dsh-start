<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type Settings } from "../api";
import { store } from "../events";
import { LOCALES, setLocale, t } from "../i18n";
import { showToast } from "../toast";
import {
  REPO_URL,
  RELEASES_URL,
  appUpdate,
  checkAppUpdate,
  updateAvailable,
} from "../update";
import ToggleRow from "../components/ToggleRow.vue";

const form = reactive<Settings>({
  port: 3080,
  controlPort: null,
  dshVersion: "latest",
  crashRestart: true,
  quitStopsDsh: true,
  registerCli: true,
  language: "zh",
});
const message = ref("");
const appVersion = ref("");

onMounted(async () => {
  try {
    Object.assign(form, await api.getSettings());
  } catch {
    /* ignore */
  }
  try {
    appVersion.value = await getVersion();
  } catch {
    /* ignore */
  }
  void checkAppUpdate(); // 已检查过则静默跳过
});

async function save() {
  message.value = "";
  // v-model.number 在清空时给出 ""，统一归一为 null（= 自动）
  const raw = form.controlPort as number | "" | null;
  const controlPort = raw === null || raw === "" ? null : Number(raw);
  if (controlPort !== null) {
    if (!Number.isInteger(controlPort) || controlPort < 1 || controlPort > 65535) {
      message.value = t("set.err.portInvalid");
      return;
    }
    if (controlPort === form.port) {
      message.value = t("set.err.portSame");
      return;
    }
  }
  try {
    await api.saveSettings({ ...form, controlPort });
    form.controlPort = controlPort;
    setLocale(form.language); // 语言随保存生效
    showToast(t("set.saved"));
  } catch (e) {
    message.value = String(e);
  }
}

// 开机启动是系统状态（非 settings 字段），开关即时生效
async function toggleAutostart(v: boolean) {
  message.value = "";
  try {
    await api.setAutostart(v);
  } catch (e) {
    message.value = String(e);
  }
}
</script>

<template>
  <div class="grid-12">
    <div class="card s12">
      <h3 class="card-title">{{ t("set.runtime") }}</h3>
      <div class="line">
        <div>
          {{ t("set.language") }}
          <span class="sub">{{ t("set.language.sub") }}</span>
        </div>
        <span class="field">
          <select v-model="form.language">
            <option v-for="l in LOCALES" :key="l.id" :value="l.id">{{ l.name }}</option>
          </select>
        </span>
      </div>
      <div class="line">
        <div>
          {{ t("set.port") }}
          <span class="sub">{{ t("set.port.sub") }}</span>
        </div>
        <span class="field">
          <input v-model.number="form.port" type="number" min="1" max="65535" />
        </span>
      </div>
      <div class="line">
        <div>
          {{ t("set.controlPort") }}
          <span class="sub">{{ t("set.controlPort.sub") }}</span>
        </div>
        <span class="field">
          <input
            v-model.number="form.controlPort"
            type="number"
            min="1"
            max="65535"
            :placeholder="String(form.port + 1)"
          />
        </span>
      </div>
      <div class="line">
        <div>
          {{ t("set.dshVersion") }}
          <span class="sub">{{ t("set.dshVersion.sub") }}</span>
        </div>
        <span class="field">
          <input
            v-model="form.dshVersion"
            type="text"
            placeholder="latest"
            style="width: 130px"
          />
        </span>
      </div>
      <div class="btn-row">
        <button class="btn primary" @click="save">{{ t("set.save") }}</button>
      </div>
      <p v-if="message" class="msg">{{ message }}</p>
    </div>

    <div class="card s12">
      <h3 class="card-title">{{ t("set.behavior") }}</h3>
      <ToggleRow
        v-model="form.quitStopsDsh"
        :label="t('set.quitStops')"
        :desc="t('set.quitStops.desc')"
      />
      <ToggleRow
        v-model="form.registerCli"
        :label="t('set.registerCli')"
        :desc="t('set.registerCli.desc')"
      />
      <p class="note" style="margin: 10px 0 0">{{ t("set.saveNote") }}</p>
    </div>

    <div class="card s12">
      <h3 class="card-title">{{ t("set.automation") }}</h3>
      <ToggleRow
        :label="t('set.autostart')"
        :desc="t('set.autostart.desc')"
        :model-value="store.status?.autostart ?? false"
        @update:model-value="toggleAutostart"
      />
      <ToggleRow
        v-model="form.crashRestart"
        :label="t('set.crashRestart')"
        :desc="t('set.crashRestart.desc')"
      />
    </div>

    <div class="card s12">
      <h3 class="card-title">{{ t("about.title") }}</h3>
      <div class="about-grid">
        <div class="aline">
          <span>{{ t("about.version") }}</span>
          <b>v{{ appVersion }}</b>
        </div>
        <div class="aline">
          <span>{{ t("about.dshVersion") }}</span>
          <b>{{ store.status?.installedVersion ? `v${store.status.installedVersion}` : "—" }}</b>
        </div>
        <div class="aline">
          <span>{{ t("about.repo") }}</span>
          <button class="link" @click="openUrl(REPO_URL)">{{ REPO_URL.replace("https://", "") }}</button>
        </div>
        <div class="aline">
          <span>{{ t("about.license") }}</span>
          <b>Apache-2.0</b>
        </div>
        <div class="aline">
          <span>{{ t("about.tech") }}</span>
          <b>Tauri v2 · Vue 3 · Vite</b>
        </div>
      </div>
      <div class="btn-row">
        <button class="btn" :disabled="appUpdate.checking" @click="checkAppUpdate(true)">
          {{ appUpdate.checking ? t("about.checking") : t("about.check") }}
        </button>
        <button v-if="updateAvailable()" class="btn primary" @click="openUrl(RELEASES_URL)">
          {{ t("about.openRelease") }}
        </button>
      </div>
      <p class="note" style="margin: 10px 0 0">
        <span v-if="appUpdate.failed" class="err">{{ t("about.checkFailed") }}</span>
        <span v-else-if="updateAvailable()" class="ok">
          {{ t("about.newVersion", { v: appUpdate.latest ?? "" }) }}
        </span>
        <span v-else-if="appUpdate.checked" class="ok">{{ t("about.upToDate") }}</span>
      </p>
      <p class="note" style="margin: 10px 0 0">{{ t("set.about.text") }}</p>
    </div>
  </div>
</template>

<style scoped>
.field {
  margin-left: auto;
}
.field input {
  width: 90px;
  text-align: right;
}
.field select {
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.12);
  color: var(--text);
  border-radius: 7px;
  padding: 5px 10px;
  font-size: 12.5px;
  font-family: inherit;
  outline: none;
}
.btn-row {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 12px;
  margin-top: 14px;
}
.msg {
  color: var(--red);
  font-size: 12px;
  margin: 8px 0 0;
}
.about-grid {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.aline {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  font-size: 13px;
}
.aline span {
  color: var(--text-dim);
}
.aline b {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
.aline .link {
  background: none;
  border: none;
  padding: 0;
  color: var(--accent);
  font-size: 13px;
  cursor: pointer;
}
.aline .link:hover {
  text-decoration: underline;
}
.ok {
  color: var(--green);
}
.err {
  color: var(--red);
}
</style>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, type RuntimeInfo } from "../api";
import { store } from "../events";
import { t } from "../i18n";

const step = ref<"env" | "installing" | "done" | "error">("env");
const message = ref("");
const result = ref("");
const autostartOnDone = ref(false);
const busy = ref(false);
const checking = ref(false);
const info = ref<RuntimeInfo | null>(null);
const nodeMissing = ref(false);

const progressTail = () => store.installProgress.slice(-200);

onMounted(async () => {
  await check();
});

async function check() {
  checking.value = true;
  message.value = "";
  try {
    info.value = await api.getRuntimeInfo();
    nodeMissing.value = !info.value.nodePresent;
  } catch (e) {
    message.value = String(e);
  } finally {
    checking.value = false;
  }
}

async function installNode() {
  busy.value = true;
  message.value = "";
  try {
    message.value = await api.installNodeGuided();
  } catch (e) {
    message.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function beginInstall() {
  if (nodeMissing.value) return;
  step.value = "installing";
  store.installProgress = [];
  try {
    const res = await api.ensureRuntime();
    result.value = t("wiz.installedOk", { v: res.version });
    step.value = "done";
  } catch (e) {
    message.value = String(e);
    step.value = "error";
  }
}

async function finish() {
  if (autostartOnDone.value) {
    try {
      await api.setAutostart(true);
    } catch (e) {
      message.value = String(e);
    }
  }
  store.wizardDismissed = true;
  try {
    await api.dismissWizard(); // 持久化：下次启动不再弹
  } catch {
    /* 忽略 */
  }
}

function skip() {
  store.wizardDismissed = true;
  void api.dismissWizard().catch(() => {}); // 持久化：下次启动不再弹
}
</script>

<template>
  <div class="mask">
    <div class="wizard card">
      <div class="head">
        <span class="logo">DSH</span>
        <span class="title">{{ t("wiz.title") }}</span>
        <button class="close" :title="t('wiz.skip')" @click="skip">✕</button>
      </div>

      <!-- 环境检测 -->
      <div v-if="step === 'env'" class="body">
        <h3>{{ t("wiz.env") }}</h3>
        <p class="muted">{{ t("wiz.env.desc") }}</p>
        <div class="detect">
          <div class="drow">
            <span class="dk">Node.js</span>
            <span v-if="checking" class="dv muted">{{ t("wiz.checking") }}</span>
            <template v-else-if="nodeMissing">
              <span class="dv bad">{{ t("wiz.nodeMissing") }}</span>
              <button class="btn mini" :disabled="busy" @click="installNode">
                {{ busy ? t("wiz.installingNode") : t("wiz.installNode") }}
              </button>
            </template>
            <template v-else>
              <span class="dv ok">✓ {{ info?.nodeVersion }}</span>
            </template>
          </div>
          <div class="drow">
            <span class="dk">DSH</span>
            <span v-if="checking" class="dv muted">{{ t("wiz.checking") }}</span>
            <span v-else-if="info?.installedVersion" class="dv ok">
              {{ t("wiz.dshInstalled", { v: info.installedVersion }) }}
            </span>
            <span v-else-if="info?.systemDshVersion" class="dv ok">
              {{ t("wiz.dshSystem", { v: info.systemDshVersion }) }}
            </span>
            <span v-else class="dv">{{ t("wiz.dshMissing") }}</span>
          </div>
        </div>
        <p v-if="message" class="msg">{{ message }}</p>
        <p class="muted small">{{ t("wiz.env.note") }}</p>
        <div class="btn-row">
          <button
            class="btn primary"
            :disabled="nodeMissing || busy || checking"
            @click="beginInstall"
          >
            {{ t("wiz.begin") }}
          </button>
          <button class="btn" :disabled="checking" @click="check">{{ t("wiz.recheck") }}</button>
        </div>
      </div>

      <!-- 安装中 -->
      <div v-else-if="step === 'installing'" class="body">
        <h3>{{ t("wiz.installing") }}</h3>
        <p class="muted">{{ t("wiz.installing.desc") }}</p>
        <div class="progress">
          <div v-for="(l, i) in progressTail()" :key="i" class="pl">{{ l }}</div>
        </div>
      </div>

      <!-- 安装完成 -->
      <div v-else-if="step === 'done'" class="body">
        <h3 class="ok">✅ {{ result }}</h3>
        <p class="muted">{{ t("wiz.done.desc") }}</p>
        <label class="opt">
          <input v-model="autostartOnDone" type="checkbox" />
          {{ t("wiz.autostart") }}
        </label>
        <div class="btn-row">
          <button class="btn primary" @click="finish">{{ t("wiz.finish") }}</button>
        </div>
        <p v-if="message" class="msg">{{ message }}</p>
      </div>

      <!-- 安装失败 -->
      <div v-else class="body">
        <h3 class="err">{{ t("wiz.failed") }}</h3>
        <p class="msg">{{ message }}</p>
        <div class="btn-row">
          <button class="btn primary" @click="beginInstall">{{ t("wiz.retry") }}</button>
          <button class="btn" @click="step = 'env'">{{ t("wiz.back") }}</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.mask {
  position: fixed;
  inset: 0;
  background: rgba(237, 239, 244, 0.6);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
}
.wizard {
  width: min(620px, 92vw);
  max-height: 84vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border);
}
.close {
  margin-left: auto;
  background: transparent;
  border: none;
  color: var(--text-dim);
  font-size: 14px;
  padding: 4px 8px;
  border-radius: 6px;
}
.close:hover {
  color: var(--text);
  background: var(--bg-soft);
}
.logo {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: 8px;
  background: linear-gradient(135deg, var(--accent), var(--accent-2));
  color: #fff;
  font-weight: 800;
  font-size: 12px;
}
.title {
  font-weight: 700;
  font-size: 15px;
}
.body {
  padding: 14px 4px;
  overflow-y: auto;
}
h3 {
  margin: 0 0 8px;
  font-size: 16px;
}
h3.ok {
  color: var(--green);
}
h3.err {
  color: var(--red);
}
.detect {
  display: flex;
  flex-direction: column;
  gap: 8px;
  background: var(--bg-soft);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 12px 14px;
  margin: 12px 0;
}
.drow {
  display: flex;
  align-items: center;
  gap: 10px;
}
.dk {
  width: 84px;
  flex: none;
  color: var(--text-dim);
  font-size: 13px;
}
.dv {
  font-weight: 600;
  font-size: 13px;
}
.dv.ok {
  color: var(--green);
}
.dv.bad {
  color: var(--red);
}
.btn.mini {
  padding: 4px 10px;
  font-size: 12px;
}
.progress {
  background: var(--bg-soft);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 10px;
  margin-top: 10px;
  max-height: 300px;
  overflow-y: auto;
  font-family: ui-monospace, Consolas, monospace;
  font-size: 11.5px;
  line-height: 1.55;
  user-select: text;
}
.pl {
  white-space: pre-wrap;
  word-break: break-all;
  color: var(--text-dim);
}
.btn-row {
  display: flex;
  gap: 10px;
  margin-top: 14px;
}
.opt {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
  font-size: 13px;
}
.msg {
  color: var(--red);
  font-size: 12.5px;
  margin: 8px 0 0;
}
.small {
  font-size: 12px;
  margin-top: 10px;
}
</style>

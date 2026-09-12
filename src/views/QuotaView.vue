<script setup lang="ts">
import { computed, ref } from "vue";
import {
  api,
  toBalanceError,
  type DeepSeekBalance,
  type DeepSeekBalanceError,
} from "../api";
import { t } from "../i18n";

// ===== 状态（全部为页面内存：离开页面即随组件卸载清空） =====
const input = ref("");
const showKey = ref(false);
const loading = ref(false);
const data = ref<DeepSeekBalance | null>(null);
/** 产生 data 的 Key（已 trim），用于识别「同一 Key 刷新」 */
const dataKey = ref("");
const lastSuccessAt = ref<Date | null>(null);
/** 无数据时的失败（首查失败 / 换 Key 后失败） */
const error = ref<DeepSeekBalanceError | null>(null);
/** 同一 Key 刷新失败：保留旧数据，仅标注失败 */
const refreshErr = ref<DeepSeekBalanceError | null>(null);

/** 请求代数：换 Key / 清除后，在途旧响应一律作废 */
let gen = 0;
let inFlightKey = "";

const trimmed = computed(() => input.value.trim());
const maskedDataKey = computed(() => maskKey(dataKey.value));
const canClear = computed(
  () => !!input.value || !!dataKey.value || !!error.value || !!refreshErr.value,
);

function maskKey(k: string): string {
  return k.length > 12 ? `${k.slice(0, 6)}…${k.slice(-4)}` : k;
}

function errText(code: DeepSeekBalanceError["code"]): string {
  switch (code) {
    case "empty":
      return t("quota.err.empty");
    case "invalid_input":
      return t("quota.err.invalid_input");
    case "auth_failed":
      return t("quota.err.auth_failed");
    case "rate_limited":
      return t("quota.err.rate_limited");
    case "network":
      return t("quota.err.network");
    case "server":
      return t("quota.err.server");
    case "bad_response":
      return t("quota.err.bad_response");
    default:
      return t("quota.err.unknown");
  }
}

const errorMsg = computed(() => (error.value ? errText(error.value.code) : ""));
/** 后端附带的技术性详情（不含 Key），与映射文案不同时才展示 */
const errorDetail = computed(() => {
  const m = error.value?.message.trim() ?? "";
  return m && m !== errorMsg.value ? m : "";
});
const refreshBanner = computed(() =>
  refreshErr.value
    ? t("quota.refreshFailed", { reason: errText(refreshErr.value.code) })
    : "",
);
const lastSuccessText = computed(() =>
  lastSuccessAt.value
    ? t("quota.lastSuccess", { time: lastSuccessAt.value.toLocaleString() })
    : "",
);

// 同一 Key 的在途请求直接忽略（防重复提交）；换 Key 后允许立即重新查询
const queryDisabled = computed(
  () => !trimmed.value || (loading.value && trimmed.value === inFlightKey),
);
const queryLabel = computed(() => {
  if (loading.value && trimmed.value === inFlightKey) return t("quota.refreshing");
  // 已展示同一 Key 的结果 → 按钮语义为刷新
  if (data.value && dataKey.value === trimmed.value) return t("quota.refresh");
  return t("quota.query");
});

async function query() {
  const key = trimmed.value;
  if (!key) {
    error.value = { code: "empty", message: t("quota.err.empty") };
    return;
  }
  if (queryDisabled.value) return;

  const seq = ++gen;
  inFlightKey = key;
  loading.value = true;
  error.value = null;
  refreshErr.value = null;
  try {
    const res = await api.getDeepSeekBalance(key);
    // 期间换过 Key / 清除 / 发起了更新的查询 → 旧响应作废
    if (seq !== gen || trimmed.value !== key) return;
    data.value = res;
    dataKey.value = key;
    lastSuccessAt.value = new Date();
  } catch (e) {
    if (seq !== gen || trimmed.value !== key) return;
    const err = toBalanceError(e);
    if (data.value && dataKey.value === key) {
      // 同一 Key 刷新失败：保留旧余额，标注失败原因与上次成功时间
      refreshErr.value = err;
    } else {
      data.value = null;
      dataKey.value = "";
      lastSuccessAt.value = null;
      error.value = err;
    }
  } finally {
    if (seq === gen) loading.value = false;
  }
}

function clearAll() {
  gen++; // 在途请求全部作废
  input.value = "";
  showKey.value = false;
  loading.value = false;
  data.value = null;
  dataKey.value = "";
  lastSuccessAt.value = null;
  error.value = null;
  refreshErr.value = null;
}
</script>

<template>
  <div class="grid-12">
    <div class="card s12">
      <h3 class="card-title">{{ t("quota.key") }}</h3>
      <div class="key-row">
        <input
          v-model="input"
          class="key-input"
          :type="showKey ? 'text' : 'password'"
          :placeholder="t('quota.keyPlaceholder')"
          spellcheck="false"
          autocomplete="off"
          @keyup.enter="query"
        />
        <button class="btn mini" @click="showKey = !showKey">
          {{ showKey ? t("quota.hide") : t("quota.show") }}
        </button>
        <button class="btn primary" :disabled="queryDisabled" @click="query">
          {{ queryLabel }}
        </button>
        <button class="btn danger" :disabled="!canClear" @click="clearAll">
          {{ t("quota.clear") }}
        </button>
      </div>
      <p class="note" style="margin: 10px 0 0">{{ t("quota.memoryNote") }}</p>
      <p v-if="error" class="err">
        {{ errorMsg }}
        <span v-if="errorDetail" class="err-detail">{{ errorDetail }}</span>
      </p>
    </div>

    <div class="card s12">
      <h3 class="card-title">
        {{ t("quota.result.title") }}
        <span v-if="dataKey" class="r">{{ t("quota.dataFor", { k: maskedDataKey }) }}</span>
      </h3>

      <!-- 同 Key 刷新失败：保留旧数据并标注 -->
      <div v-if="refreshBanner" class="banner">{{ refreshBanner }}</div>

      <template v-if="data">
        <template v-if="data.balance_infos.length > 0">
          <!-- 按币种逐项展示，不做跨币种合计 -->
          <div
            v-for="(b, i) in data.balance_infos"
            :key="b.currency + i"
            class="bal"
            :class="{ split: i > 0 }"
          >
            <div class="bal-head">
              <span class="mono-chip">{{ b.currency }}</span>
              <span class="avail" :class="data.is_available ? 'ok' : 'bad'">
                {{ data.is_available ? t("quota.available.ok") : t("quota.available.bad") }}
              </span>
            </div>
            <div class="line">
              <div>{{ t("quota.total") }}</div>
              <b class="amount">{{ b.total_balance }} {{ b.currency }}</b>
            </div>
            <div class="line">
              <div>{{ t("quota.granted") }}</div>
              <b class="amount dim">{{ b.granted_balance }} {{ b.currency }}</b>
            </div>
            <div class="line">
              <div>{{ t("quota.toppedUp") }}</div>
              <b class="amount dim">{{ b.topped_up_balance }} {{ b.currency }}</b>
            </div>
          </div>
        </template>
        <p v-else class="note">{{ t("quota.emptyResult") }}</p>
        <p v-if="lastSuccessText" class="note last">{{ lastSuccessText }}</p>
      </template>

      <p v-else-if="loading" class="note">{{ t("quota.loading") }}</p>
      <p v-else-if="!error" class="note">{{ t("quota.idle") }}</p>
    </div>
  </div>
</template>

<style scoped>
.key-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.key-input {
  flex: 1;
  min-width: 0;
  background: #fff;
  border: 1px solid rgba(0, 0, 0, 0.12);
  color: var(--text);
  border-radius: 7px;
  padding: 6px 10px;
  font-size: 12.5px;
  font-family: ui-monospace, "SF Mono", Consolas, monospace;
  outline: none;
  user-select: text;
}
.key-input:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px rgba(94, 106, 210, 0.15);
}

.err {
  color: var(--red);
  font-size: 12px;
  margin: 10px 0 0;
}
.err-detail {
  color: var(--text-faint);
  font-weight: 400;
  margin-left: 6px;
}

.banner {
  font-size: 12px;
  font-weight: 550;
  color: var(--yellow);
  background: rgba(192, 138, 0, 0.08);
  border: 1px solid rgba(192, 138, 0, 0.25);
  border-radius: 8px;
  padding: 8px 12px;
  margin-bottom: 4px;
}

.bal.split {
  border-top: 1px dashed var(--border);
  margin-top: 10px;
  padding-top: 10px;
}
.bal-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 2px;
}
.avail {
  font-size: 11.5px;
  font-weight: 550;
}
.avail.ok {
  color: var(--green);
}
.avail.bad {
  color: var(--red);
}
.amount {
  margin-left: auto;
  font-variant-numeric: tabular-nums;
  font-weight: 600;
}
.amount.dim {
  font-weight: 550;
  color: var(--text-dim);
}
.last {
  margin-top: 10px;
}
</style>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import {
  api,
  toConfigError,
  toQuotaError,
  type ConfigError,
  type DeepSeekBalanceInfo,
  type ProviderSummary,
  type QuotaErrorCode,
  type QuotaResult,
} from "../api";
import { t } from "../i18n";

// ===== 配置列表（读取 ~/.dsh 下的 settings.yaml / .credentials.yaml） =====
const loadingList = ref(true);
const configErr = ref<ConfigError | null>(null);
const home = ref("");
const providers = ref<ProviderSummary[]>([]);

/** 归一化的卡片数据视图：模板里按 kind 窄化渲染 */
type DataVM =
  | { kind: "deep_seek"; isAvailable: boolean; items: DeepSeekBalanceInfo[] }
  | { kind: "open_router"; total: number; used: number; remaining: number }
  | { kind: "simple"; balance: number; unit: string | null };

interface CardState {
  loading: boolean;
  data: DataVM | null;
  lastSuccessAt: Date | null;
  /** 无数据时的失败（首查失败 / 无 Key / 不支持） */
  error: QuotaErrorCode | null;
  errorMessage: string;
  /** 刷新失败：保留旧数据，仅标注 */
  refreshErr: QuotaErrorCode | null;
}

const cards = reactive<Record<string, CardState>>({});
/** 请求代数：重新发起查询后，在途旧响应一律作废 */
const gens = new Map<string, number>();

function cardOf(id: string): CardState {
  return (cards[id] ??= {
    loading: false,
    data: null,
    lastSuccessAt: null,
    error: null,
    errorMessage: "",
    refreshErr: null,
  });
}

function toVM(r: QuotaResult): DataVM {
  switch (r.kind) {
    case "deep_seek":
      return { kind: "deep_seek", isAvailable: r.is_available, items: r.balance_infos };
    case "open_router":
      return {
        kind: "open_router",
        total: r.total_credits,
        used: r.total_usage,
        remaining: r.remaining,
      };
    case "simple":
      return { kind: "simple", balance: r.balance, unit: r.unit };
  }
}

async function loadProviders() {
  loadingList.value = true;
  configErr.value = null;
  try {
    const list = await api.listQuotaProviders();
    home.value = list.home;
    providers.value = list.providers;
    if (list.providers.length > 0) refreshAll();
  } catch (e) {
    configErr.value = toConfigError(e);
  } finally {
    loadingList.value = false;
  }
}

async function queryOne(p: ProviderSummary) {
  const card = cardOf(p.id);
  const seq = (gens.get(p.id) ?? 0) + 1;
  gens.set(p.id, seq);
  card.loading = true;
  card.error = null;
  card.errorMessage = "";
  card.refreshErr = null;
  try {
    const res = await api.queryProviderQuota(p.id);
    if (gens.get(p.id) !== seq) return;
    card.data = toVM(res);
    card.lastSuccessAt = new Date();
  } catch (e) {
    if (gens.get(p.id) !== seq) return;
    const err = toQuotaError(e);
    if (card.data) {
      // 刷新失败：保留上次成功结果
      card.refreshErr = err.code;
    } else {
      card.error = err.code;
      card.errorMessage = err.message;
    }
  } finally {
    if (gens.get(p.id) === seq) card.loading = false;
  }
}

function refreshAll() {
  for (const p of providers.value) void queryOne(p);
}

onMounted(loadProviders);

// ===== 展示辅助 =====
const view = computed(() => providers.value.map((p) => ({ p, card: cardOf(p.id) })));
const sourceText = computed(() => t("quota.source", { home: home.value || "~/.dsh" }));
const configErrText = computed(() =>
  configErr.value
    ? configErr.value.code === "not_found"
      ? t("quota.configNotFound", { path: home.value || "~/.dsh" })
      : t("quota.configFailed")
    : "",
);

function hostOf(url: string | null): string {
  if (!url) return "";
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

function fmtAmount(n: number): string {
  return n.toLocaleString(undefined, { maximumFractionDigits: 4 });
}

function errText(code: QuotaErrorCode | null): string {
  switch (code) {
    case "no_key":
      return t("quota.err.no_key");
    case "unsupported":
      return t("quota.err.unsupported");
    case "config":
      return t("quota.err.config");
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
</script>

<template>
  <div class="grid-12">
    <!-- 顶部:数据来源 + 全部刷新 -->
    <div class="card s12">
      <h3 class="card-title">
        {{ t("quota.source.title") }}
        <button
          class="btn primary r"
          :disabled="loadingList || providers.length === 0"
          @click="refreshAll"
        >
          {{ t("quota.refreshAll") }}
        </button>
      </h3>
      <p class="note" style="margin: 8px 0 0">{{ sourceText }}</p>
      <p class="note">{{ t("quota.privacyNote") }}</p>
      <p v-if="configErr" class="err">
        {{ configErrText }}
        <span v-if="configErr?.message" class="err-detail">{{ configErr.message }}</span>
      </p>
    </div>

    <div v-if="loadingList" class="card s12">
      <p class="note">{{ t("quota.loadingList") }}</p>
    </div>

    <div v-else-if="!configErr && providers.length === 0" class="card s12">
      <p class="note">{{ t("quota.emptyConfig") }}</p>
    </div>

    <!-- 每个 API 一张卡片 -->
    <div v-for="{ p, card } in view" :key="p.id" class="card s12">
      <h3 class="card-title">
        {{ p.displayName }}
        <button class="btn mini r" :disabled="card.loading" @click="queryOne(p)">
          {{ card.loading ? t("quota.refreshing") : t("quota.refresh") }}
        </button>
      </h3>

      <div class="meta">
        <span class="mono-chip">{{ p.protocol ?? t("quota.protocolUnknown") }}</span>
        <span v-if="p.baseUrl" class="host">{{ hostOf(p.baseUrl) }}</span>
        <span v-if="p.maskedKey" class="key-mask">{{ t("quota.keyMasked", { k: p.maskedKey }) }}</span>
        <span v-else class="key-missing">
          {{ p.apiKeyEnv ? t("quota.keyMissing", { env: p.apiKeyEnv }) : t("quota.keyMissingNoEnv") }}
        </span>
      </div>

      <!-- 刷新失败:保留旧数据并标注 -->
      <div v-if="card.refreshErr" class="banner">
        {{ t("quota.refreshFailed", { reason: errText(card.refreshErr) }) }}
      </div>

      <template v-if="card.data">
        <!-- DeepSeek 官方:按币种逐项 -->
        <template v-if="card.data.kind === 'deep_seek'">
          <template v-if="card.data.items.length > 0">
            <div
              v-for="(b, i) in card.data.items"
              :key="b.currency + i"
              class="bal"
              :class="{ split: i > 0 }"
            >
              <div class="bal-head">
                <span class="mono-chip">{{ b.currency }}</span>
                <span class="avail" :class="card.data.isAvailable ? 'ok' : 'bad'">
                  {{ card.data.isAvailable ? t("quota.available.ok") : t("quota.available.bad") }}
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
        </template>

        <!-- OpenRouter:总额度 / 已用 / 剩余 -->
        <template v-else-if="card.data.kind === 'open_router'">
          <div class="line">
            <div>{{ t("quota.orTotal") }}</div>
            <b class="amount dim">{{ fmtAmount(card.data.total) }} USD</b>
          </div>
          <div class="line">
            <div>{{ t("quota.orUsed") }}</div>
            <b class="amount dim">{{ fmtAmount(card.data.used) }} USD</b>
          </div>
          <div class="line">
            <div>{{ t("quota.orRemaining") }}</div>
            <b class="amount" :class="{ neg: card.data.remaining <= 0 }">
              {{ fmtAmount(card.data.remaining) }} USD
            </b>
          </div>
        </template>

        <!-- 其他单一余额端点 -->
        <template v-else>
          <div class="line">
            <div>{{ t("quota.balance") }}</div>
            <b class="amount" :class="{ neg: card.data.balance <= 0 }">
              {{ fmtAmount(card.data.balance) }}
              <template v-if="card.data.unit">{{ card.data.unit }}</template>
            </b>
          </div>
        </template>

        <p v-if="card.lastSuccessAt" class="note last">
          {{ t("quota.lastSuccess", { time: card.lastSuccessAt.toLocaleString() }) }}
        </p>
      </template>

      <p v-else-if="card.loading" class="note">{{ t("quota.loading") }}</p>
      <p v-else-if="card.error" class="err">
        {{ errText(card.error) }}
        <span v-if="card.errorMessage" class="err-detail">{{ card.errorMessage }}</span>
      </p>
    </div>
  </div>
</template>

<style scoped>
.meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 8px;
}
.host {
  font-size: 11.5px;
  color: var(--text-dim);
  font-family: ui-monospace, "SF Mono", Consolas, monospace;
}
.key-mask {
  font-size: 11.5px;
  color: var(--text-faint);
  font-family: ui-monospace, "SF Mono", Consolas, monospace;
  user-select: text;
}
.key-missing {
  font-size: 11.5px;
  color: var(--yellow);
  font-weight: 550;
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
.amount.neg {
  color: var(--red);
}
.last {
  margin-top: 10px;
}
</style>

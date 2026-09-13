<script setup lang="ts">
import { computed, defineComponent, h, onMounted, reactive, ref } from "vue";
import {
  api,
  toConfigError,
  toQuotaError,
  type ConfigError,
  type DeepSeekBalanceInfo,
  type ProviderSummary,
  type QuotaErrorCode,
  type QuotaResult,
  type QuotaTier,
  type Settings,
} from "../api";
import { t } from "../i18n";
import { showToast } from "../toast";

// ===== 配置列表（读取 ~/.dsh 下的 settings.yaml / .credentials.yaml） =====
const loadingList = ref(true);
const configErr = ref<ConfigError | null>(null);
const home = ref("");
const providers = ref<ProviderSummary[]>([]);

// ===== 每卡查询开关（持久化到本应用设置 quotaDisabled） =====
const settings = ref<Settings | null>(null);
const disabledIds = computed(() => new Set(settings.value?.quotaDisabled ?? []));
function isEnabled(p: ProviderSummary): boolean {
  return !disabledIds.value.has(p.id);
}
async function setEnabled(p: ProviderSummary, on: boolean) {
  const s = settings.value;
  if (!s) return;
  const next = new Set(s.quotaDisabled ?? []);
  if (on) next.delete(p.id);
  else next.add(p.id);
  const updated: Settings = { ...s, quotaDisabled: [...next] };
  settings.value = updated; // 乐观更新
  try {
    await api.saveSettings(updated);
  } catch (e) {
    settings.value = s; // 失败回滚
    showToast(String(e));
    return;
  }
  if (on) {
    void queryOne(p);
  } else {
    // 作废在途请求并停止加载态
    gens.set(p.id, (gens.get(p.id) ?? 0) + 1);
    cardOf(p.id).loading = false;
  }
}

/** 归一化的卡片数据视图：模板里按 kind 窄化渲染 */
type DataVM =
  | { kind: "deep_seek"; items: DeepSeekBalanceInfo[] }
  | { kind: "open_router"; total: number; used: number; remaining: number }
  | { kind: "plan"; name: string | null; tiers: QuotaTier[] }
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

// 注意：必须返回 cards[id]（响应式代理）。写成 `cards[id] ??= {...}` 会在首次创建时
// 返回原始对象，后续状态变更不触发渲染（表现为卡片一直停在「查询中」）。
function cardOf(id: string): CardState {
  if (!cards[id]) {
    cards[id] = {
      loading: false,
      data: null,
      lastSuccessAt: null,
      error: null,
      errorMessage: "",
      refreshErr: null,
    };
  }
  return cards[id];
}

function toVM(r: QuotaResult): DataVM {
  switch (r.kind) {
    case "deep_seek":
      return { kind: "deep_seek", items: r.balance_infos };
    case "open_router":
      return {
        kind: "open_router",
        total: r.total_credits,
        used: r.total_usage,
        remaining: r.remaining,
      };
    case "plan":
      return { kind: "plan", name: r.name, tiers: r.tiers };
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
    // 先为所有 provider 预建卡片状态，避免在 computed 中创建
    for (const p of list.providers) cardOf(p.id);
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
  for (const p of providers.value) {
    if (isEnabled(p)) void queryOne(p);
  }
}

onMounted(async () => {
  void loadProviders();
  try {
    settings.value = await api.getSettings();
  } catch {
    /* 开关降级为页面内存态 */
  }
});

// ===== 展示辅助 =====
const view = computed(() => providers.value.map((p) => ({ p, card: cardOf(p.id) })));
const anyLoading = computed(() => view.value.some(({ card }) => card.loading));
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

function fmtPct(n: number): string {
  return `${n.toLocaleString(undefined, { maximumFractionDigits: 1 })}%`;
}

function tierText(name: string): string {
  switch (name) {
    case "five_hour":
      return t("quota.tier.five_hour");
    case "weekly":
      return t("quota.tier.weekly");
    case "monthly":
      return t("quota.tier.monthly");
    default:
      return name;
  }
}

function tierLevel(u: number): string {
  return u < 75 ? "ok" : u < 95 ? "warn" : "bad";
}

function resetText(iso: string): string {
  const d = new Date(iso);
  const shown = Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
  return t("quota.plan.reset", { time: shown });
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

/** 环形箭头图标;spin=true 时为旋转 loading */
const RefreshIcon = defineComponent({
  props: { spin: { type: Boolean, default: false } },
  setup(props) {
    return () =>
      h(
        "svg",
        {
          viewBox: "0 0 24 24",
          fill: "none",
          stroke: "currentColor",
          "stroke-width": 2,
          "stroke-linecap": "round",
          "stroke-linejoin": "round",
          class: props.spin ? "spin" : "",
        },
        props.spin
          ? [
              h("line", { x1: 12, y1: 2, x2: 12, y2: 6 }),
              h("line", { x1: 12, y1: 18, x2: 12, y2: 22 }),
              h("line", { x1: 4.93, y1: 4.93, x2: 7.76, y2: 7.76 }),
              h("line", { x1: 16.24, y1: 16.24, x2: 19.07, y2: 19.07 }),
              h("line", { x1: 2, y1: 12, x2: 6, y2: 12 }),
              h("line", { x1: 18, y1: 12, x2: 22, y2: 12 }),
              h("line", { x1: 4.93, y1: 19.07, x2: 7.76, y2: 16.24 }),
              h("line", { x1: 16.24, y1: 7.76, x2: 19.07, y2: 4.93 }),
            ]
          : [
              h("polyline", { points: "23 4 23 10 17 10" }),
              h("polyline", { points: "1 20 1 14 7 14" }),
              h("path", {
                d: "M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15",
              }),
            ],
      );
  },
});
</script>

<template>
  <div class="grid-12">
    <!-- 顶部:数据来源 + 全部刷新 -->
    <div class="card s12">
      <h3 class="card-title">
        {{ t("quota.source.title") }}
        <button
          class="icon-btn r"
          :title="anyLoading ? t('quota.refreshing') : t('quota.refreshAll')"
          :aria-label="anyLoading ? t('quota.refreshing') : t('quota.refreshAll')"
          :disabled="loadingList || providers.length === 0 || anyLoading"
          @click="refreshAll"
        >
          <RefreshIcon :spin="anyLoading" />
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
    <div v-for="{ p, card } in view" :key="p.id" class="card s12" :class="{ off: !isEnabled(p) }">
      <h3 class="card-title">
        {{ p.displayName }}
        <span class="r tools">
          <span class="switch-label">{{ t("quota.autoQuery") }}</span>
          <button
            class="switch"
            :class="{ on: isEnabled(p) }"
            role="switch"
            :aria-checked="isEnabled(p)"
            @click="setEnabled(p, !isEnabled(p))"
          >
            <span class="knob"></span>
          </button>
          <button
            class="icon-btn"
            :title="!isEnabled(p) ? t('quota.queryOff') : card.loading ? t('quota.refreshing') : t('quota.refresh')"
            :aria-label="!isEnabled(p) ? t('quota.queryOff') : card.loading ? t('quota.refreshing') : t('quota.refresh')"
            :disabled="card.loading || !isEnabled(p)"
            @click="queryOne(p)"
          >
            <RefreshIcon :spin="card.loading" />
          </button>
        </span>
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

        <!-- 订阅套餐:按窗口展示用量 -->
        <template v-else-if="card.data.kind === 'plan'">
          <div v-for="tier in card.data.tiers" :key="tier.name" class="tier">
            <div class="tier-head">
              <span>{{ tierText(tier.name) }}</span>
              <b class="pct" :class="tierLevel(tier.utilization)">{{ fmtPct(tier.utilization) }}</b>
            </div>
            <div class="bar">
              <div
                class="bar-fill"
                :class="tierLevel(tier.utilization)"
                :style="{ width: Math.min(100, Math.max(0, tier.utilization)) + '%' }"
              ></div>
            </div>
            <div class="tier-sub">
              <span v-if="tier.used !== null && tier.total !== null">
                {{ t("quota.plan.usedOf", { used: fmtAmount(tier.used), total: fmtAmount(tier.total) }) }}
                <template v-if="tier.unit"> {{ tier.unit }}</template>
              </span>
              <span v-if="tier.resets_at" class="reset">{{ resetText(tier.resets_at) }}</span>
            </div>
          </div>
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
.tools {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}
.switch-label {
  font-size: 11.5px;
  color: var(--text-faint);
  font-weight: 500;
}
/* 与 ToggleRow 同款开关（缩小版） */
.switch {
  flex: none;
  width: 30px;
  height: 18px;
  border-radius: 999px;
  border: none;
  background: #d8dbe1;
  position: relative;
  transition: background 0.18s ease;
  padding: 0;
  cursor: pointer;
}
.switch.on {
  background: var(--accent);
}
.knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  transition: transform 0.18s ease;
}
.switch.on .knob {
  transform: translateX(12px);
}

/* 图标刷新按钮:常规为环形箭头,加载中变旋转 loader */
.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border-radius: 7px;
  border: 1px solid var(--border);
  background: #fff;
  color: var(--text-dim);
  cursor: pointer;
  transition: color 0.15s ease, border-color 0.15s ease;
}
.icon-btn:hover:not(:disabled) {
  color: var(--accent);
  border-color: var(--accent);
}
.icon-btn:disabled {
  opacity: 0.6;
  cursor: default;
}
.icon-btn svg {
  width: 13px;
  height: 13px;
}
.spin {
  animation: qspin 0.8s linear infinite;
}
@keyframes qspin {
  to {
    transform: rotate(360deg);
  }
}

.card.off {
  opacity: 0.62;
}

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

.tier + .tier {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px dashed var(--border);
}
.tier-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
  font-size: 12.5px;
  font-weight: 550;
}
.pct {
  font-variant-numeric: tabular-nums;
  font-weight: 600;
}
.pct.ok {
  color: var(--green);
}
.pct.warn {
  color: var(--yellow);
}
.pct.bad {
  color: var(--red);
}
.bar {
  height: 6px;
  border-radius: 999px;
  background: rgba(0, 0, 0, 0.08);
  overflow: hidden;
  margin: 6px 0 4px;
}
.bar-fill {
  height: 100%;
  border-radius: 999px;
  background: var(--green);
  transition: width 0.25s ease;
}
.bar-fill.warn {
  background: var(--yellow);
}
.bar-fill.bad {
  background: var(--red);
}
.tier-sub {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  font-size: 11.5px;
  color: var(--text-dim);
  font-variant-numeric: tabular-nums;
}
</style>

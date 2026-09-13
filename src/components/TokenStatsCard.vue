<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import * as echarts from "echarts/core";
import { LineChart } from "echarts/charts";
import {
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { api, type UsagePoint } from "../api";
import { i18n, t } from "../i18n";
import HintTip from "./HintTip.vue";
import RefreshIcon from "./RefreshIcon.vue";

echarts.use([LineChart, GridComponent, TooltipComponent, LegendComponent, CanvasRenderer]);

type RangeKey = "12h" | "24h" | "7d" | "custom";
type BucketUnit = "hour" | "day";
const HOUR = 3_600_000;
const DAY = 24 * HOUR;

const range = ref<RangeKey>("12h");
const customFrom = ref("");
const customTo = ref("");
const snapshot = ref<{
  start: number;
  end: number;
  points: UsagePoint[];
  refreshedAt: Date;
} | null>(null);
const loading = ref(false);
const rangeError = ref<"incomplete" | "invalid" | "order" | "fetch" | null>(null);
let timer = 0;
let requestSeq = 0;

type RangeError = "incomplete" | "invalid" | "order" | "fetch";
type QueryRange = { start: number; end: number } | { error: Exclude<RangeError, "fetch"> };

function localHourStart(ts: number): number {
  const date = new Date(ts);
  return ts - date.getMinutes() * 60_000 - date.getSeconds() * 1000 - date.getMilliseconds();
}

function bucketUnitForRange(start: number, end: number): BucketUnit {
  return end - start > DAY ? "day" : "hour";
}

function localDayStart(ts: number): number {
  const date = new Date(ts);
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

function localBucketStart(ts: number, unit: BucketUnit): number {
  return unit === "day" ? localDayStart(ts) : localHourStart(ts);
}

function localBucketEnd(bucketStart: number, unit: BucketUnit): number {
  if (unit === "hour") return bucketStart + HOUR;
  const date = new Date(bucketStart);
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + 1).getTime();
}

function aggregateUsagePoints(points: UsagePoint[], start: number, end: number): UsagePoint[] {
  const unit = bucketUnitForRange(start, end);
  const buckets = new Map<number, UsagePoint>();
  for (const point of points) {
    if (point.ts < start || point.ts > end) continue;
    const ts = localBucketStart(point.ts, unit);
    const bucket = buckets.get(ts);
    if (bucket) {
      bucket.inputTokens += point.inputTokens;
      bucket.outputTokens += point.outputTokens;
      bucket.cacheReadTokens += point.cacheReadTokens;
      bucket.cacheWriteTokens += point.cacheWriteTokens;
    } else {
      buckets.set(ts, {
        ts,
        inputTokens: point.inputTokens,
        outputTokens: point.outputTokens,
        cacheReadTokens: point.cacheReadTokens,
        cacheWriteTokens: point.cacheWriteTokens,
      });
    }
  }
  return [...buckets.values()].sort((a, b) => a.ts - b.ts);
}

function parseDateInput(value: string, endOfDay: boolean): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const leapYear = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const daysInMonth = [31, leapYear ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  if (month < 1 || month > 12 || day < 1 || day > daysInMonth[month - 1]) return null;

  const time = endOfDay ? "T23:59:59.999" : "T00:00:00";
  const timestamp = Date.parse(`${value}${time}`);
  return Number.isFinite(timestamp) ? timestamp : null;
}

function resolveRange(now: number): QueryRange {
  if (range.value === "custom") {
    if (!customFrom.value || !customTo.value) return { error: "incomplete" };
    const start = parseDateInput(customFrom.value, false);
    const end = parseDateInput(customTo.value, true);
    if (start === null || end === null) return { error: "invalid" };
    if (end < start) return { error: "order" };
    return { start, end };
  }

  const span = range.value === "12h" ? 12 * HOUR : range.value === "24h" ? DAY : 7 * DAY;
  return { start: now - span, end: now };
}

async function refresh() {
  // Capture the moving preset boundary for every query. The displayed chart
  // range is committed together with its points only after this request wins.
  const query = resolveRange(Date.now());
  const seq = ++requestSeq;
  if ("error" in query) {
    rangeError.value = query.error;
    loading.value = false;
    return;
  }

  rangeError.value = null;
  loading.value = true;
  try {
    const res = await api.getTokenStats(query.start, query.end);
    if (seq !== requestSeq) return;
    snapshot.value = {
      start: query.start,
      end: query.end,
      points: aggregateUsagePoints(res, query.start, query.end),
      refreshedAt: new Date(),
    };
  } catch {
    if (seq === requestSeq) rangeError.value = "fetch";
  } finally {
    if (seq === requestSeq) loading.value = false;
  }
}

function setRange(r: RangeKey) {
  if (range.value === r) return;
  requestSeq += 1;
  loading.value = false;
  rangeError.value = null;
  range.value = r;
  void refresh();
}

function applyCustom() {
  void refresh();
}

const fmtCompact = computed(
  () =>
    new Intl.NumberFormat(i18n.locale === "en" ? "en-US" : "zh-CN", {
      notation: "compact",
      maximumFractionDigits: 1,
    }),
);
function fmtN(n: number): string {
  return fmtCompact.value.format(n);
}

function fmtExact(n: number): string {
  return new Intl.NumberFormat(i18n.locale === "en" ? "en-US" : "zh-CN").format(n);
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (char) => {
    const entities: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;",
    };
    return entities[char];
  });
}

type TooltipParam = { axisValue?: unknown; value?: unknown; dataIndex?: unknown };

function isTooltipParam(value: unknown): value is TooltipParam {
  return typeof value === "object" && value !== null;
}

function formatLocalDateTime(ts: number): string {
  return new Date(ts).toLocaleString(i18n.locale === "en" ? "en-US" : "zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
    timeZoneName: "short",
  });
}

function formatBucketCaption(bucketStart: number, start: number, end: number): string {
  const unit = bucketUnitForRange(start, end);
  const bucketEnd = localBucketEnd(bucketStart, unit);
  const heading = `${formatLocalDateTime(bucketStart)} – ${formatLocalDateTime(bucketEnd)}`;
  const boundaries: string[] = [];
  if (start > bucketStart) {
    boundaries.push(i18n.locale === "en"
      ? `Selected range starts at ${formatLocalDateTime(start)}`
      : `所选范围从 ${formatLocalDateTime(start)} 开始`);
  }
  if (end < bucketEnd - 1) {
    boundaries.push(i18n.locale === "en"
      ? `Selected range ends at ${formatLocalDateTime(end)}`
      : `所选范围截至 ${formatLocalDateTime(end)}`);
  }
  return `<div>${escapeHtml(heading)}</div>${boundaries.map((text) => `<div>${escapeHtml(text)}</div>`).join("")}`;
}

function tooltipFor(params: unknown): string {
  const values = Array.isArray(params) ? params : [params];
  const first = values.find(isTooltipParam);
  const dataIndex = first?.dataIndex;
  const indexedPoint = typeof dataIndex === "number" && Number.isInteger(dataIndex)
    ? snapshot.value?.points[dataIndex]
    : undefined;
  const axisValue = first?.axisValue ?? (Array.isArray(first?.value) ? first.value[0] : null);
  const ts = typeof axisValue === "number"
    ? axisValue
    : typeof axisValue === "string" && /^\d+$/.test(axisValue)
      ? Number(axisValue)
      : typeof axisValue === "string"
        ? Date.parse(axisValue)
        : Number.NaN;
  const point = indexedPoint ?? snapshot.value?.points.find((candidate) => candidate.ts === ts);
  const current = snapshot.value;
  if (!point || !current) return "";

  const cacheWriteName = i18n.locale === "en" ? "Cache write" : "缓存写";
  const rows: Array<[string, string, number]> = [
    [t("dash.token.input"), COLORS.input, point.inputTokens],
    [t("dash.token.output"), COLORS.output, point.outputTokens],
    [t("dash.token.cacheRead"), COLORS.cache, point.cacheReadTokens],
    [cacheWriteName, COLORS.cacheWrite, point.cacheWriteTokens],
  ];
  return `<div>${formatBucketCaption(point.ts, current.start, current.end)}${rows.map(([name, color, value]) =>
    `<div><span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:${color};margin-right:6px"></span>${escapeHtml(name)}: ${fmtExact(value)}</div>`,
  ).join("")}</div>`;
}

// ===== ECharts(log 纵轴自动生成 1/10/100… 刻度,空数据也有默认轴) =====
const chartEl = ref<HTMLElement | null>(null);
let chart: echarts.EChartsType | null = null;
let ro: ResizeObserver | null = null;

const COLORS = { input: "#5b67d1", output: "#1f9d61", cache: "#e58e26", cacheWrite: "#be6f91" };

function buildOption(): echarts.EChartsCoreOption {
  const current = snapshot.value;
  const pts = current?.points ?? [];
  // 空数据时给 log 轴一个默认上限,否则 ECharts 不生成任何 Y 刻度
  const dataMax = pts.reduce(
    (max, p) => Math.max(max, p.inputTokens, p.outputTokens, p.cacheReadTokens, p.cacheWriteTokens),
    1,
  );
  const yMax = pts.length > 0 ? dataMax : 1000;
  const showSinglePoint = pts.length === 1;
  const makeSeries = (
    name: string,
    color: string,
    field: "inputTokens" | "outputTokens" | "cacheReadTokens" | "cacheWriteTokens",
    lineType: "solid" | "dashed" = "solid",
  ) => ({
    name,
    type: "line" as const,
    smooth: 0.35,
    showSymbol: showSinglePoint,
    symbol: "circle",
    symbolSize: 6,
    lineStyle: { width: 2, color, type: lineType },
    itemStyle: { color },
    emphasis: { focus: "series" as const },
    // Log axes cannot draw zero. Tooltip reads the untouched snapshot values.
    data: pts.map((p) => [p.ts, p[field] > 0 ? p[field] : null]),
  });
  return {
    animation: false,
    grid: { left: 56, right: 16, top: 34, bottom: 42 },
    legend: {
      top: 2,
      left: 2,
      icon: "roundRect",
      itemWidth: 14,
      itemHeight: 4,
      itemGap: 14,
      textStyle: { color: "#5a6072", fontSize: 11.5 },
    },
    tooltip: {
      trigger: "axis",
      backgroundColor: "#232838",
      borderWidth: 0,
      padding: [8, 12],
      textStyle: { color: "#fff", fontSize: 12 },
      axisPointer: { type: "line", lineStyle: { color: "#b6bccd", type: "dashed" } },
      formatter: tooltipFor,
    },
    xAxis: {
      type: "time",
      min: pts.length > 0 ? Math.min(current?.start ?? pts[0].ts, pts[0].ts) : current?.start,
      max: current?.end,
      minInterval: current && bucketUnitForRange(current.start, current.end) === "day" ? DAY : HOUR,
      axisLine: { lineStyle: { color: "#d8dbe1" } },
      axisTick: { show: false },
      axisLabel: { hideOverlap: true, fontSize: 10.5, color: "#8a90a3" },
      splitLine: { show: false },
    },
    yAxis: {
      type: "log",
      min: 1,
      max: yMax,
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: { fontSize: 10.5, color: "#8a90a3", formatter: (v: number) => fmtN(v) },
      splitLine: { lineStyle: { color: "rgba(0,0,0,0.06)" } },
    },
    series: [
      makeSeries(t("dash.token.input"), COLORS.input, "inputTokens"),
      makeSeries(t("dash.token.output"), COLORS.output, "outputTokens"),
      makeSeries(t("dash.token.cacheRead"), COLORS.cache, "cacheReadTokens", "dashed"),
      makeSeries(i18n.locale === "en" ? "Cache write" : "缓存写", COLORS.cacheWrite, "cacheWriteTokens"),
    ],
  };
}

function updateChart() {
  chart?.setOption(buildOption());
}

onMounted(() => {
  if (chartEl.value) {
    chart = echarts.init(chartEl.value);
    chart.setOption(buildOption());
    ro = new ResizeObserver(() => chart?.resize());
    ro.observe(chartEl.value);
  }
  void refresh();
  timer = window.setInterval(() => void refresh(), 30_000);
});
onUnmounted(() => {
  window.clearInterval(timer);
  ro?.disconnect();
  chart?.dispose();
  chart = null;
});

watch([snapshot, () => i18n.locale], updateChart);

const errorMessage = computed(() => {
  if (!rangeError.value) return "";
  if (i18n.locale === "en") {
    if (rangeError.value === "fetch") return "Token usage refresh failed. Showing the last successful range.";
    if (rangeError.value === "order") return "The end date must be on or after the start date.";
    if (rangeError.value === "invalid") return "Enter valid start and end dates. The chart keeps the last successful range.";
    return "Choose both a start and end date. The chart keeps the last successful range.";
  }
  if (rangeError.value === "fetch") return "用量刷新失败，图表保留上次成功查询的数据和时间范围。";
  if (rangeError.value === "order") return "结束日期不能早于开始日期；图表保留上次成功查询的数据和时间范围。";
  if (rangeError.value === "invalid") return "请输入有效的开始和结束日期；图表保留上次成功查询的数据和时间范围。";
  return "请选择开始和结束日期；图表保留上次成功查询的数据和时间范围。";
});
</script>

<template>
  <div class="card s12">
    <h3 class="card-title">
      {{ t("dash.token.title") }}
      <span class="r tools">
        <HintTip :text="t('dash.token.hint')" />
        <button class="icon-btn" :disabled="loading" :title="t('quota.refresh')" @click="refresh">
          <RefreshIcon :spin="loading" />
        </button>
      </span>
    </h3>

    <div class="range-row">
      <div class="range-btns">
        <button :class="{ on: range === '12h' }" @click="setRange('12h')">
          {{ t("dash.token.r12h") }}
        </button>
        <button :class="{ on: range === '24h' }" @click="setRange('24h')">
          {{ t("dash.token.r24h") }}
        </button>
        <button :class="{ on: range === '7d' }" @click="setRange('7d')">
          {{ t("dash.token.r7d") }}
        </button>
        <button :class="{ on: range === 'custom' }" @click="setRange('custom')">
          {{ t("dash.token.rcustom") }}
        </button>
      </div>
      <div v-if="range === 'custom'" class="custom-range">
        <input v-model="customFrom" type="date" @change="applyCustom" />
        <span class="sep">~</span>
        <input v-model="customTo" type="date" @change="applyCustom" />
      </div>
    </div>

    <div ref="chartEl" class="chart-wrap">
      <div v-if="snapshot && snapshot.points.length === 0" class="no-data">
        {{ t("dash.token.noDataInRange") }}
      </div>
    </div>

    <p v-if="errorMessage" class="note refresh-error" role="alert">
      {{ errorMessage }}
    </p>
    <p v-if="snapshot" class="note refreshed">
      {{ t("dash.token.refreshedAt", { time: snapshot.refreshedAt.toLocaleString() }) }}
    </p>
  </div>
</template>

<style scoped>
.tools {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}
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

.range-row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  margin-bottom: 4px;
}
.range-btns {
  display: inline-flex;
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
}
.range-btns button {
  border: none;
  background: #fff;
  padding: 5px 12px;
  font-size: 12px;
  color: var(--text-dim);
  cursor: pointer;
}
.range-btns button + button {
  border-left: 1px solid var(--border);
}
.range-btns button.on {
  background: var(--accent);
  color: #fff;
  font-weight: 550;
}
.custom-range {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-dim);
}
.custom-range input {
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 4px 6px;
  font-size: 12px;
  color: var(--text);
  background: #fff;
}

.chart-wrap {
  position: relative;
  height: 250px;
}
.no-data {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-faint);
  font-size: 12px;
}
.refreshed {
  margin: 6px 0 0;
}
.refresh-error {
  margin: 6px 0 0;
  color: #b54747;
}
</style>

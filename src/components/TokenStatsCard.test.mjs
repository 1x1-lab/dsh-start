import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import vm from "node:vm";
import ts from "typescript";
import { computed, ref } from "vue";

const source = await readFile(new URL("./TokenStatsCard.vue", import.meta.url), "utf8");
const script = source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)?.[1];
assert.ok(script, "TokenStatsCard script setup exists");
const withoutImports = script.replace(/^import[\s\S]*?;\s*/gm, "");
const instrumented = `${withoutImports}\n(globalThis).__spec = { refresh, setRange, snapshot, aggregateUsagePoints, bucketUnitForRange, buildOption, tooltipFor };`;
const javascript = ts.transpile(instrumented, { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None });

function createHarness(api) {
  let now = 1_800_000_000_000;
  class ClockDate extends Date {
    constructor(...args) {
      super(...(args.length ? args : [now]));
    }
    static now() { return now; }
  }
  const chart = { setOption() {}, resize() {}, dispose() {} };
  const echarts = { use() {}, init() { return chart; } };
  const context = {
    Date: ClockDate,
    api,
    echarts,
    LineChart: {},
    GridComponent: {},
    LegendComponent: {},
    TooltipComponent: {},
    CanvasRenderer: {},
    i18n: { locale: "en" },
    t: (key) => key,
    ref,
    computed,
    watch() {},
    onMounted() {},
    onUnmounted() {},
    window: { setInterval() { return 0; }, clearInterval() {} },
  };
  vm.createContext(context);
  vm.runInContext(javascript, context);
  return { spec: context.__spec, setNow(value) { now = value; } };
}

function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}

const point = (inputTokens) => ({
  ts: 1_800_000_000_000,
  inputTokens,
  outputTokens: 0,
  cacheReadTokens: 0,
  cacheWriteTokens: 0,
});

const usagePoint = (ts, inputTokens, outputTokens, cacheReadTokens, cacheWriteTokens) => ({
  ts,
  inputTokens,
  outputTokens,
  cacheReadTokens,
  cacheWriteTokens,
});

function localTime(year, month, day, hour, minute) {
  return new Date(year, month - 1, day, hour, minute).getTime();
}

test("each refresh captures a fresh preset end time", async () => {
  const calls = [];
  const harness = createHarness({
    getTokenStats(start, end) {
      calls.push({ start, end });
      return Promise.resolve([]);
    },
  });

  await harness.spec.refresh();
  harness.setNow(1_800_000_030_000);
  await harness.spec.refresh();

  assert.equal(calls.length, 2);
  assert.equal(calls[0].end, 1_800_000_000_000);
  assert.equal(calls[1].end, 1_800_000_030_000);
  assert.equal(calls[1].start, calls[1].end - 12 * 60 * 60 * 1000);
});

test("a late response cannot replace the newest range snapshot", async () => {
  const first = deferred();
  const second = deferred();
  const calls = [];
  const harness = createHarness({
    getTokenStats(start, end) {
      calls.push({ start, end });
      return calls.length === 1 ? first.promise : second.promise;
    },
  });

  const oldRequest = harness.spec.refresh();
  harness.setNow(1_800_000_030_000);
  harness.spec.setRange("24h");
  assert.equal(calls.length, 2);

  second.resolve([point(200)]);
  await new Promise((resolve) => setImmediate(resolve));
  first.resolve([point(100)]);
  await oldRequest;

  assert.equal(harness.spec.snapshot.value.start, calls[1].start);
  assert.equal(harness.spec.snapshot.value.end, calls[1].end);
  assert.equal(harness.spec.snapshot.value.points[0].inputTokens, 200);
});

test("hourly aggregation sums same-hour points and keeps separate hours across midnight", () => {
  const harness = createHarness({ getTokenStats: async () => [] });
  const first = localTime(2026, 9, 13, 23, 5);
  const second = localTime(2026, 9, 13, 23, 55);
  const nextHour = localTime(2026, 9, 14, 0, 10);
  const buckets = harness.spec.aggregateUsagePoints([
    usagePoint(first, 10, 2, 30, 4),
    usagePoint(second, 20, 3, 40, 5),
    usagePoint(nextHour, 7, 8, 9, 10),
  ], first, nextHour);

  assert.equal(buckets.length, 2);
  assert.equal(new Date(buckets[0].ts).getHours(), 23);
  assert.deepEqual(
    [buckets[0].inputTokens, buckets[0].outputTokens, buckets[0].cacheReadTokens, buckets[0].cacheWriteTokens],
    [30, 5, 70, 9],
  );
  assert.equal(new Date(buckets[1].ts).getDate(), 14);
  assert.equal(new Date(buckets[1].ts).getHours(), 0);
  assert.deepEqual(
    [buckets[1].inputTokens, buckets[1].outputTokens, buckets[1].cacheReadTokens, buckets[1].cacheWriteTokens],
    [7, 8, 9, 10],
  );
});

test("partial boundary hours filter by the selected range and conserve all token totals", () => {
  const harness = createHarness({ getTokenStats: async () => [] });
  const start = localTime(2026, 9, 13, 10, 30);
  const end = localTime(2026, 9, 13, 12, 15);
  const points = [
    usagePoint(start - 1, 999, 999, 999, 999),
    usagePoint(localTime(2026, 9, 13, 10, 45), 10, 1, 2, 3),
    usagePoint(localTime(2026, 9, 13, 11, 5), 20, 2, 4, 6),
    usagePoint(end, 30, 3, 6, 9),
    usagePoint(end + 1, 888, 888, 888, 888),
  ];
  const buckets = harness.spec.aggregateUsagePoints(points, start, end);
  assert.equal(buckets.length, 3);
  assert.ok(buckets[0].ts < start);
  const totals = (items) => ["inputTokens", "outputTokens", "cacheReadTokens", "cacheWriteTokens"]
    .map((key) => items.reduce((sum, item) => sum + item[key], 0));
  const selectedPoints = points.filter((p) => p.ts >= start && p.ts <= end);
  assert.deepEqual(totals(buckets), totals(selectedPoints));

  harness.spec.snapshot.value = { start, end, points: buckets, refreshedAt: new Date() };
  const option = harness.spec.buildOption();
  assert.equal(option.xAxis.min, buckets[0].ts);
  assert.equal(option.xAxis.max, end);
  assert.equal(option.xAxis.minInterval, 60 * 60 * 1000);
  assert.equal(option.series[0].showSymbol, false);
  const tooltip = harness.spec.tooltipFor([{ dataIndex: 0 }]);
  assert.match(tooltip, /Selected range starts at/);
  assert.match(tooltip, /10:00/);
  assert.match(tooltip, /11:00/);
  assert.ok(tooltip.includes("dash.token.input: 10"));
  assert.ok(tooltip.includes("dash.token.output: 1"));
  assert.ok(tooltip.includes("dash.token.cacheRead: 2"));
  assert.ok(tooltip.includes("Cache write: 3"));
});

test("empty hourly input remains empty", () => {
  const harness = createHarness({ getTokenStats: async () => [] });
  const start = localTime(2026, 9, 13, 10, 30);
  assert.equal(harness.spec.aggregateUsagePoints([], start, start + 60_000).length, 0);
});

test("an exact 24-hour range stays hourly", () => {
  const harness = createHarness({ getTokenStats: async () => [] });
  const end24h = localTime(2026, 9, 14, 12, 0);
  const start24h = end24h - 24 * 60 * 60 * 1000;
  assert.equal(harness.spec.bucketUnitForRange(start24h, end24h), "hour");
  const hourly = harness.spec.aggregateUsagePoints([
    usagePoint(localTime(2026, 9, 14, 10, 5), 1, 0, 0, 0),
    usagePoint(localTime(2026, 9, 14, 11, 5), 2, 0, 0, 0),
  ], start24h, end24h);
  assert.equal(hourly.length, 2);
});

test("ranges longer than 24 hours, including 7 days, use daily buckets", () => {
  const harness = createHarness({ getTokenStats: async () => [] });
  const end24h = localTime(2026, 9, 14, 12, 0);
  assert.equal(harness.spec.bucketUnitForRange(end24h - 24 * 60 * 60 * 1000 - 1, end24h), "day");
  const start7d = localTime(2026, 9, 7, 12, 0);
  assert.equal(harness.spec.bucketUnitForRange(start7d, end24h), "day");
  const daily = harness.spec.aggregateUsagePoints([
    usagePoint(localTime(2026, 9, 12, 10, 0), 3, 1, 0, 0),
    usagePoint(localTime(2026, 9, 12, 20, 0), 4, 2, 0, 0),
    usagePoint(localTime(2026, 9, 13, 8, 0), 5, 3, 0, 0),
  ], start7d, end24h);
  assert.equal(daily.length, 2);
  assert.deepEqual([daily[0].inputTokens, daily[0].outputTokens], [7, 3]);
  assert.deepEqual([daily[1].inputTokens, daily[1].outputTokens], [5, 3]);
});

test("daily aggregation sums points across each local calendar day", () => {
  const harness = createHarness({ getTokenStats: async () => [] });
  const start = localTime(2026, 9, 12, 0, 0);
  const end = localTime(2026, 9, 15, 0, 0);
  const points = harness.spec.aggregateUsagePoints([
    usagePoint(localTime(2026, 9, 12, 0, 5), 10, 1, 2, 3),
    usagePoint(localTime(2026, 9, 12, 23, 55), 20, 2, 4, 6),
    usagePoint(localTime(2026, 9, 13, 0, 5), 30, 3, 6, 9),
    usagePoint(localTime(2026, 9, 14, 12, 0), 40, 4, 8, 12),
  ], start, end);

  assert.equal(points.length, 3);
  assert.equal(new Date(points[0].ts).getDate(), 12);
  assert.deepEqual([points[0].inputTokens, points[0].outputTokens, points[0].cacheReadTokens, points[0].cacheWriteTokens], [30, 3, 6, 9]);
  assert.equal(new Date(points[1].ts).getDate(), 13);
  assert.equal(new Date(points[2].ts).getDate(), 14);
  assert.deepEqual([points[2].inputTokens, points[2].outputTokens, points[2].cacheReadTokens, points[2].cacheWriteTokens], [40, 4, 8, 12]);
});

test("partial daily boundaries filter points, conserve totals, and describe selected portions", () => {
  const harness = createHarness({ getTokenStats: async () => [] });
  const start = localTime(2026, 9, 13, 10, 30);
  const end = localTime(2026, 9, 15, 0, 15);
  const raw = [
    usagePoint(start - 1, 999, 999, 999, 999),
    usagePoint(start, 10, 1, 2, 3),
    usagePoint(localTime(2026, 9, 14, 12, 0), 20, 2, 4, 6),
    usagePoint(end, 30, 3, 6, 9),
    usagePoint(end + 1, 888, 888, 888, 888),
  ];
  const points = harness.spec.aggregateUsagePoints(raw, start, end);
  assert.equal(points.length, 3);
  assert.ok(points[0].ts < start);
  const totals = (items) => ["inputTokens", "outputTokens", "cacheReadTokens", "cacheWriteTokens"]
    .map((key) => items.reduce((sum, item) => sum + item[key], 0));
  assert.deepEqual(totals(points), totals(raw.filter((p) => p.ts >= start && p.ts <= end)));

  harness.spec.snapshot.value = { start, end, points, refreshedAt: new Date() };
  const option = harness.spec.buildOption();
  assert.equal(option.xAxis.min, points[0].ts);
  assert.equal(option.xAxis.max, end);
  assert.equal(option.xAxis.minInterval, 24 * 60 * 60 * 1000);
  assert.match(harness.spec.tooltipFor([{ dataIndex: 0 }]), /Selected range starts at/);
  assert.match(harness.spec.tooltipFor([{ dataIndex: 2 }]), /Selected range ends at/);
});

test("the chart option has no wheel zoom or pan component", () => {
  const harness = createHarness({ getTokenStats: async () => [] });
  const option = harness.spec.buildOption();
  assert.equal(option.dataZoom, undefined);
});

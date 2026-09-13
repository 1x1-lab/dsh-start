<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";

defineProps<{ text: string }>();

const iconEl = ref<HTMLElement | null>(null);
const popEl = ref<HTMLElement | null>(null);
const visible = ref(false);

let scrollHide: (() => void) | null = null;

function show() {
  const icon = iconEl.value;
  const pop = popEl.value;
  if (!icon || !pop) return;

  // 先按不可见渲染,量出气泡实际尺寸后再定位
  pop.style.opacity = "0";
  pop.style.visibility = "visible";

  const r = icon.getBoundingClientRect();
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const margin = 8;
  const gap = 7;
  pop.style.maxWidth = Math.min(280, vw - margin * 2) + "px";

  const pw = pop.offsetWidth;
  const ph = pop.offsetHeight;
  const cx = r.left + r.width / 2;

  // 水平:优先对准图标中心,钳制在视口内
  const left = Math.min(Math.max(margin, cx - pw / 2), Math.max(margin, vw - margin - pw));
  // 垂直:默认在图标下方,视口放不下时翻到上方,再放不下则钳在视口内
  let top = r.bottom + gap;
  let above = false;
  if (top + ph > vh - margin) {
    const alt = r.top - gap - ph;
    if (alt >= margin) {
      top = alt;
      above = true;
    } else {
      top = Math.min(Math.max(margin, top), Math.max(margin, vh - margin - ph));
    }
  }

  pop.style.left = Math.round(left) + "px";
  pop.style.top = Math.round(top) + "px";
  pop.style.setProperty("--arrow-left", Math.round(Math.min(Math.max(10, cx - left), pw - 10)) + "px");
  pop.classList.toggle("above", above);
  pop.style.opacity = "1";
  visible.value = true;

  // 页面滚动会让 fixed 定位失效对准,滚动时直接隐藏
  if (!scrollHide) {
    scrollHide = () => hide();
    window.addEventListener("scroll", scrollHide, true);
  }
}

function hide() {
  visible.value = false;
  const pop = popEl.value;
  if (pop) {
    pop.style.opacity = "0";
    pop.style.visibility = "hidden";
  }
  if (scrollHide) {
    window.removeEventListener("scroll", scrollHide, true);
    scrollHide = null;
  }
}

onBeforeUnmount(hide);
</script>

<template>
  <span ref="iconEl" class="hint-tip" @mouseenter="show" @mouseleave="hide">
    <svg
      class="hint-icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
    >
      <circle cx="12" cy="12" r="10" />
      <line x1="12" y1="16" x2="12" y2="12" />
      <line x1="12" y1="8" x2="12.01" y2="8" />
    </svg>
  </span>
  <Teleport to="body">
    <span ref="popEl" class="hint-pop" :class="{ show: visible }">{{ text }}</span>
  </Teleport>
</template>

<style scoped>
.hint-tip {
  position: relative;
  display: inline-flex;
  margin-left: 6px;
  vertical-align: middle;
  cursor: help;
}
.hint-icon {
  width: 13px;
  height: 13px;
  color: var(--text-faint);
  transition: color 0.15s ease;
}
.hint-tip:hover .hint-icon {
  color: var(--accent);
}
</style>

<style>
/* 气泡 Teleport 到 body:脱离所有裁剪容器,fixed 定位只相对视口 */
.hint-pop {
  position: fixed;
  left: 0;
  top: 0;
  background: #232838;
  color: #fff;
  font-size: 11.5px;
  line-height: 1.55;
  font-weight: 400;
  padding: 7px 10px;
  border-radius: 8px;
  width: max-content;
  white-space: normal;
  text-align: left;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
  transition: opacity 0.15s ease, visibility 0.15s ease;
  z-index: 60;
}
.hint-pop::after {
  content: "";
  position: absolute;
  left: var(--arrow-left, 50%);
  transform: translateX(-50%);
  top: -10px;
  border: 5px solid transparent;
  border-bottom-color: #232838;
}
.hint-pop.above::after {
  top: auto;
  bottom: -10px;
  border-bottom-color: transparent;
  border-top-color: #232838;
}
</style>

import { reactive } from "vue";

export const toast = reactive({ text: "" });
let timer = 0;

/** 顶部居中轻提示，默认 2s 自动消失 */
export function showToast(text: string, ms = 2000) {
  toast.text = text;
  window.clearTimeout(timer);
  timer = window.setTimeout(() => (toast.text = ""), ms);
}

import { reactive } from "vue";

export const toast = reactive({
  text: "",
  /** 点击 toast 时执行的动作（如打开 Release 页）；无动作时不可点击 */
  action: null as null | (() => void),
});
let timer = 0;

/** 屏幕居中轻提示；action 提供时可点击（默认 2s，可传 ms 覆盖） */
export function showToast(text: string, ms = 2000, action?: () => void) {
  toast.text = text;
  toast.action = action ?? null;
  window.clearTimeout(timer);
  timer = window.setTimeout(() => (toast.text = ""), ms);
}

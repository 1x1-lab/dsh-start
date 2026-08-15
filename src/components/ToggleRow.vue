<script setup lang="ts">
defineProps<{
  label: string;
  desc?: string;
  modelValue: boolean;
}>();
defineEmits<{
  (e: "update:modelValue", v: boolean): void;
}>();
</script>

<template>
  <div class="toggle-row">
    <div>
      <div class="label">{{ label }}</div>
      <div v-if="desc" class="desc">{{ desc }}</div>
    </div>
    <button
      class="switch"
      :class="{ on: modelValue }"
      role="switch"
      :aria-checked="modelValue"
      @click="$emit('update:modelValue', !modelValue)"
    >
      <span class="knob"></span>
    </button>
  </div>
</template>

<style scoped>
.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 10px 0;
}
.toggle-row + .toggle-row {
  border-top: 1px solid rgba(0, 0, 0, 0.055);
}
.label {
  font-weight: 550;
  font-size: 13px;
}
.desc {
  font-size: 11px;
  color: var(--text-faint);
  margin-top: 1px;
}
.switch {
  flex: none;
  width: 34px;
  height: 20px;
  border-radius: 999px;
  border: none;
  background: #d8dbe1;
  position: relative;
  transition: background 0.18s ease;
  padding: 0;
}
.switch.on {
  background: var(--accent);
}
.knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  transition: transform 0.18s ease;
}
.switch.on .knob {
  transform: translateX(14px);
}
</style>

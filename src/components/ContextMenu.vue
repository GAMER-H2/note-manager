<script setup>
import { useContextMenu } from "../composables/useContextMenu.js";

const { menu, closeMenu } = useContextMenu();

const onItem = async (item) => {
  if (item.disabled) return;
  closeMenu();
  try {
    await item.action();
  } catch (e) {
    console.error("Context menu action failed:", e);
  }
};
</script>

<template>
  <template v-if="menu.visible">
    <div
      class="context-menu__backdrop"
      @click="closeMenu"
      @contextmenu.prevent="closeMenu"
    ></div>
    <div
      class="context-menu"
      role="menu"
      :style="{ left: `${menu.x}px`, top: `${menu.y}px` }"
    >
      <button
        v-for="(item, i) in menu.items"
        :key="i"
        type="button"
        role="menuitem"
        class="context-menu__item"
        :class="{
          'context-menu__item--danger': item.danger,
          'context-menu__item--disabled': item.disabled,
        }"
        :disabled="item.disabled"
        @click="onItem(item)"
      >
        {{ item.label }}
      </button>
    </div>
  </template>
</template>

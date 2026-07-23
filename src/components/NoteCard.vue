<script setup>
import { computed } from "vue";
import { firstLineTitle, previewText } from "../lib/notes.js";

const props = defineProps({
  note: { type: Object, required: true },
});

defineEmits(["open"]);

const title = computed(() => firstLineTitle(props.note.content));
const preview = computed(() => previewText(props.note.content));
</script>

<template>
  <article
    class="note-card"
    tabindex="0"
    :data-note-id="note.id"
    @click="$emit('open', note)"
    @keydown.enter.prevent="$emit('open', note)"
  >
    <h2 class="note-title">{{ title }}</h2>
    <p class="note-preview">{{ preview }}</p>
  </article>
</template>

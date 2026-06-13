<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { EditorView, basicSetup } from "codemirror";
import { EditorState } from "@codemirror/state";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags } from "@lezer/highlight";
import { parse as parseToml } from "smol-toml";
import { fetchConfig, updateConfig } from "@/api";

// Syntax highlighting — stencil yard palette
const syntaxTheme = syntaxHighlighting(
  HighlightStyle.define([
    { tag: tags.string, color: "#4ADE80" }, // strings: success green
    { tag: tags.number, color: "#fbbf24" }, // numbers: warning amber
    { tag: tags.bool, color: "#60a5fa" }, // booleans: info blue
    { tag: tags.null, color: "#60a5fa" }, // null: info blue
    { tag: tags.propertyName, color: "#C8C6BE" }, // keys: text-secondary
    { tag: tags.comment, color: "#959390" }, // comments: text-muted
    { tag: tags.punctuation, color: "#7A7870" }, // brackets/colons: dim
  ]),
);

// Dark theme matching the design system
const darkTheme = EditorView.theme(
  {
    "&": {
      color: "var(--color-text-primary)",
      backgroundColor: "var(--color-surface-0)",
    },
    ".cm-content": {
      caretColor: "var(--color-accent)",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "var(--color-accent)",
    },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
      backgroundColor: "var(--color-surface-3)",
    },
    ".cm-panels": {
      backgroundColor: "var(--color-surface-2)",
      color: "var(--color-text-primary)",
    },
    ".cm-panels.cm-panels-top": {
      borderBottom: "1px solid var(--color-border)",
    },
    ".cm-panels.cm-panels-bottom": {
      borderTop: "1px solid var(--color-border)",
    },
    ".cm-searchMatch": {
      backgroundColor: "rgba(255, 214, 0, 0.15)",
      outline: "1px solid rgba(255, 214, 0, 0.3)",
    },
    ".cm-searchMatch.cm-searchMatch-selected": {
      backgroundColor: "rgba(255, 214, 0, 0.25)",
    },
    ".cm-activeLine": {
      backgroundColor: "rgba(30, 30, 26, 0.5)",
    },
    ".cm-selectionMatch": {
      backgroundColor: "rgba(42, 42, 36, 0.5)",
    },
    "&.cm-focused .cm-matchingBracket, &.cm-focused .cm-nonmatchingBracket": {
      backgroundColor: "rgba(42, 42, 36, 0.8)",
    },
    ".cm-gutters": {
      backgroundColor: "var(--color-surface-0)",
      color: "var(--color-text-muted)",
      border: "none",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "rgba(30, 30, 26, 0.5)",
    },
    ".cm-foldPlaceholder": {
      backgroundColor: "transparent",
      border: "none",
      color: "var(--color-text-muted)",
    },
    ".cm-tooltip": {
      border: "1px solid var(--color-border)",
      backgroundColor: "var(--color-surface-2)",
    },
    ".cm-tooltip .cm-tooltip-arrow:before": {
      borderTopColor: "transparent",
      borderBottomColor: "transparent",
    },
    ".cm-tooltip .cm-tooltip-arrow:after": {
      borderTopColor: "var(--color-surface-2)",
      borderBottomColor: "var(--color-surface-2)",
    },
    ".cm-tooltip-autocomplete": {
      "& > ul > li[aria-selected]": {
        backgroundColor: "var(--color-surface-3)",
        color: "var(--color-text-primary)",
      },
    },
  },
  { dark: true },
);

type SaveStatus = "idle" | "saving" | "saved" | "error" | "conflict";

const loading = ref(true);
const error = ref<string | null>(null);
const saveStatus = ref<SaveStatus>("idle");
const saveError = ref<string | null>(null);
const hash = ref("");
const validationError = ref<string | null>(null);
const editorReady = ref(false);

const editorContainer = ref<HTMLDivElement | null>(null);
let editorView: EditorView | undefined;

function errorMessage(err: unknown, fallback: string): string {
  return err instanceof Error && err.message ? err.message : fallback;
}

async function loadConfig() {
  try {
    error.value = null;
    loading.value = true;
    const data = await fetchConfig();
    hash.value = data.hash;

    if (editorView) {
      editorView.dispatch({
        changes: {
          from: 0,
          to: editorView.state.doc.length,
          insert: data.content,
        },
      });
    }
  } catch (err: unknown) {
    error.value = errorMessage(err, "Failed to load config");
  } finally {
    loading.value = false;
  }
}

function validateToml(content: string): string | null {
  try {
    parseToml(content);
    return null;
  } catch (err: unknown) {
    return errorMessage(err, "Invalid TOML");
  }
}

async function handleSave() {
  if (!editorView) return;

  const content = editorView.state.doc.toString();

  const validErr = validateToml(content);
  if (validErr) {
    validationError.value = validErr;
    saveStatus.value = "error";
    saveError.value = `Invalid TOML: ${validErr}`;
    return;
  }
  validationError.value = null;

  try {
    saveStatus.value = "saving";
    saveError.value = null;
    const result = await updateConfig(content, hash.value);
    hash.value = result.hash;
    saveStatus.value = "saved";
    setTimeout(() => {
      if (saveStatus.value === "saved") saveStatus.value = "idle";
    }, 3000);
  } catch (err: unknown) {
    const message = errorMessage(err, "Failed to save");
    if (message.includes("modified externally")) {
      saveStatus.value = "conflict";
      saveError.value = "Config was modified externally. Reload to get the latest version.";
    } else {
      saveStatus.value = "error";
      saveError.value = message;
    }
  }
}

onMounted(() => {
  if (!editorContainer.value) return;

  const state = EditorState.create({
    doc: "",
    extensions: [
      basicSetup,
      darkTheme,
      syntaxTheme,
      EditorView.theme({
        "&": { height: "100%" },
        ".cm-scroller": { overflow: "auto" },
      }),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          saveStatus.value = "idle";
          const content = update.state.doc.toString();
          validationError.value = validateToml(content);
        }
      }),
    ],
  });

  editorView = new EditorView({
    state,
    parent: editorContainer.value,
  });
  editorReady.value = true;

  loadConfig();
});

onBeforeUnmount(() => {
  editorView?.destroy();
  editorView = undefined;
});

const statusColor = computed(() => {
  switch (saveStatus.value) {
    case "saved":
      return "text-success";
    case "error":
      return "text-error";
    case "conflict":
      return "text-warning";
    case "saving":
      return "text-accent";
    default:
      return "text-text-muted";
  }
});

const statusText = computed(() => {
  switch (saveStatus.value) {
    case "saved":
      return "Saved";
    case "error":
      return saveError.value || "Error";
    case "conflict":
      return saveError.value || "Conflict";
    case "saving":
      return "Saving...";
    default:
      return validationError.value ? `TOML error: ${validationError.value}` : "";
  }
});
</script>

<template>
  <div class="flex flex-col h-full">
    <div class="px-8 py-6 border-b-2 border-border flex items-center justify-between">
      <div>
        <h2
          class="font-display text-4xl text-accent tracking-[0.1em] uppercase"
          style="text-shadow: 2px 2px 0 rgba(0, 0, 0, 0.5)"
        >
          Configuration
        </h2>
        <p class="font-label text-[10px] text-text-secondary uppercase tracking-[0.1em] mt-1">
          Edit devrig.toml
        </p>
      </div>
      <div class="flex items-center gap-3">
        <span v-if="statusText" class="text-xs" :class="statusColor">{{ statusText }}</span>
        <button
          v-if="saveStatus === 'conflict'"
          class="border border-error/40 bg-surface-1 hover:border-error text-error text-[11px] px-3 py-1.5 font-label uppercase tracking-[0.1em] cursor-pointer"
          @click="loadConfig()"
        >
          Reload
        </button>
        <button
          class="border border-border bg-surface-1 hover:border-accent/40 text-text-primary text-[11px] px-3 py-1.5 font-label uppercase tracking-[0.1em] cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
          :disabled="saveStatus === 'saving' || !!validationError"
          @click="handleSave"
        >
          Save
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-hidden">
      <div v-if="error" class="m-6 bg-error/10 border border-error/20 rounded-lg p-4 text-center">
        <p class="text-error text-sm">{{ error }}</p>
        <button
          class="mt-2 text-accent hover:text-accent-hover text-sm cursor-pointer"
          @click="loadConfig()"
        >
          Retry
        </button>
      </div>

      <div v-if="loading && !editorReady" class="py-12 text-center text-text-muted text-sm">
        Loading configuration...
      </div>

      <div ref="editorContainer" class="h-full" style="font-size: 14px" />
    </div>
  </div>
</template>

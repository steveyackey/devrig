import { ref } from "vue";

export type Theme = "dark" | "light";

const STORAGE_KEY = "devrig-theme";

function getStored(): Theme | null {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "dark" || v === "light") return v;
  } catch {
    /* ignore */
  }
  return null;
}

function applyToDoc(t: Theme) {
  const root = document.documentElement;
  const body = document.body;
  root.setAttribute("data-theme", t);
  body.setAttribute("data-theme", t);
  if (t === "dark") {
    root.classList.add("dark");
    root.classList.remove("light");
    body.classList.add("dark");
    body.classList.remove("light");
  } else {
    root.classList.add("light");
    root.classList.remove("dark");
    body.classList.add("light");
    body.classList.remove("dark");
  }
}

export const theme = ref<Theme>("dark");

export function setTheme(value: Theme) {
  theme.value = value;
  try {
    localStorage.setItem(STORAGE_KEY, value);
  } catch {
    /* ignore */
  }
  applyToDoc(value);
}

export function toggleTheme() {
  setTheme(theme.value === "dark" ? "light" : "dark");
}

export function initTheme() {
  const initial = getStored() ?? "dark";
  theme.value = initial;
  applyToDoc(initial);
}

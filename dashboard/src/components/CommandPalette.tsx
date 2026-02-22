import { Component, createSignal, createEffect, onCleanup, For, Show } from 'solid-js';
import { Activity, ScrollText, BarChart3, CircleDot, Settings } from 'lucide-solid';

interface CommandItem {
  label: string;
  icon: Component<{ size?: number; class?: string }>;
  route: string;
}

const commands: CommandItem[] = [
  { label: 'Traces', icon: Activity, route: '#/traces' },
  { label: 'Logs', icon: ScrollText, route: '#/logs' },
  { label: 'Metrics', icon: BarChart3, route: '#/metrics' },
  { label: 'Status', icon: CircleDot, route: '#/status' },
  { label: 'Config', icon: Settings, route: '#/config' },
];

const CommandPalette: Component = () => {
  const [open, setOpen] = createSignal(false);
  const [query, setQuery] = createSignal('');
  const [selectedIndex, setSelectedIndex] = createSignal(0);

  const filtered = () => {
    const q = query().toLowerCase().trim();
    if (!q) return commands;
    return commands.filter((cmd) => cmd.label.toLowerCase().includes(q));
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    // Open palette
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
      e.preventDefault();
      setOpen(true);
      setQuery('');
      setSelectedIndex(0);
    }

    if (!open()) return;

    if (e.key === 'Escape') {
      e.preventDefault();
      setOpen(false);
    }

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, filtered().length - 1));
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
    }

    if (e.key === 'Enter') {
      e.preventDefault();
      const items = filtered();
      if (items.length > 0 && selectedIndex() < items.length) {
        navigate(items[selectedIndex()]);
      }
    }
  };

  const navigate = (item: CommandItem) => {
    window.location.hash = item.route.replace('#', '');
    setOpen(false);
  };

  createEffect(() => {
    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  // Reset selected index when query changes
  createEffect(() => {
    query();
    setSelectedIndex(0);
  });

  return (
    <Show when={open()}>
      <div
        data-testid="command-palette"
        class="fixed inset-0 z-50 flex items-start justify-center pt-[20vh]"
        onClick={() => setOpen(false)}
      >
        {/* Backdrop */}
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" />

        {/* Dialog */}
        <div
          class="relative w-full max-w-md bg-surface-1 border border-border rounded-xl shadow-2xl animate-slide-up overflow-hidden"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Search input */}
          <div class="px-4 py-3 border-b border-border">
            <input
              data-testid="command-palette-input"
              type="text"
              placeholder="Type a command..."
              value={query()}
              onInput={(e) => setQuery(e.currentTarget.value)}
              class="w-full bg-transparent text-text-primary text-sm placeholder:text-text-muted focus:outline-none"
              autofocus
            />
          </div>

          {/* Results */}
          <div class="py-2 max-h-64 overflow-auto">
            <Show when={filtered().length === 0}>
              <div class="px-4 py-6 text-center text-sm text-text-muted">
                No results found
              </div>
            </Show>

            <For each={filtered()}>
              {(item, index) => {
                const Icon = item.icon;
                return (
                  <button
                    data-testid="command-palette-item"
                    class={`w-full flex items-center gap-3 px-4 py-2.5 text-sm text-left transition-colors ${
                      index() === selectedIndex()
                        ? 'bg-accent/15 text-accent'
                        : 'text-text-secondary hover:bg-surface-2'
                    }`}
                    onClick={() => navigate(item)}
                    onMouseEnter={() => setSelectedIndex(index())}
                  >
                    <Icon size={16} class={index() === selectedIndex() ? 'text-accent' : 'text-text-muted'} />
                    <span>{item.label}</span>
                    <span class="ml-auto text-xs text-text-muted">Navigate</span>
                  </button>
                );
              }}
            </For>
          </div>

          {/* Footer */}
          <div class="px-4 py-2 border-t border-border flex items-center gap-4 text-xs text-text-muted">
            <span class="flex items-center gap-1">
              <kbd class="px-1.5 py-0.5 bg-surface-2 rounded text-[10px]">↑↓</kbd> Navigate
            </span>
            <span class="flex items-center gap-1">
              <kbd class="px-1.5 py-0.5 bg-surface-2 rounded text-[10px]">↵</kbd> Select
            </span>
            <span class="flex items-center gap-1">
              <kbd class="px-1.5 py-0.5 bg-surface-2 rounded text-[10px]">esc</kbd> Close
            </span>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default CommandPalette;

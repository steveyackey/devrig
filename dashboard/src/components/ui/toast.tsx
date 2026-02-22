import { createSignal, For, type Component } from 'solid-js';
import { cn } from '../../lib/cn';

interface Toast {
  id: number;
  message: string;
  variant: 'success' | 'error' | 'info';
}

const [toasts, setToasts] = createSignal<Toast[]>([]);
let nextId = 0;

export function showToast(message: string, variant: Toast['variant'] = 'info') {
  const id = nextId++;
  setToasts((prev) => [...prev, { id, message, variant }]);
  setTimeout(() => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, 3000);
}

const variantStyles = {
  success: 'border-2 border-success/30 bg-surface-1 text-success',
  error: 'border-2 border-error/30 bg-surface-1 text-error',
  info: 'border-2 border-accent/30 bg-surface-1 text-accent',
};

const ToastProvider: Component = () => {
  return (
    <div class="fixed bottom-4 right-4 z-[100] flex flex-col gap-2">
      <For each={toasts()}>
        {(toast) => (
          <div
            class={cn(
              'border px-4 py-3 text-sm shadow-lg animate-slide-up font-label',
              variantStyles[toast.variant]
            )}
          >
            {toast.message}
          </div>
        )}
      </For>
    </div>
  );
};

export { ToastProvider };

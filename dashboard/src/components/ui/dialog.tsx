import { type JSX, type Component, splitProps } from 'solid-js';
import { Dialog as KDialog } from '@kobalte/core/dialog';
import { cn } from '../../lib/cn';

const Dialog = KDialog;

const DialogContent: Component<{ class?: string; children: JSX.Element }> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <KDialog.Portal>
      <KDialog.Overlay class="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm animate-fade-in" />
      <div class="fixed inset-0 z-50 flex items-start justify-center pt-[20vh]">
        <KDialog.Content
          class={cn(
            'w-full max-w-lg rounded-xl border border-border bg-surface-1 shadow-2xl animate-slide-up',
            local.class
          )}
          {...rest}
        >
          {local.children}
        </KDialog.Content>
      </div>
    </KDialog.Portal>
  );
};

const DialogTitle: Component<{ class?: string; children: JSX.Element }> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <KDialog.Title class={cn('text-lg font-semibold text-text-primary', local.class)} {...rest}>
      {local.children}
    </KDialog.Title>
  );
};

export { Dialog, DialogContent, DialogTitle };

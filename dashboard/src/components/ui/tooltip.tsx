import { type JSX, type Component, splitProps } from 'solid-js';
import { Tooltip as KTooltip } from '@kobalte/core/tooltip';
import { cn } from '../../lib/cn';

export interface TooltipProps {
  content: JSX.Element;
  children: JSX.Element;
  class?: string;
}

const Tooltip: Component<TooltipProps> = (props) => {
  return (
    <KTooltip gutter={4}>
      <KTooltip.Trigger as="span" class="inline-flex">
        {props.children}
      </KTooltip.Trigger>
      <KTooltip.Portal>
        <KTooltip.Content
          class={cn(
            'z-50 rounded-md bg-surface-2 border border-border px-3 py-1.5 text-xs text-text-primary shadow-md animate-fade-in',
            props.class
          )}
        >
          <KTooltip.Arrow />
          {props.content}
        </KTooltip.Content>
      </KTooltip.Portal>
    </KTooltip>
  );
};

export { Tooltip };

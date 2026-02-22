import { type JSX, type Component, splitProps } from 'solid-js';
import { Tabs as KTabs } from '@kobalte/core/tabs';
import { cn } from '../../lib/cn';

const Tabs = KTabs;

const TabsList: Component<{ class?: string; children: JSX.Element }> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <KTabs.List
      class={cn('flex border-b border-border px-7', local.class)}
      {...rest}
    >
      {local.children}
    </KTabs.List>
  );
};

const TabsTrigger: Component<{ value: string; class?: string; children: JSX.Element }> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children', 'value']);
  return (
    <KTabs.Trigger
      value={local.value}
      class={cn(
        'px-4 py-2.5 text-sm font-medium border-b-2 -mb-px transition-colors',
        'data-[selected]:border-accent data-[selected]:text-accent',
        'border-transparent text-text-muted hover:text-text-secondary',
        local.class
      )}
      {...rest}
    >
      {local.children}
    </KTabs.Trigger>
  );
};

const TabsContent: Component<{ value: string; class?: string; children: JSX.Element }> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children', 'value']);
  return (
    <KTabs.Content value={local.value} class={cn('animate-fade-in', local.class)} {...rest}>
      {local.children}
    </KTabs.Content>
  );
};

export { Tabs, TabsList, TabsTrigger, TabsContent };

import { splitProps, type JSX, type Component } from 'solid-js';
import { cn } from '../../lib/cn';

export interface CardProps extends JSX.HTMLAttributes<HTMLDivElement> {}

const Card: Component<CardProps> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <div class={cn('rounded-lg border border-border bg-surface-1 shadow-sm', local.class)} {...rest}>
      {local.children}
    </div>
  );
};

const CardHeader: Component<CardProps> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <div class={cn('px-6 py-5 border-b border-border', local.class)} {...rest}>
      {local.children}
    </div>
  );
};

const CardContent: Component<CardProps> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <div class={cn('p-6', local.class)} {...rest}>
      {local.children}
    </div>
  );
};

export { Card, CardHeader, CardContent };

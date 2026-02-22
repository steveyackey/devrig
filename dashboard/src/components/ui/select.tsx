import { splitProps, type JSX, type Component } from 'solid-js';
import { cn } from '../../lib/cn';

export interface SelectProps extends JSX.SelectHTMLAttributes<HTMLSelectElement> {}

const Select: Component<SelectProps> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <select
      class={cn(
        'bg-surface-2 border border-border rounded-md px-3 py-1.5 text-sm text-text-primary',
        'focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent/30',
        local.class
      )}
      {...rest}
    >
      {local.children}
    </select>
  );
};

export { Select };

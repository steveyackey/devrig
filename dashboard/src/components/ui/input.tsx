import { splitProps, type JSX, type Component } from 'solid-js';
import { cn } from '../../lib/cn';

export interface InputProps extends JSX.InputHTMLAttributes<HTMLInputElement> {}

const Input: Component<InputProps> = (props) => {
  const [local, rest] = splitProps(props, ['class']);
  return (
    <input
      class={cn(
        'bg-surface-1 border-2 border-border px-3.5 py-2 text-sm text-text-primary',
        'focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent/30',
        'placeholder:text-text-muted',
        local.class
      )}
      {...rest}
    />
  );
};

export { Input };

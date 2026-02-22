import { splitProps, type JSX, type Component } from 'solid-js';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../../lib/cn';

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-2 rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50 disabled:pointer-events-none disabled:opacity-50',
  {
    variants: {
      variant: {
        default: 'bg-accent text-white hover:bg-accent-hover',
        outline: 'border border-border text-text-secondary hover:bg-surface-2 hover:text-text-primary',
        ghost: 'text-text-secondary hover:bg-surface-2 hover:text-text-primary',
        destructive: 'bg-error text-white hover:bg-error/90',
      },
      size: {
        default: 'h-9 px-4 py-2',
        sm: 'h-8 px-3 text-xs',
        lg: 'h-10 px-6',
        icon: 'h-9 w-9',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  }
);

export interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonVariants> {}

const Button: Component<ButtonProps> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'variant', 'size', 'children']);
  return (
    <button class={cn(buttonVariants({ variant: local.variant, size: local.size }), local.class)} {...rest}>
      {local.children}
    </button>
  );
};

export { Button, buttonVariants };

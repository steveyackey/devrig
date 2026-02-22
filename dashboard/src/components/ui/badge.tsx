import { splitProps, type JSX, type Component } from 'solid-js';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../../lib/cn';

const badgeVariants = cva(
  'inline-flex items-center px-2 py-0.5 text-[9px] font-label uppercase tracking-wider transition-colors',
  {
    variants: {
      variant: {
        default: 'text-text-muted border border-border',
        success: 'text-success border border-success/30',
        error: 'text-error border border-error/30',
        warning: 'text-warning border border-warning/30',
        info: 'text-info border border-info/30',
        accent: 'text-accent border border-accent/30',
        counter: 'text-info border border-info/30',
        gauge: 'text-success border border-success/30',
        histogram: 'text-[#a855f7] border border-[#a855f7]/30',
        fatal: 'bg-error/20 text-error border border-error/30',
        debug: 'text-text-secondary border border-border',
        trace: 'text-text-muted border border-border',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  }
);

export interface BadgeProps extends JSX.HTMLAttributes<HTMLSpanElement>, VariantProps<typeof badgeVariants> {}

const Badge: Component<BadgeProps> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'variant', 'children']);
  return (
    <span class={cn(badgeVariants({ variant: local.variant }), local.class)} {...rest}>
      {local.children}
    </span>
  );
};

export { Badge, badgeVariants };

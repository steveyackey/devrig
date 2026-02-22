import { splitProps, type JSX, type Component } from 'solid-js';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../../lib/cn';

const badgeVariants = cva(
  'inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium transition-colors',
  {
    variants: {
      variant: {
        default: 'bg-surface-2 text-text-secondary border border-border',
        success: 'bg-success/15 text-success border border-success/20',
        error: 'bg-error/15 text-error border border-error/20',
        warning: 'bg-warning/15 text-warning border border-warning/20',
        info: 'bg-info/15 text-info border border-info/20',
        accent: 'bg-accent/15 text-accent border border-accent/20',
        counter: 'bg-info/15 text-info border border-info/20',
        gauge: 'bg-success/15 text-success border border-success/20',
        histogram: 'bg-[#a855f7]/15 text-[#a855f7] border border-[#a855f7]/20',
        fatal: 'bg-error text-white',
        debug: 'bg-surface-3/50 text-text-secondary border border-border',
        trace: 'bg-surface-2/50 text-text-muted border border-border',
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

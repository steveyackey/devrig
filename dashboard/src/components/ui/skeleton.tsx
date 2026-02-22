import { splitProps, type JSX, type Component } from 'solid-js';
import { cn } from '../../lib/cn';

export interface SkeletonProps extends JSX.HTMLAttributes<HTMLDivElement> {}

const Skeleton: Component<SkeletonProps> = (props) => {
  const [local, rest] = splitProps(props, ['class']);
  return (
    <div
      class={cn(
        'rounded-md bg-gradient-to-r from-surface-2 via-surface-3 to-surface-2 bg-[length:400%_100%] animate-skeleton',
        local.class
      )}
      {...rest}
    />
  );
};

export { Skeleton };

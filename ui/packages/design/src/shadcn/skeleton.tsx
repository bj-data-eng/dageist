import { cn } from './cn';

// Changed from shadcn/ui: screen readers skip the stand-in, which says nothing.
function Skeleton({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      aria-hidden="true"
      data-slot="skeleton"
      className={cn('animate-pulse rounded-md bg-line', className)}
      {...props}
    />
  );
}

export { Skeleton };

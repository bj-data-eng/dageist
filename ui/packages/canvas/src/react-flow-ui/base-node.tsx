import type { ComponentProps } from 'react';

import { cn } from '@dageist/design';

// Changed from React Flow UI: drawn with the canvas's floating look (the panel
// colour, the line colour, round corners and a soft shadow) and a ring in the
// strong line colour when pointed at, and filling the size React Flow gives a
// resized node. It takes no tab stop of its own: React Flow's wrapper around it
// is already focusable.
export function BaseNode({ className, ...props }: ComponentProps<'div'>): React.JSX.Element {
  return (
    <div
      className={cn(
        'relative flex h-full w-full flex-col rounded-xl border bg-card text-card-foreground shadow-lg shadow-black/30',
        'hover:ring-1 hover:ring-line-strong',
        // React Flow displays node elements inside of a `NodeWrapper`
        // component, which compiles down to a div with the class
        // `react-flow__node`. When a node is selected, the class `selected` is
        // added to the `react-flow__node` element. This allows us to style the
        // node when it is selected.
        'in-[.selected]:border-muted-foreground',
        'in-[.selected]:shadow-xl',
        className,
      )}
      {...props}
    />
  );
}

/**
 * A container for a consistent header layout intended to be used inside the
 * `<BaseNode />` component.
 */
export function BaseNodeHeader({
  className,
  ...props
}: ComponentProps<'header'>): React.JSX.Element {
  return (
    <header
      {...props}
      className={cn(
        'mx-0 my-0 flex flex-row items-center justify-between gap-2 border-b px-3 py-2',
        // Remove or modify these classes if you modify the padding in the
        // `<BaseNode />` component.
        className,
      )}
    />
  );
}

/**
 * The title text for the node. To maintain a native application feel, the title
 * text is not selectable.
 */
export function BaseNodeHeaderTitle({
  className,
  children,
  ...props
}: ComponentProps<'h3'>): React.JSX.Element {
  return (
    <h3
      data-slot="base-node-title"
      className={cn('m-0 min-w-0 flex-1 truncate text-[14px] font-semibold select-none', className)}
      {...props}
    >
      {children}
    </h3>
  );
}

export function BaseNodeContent({ className, ...props }: ComponentProps<'div'>): React.JSX.Element {
  return (
    <div
      data-slot="base-node-content"
      className={cn('flex flex-col gap-y-2 p-3', className)}
      {...props}
    />
  );
}

export function BaseNodeFooter({ className, ...props }: ComponentProps<'div'>): React.JSX.Element {
  return (
    <div
      data-slot="base-node-footer"
      className={cn('flex flex-col items-center gap-y-2 border-t px-3 pt-2 pb-3', className)}
      {...props}
    />
  );
}

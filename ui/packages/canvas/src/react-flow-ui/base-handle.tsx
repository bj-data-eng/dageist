import type { ComponentProps } from 'react';

import { Handle } from '@xyflow/react';
import type { HandleProps } from '@xyflow/react';

import { cn } from '@dageist/design';

export type BaseHandleProps = HandleProps;

// Changed from React Flow UI: a ring in the muted colour on the panel colour,
// in place of slate greys, as the canvas's other pins are drawn.
export function BaseHandle({
  className,
  children,
  ...props
}: ComponentProps<typeof Handle>): React.JSX.Element {
  return (
    <Handle
      {...props}
      className={cn(
        'h-[11px] w-[11px] rounded-full border border-muted bg-panel transition',
        className,
      )}
    >
      {children}
    </Handle>
  );
}

import { Position } from '@xyflow/react';
import type { HandleProps } from '@xyflow/react';

import { cn } from '@dageist/design';

import { BaseHandle } from './base-handle';

const wrapperClassNames: Record<Position, string> = {
  [Position.Top]: 'flex-col-reverse left-1/2 -translate-y-full -translate-x-1/2',
  [Position.Bottom]: 'flex-col left-1/2 translate-y-[10px] -translate-x-1/2',
  [Position.Left]: 'flex-row-reverse top-1/2 -translate-x-full -translate-y-1/2',
  [Position.Right]: 'top-1/2 -translate-y-1/2 translate-x-[10px]',
};

// Changed from React Flow UI: the stalk is drawn in the strong line colour, and
// the position has no default, since React Flow requires one.
export function ButtonHandle({
  showButton = true,
  position,
  children,
  ...props
}: HandleProps & { showButton?: boolean; children?: React.ReactNode }): React.JSX.Element {
  const wrapperClassName = wrapperClassNames[position];
  const vertical = position === Position.Top || position === Position.Bottom;

  return (
    <BaseHandle position={position} {...props}>
      {showButton && (
        <div className={cn('pointer-events-none absolute flex items-center', wrapperClassName)}>
          <div className={cn('bg-line-strong', vertical ? 'h-10 w-px' : 'h-px w-10')} />
          <div className="nodrag nopan pointer-events-auto">{children}</div>
        </div>
      )}
    </BaseHandle>
  );
}

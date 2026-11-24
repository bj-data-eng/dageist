import type { ComponentProps } from 'react';

import type { HandleProps } from '@xyflow/react';

import { cn } from '@dageist/design';

import { BaseHandle } from './base-handle';

const flexDirections = {
  top: 'flex-col',
  right: 'flex-row-reverse justify-end',
  bottom: 'flex-col-reverse justify-end',
  left: 'flex-row',
};

// Changed from React Flow UI: the label is any content, such as a name and its
// dtype, not only a string, so `title` is the name a screen reader gives it.
export function LabeledHandle({
  className,
  labelClassName,
  handleClassName,
  title,
  label,
  position,
  ...props
}: HandleProps &
  Omit<ComponentProps<'div'>, 'title'> & {
    title: string;
    label?: React.ReactNode;
    handleClassName?: string;
    labelClassName?: string;
  }): React.JSX.Element {
  const { ref, ...handleProps } = props;

  return (
    <div
      title={title}
      className={cn('relative flex items-center', flexDirections[position], className)}
      ref={ref}
    >
      <BaseHandle position={position} className={handleClassName} {...handleProps} />
      <span className={cn('min-w-0 px-3 text-foreground', labelClassName)}>{label ?? title}</span>
    </div>
  );
}

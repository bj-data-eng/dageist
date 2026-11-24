/**
 * @file A table's fields under a header that folds them away: "Fields" with
 * how many there are, then one field to a line with its dtype. The header
 * opens and shuts the list; it starts open.
 */

import { useState } from 'react';

import { Collapsible, CollapsibleContent, CollapsibleTrigger, cn } from '@dageist/design';
import type { FieldView } from '@dageist/host';

/** What a list of fields shows. */
export interface FieldListProps {
  readonly fields: readonly FieldView[];
}

/** Shows `fields` as lines under a "Fields" header that opens and shuts them. */
export function FieldList({ fields }: FieldListProps): React.JSX.Element {
  const [open, setOpen] = useState(true);
  return (
    <Collapsible open={open} onOpenChange={setOpen} className="flex w-full min-w-0 flex-col">
      <CollapsibleTrigger className="nodrag flex h-[22px] w-full cursor-pointer items-center gap-1.5 text-left text-[12px] text-muted hover:text-ink">
        <span
          aria-hidden="true"
          className={cn('inline-block w-3 text-center text-[10px]', open && 'rotate-90')}
        >
          ▸
        </span>
        <span>Fields</span>
        <span className="tabular-nums">{fields.length}</span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <ul className="m-0 list-none p-0 pl-[18px]">
          {fields.map((field) => (
            <li
              key={field.name}
              className="flex h-[22px] min-w-0 items-baseline gap-2 font-mono text-[12px] leading-[22px]"
            >
              <span className="truncate text-column-ink">{field.name}</span>
              <span className="shrink-0 text-dtype">{field.dtype}</span>
            </li>
          ))}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  );
}

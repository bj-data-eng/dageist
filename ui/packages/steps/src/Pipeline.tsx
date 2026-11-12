/**
 * @file The Polars code the step list stands for, one line per step.
 */

import { cx } from '@dageist/design';

import type { Step } from './model/steps';
import { stepText } from './model/steps';

/** What the pipeline shows and reports. */
export interface PipelineProps {
  /** The name of the frame the pipeline starts from. */
  readonly source: string;
  readonly steps: readonly Step[];
  readonly selectedId: string | null;
  readonly onSelect: (id: string) => void;
}

/** Draws `steps` as Polars code; choosing a line chooses its step. */
export function Pipeline({
  source,
  steps,
  selectedId,
  onSelect,
}: PipelineProps): React.JSX.Element {
  return (
    <div className="flex flex-col overflow-x-auto rounded-[10px] border border-dashed border-line-strong py-2 font-mono text-[13px]">
      <div className="min-w-max px-3.5 leading-5">{source}</div>
      {steps.map((step) => (
        <button
          key={step.id}
          type="button"
          className={cx(
            'min-w-max cursor-pointer px-3.5 text-left leading-5 whitespace-pre hover:bg-hover',
            step.id === selectedId && 'bg-fresh shadow-[inset_2px_0_0_var(--color-accent)]',
          )}
          onClick={() => {
            onSelect(step.id);
          }}
        >
          {`  ${stepText(step)}`}
        </button>
      ))}
    </div>
  );
}

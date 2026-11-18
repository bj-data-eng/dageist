/**
 * @file The Python Polars module the step list stands for, as the engine
 * wrote it, line by line.
 */

import type { SourceLineView } from '@dageist/host';

import { cx } from '@dageist/design';

/** What the pipeline shows and reports. */
export interface PipelineProps {
  /** The lines of the module the engine wrote of the steps, or none before it has answered. */
  readonly module: readonly SourceLineView[];
  readonly selectedId: string | null;
  readonly onSelect: (id: string) => void;
}

/**
 * Draws the engine's module as Polars code. A line that is the call of a step
 * chooses that step; any other line is only drawn.
 */
export function Pipeline({ module, selectedId, onSelect }: PipelineProps): React.JSX.Element {
  return (
    <div className="flex flex-col overflow-x-auto rounded-[10px] border border-dashed border-line-strong py-2 font-mono text-[13px]">
      {module.map(({ text, step }, index) =>
        step === null ? (
          // The module is written afresh for every change, and its lines have no identity of their own.
          <div key={index} className="min-h-5 min-w-max px-3.5 leading-5 whitespace-pre">
            {text}
          </div>
        ) : (
          <button
            key={index}
            type="button"
            className={cx(
              'min-w-max cursor-pointer px-3.5 text-left leading-5 whitespace-pre hover:bg-hover',
              step === selectedId && 'bg-fresh shadow-[inset_2px_0_0_var(--color-accent)]',
            )}
            onClick={() => {
              onSelect(step);
            }}
          >
            {text}
          </button>
        ),
      )}
    </div>
  );
}

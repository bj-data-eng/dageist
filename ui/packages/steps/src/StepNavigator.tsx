/**
 * @file The steps as a list of names: each step said by its head, such as
 * "Sort table by dcn" or "Add column step_index", without the pieces under
 * it. It is how a step is found and chosen where the steps themselves are
 * not shown.
 */

import { cx } from '@dageist/design';

import type { Step, StepResult } from './model/steps';
import { stepDetail } from './summary';
import { VERB_COLOURS, VERB_LABELS } from './verbs';

/** What a step navigator shows and reports. */
export interface StepNavigatorProps {
  readonly steps: readonly Step[];
  /** What each step produced, in the same order. */
  readonly results: readonly StepResult[];
  readonly selectedId: string | null;
  readonly onSelect: (id: string) => void;
}

/** Lists `steps` by name and reports the one that is chosen. */
export function StepNavigator({
  steps,
  results,
  selectedId,
  onSelect,
}: StepNavigatorProps): React.JSX.Element {
  return (
    <ol className="m-0 flex list-none flex-col p-0">
      {steps.map((step, index) => {
        const problem = results[index]?.problem ?? null;
        const selected = step.id === selectedId;
        return (
          <li key={step.id}>
            <button
              type="button"
              aria-current={selected ? 'step' : undefined}
              title={problem ?? undefined}
              className={cx(
                'flex w-full cursor-pointer items-baseline gap-2 border-b border-line px-3 py-1.5 text-left text-[13px]',
                'hover:bg-hover focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent',
                VERB_COLOURS[step.verb],
                selected && 'bg-row shadow-[inset_2px_0_0_var(--color-accent)]',
                problem !== null && !selected && 'shadow-[inset_2px_0_0_var(--color-wrong)]',
              )}
              onClick={() => {
                onSelect(step.id);
              }}
            >
              <span className="w-5 shrink-0 text-right font-mono text-[11px] text-muted">
                {index + 1}
              </span>
              <span className="shrink-0 font-semibold text-(--verb)">{VERB_LABELS[step.verb]}</span>
              <span
                className={cx(
                  'min-w-0 truncate font-mono text-[12.5px]',
                  problem === null ? 'text-ink' : 'text-wrong',
                )}
              >
                {stepDetail(step)}
              </span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}

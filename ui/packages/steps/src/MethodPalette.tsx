/**
 * @file The side list of methods that can be added to the chosen step's
 * chain, grouped by what they do, with why a method does not fit.
 */

import { cx } from '@dageist/design';

import type { Method, MethodName } from './model/expr';
import {
  METHOD_GROUPS,
  METHOD_GROUP_ORDER,
  METHOD_GROUP_TITLES,
  METHOD_HINTS,
  METHOD_NAMES,
  checkExpr,
  defaultArgument,
  methodDtype,
  methodRequirement,
} from './model/expr';
import type { Frame } from './model/frame';
import { nextId } from './model/ids';
import type { Step } from './model/steps';

/** What the palette needs to know and report. */
export interface MethodPaletteProps {
  /** The chosen step, if any. */
  readonly step: Step | null;
  /** The frame the chosen step is given. */
  readonly frame: Frame;
  readonly onAdd: (method: Method) => void;
}

function note(name: MethodName, last: string | undefined, fits: boolean): string {
  if (last === undefined) return 'fix the marked part first';
  return fits ? METHOD_HINTS[name] : `needs ${methodRequirement(name)}`;
}

/** Lists the methods that can be added to `step`'s chain. */
export function MethodPalette({ step, frame, onAdd }: MethodPaletteProps): React.JSX.Element {
  if (step?.verb !== 'with_columns') {
    return (
      <aside className="overflow-y-auto border-l border-line bg-raised px-4 pt-2 pb-6">
        <h2 className="mt-4 mb-2 text-xs font-semibold tracking-wide text-muted">Methods</h2>
        <p className="m-0 text-[13px] text-muted">
          Choose an Add column step to add methods to its chain.
        </p>
      </aside>
    );
  }
  const check = checkExpr(step.expr, frame);
  const last = check.problem === null ? check.dtypes.at(-1) : undefined;
  return (
    <aside className="overflow-y-auto border-l border-line bg-raised px-4 pt-2 pb-6">
      {METHOD_GROUP_ORDER.map((group) => (
        <section key={group}>
          <h2 className="mt-4 mb-2 text-xs font-semibold tracking-wide text-muted">
            {METHOD_GROUP_TITLES[group]}
          </h2>
          <div className="flex flex-col gap-1">
            {METHOD_NAMES.filter((name) => METHOD_GROUPS[name] === group).map((name) => {
              const fits = last !== undefined && methodDtype(name, last) !== null;
              return (
                <button
                  key={name}
                  type="button"
                  disabled={!fits}
                  className={cx(
                    'flex items-baseline gap-2 rounded-lg border border-line bg-row px-2.5 py-1 text-left text-[13px] leading-[22px]',
                    'enabled:cursor-pointer enabled:hover:border-accent disabled:text-muted',
                  )}
                  onClick={() => {
                    onAdd({ id: nextId('method'), name, argument: defaultArgument(name) });
                  }}
                >
                  <span aria-hidden="true" className="font-bold text-accent">
                    +
                  </span>
                  <span className="font-mono text-[12.5px]">{name}</span>
                  <span className="ml-auto text-[11px] text-muted">{note(name, last, fits)}</span>
                </button>
              );
            })}
          </div>
        </section>
      ))}
    </aside>
  );
}

/**
 * @file The menu of methods that can be added to a chain at one place. It
 * opens where the method will go, filters as you type, and says why a method
 * does not fit the value it would be handed.
 */

import { useEffect, useRef, useState } from 'react';

import { cx } from '@dageist/design';

import type { Method, MethodName } from './model/expr';
import {
  METHOD_GROUPS,
  METHOD_GROUP_ORDER,
  METHOD_GROUP_TITLES,
  METHOD_HINTS,
  METHOD_NAMES,
  defaultArgument,
  methodDtype,
  methodRequirement,
} from './model/expr';
import type { Dtype, Frame } from './model/frame';
import { nextId } from './model/ids';

/** What the menu needs to know and report. */
export interface MethodMenuProps {
  /** The dtype the new method would be handed, or `null` when the chain before it is broken. */
  readonly input: Dtype | null;
  /** The frame the chain works on, for a method that starts with one of its columns. */
  readonly frame: Frame;
  readonly onPick: (method: Method) => void;
}

function matches(name: MethodName, query: string): boolean {
  return query === '' || name.includes(query) || METHOD_HINTS[name].includes(query);
}

/** Lists the methods that can be added, filtered by what is typed. */
export function MethodMenu({ input, frame, onPick }: MethodMenuProps): React.JSX.Element {
  const [typed, setTyped] = useState('');
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    field.current?.focus();
  }, []);

  const query = typed.trim().toLowerCase();
  const shown = METHOD_NAMES.filter((name) => matches(name, query));
  const fits = (name: MethodName): boolean =>
    input !== null && methodDtype(name, input, null) !== null;
  const pick = (name: MethodName): void => {
    onPick({ id: nextId('method'), name, argument: defaultArgument(name, frame, input) });
  };
  const note = (name: MethodName): string => {
    if (input === null) return 'fix the marked part first';
    return fits(name) ? METHOD_HINTS[name] : `needs ${methodRequirement(name)}`;
  };

  return (
    <div className="flex max-h-80 w-72 flex-col">
      <input
        ref={field}
        type="text"
        aria-label="Find a method"
        placeholder={input === null ? 'Find a method' : `Find a method for ${input}`}
        value={typed}
        className="m-1 rounded-md border border-line-strong bg-canvas px-2 text-[13px] leading-[26px] focus:border-accent focus:outline-none"
        onChange={(event) => {
          setTyped(event.currentTarget.value);
        }}
        onKeyDown={(event) => {
          const first = shown.find((name) => fits(name));
          if (event.key === 'Enter' && first !== undefined) pick(first);
        }}
      />
      <div className="min-h-0 overflow-y-auto pb-1">
        {METHOD_GROUP_ORDER.map((group) => {
          const names = shown.filter((name) => METHOD_GROUPS[name] === group);
          if (names.length === 0) return null;
          return (
            <section key={group}>
              <h3 className="m-0 px-2.5 pt-2 pb-0.5 text-[11px] font-semibold tracking-wide text-muted">
                {METHOD_GROUP_TITLES[group]}
              </h3>
              {names.map((name) => (
                <button
                  key={name}
                  type="button"
                  disabled={!fits(name)}
                  className={cx(
                    'flex w-full items-baseline gap-2 rounded-md px-2.5 py-0.5 text-left leading-[22px]',
                    'enabled:cursor-pointer enabled:hover:bg-hover disabled:text-muted',
                  )}
                  onClick={() => {
                    pick(name);
                  }}
                >
                  <span className="font-mono text-[12.5px]">{name}</span>
                  <span className="ml-auto text-[11px] text-muted">{note(name)}</span>
                </button>
              ))}
            </section>
          );
        })}
        {shown.length === 0 && (
          <p className="m-0 px-2.5 py-2 text-[13px] text-muted">No method matches.</p>
        )}
      </div>
    </div>
  );
}

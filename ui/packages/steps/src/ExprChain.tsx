/**
 * @file An expression drawn as a chain: a column followed by method parts.
 *
 * At rest the chain reads as plain text. Pointing at a part shows its tools:
 * its name opens a menu to swap it, the same name drags it to another place,
 * and a small cross removes it. Pointing at the seam between two parts turns
 * it into a plus that inserts a method there.
 */

import { PointerActivationConstraints, PointerSensor } from '@dnd-kit/dom';
import { useSortable } from '@dnd-kit/react/sortable';
import { useCallback, useState } from 'react';

import type { Choice } from '@dageist/design';
import { CommitField, Hint, Picker, Popover, cx } from '@dageist/design';

import { MethodMenu } from './MethodMenu';
import { columnChoices } from './choices';
import type { Expr, ExprCheck, Method, MethodName } from './model/expr';
import {
  ARGUMENT_KINDS,
  METHOD_GROUPS,
  METHOD_HINTS,
  METHOD_NAMES,
  checkExpr,
  defaultArgument,
  methodDtype,
  methodRequirement,
  operandFits,
} from './model/expr';
import type { Dtype, Frame } from './model/frame';

/**
 * A part is dragged by its name, which is also the button that opens its
 * menu. Dragging starts only after the pointer has moved, so a plain click
 * still opens the menu.
 */
const PART_SENSORS = [
  PointerSensor.configure({
    activationConstraints: [new PointerActivationConstraints.Distance({ value: 6 })],
    preventActivation: () => false,
  }),
];

function withMethods(expr: Expr, methods: readonly Method[]): Expr {
  return { column: expr.column, methods, alias: expr.alias };
}

/** Returns `expr` with the method at `from` moved to position `to`. */
export function withMethodMoved(expr: Expr, from: number, to: number): Expr {
  const methods = [...expr.methods];
  const [method] = methods.splice(from, 1);
  if (method === undefined) return expr;
  methods.splice(to, 0, method);
  return withMethods(expr, methods);
}

function withMethodInserted(expr: Expr, at: number, method: Method): Expr {
  const methods = [...expr.methods];
  methods.splice(at, 0, method);
  return withMethods(expr, methods);
}

function withoutMethod(expr: Expr, id: string): Expr {
  return withMethods(
    expr,
    expr.methods.filter((existing) => existing.id !== id),
  );
}

/** Returns `method` exchanged for `name`, keeping its argument when both take the same kind. */
function swapped(method: Method, name: MethodName, frame: Frame, input: Dtype | null): Method {
  const keeps =
    ARGUMENT_KINDS[name] !== null && ARGUMENT_KINDS[name] === ARGUMENT_KINDS[method.name];
  return {
    id: method.id,
    name,
    argument: keeps ? method.argument : defaultArgument(name, frame, input),
  };
}

/** Lists the columns `input` can be combined with, and why the others cannot. */
function operandChoices(
  frame: Frame,
  input: Dtype | null,
  current: string,
  onPick: (column: string) => void,
): Choice[] {
  return frame.map((column) => ({
    label: `${column.name} · ${column.dtype}`,
    current: column.name === current,
    reason: input === null || operandFits(input, column.dtype) ? null : 'a different dtype',
    onPick: () => {
      onPick(column.name);
    },
  }));
}

/** Lists what the method at `index` can be swapped for, plus moving and removing it. */
function methodChoices(
  index: number,
  expr: Expr,
  check: ExprCheck,
  frame: Frame,
  onChange: (expr: Expr) => void,
): Choice[] {
  const method = expr.methods[index];
  const input = check.dtypes[index];
  if (method === undefined) return [];
  const group = METHOD_GROUPS[method.name];
  const swaps: Choice[] = METHOD_NAMES.filter((name) => METHOD_GROUPS[name] === group).map(
    (name) => ({
      label: name,
      current: name === method.name,
      reason:
        input === undefined || methodDtype(name, input) !== null
          ? null
          : `needs ${methodRequirement(name)}`,
      onPick: () => {
        onChange(
          withMethods(
            expr,
            expr.methods.map((existing) =>
              existing.id === method.id ? swapped(method, name, frame, input ?? null) : existing,
            ),
          ),
        );
      },
    }),
  );
  return [
    ...swaps,
    {
      label: '← Move earlier',
      current: false,
      reason: index === 0 ? 'already first' : null,
      onPick: () => {
        onChange(withMethodMoved(expr, index, index - 1));
      },
    },
    {
      label: 'Move later →',
      current: false,
      reason: index === expr.methods.length - 1 ? 'already last' : null,
      onPick: () => {
        onChange(withMethodMoved(expr, index, index + 1));
      },
    },
    {
      label: '✕ Remove',
      current: false,
      reason: null,
      onPick: () => {
        onChange(withoutMethod(expr, method.id));
      },
    },
  ];
}

interface SeamProps {
  /** The dtype that passes through the seam, or `null` when the chain before it is broken. */
  readonly dtype: Dtype | null;
  /** Whether the seam sits after the last part, where it is a plus from the start. */
  readonly last: boolean;
  /** What the seam is for, for assistive technology. */
  readonly label: string;
  readonly onOpen: (anchor: HTMLElement) => void;
}

/** The join between two parts. Pointing at it offers to insert a method there. */
function Seam({ dtype, last, label, onOpen }: SeamProps): React.JSX.Element {
  return (
    <Hint lines={dtype === null ? [label] : [`${dtype} passes here`, label]} code={false}>
      <button
        type="button"
        aria-label={label}
        className={cx(
          'group/seam inline-flex h-[22px] w-4 cursor-pointer items-center justify-center rounded-sm text-[13px] text-muted',
          'hover:bg-hover hover:text-accent focus-visible:text-accent focus-visible:outline-1 focus-visible:outline-accent',
          // After the last part there is nothing to separate, so the plus shows
          // only while its step is pointed at or holds focus.
          last && 'opacity-0 group-focus-within/step:opacity-100 group-hover/step:opacity-100',
        )}
        onClick={(event) => {
          onOpen(event.currentTarget);
        }}
      >
        {!last && (
          <span
            aria-hidden="true"
            className="group-hover/seam:hidden group-focus-visible/seam:hidden"
          >
            ›
          </span>
        )}
        <span
          aria-hidden="true"
          className={cx(
            'font-bold',
            !last && 'hidden group-hover/seam:inline group-focus-visible/seam:inline',
          )}
        >
          +
        </span>
      </button>
    </Hint>
  );
}

interface MethodPartProps {
  /** The seam that joins this part to what comes before it. It moves with the part. */
  readonly seam: React.ReactNode;
  readonly stepId: string;
  readonly index: number;
  readonly method: Method;
  readonly expr: Expr;
  readonly check: ExprCheck;
  readonly frame: Frame;
  readonly onChange: (expr: Expr) => void;
}

function MethodPart({
  seam,
  stepId,
  index,
  method,
  expr,
  check,
  frame,
  onChange,
}: MethodPartProps): React.JSX.Element {
  const kind = ARGUMENT_KINDS[method.name];
  const argument = method.argument ?? '';
  const input = check.dtypes[index] ?? null;
  const output = check.dtypes[index + 1] ?? null;
  const flow = input === null ? [] : [`${input} → ${output ?? 'does not fit'}`];
  const { ref, handleRef, isDragSource } = useSortable({
    id: method.id,
    index,
    group: stepId,
    type: `part:${stepId}`,
    accept: `part:${stepId}`,
    sensors: PART_SENSORS,
  });
  const commit = (value: string): void => {
    onChange(
      withMethods(
        expr,
        expr.methods.map((existing) =>
          existing.id === method.id
            ? { id: method.id, name: method.name, argument: value }
            : existing,
        ),
      ),
    );
  };
  return (
    <span ref={ref} className={cx('inline-flex items-center', isDragSource && 'opacity-60')}>
      {seam}
      <span
        className={cx(
          'group/part relative inline-flex items-center gap-0.5 rounded-md',
          check.problem?.index === index && 'bg-invalid outline-1 outline-danger',
        )}
      >
        <Hint lines={[`${method.name} · ${METHOD_HINTS[method.name]}`, ...flow]} code={false}>
          <Picker
            label={method.name}
            tone="term"
            choices={methodChoices(index, expr, check, frame, onChange)}
            buttonRef={handleRef}
          />
        </Hint>
        {kind === 'operand' && (
          <Picker
            label={argument === '' ? 'choose a column' : argument}
            tone="column"
            choices={operandChoices(frame, input, argument, commit)}
          />
        )}
        {kind === 'column' && (
          <Picker label={argument} tone="column" choices={columnChoices(frame, argument, commit)} />
        )}
        {(kind === 'integer' || kind === 'duration') && (
          <CommitField
            value={argument}
            label={`${method.name} argument`}
            numeric={kind === 'integer'}
            code
            onCommit={commit}
          />
        )}
        <button
          type="button"
          aria-label={`Remove ${method.name}`}
          title={`Remove ${method.name}`}
          className={cx(
            'absolute -top-2 -right-1.5 hidden size-4 cursor-pointer items-center justify-center rounded-full',
            'border border-line-strong bg-panel text-[9px] leading-none text-muted hover:border-danger hover:text-danger',
            'group-hover/part:flex focus-visible:flex',
          )}
          onClick={() => {
            onChange(withoutMethod(expr, method.id));
          }}
        >
          ✕
        </button>
      </span>
    </span>
  );
}

/** Where a method is about to be added: its position in the chain and the seam it opens from. */
interface Insertion {
  readonly at: number;
  readonly anchor: HTMLElement;
}

/** What an expression chain shows and reports. */
export interface ExprChainProps {
  /** The step the expression belongs to; its parts can only be dragged within it. */
  readonly stepId: string;
  readonly expr: Expr;
  /** The frame the expression is worked out on. */
  readonly frame: Frame;
  readonly onChange: (expr: Expr) => void;
}

/** Draws `expr` as a column followed by its method parts. */
export function ExprChain({ stepId, expr, frame, onChange }: ExprChainProps): React.JSX.Element {
  const check = checkExpr(expr, frame);
  const [insertion, setInsertion] = useState<Insertion | null>(null);
  const closeMenu = useCallback(() => {
    setInsertion(null);
  }, []);
  const seam = (at: number): React.JSX.Element => (
    <Seam
      dtype={check.dtypes[at] ?? null}
      last={at === expr.methods.length}
      label={at === expr.methods.length ? 'Add a method' : 'Insert a method here'}
      onOpen={(anchor) => {
        setInsertion({ at, anchor });
      }}
    />
  );

  return (
    <span className="inline-flex flex-wrap items-center gap-y-1">
      <span
        className={cx(
          'inline-flex rounded-md',
          check.problem?.index === -1 && 'bg-invalid outline-1 outline-danger',
        )}
      >
        <Picker
          label={expr.column}
          tone="column"
          choices={columnChoices(frame, expr.column, (column) => {
            onChange({ column, methods: expr.methods, alias: expr.alias });
          })}
        />
      </span>
      {expr.methods.map((method, index) => (
        <MethodPart
          key={method.id}
          seam={seam(index)}
          stepId={stepId}
          index={index}
          method={method}
          expr={expr}
          check={check}
          frame={frame}
          onChange={onChange}
        />
      ))}
      {seam(expr.methods.length)}
      {insertion !== null && (
        <Popover anchor={insertion.anchor} onDismiss={closeMenu}>
          <MethodMenu
            input={check.dtypes[insertion.at] ?? null}
            frame={frame}
            onPick={(method) => {
              onChange(withMethodInserted(expr, insertion.at, method));
              setInsertion(null);
            }}
          />
        </Popover>
      )}
    </span>
  );
}

/**
 * @file An expression drawn as a chain: a column followed by method parts.
 * Each part can be swapped from its menu, edited through its argument and
 * dragged to another place in the chain.
 */

import { PointerActivationConstraints, PointerSensor } from '@dnd-kit/dom';
import { useSortable } from '@dnd-kit/react/sortable';

import type { Choice } from '@dageist/design';
import { CommitField, Picker, cx } from '@dageist/design';

import { columnChoices } from './choices';
import type { Expr, ExprCheck, Method, MethodName } from './model/expr';
import {
  ARGUMENT_KINDS,
  METHOD_GROUPS,
  METHOD_NAMES,
  checkExpr,
  defaultArgument,
  methodDtype,
  methodRequirement,
} from './model/expr';
import type { Frame } from './model/frame';

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

/** Returns `method` exchanged for `name`, keeping its argument when both take the same kind. */
function swapped(method: Method, name: MethodName): Method {
  const keeps =
    ARGUMENT_KINDS[name] !== null && ARGUMENT_KINDS[name] === ARGUMENT_KINDS[method.name];
  return { id: method.id, name, argument: keeps ? method.argument : defaultArgument(name) };
}

/** Lists what the method at `index` can be swapped for, plus moving and removing it. */
function methodChoices(
  index: number,
  expr: Expr,
  check: ExprCheck,
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
              existing.id === method.id ? swapped(method, name) : existing,
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
        onChange(
          withMethods(
            expr,
            expr.methods.filter((existing) => existing.id !== method.id),
          ),
        );
      },
    },
  ];
}

interface MethodPartProps {
  readonly stepId: string;
  readonly index: number;
  readonly method: Method;
  readonly expr: Expr;
  readonly check: ExprCheck;
  readonly frame: Frame;
  readonly onChange: (expr: Expr) => void;
}

function MethodPart({
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
    <span
      ref={ref}
      className={cx(
        'inline-flex items-center gap-1.5 border-l border-line-strong px-2 py-1',
        check.problem?.index === index && 'bg-invalid shadow-[inset_0_0_0_1px_var(--color-danger)]',
        isDragSource && 'opacity-60',
      )}
    >
      <Picker
        label={method.name}
        tone="term"
        choices={methodChoices(index, expr, check, onChange)}
        buttonRef={handleRef}
      />
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
    </span>
  );
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
  return (
    <span className="inline-flex flex-wrap items-center rounded-[9px] border border-line-strong bg-chain">
      <span
        className={cx(
          'inline-flex items-center px-2 py-1',
          check.problem?.index === -1 && 'bg-invalid shadow-[inset_0_0_0_1px_var(--color-danger)]',
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
          stepId={stepId}
          index={index}
          method={method}
          expr={expr}
          check={check}
          frame={frame}
          onChange={onChange}
        />
      ))}
    </span>
  );
}

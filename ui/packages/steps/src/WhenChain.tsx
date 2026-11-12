/**
 * @file An expression's start drawn as `when … then … otherwise …`: one or
 * more conditions, each with the value it returns, and the value returned
 * when none is true. Everything is inline, so a condition takes one row like
 * any other step.
 */

import { Picker, cx } from '@dageist/design';

import { ConditionSlots, ValueSlot } from './ConditionSlots';
import type { Frame } from './model/frame';
import { distinctValues } from './model/frame';
import { nextId } from './model/ids';
import type { Arm, WhenSource } from './model/source';

function Keyword({ children }: { readonly children: string }): React.JSX.Element {
  return <span className="px-1 font-mono text-[12.5px] text-operator">{children}</span>;
}

/** Returns a new arm that tests the frame's first column against its first value. */
export function defaultArm(frame: Frame, returns: Arm['returns']): Arm {
  const first = frame[0]?.name ?? '';
  return {
    id: nextId('arm'),
    condition: {
      column: first,
      comparison: '==',
      value: { kind: 'literal', value: distinctValues(frame, first)[0] ?? null },
    },
    returns,
  };
}

/** What the `when` chain shows and reports. */
export interface WhenChainProps {
  readonly source: WhenSource;
  readonly frame: Frame;
  readonly onChange: (source: WhenSource) => void;
  /** Goes back to starting the expression from a column. */
  readonly onUseColumn: () => void;
}

/** Draws `source` as its conditions followed by `otherwise`. */
export function WhenChain({
  source,
  frame,
  onChange,
  onUseColumn,
}: WhenChainProps): React.JSX.Element {
  const withArm = (next: Arm): void => {
    onChange({ ...source, arms: source.arms.map((arm) => (arm.id === next.id ? next : arm)) });
  };
  return (
    <>
      {source.arms.map((arm, position) => (
        <span key={arm.id} className="inline-flex items-center">
          <Picker
            label="when"
            tone="operator"
            className="font-normal"
            choices={[
              {
                label: '✕ Remove this condition',
                current: false,
                reason: source.arms.length === 1 ? 'the only condition' : null,
                onPick: () => {
                  onChange({
                    ...source,
                    arms: source.arms.filter((existing) => existing.id !== arm.id),
                  });
                },
              },
              {
                label: 'Start from a column instead',
                current: false,
                reason: null,
                onPick: onUseColumn,
              },
            ]}
          />
          <ConditionSlots
            condition={arm.condition}
            frame={frame}
            onChange={(condition) => {
              withArm({ ...arm, condition });
            }}
          />
          <Keyword>then</Keyword>
          <ValueSlot
            value={arm.returns}
            frame={frame}
            suggestions={[]}
            label={`Value returned by condition ${String(position + 1)}`}
            onChange={(returns) => {
              withArm({ ...arm, returns });
            }}
          />
        </span>
      ))}
      <button
        type="button"
        aria-label="Add another condition"
        title="Add another condition"
        className={cx(
          'inline-flex h-[22px] w-4 cursor-pointer items-center justify-center rounded-sm text-[13px] font-bold text-muted',
          'opacity-0 group-focus-within/step:opacity-100 group-hover/step:opacity-100 hover:bg-hover hover:text-accent',
        )}
        onClick={() => {
          onChange({ ...source, arms: [...source.arms, defaultArm(frame, source.otherwise)] });
        }}
      >
        +
      </button>
      <Keyword>otherwise</Keyword>
      <ValueSlot
        value={source.otherwise}
        frame={frame}
        suggestions={[]}
        label="Value returned when no condition is true"
        onChange={(otherwise) => {
          onChange({ ...source, otherwise });
        }}
      />
    </>
  );
}

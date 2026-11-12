/**
 * @file The slots a condition is made of: a column, a comparison and a
 * value. A value is either one of the frame's columns or a literal typed in
 * place. A filter and each `when` use the same slots.
 */

import type { Choice } from '@dageist/design';
import { CommitField, Picker } from '@dageist/design';

import { columnChoices } from './choices';
import type { Cell, Frame } from './model/frame';
import { distinctValues, findColumn } from './model/frame';
import type { Condition, Value } from './model/source';
import { COMPARISONS, literalText } from './model/source';

const WHOLE_NUMBER = /^-?\d+$/;
const QUOTED = /^"(?<text>.*)"$/;

/**
 * Reads what was typed into a value's field. A whole number is a number,
 * `None` is a null, text in double quotes is a string, a column's name is
 * that column, and anything else is taken as a string.
 */
function readValue(typed: string, frame: Frame): Value {
  const text = typed.trim();
  if (WHOLE_NUMBER.test(text)) return { kind: 'literal', value: Number(text) };
  if (text === 'None') return { kind: 'literal', value: null };
  const inQuotes = QUOTED.exec(text)?.groups?.text;
  if (inQuotes !== undefined) return { kind: 'literal', value: inQuotes };
  if (findColumn(frame, text) !== null) return { kind: 'column', column: text };
  return { kind: 'literal', value: text };
}

/** What a value slot shows and reports. */
export interface ValueSlotProps {
  readonly value: Value;
  readonly frame: Frame;
  /** Literals worth offering by name, such as the values the compared column holds. */
  readonly suggestions: readonly Cell[];
  /** What the value is for, for assistive technology. */
  readonly label: string;
  readonly onChange: (value: Value) => void;
}

/**
 * Draws a value. A column is a picker; a literal is a field. Typing a
 * column's name into the field turns the value into that column.
 */
export function ValueSlot({
  value,
  frame,
  suggestions,
  label,
  onChange,
}: ValueSlotProps): React.JSX.Element {
  if (value.kind === 'literal') {
    return (
      <CommitField
        value={literalText(value.value)}
        label={label}
        numeric={false}
        code
        onCommit={(typed) => {
          onChange(readValue(typed, frame));
        }}
      />
    );
  }
  const literals: Choice[] = suggestions.map((suggestion) => ({
    label: literalText(suggestion),
    current: false,
    reason: null,
    onPick: () => {
      onChange({ kind: 'literal', value: suggestion });
    },
  }));
  return (
    <Picker
      label={value.column}
      tone="column"
      choices={[
        ...columnChoices(frame, value.column, (column) => {
          onChange({ kind: 'column', column });
        }),
        ...literals,
        {
          label: 'type a value…',
          current: false,
          reason: null,
          onPick: () => {
            onChange({ kind: 'literal', value: 0 });
          },
        },
      ]}
    />
  );
}

/** What the condition slots show and report. */
export interface ConditionSlotsProps {
  readonly condition: Condition;
  readonly frame: Frame;
  readonly onChange: (condition: Condition) => void;
}

/** Draws `condition` as a column, a comparison and a value. */
export function ConditionSlots({
  condition,
  frame,
  onChange,
}: ConditionSlotsProps): React.JSX.Element {
  return (
    <>
      <Picker
        label={condition.column}
        tone="column"
        choices={columnChoices(frame, condition.column, (column) => {
          // The old value belonged to the old column, so start from one of the new column's.
          const first = distinctValues(frame, column)[0] ?? null;
          onChange({ ...condition, column, value: { kind: 'literal', value: first } });
        })}
      />
      <Picker
        label={condition.comparison}
        tone="operator"
        choices={COMPARISONS.map((comparison) => ({
          label: comparison,
          current: comparison === condition.comparison,
          reason: null,
          onPick: () => {
            onChange({ ...condition, comparison });
          },
        }))}
      />
      <ValueSlot
        value={condition.value}
        frame={frame}
        suggestions={distinctValues(frame, condition.column)}
        label={`Value compared with ${condition.column}`}
        onChange={(value) => {
          onChange({ ...condition, value });
        }}
      />
    </>
  );
}

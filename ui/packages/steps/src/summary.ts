/**
 * @file A step said in a few words: its head, without the pieces under it.
 * This is what a list of steps shows where there is no room for the steps
 * themselves.
 */

import type { Condition, Value } from './model/source';
import { literalText } from './model/source';
import type { Step } from './model/steps';
import { VERB_LABELS } from './verbs';

function valueSummary(value: Value): string {
  switch (value.kind) {
    case 'literal':
      return literalText(value.value);
    case 'column':
      return value.column;
    case 'input':
      return value.name;
    case 'chain':
      return '…';
    case 'unset':
      return 'value';
    default:
      return '';
  }
}

function conditionSummary(condition: Condition): string {
  if (condition.kind === 'group') {
    return `${condition.every ? 'all of' : 'any of'} ${String(condition.items.length)} conditions`;
  }
  const field = condition.column === '' ? 'field' : condition.column;
  return `${field} ${condition.comparison} ${valueSummary(condition.value)}`;
}

/** Returns `names` as a short list, or how many there are when they are too many to list. */
function fieldList(names: readonly string[]): string {
  if (names.length === 0) return 'no fields';
  return names.length <= 3 ? names.join(', ') : `${String(names.length)} fields`;
}

/** Returns what `step` works on, as the words that follow its name. */
export function stepDetail(step: Step): string {
  switch (step.verb) {
    case 'with_columns':
      return step.expr.alias;
    case 'filter':
      return conditionSummary(step.condition);
    case 'sort':
      return `by ${fieldList(step.by.map((key) => key.column))}`;
    case 'select':
      return fieldList(step.columns);
    case 'rename':
      return step.names.length <= 2 && step.names.length > 0
        ? step.names.map(({ from, to }) => `${from} → ${to}`).join(', ')
        : fieldList(step.names.map((naming) => naming.from));
    case 'join':
      return `${step.table} on ${step.leftOn}`;
    case 'pivot':
      return `on ${step.on}`;
    default:
      return '';
  }
}

/** Returns `step` as one phrase, such as "Sort table by dcn, timestamp". */
export function stepTitle(step: Step): string {
  return `${VERB_LABELS[step.verb]} ${stepDetail(step)}`;
}

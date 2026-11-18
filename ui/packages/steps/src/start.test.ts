import { describe, expect, it } from 'vitest';

import { decodeReadings } from '@dageist/host';

import typedSampleValues from '../../host/fixtures/typed-sample-values.json';
import { START } from './start';
import type { Chain, Condition, Value } from './model/source';

/** The numbers and text that `value` holds, and what it holds inside, which are literals of a chain. */
function literalsOf(value: Value): Array<Extract<Value, { kind: 'literal' }>> {
  switch (value.kind) {
    case 'literal':
      return [value];
    case 'chain':
      return literalsInChain(value.chain);
    default:
      return [];
  }
}

function literalsInCondition(condition: Condition): Array<Extract<Value, { kind: 'literal' }>> {
  switch (condition.kind) {
    case 'test':
      return literalsOf(condition.value);
    case 'group':
      return condition.items.flatMap(literalsInCondition);
    case 'chain':
      return literalsInChain(condition.chain);
    default:
      return [];
  }
}

function literalsInChain(chain: Chain): Array<Extract<Value, { kind: 'literal' }>> {
  const start =
    chain.source.kind === 'when'
      ? [
          ...chain.source.arms.flatMap((arm) => [
            ...literalsInCondition(arm.condition),
            ...literalsOf(arm.returns),
          ]),
          ...literalsOf(chain.source.otherwise),
        ]
      : [];
  const arguments_ = chain.methods.flatMap((method) =>
    method.argument === null ? [] : literalsOf(method.argument),
  );
  return [...start, ...arguments_];
}

/** Every number and text the starting steps hold. */
const literals = START.flatMap((step) => {
  if (step.verb === 'with_columns') return literalsOf(step.value);
  if (step.verb === 'filter') return literalsInCondition(step.condition);
  return [];
});

describe('the numbers and text the editor opens with', () => {
  it('are held as the engine reads them and shows them', () => {
    // The engine read each typed into an expression, as a number first where it is one.
    const readings = new Map(
      Object.entries(typedSampleValues).map(([text, answer]) => [
        text,
        decodeReadings(answer).readings.flatMap((reading) =>
          reading.kind === 'read' ? [reading.literal] : [],
        ),
      ]),
    );
    expect(literals.length).toBeGreaterThan(0);
    for (const literal of literals) {
      if (literal.value === null) throw new Error('a starting step holds no None');
      const text = String(literal.value);
      const read = readings.get(text);
      expect(read, `the engine read ${text}`).toBeDefined();
      const kind = typeof literal.value === 'string' ? 'text' : 'integer';
      expect(read?.find((candidate) => candidate.kind === kind)).toMatchObject({
        value: literal.value,
        shown: literal.shown,
      });
    }
  });
});

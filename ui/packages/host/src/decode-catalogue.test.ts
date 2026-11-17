import { describe, expect, it } from 'vitest';

import { DecodeError } from './decode';
import { decodeCatalogue } from './decode-catalogue';
import catalogueFixture from '../fixtures/catalogue.json';
import { at, replaced, withField, without } from './tamper';

// The fixture is the host's catalogue, written from Rust by its snapshot test. It is read as
// `unknown`, as the host's answer is, and the decoder is what says what it is.
const catalogue: unknown = catalogueFixture;

/** The entry `id` of `value`, which is not decoded: where it is among the entries. */
function entryAt(value: unknown, id: string): number {
  const entries = at(value, ['entries']);
  if (!Array.isArray(entries)) throw new Error('no entries');
  const found = entries.findIndex(
    (entry: unknown) =>
      typeof entry === 'object' && entry !== null && 'id' in entry && entry.id === id,
  );
  if (found < 0) throw new Error(`no entry ${id}`);
  return found;
}

/** The path to the parameter `name` of the entry `id`. */
function parameterPath(value: unknown, id: string, name: string): Array<string | number> {
  const entry = entryAt(value, id);
  const parameters = at(value, ['entries', entry, 'parameters']);
  if (!Array.isArray(parameters)) throw new Error('no parameters');
  const found = parameters.findIndex(
    (parameter: unknown) =>
      typeof parameter === 'object' &&
      parameter !== null &&
      'name' in parameter &&
      parameter.name === name,
  );
  if (found < 0) throw new Error(`no parameter ${name} of ${id}`);
  return ['entries', entry, 'parameters', found];
}

function refusal(value: unknown): DecodeError {
  try {
    decodeCatalogue(value);
  } catch (error) {
    if (error instanceof DecodeError) return error;
    throw error;
  }
  throw new Error('the value was not refused');
}

describe('the catalogue the host sends', () => {
  it('is read with its families in the order they are offered', () => {
    const decoded = decodeCatalogue(catalogue);
    expect(decoded.families.map((family) => family.key)).toEqual([
      'steps',
      'rows',
      'cumulative',
      'arithmetic',
      'aggregate',
      'temporal',
      'window',
      'compare',
      'start',
      'condition',
    ]);
    expect(decoded.dtypes[0]).toEqual({ key: 'any', title: 'any' });
  });

  it('is read with the parameters of an entry as Polars gives them', () => {
    // `Expr.shift(n=1, *, fill_value=None)`.
    const shift = decodeCatalogue(catalogue).entries.find((entry) => entry.id === 'Expr.shift');
    expect(shift).toEqual({
      id: 'Expr.shift',
      applies: 'expression',
      family: 'rows',
      name: 'shift',
      hint: 'value n rows earlier',
      head: null,
      accepts: ['any'],
      lazy: true,
      parameters: [
        {
          name: 'n',
          shown: 'n',
          label: 'rows',
          kind: { kind: 'expression', dtypes: ['integer'] },
          shape: { shape: 'one' },
          default: { kind: 'integer', value: 1 },
          python: '1',
        },
        {
          name: 'fill_value',
          shown: 'fill_value',
          label: 'fill with',
          kind: { kind: 'expression', dtypes: ['any'] },
          shape: { shape: 'one' },
          default: { kind: 'none' },
          python: 'None',
        },
      ],
    });
  });

  it('is read with a choice, a per-item shape and the kinds of a pair', () => {
    const entries = decodeCatalogue(catalogue).entries;
    const parameter = (id: string, name: string) =>
      entries.find((entry) => entry.id === id)?.parameters.find((found) => found.name === name);
    expect(parameter('LazyFrame.join', 'how')?.kind).toEqual({
      kind: 'choice',
      words: ['inner', 'left', 'right', 'full', 'semi', 'anti', 'cross'].map((word) => ({
        word,
        shown: word,
      })),
    });
    expect(parameter('LazyFrame.sort', 'descending')?.shape).toEqual({
      shape: 'perItem',
      of: 'by',
    });
    expect(parameter('LazyFrame.with_columns', 'named_exprs')?.kind).toEqual({
      kind: 'pairs',
      key: { kind: 'column' },
      value: { kind: 'expression', dtypes: ['any'] },
    });
    // `LazyFrame.join(other, ...)` has no default, which Python writes as nothing.
    expect(parameter('LazyFrame.join', 'other')).toMatchObject({
      kind: { kind: 'table' },
      default: { kind: 'required' },
      python: null,
    });
  });

  it('is refused where an entry names an applies the engine does not have', () => {
    const at0 = entryAt(catalogue, 'Expr.shift');
    expect(refusal(replaced(catalogue, ['entries', at0, 'applies'], 'method')).message).toBe(
      `$.entries[${String(at0)}].applies: must be one of "expression", "frame", "function", "condition", not "method"`,
    );
  });

  it('is refused where a parameter has a kind the view does not draw', () => {
    const shift = entryAt(catalogue, 'Expr.shift');
    const refused = refusal(
      replaced(catalogue, ['entries', shift, 'parameters', 0, 'kind'], { kind: 'map' }),
    );
    expect(refused.path).toBe(`$.entries[${String(shift)}].parameters[0].kind.kind`);
    expect(refused.problem).toBe(
      'must be one of "expression", "column", "value", "flag", "choice", "dtype", "duration", "table", "path", "pairs", not "map"',
    );
  });

  it('is refused where the kind of a pair is wrong inside the pair', () => {
    const named = parameterPath(catalogue, 'LazyFrame.with_columns', 'named_exprs');
    const found = refusal(
      replaced(catalogue, [...named, 'kind', 'key'], { kind: 'column', extra: 1 }),
    );
    expect(found.path).toBe(
      `$.entries[${String(named[1])}].parameters[${String(named[3])}].kind.key`,
    );
    expect(found.problem).toBe('has the field "extra", which the contract lacks');
  });

  it('is refused where a field is missing, has another kind or is one the contract lacks', () => {
    const shift = entryAt(catalogue, 'Expr.shift');
    const base = ['entries', shift];
    expect(refusal(without(catalogue, base, 'hint')).message).toBe(
      `$.entries[${String(shift)}]: is missing the field "hint"`,
    );
    expect(refusal(replaced(catalogue, [...base, 'lazy'], 'yes')).message).toBe(
      `$.entries[${String(shift)}].lazy: must be a boolean, not a string`,
    );
    expect(refusal(withField(catalogue, [], 'version', 1)).message).toBe(
      '$: has the field "version", which the contract lacks',
    );
    expect(refusal(replaced(catalogue, ['entries'], {})).message).toBe(
      '$.entries: must be a list, not an object',
    );
  });

  it('is refused where a default carries a value of another kind', () => {
    const shift = entryAt(catalogue, 'Expr.shift');
    const found = refusal(
      replaced(catalogue, ['entries', shift, 'parameters', 0, 'default'], {
        kind: 'integer',
        value: 1.5,
      }),
    );
    expect(found.problem).toBe('must be a whole number, not 1.5');
  });
});

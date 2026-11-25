/**
 * @file What the editor asks the host's engine to put into a document, said as the arguments of
 * its edits. These are not the document's JSON: the engine reads each into the typed model and
 * checks it. Which kind of argument a parameter takes is the catalogue's, so what is here only
 * names the one the person's pick is for.
 */

import type {
  ExpressionRequest,
  KindView,
  LiteralRequest,
  LiteralView,
  StartRequest,
  ValueRequest,
} from '@dageist/host';

/** An expression with nothing picked yet, as an argument. */
export const HOLE: ValueRequest = { kind: 'expression', expression: { kind: 'hole' } };

/** Returns `expression` as an argument. */
export function expressionArgument(expression: ExpressionRequest): ValueRequest {
  return { kind: 'expression', expression };
}

/** Returns the expression that reads the field `name`, as an argument. */
export function fieldArgument(name: string): ValueRequest {
  return expressionArgument({ kind: 'field', name });
}

/** Returns the expression that is the connected input `name`, as an argument. */
export function inputArgument(name: string): ValueRequest {
  return expressionArgument({ kind: 'input', name });
}

/** Returns what a number or text the engine read is, as the one word of an expression. */
function literalOf(literal: LiteralView): LiteralRequest {
  switch (literal.kind) {
    case 'integer':
      return { kind: 'integer', value: literal.value };
    case 'float':
      return { kind: 'float', value: literal.value };
    case 'text':
      return { kind: 'text', value: literal.value };
    default:
      return literal satisfies never;
  }
}

/** Returns the expression that is `literal`, as an argument. */
export function literalExpression(literal: LiteralView): ValueRequest {
  return expressionArgument({ kind: 'literal', literal: literalOf(literal) });
}

/** Returns what an expression starts from where it starts from `literal`, typed in, as an edit puts it where the start was. */
export function literalStart(literal: LiteralView): StartRequest {
  return { kind: 'literal', literal: literalOf(literal) };
}

/** Returns the text a literal holds, whichever kind it is. */
function textOf(literal: LiteralView): string {
  return literal.kind === 'text' ? literal.value : literal.shown;
}

/**
 * Returns `literal` as the argument of a parameter of `kind`: a number for a parameter that takes
 * a number of its type, the text as a duration, a pattern, a path or a time zone where the kind is
 * one, and as the expression of one word where the parameter takes an expression. It is `null`
 * where a number is wanted and the literal holds none, text, or a number that is not one: there
 * is nothing to send, and the slot is left as it was.
 */
export function literalArgument(kind: KindView, literal: LiteralView): ValueRequest | null {
  const number = literal.kind === 'text' ? Number.NaN : literal.value;
  switch (kind.kind) {
    case 'expression':
      return literalExpression(literal);
    case 'value':
      if (kind.type === 'string') return { kind: 'string', value: textOf(literal) };
      // Text holds no number for a parameter that takes one, and a number that is not one is
      // nothing the engine could read.
      if (kind.type === 'integer' || kind.type === 'float') {
        if (!Number.isFinite(number)) return null;
        return { kind: kind.type, value: number };
      }
      // A typed value of any type, as `lit` takes: its own kind.
      if (literal.kind === 'text') return { kind: 'string', value: literal.value };
      if (!Number.isFinite(number)) return null;
      return literal.kind === 'float' || !Number.isSafeInteger(number)
        ? { kind: 'float', value: number }
        : { kind: 'integer', value: number };
    case 'duration':
      return { kind: 'duration', text: textOf(literal) };
    case 'pattern':
      return { kind: 'pattern', text: textOf(literal) };
    case 'timeZone':
      return { kind: 'timeZone', zone: textOf(literal) };
    case 'path':
      return { kind: 'path', text: textOf(literal) };
    case 'field':
      return { kind: 'field', name: textOf(literal) };
    default:
      return { kind: 'string', value: textOf(literal) };
  }
}

/**
 * Returns the word `word` as the argument of a parameter of `kind`: a choice's word, a dtype, a
 * column or a table by its name, or, for a time zone that may be none, none.
 */
export function wordArgument(kind: KindView, word: string): ValueRequest {
  switch (kind.kind) {
    case 'choice':
      return { kind: 'choice', word };
    case 'dtype':
      return { kind: 'dtype', name: word };
    case 'column':
      return { kind: 'columns', names: [word] };
    case 'field':
      return { kind: 'field', name: word };
    case 'table':
      return { kind: 'table', name: word };
    case 'timeZone':
      return { kind: 'timeZone', zone: null };
    default:
      return { kind: 'string', value: word };
  }
}

/** Returns the flag `value` as an argument. */
export function flagArgument(value: boolean): ValueRequest {
  return { kind: 'flag', value };
}

/** Returns the list of `items` as an argument. */
export function listArgument(items: readonly ValueRequest[]): ValueRequest {
  return { kind: 'many', items };
}

/** Returns `request` as the one item of a list, for a parameter that holds a list. */
export function listOfOne(request: ValueRequest): ValueRequest {
  return listArgument([request]);
}

/** Whether a parameter of `kind` has an argument that stands for nothing yet: only an expression does. */
export function canBeLeftUnpicked(kind: KindView): boolean {
  return kind.kind === 'expression';
}

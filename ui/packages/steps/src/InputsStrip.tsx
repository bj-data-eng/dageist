/**
 * @file The strip that shows what the node is connected to. These inputs are
 * everything the steps can refer to besides the columns they make: there are
 * no variables.
 */

import { Hint } from '@dageist/design';

import { formatCell } from './format';
import type { Input } from './model/inputs';
import { INPUTS } from './model/inputs';

/** Returns the short description shown beside an input's name. */
function detail(input: Input): string {
  switch (input.kind) {
    case 'table':
      return `${String(input.frame[0]?.cells.length ?? 0)} × ${String(input.frame.length)}`;
    case 'value':
      return input.dtype;
    case 'list':
      return `list of ${input.dtype}`;
    default:
      return '';
  }
}

/** Returns the lines of an input's hover note: where it comes from and what it holds. */
export function inputLines(input: Input): string[] {
  const from = `from ${input.from}`;
  switch (input.kind) {
    case 'table':
      return [
        `${input.name} · table${input.primary ? ' · the steps start from its rows' : ''}`,
        from,
        ...input.frame.map((column) => `${column.name} · ${column.dtype}`),
      ];
    case 'value':
      return [`${input.name} · ${input.dtype}`, from, formatCell(input.value, input.dtype)];
    case 'list':
      return [
        `${input.name} · list of ${input.dtype}`,
        from,
        input.values.map((value) => formatCell(value, input.dtype)).join(', '),
      ];
    default:
      return [];
  }
}

/** Lists the node's inputs, each with its kind and dtype. */
export function InputsStrip(): React.JSX.Element {
  return (
    <div className="dg-ground mb-3 flex flex-wrap items-center gap-1.5 text-xs">
      <span className="pr-1 font-semibold tracking-wide text-muted">Inputs</span>
      {INPUTS.map((input) => (
        <Hint key={input.name} lines={inputLines(input)} code={false}>
          <span className="inline-flex items-center gap-px">
            <span
              data-tone={input.kind === 'table' ? 'table' : 'input'}
              className="dg-block dg-fixed"
            >
              {input.name}
            </span>
            <span data-tone="ink" className="dg-block dg-fixed text-[11px] opacity-70">
              {detail(input)}
            </span>
          </span>
        </Hint>
      ))}
    </div>
  );
}

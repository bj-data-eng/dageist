/**
 * @file The strip that shows what the node is connected to. These inputs are
 * everything the steps can refer to besides the columns they make: there are
 * no variables. The host reports them; the view keeps none.
 */

import { Peek } from '@dageist/design';
import type { InputView, NodeView } from '@dageist/host';

import { useNode } from './EngineProvider';

/** Returns the short description shown beside an input's name. */
function detail(input: InputView): string {
  switch (input.kind) {
    case 'table':
      return `${String(input.columns.length)} columns`;
    case 'value':
      return input.dtype;
    case 'list':
      return `list of ${input.dtype}`;
    default:
      return '';
  }
}

/** Returns the lines that describe an input: what it is and what it holds. */
export function inputLines(input: InputView): string[] {
  switch (input.kind) {
    case 'table':
      return [
        `${input.name} · table`,
        ...input.columns.map((column) => `${column.name} · ${column.dtype}`),
      ];
    case 'value':
      return [`${input.name} · ${input.dtype}`];
    case 'list':
      return [`${input.name} · list of ${input.dtype}`];
    default:
      return [];
  }
}

/** Returns the lines that describe the table the steps start from. */
function sourceLines(node: NodeView): string[] {
  return [
    `${node.source.name} · table · the steps start from its rows`,
    ...node.source.columns.map((column) => `${column.name} · ${column.dtype}`),
  ];
}

/** Lists the node's inputs, each with its kind and dtype. */
export function InputsStrip(): React.JSX.Element {
  const node = useNode();
  return (
    <div className="dg-ground mb-3 flex flex-wrap items-center gap-1.5 text-xs">
      <span className="pr-1 font-semibold tracking-wide text-muted">Inputs</span>
      <Peek
        describedAs={`Input ${node.source.name}: show what it holds`}
        lines={sourceLines(node)}
        code={false}
      >
        <span data-tone="table" className="text-(--tone)">
          {node.source.name}
        </span>{' '}
        <span className="text-[11px] text-muted">{`${String(node.source.columns.length)} columns`}</span>
      </Peek>
      {node.inputs.map((input) => (
        <Peek
          key={input.name}
          describedAs={`Input ${input.name}: show what it holds`}
          lines={inputLines(input)}
          code={false}
        >
          <span data-tone={input.kind === 'table' ? 'table' : 'input'} className="text-(--tone)">
            {input.name}
          </span>{' '}
          <span className="text-[11px] text-muted">{detail(input)}</span>
        </Peek>
      ))}
    </div>
  );
}

/**
 * @file The boxes the canvas draws: a supplier with its outputs on the right
 * edge, and the node with its inputs on the left edge and its steps below
 * them. Text the host does not report yet, such as a node's steps, is drawn
 * as stand-ins of the size it will take.
 */

import { Handle, Position } from '@xyflow/react';
import type { Node, NodeProps } from '@xyflow/react';

import { FLOATING_SURFACE, Skeleton, cx } from '@dageist/design';

import type { Input, Output, Supplier } from './graph';

// React Flow requires a node's data to be an object type that has an index
// signature, which an interface does not, so these are type aliases.

/** A supplier's box, as React Flow holds it. */
export type SupplierNode = Node<{ readonly supplier: Supplier }, 'supplier'>;

/** The node's own box, as React Flow holds it. */
export type StepsNode = Node<{ readonly inputs: readonly Input[] }, 'steps'>;

/** The frame every box is drawn in. */
function Frame({
  mark,
  title,
  wide,
  children,
}: {
  readonly mark: string;
  readonly title: React.ReactNode;
  readonly wide: boolean;
  readonly children: React.ReactNode;
}): React.JSX.Element {
  return (
    <article className={cx(FLOATING_SURFACE, 'text-[13px]', wide ? 'w-[320px]' : 'w-[280px]')}>
      <header className="flex h-11 items-center gap-2.5 border-b border-line px-3.5">
        <span aria-hidden="true" className="text-base text-accent">
          {mark}
        </span>
        <h2 className="m-0 min-w-0 flex-1 truncate text-[14px] font-semibold">{title}</h2>
      </header>
      {children}
    </article>
  );
}

/** A name and its dtype on one line, as code is written. */
function Field({
  name,
  dtype,
  ink,
}: {
  readonly name: string;
  readonly dtype: string;
  readonly ink: string;
}): React.JSX.Element {
  return (
    <span className="flex min-w-0 items-baseline gap-2 font-mono text-[12px]">
      <span className={cx('truncate', ink)}>{name}</span>
      <span className="shrink-0 text-dtype">{dtype}</span>
    </span>
  );
}

/** One output of a supplier, with its pin on the box's right edge. */
function OutputRow({ output }: { readonly output: Output }): React.JSX.Element {
  const { gives } = output;
  return (
    <li className="relative">
      <div className="flex h-[22px] items-center px-3.5">
        {gives.kind === 'table' && <span className="text-muted">table</span>}
        {gives.kind === 'column' && (
          <Field name={gives.column} dtype={gives.dtype} ink="text-column-ink" />
        )}
        {gives.kind === 'parameter' && (
          <span className="flex min-w-0 items-baseline gap-2">
            <Field name={gives.name} dtype={gives.dtype} ink="text-input-ink" />
            <span className="truncate font-mono text-[12px] text-value">{gives.value}</span>
          </span>
        )}
      </div>
      <Handle type="source" position={Position.Right} id={output.id} isConnectable={false} />
      {gives.kind === 'table' && (
        <ul className="m-0 list-none p-0 pb-1 pl-6">
          {gives.columns.map((column) => (
            <li key={column.name} className="flex h-[22px] items-center pr-3.5">
              <Field name={column.name} dtype={column.dtype} ink="text-column-ink" />
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

/**
 * Stand-ins for a node's steps: one line for each, a verb and what it is
 * given. Each line's width class tells it apart, so no two may share one.
 */
function StepStandIns({ lines }: { readonly lines: readonly string[] }): React.JSX.Element {
  return (
    <div className="flex flex-col gap-2 px-3.5 py-3">
      <span className="sr-only">Steps not loaded</span>
      {lines.map((width) => (
        <div key={width} className="flex items-center gap-2">
          <Skeleton className="h-3.5 w-12 shrink-0" />
          <Skeleton className={cx('h-3.5', width)} />
        </div>
      ))}
    </div>
  );
}

/** Draws a supplier: another node, or the graph's parameters. */
export function SupplierBox({ data }: NodeProps<SupplierNode>): React.JSX.Element {
  const { supplier } = data;
  return (
    <Frame mark={supplier.kind === 'parameters' ? '≔' : '▦'} title={supplier.title} wide={false}>
      <ul className="m-0 list-none p-0 py-1.5">
        {supplier.outputs.map((output) => (
          <OutputRow key={output.id} output={output} />
        ))}
      </ul>
      {supplier.kind === 'node' && (
        <div className="border-t border-line">
          <StepStandIns lines={['w-[62%]', 'w-[44%]']} />
        </div>
      )}
    </Frame>
  );
}

/** Draws the node: its inputs, each with a pin on the box's left edge, then its steps. */
export function StepsBox({ data }: NodeProps<StepsNode>): React.JSX.Element {
  return (
    <Frame mark="ƒx" title={<Skeleton className="h-4 w-36" />} wide>
      <ul className="m-0 list-none p-0 py-1.5">
        {data.inputs.map((input) => (
          <li key={input.name} className="relative flex h-[22px] items-center px-3.5">
            <Handle type="target" position={Position.Left} id={input.name} isConnectable={false} />
            <Field name={input.name} dtype={input.takes} ink="text-input-ink" />
          </li>
        ))}
      </ul>
      <div className="border-t border-line">
        <StepStandIns
          lines={['w-[70%]', 'w-[52%]', 'w-[84%]', 'w-[40%]', 'w-[64%]', 'w-[58%]', 'w-[76%]']}
        />
      </div>
    </Frame>
  );
}

/**
 * @file The boxes the canvas draws, built on React Flow UI's node and pins: a
 * supplier with its outputs on the right edge, and the node with its inputs
 * on the left edge, its steps in a list that scrolls, a button above it for a
 * trigger and one beside it for a following node. A table's fields are listed
 * under a header that folds them away. The node can be resized by any edge or corner. Text
 * the host does not report yet, such as the node's steps and name, is drawn as
 * stand-ins of the size it will take.
 */

import { NodeResizer, Position } from '@xyflow/react';
import type { Node, NodeProps } from '@xyflow/react';

import { Skeleton, cn } from '@dageist/design';

import { FieldList } from './FieldList';
import type { Input, Output, Supplier } from './graph';
import {
  BaseNode,
  BaseNodeContent,
  BaseNodeFooter,
  BaseNodeHeader,
  BaseNodeHeaderTitle,
} from './react-flow-ui/base-node';
import { ButtonHandle } from './react-flow-ui/button-handle';
import { LabeledHandle } from './react-flow-ui/labeled-handle';

// React Flow requires a node's data to be an object type that has an index
// signature, which an interface does not, so these are type aliases.

/** A supplier's box, as React Flow holds it. */
export type SupplierNode = Node<{ readonly supplier: Supplier }, 'supplier'>;

/** The node's own box, as React Flow holds it. */
export type StepsNode = Node<{ readonly inputs: readonly Input[] }, 'steps'>;

/** The least size the node's box can be resized to. */
export const STEPS_LEAST = { width: 300, height: 300 } as const;

/** The stand-in lines for a node's steps, each a width class, enough to need scrolling. */
const STEP_STAND_INS = [
  'w-[62%]',
  'w-[48%]',
  'w-[80%]',
  'w-[36%]',
  'w-[70%]',
  'w-[54%]',
  'w-[76%]',
  'w-[44%]',
  'w-[66%]',
  'w-[58%]',
  'w-[84%]',
  'w-[40%]',
  'w-[72%]',
  'w-[50%]',
];

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
      <span className={cn('truncate', ink)}>{name}</span>
      <span className="shrink-0 text-dtype">{dtype}</span>
    </span>
  );
}

/** Returns what an output says beside its pin. */
function outputLabel(output: Output): React.ReactNode {
  const { gives } = output;
  switch (gives.kind) {
    case 'table':
      return gives.columns.length === 0 ? (
        <span className="text-muted">table</span>
      ) : (
        <FieldList fields={gives.columns} />
      );
    case 'column':
      return <Field name={gives.column} dtype={gives.dtype} ink="text-column-ink" />;
    case 'parameter':
      return (
        <span className="flex min-w-0 items-baseline gap-2">
          <Field name={gives.name} dtype={gives.dtype} ink="text-input-ink" />
          <span className="truncate font-mono text-[12px] text-value">{gives.value}</span>
        </span>
      );
    default:
      return gives satisfies never;
  }
}

/** Returns the name a screen reader gives an output's pin. */
function outputTitle(output: Output): string {
  const { gives } = output;
  switch (gives.kind) {
    case 'table':
      return `table of ${String(gives.columns.length)} columns`;
    case 'column':
      return `column ${gives.column}`;
    case 'parameter':
      return `parameter ${gives.name}`;
    default:
      return gives satisfies never;
  }
}

/** Stand-ins for a list of steps: one line each, a verb and what it is given. */
function StepStandIns({ lines }: { readonly lines: readonly string[] }): React.JSX.Element {
  return (
    <div className="flex flex-col gap-2.5">
      {lines.map((width) => (
        <div key={width} className="flex items-center gap-2">
          <Skeleton className="h-3.5 w-12 shrink-0" />
          <Skeleton className={cn('h-3.5', width)} />
        </div>
      ))}
    </div>
  );
}

/**
 * A small round button on the end of a pin's stalk, where something will be
 * added. Adding needs the engine, so the button is shown disabled.
 */
function AddButton({
  describedAs,
  label,
}: {
  readonly describedAs: string;
  readonly label: string;
}): React.JSX.Element {
  return (
    <button
      type="button"
      aria-label={describedAs}
      disabled
      className="flex h-6 items-center gap-1 rounded-full border border-line-strong bg-panel px-2 font-mono text-[11px] text-muted"
    >
      <span aria-hidden="true">+</span>
      {label}
    </button>
  );
}

/** Draws a supplier: another node, or the graph's parameters. */
export function SupplierBox({ data }: NodeProps<SupplierNode>): React.JSX.Element {
  const { supplier } = data;
  return (
    <BaseNode className="w-[280px] text-[13px]">
      <BaseNodeHeader>
        <span aria-hidden="true" className="text-accent">
          {supplier.kind === 'parameters' ? '≔' : '▦'}
        </span>
        <BaseNodeHeaderTitle>{supplier.title}</BaseNodeHeaderTitle>
      </BaseNodeHeader>
      <BaseNodeContent className="gap-y-0.5 px-0 py-2">
        {supplier.outputs.map((output) => (
          <LabeledHandle
            key={output.id}
            type="source"
            position={Position.Right}
            id={output.id}
            isConnectable={false}
            title={outputTitle(output)}
            label={outputLabel(output)}
            className="min-h-[22px] pl-3"
            // A table's pin stays level with its first line, the header of its fields, however
            // many lines open beneath it. React Flow's own stylesheet places pins, so this wins
            // over it.
            handleClassName={output.gives.kind === 'table' ? 'top-[11px]!' : ''}
            labelClassName="flex-1 px-0 pr-3"
          />
        ))}
      </BaseNodeContent>
      {supplier.kind === 'node' && (
        <BaseNodeFooter className="items-stretch pt-2.5">
          <StepStandIns lines={['w-[62%]', 'w-[44%]']} />
        </BaseNodeFooter>
      )}
    </BaseNode>
  );
}

/**
 * Draws the node: a pin above for a trigger, its inputs each with a pin on
 * the left edge, its steps in a list that scrolls within the box, and a pin
 * beside it for a following node. Its edges and corners resize it, and the
 * list of steps takes whatever height is left.
 */
export function StepsBox({ data }: NodeProps<StepsNode>): React.JSX.Element {
  return (
    <>
      {/* Always there, so an edge can be grabbed without selecting the box first. */}
      <NodeResizer isVisible minWidth={STEPS_LEAST.width} minHeight={STEPS_LEAST.height} />
      <ButtonHandle type="target" position={Position.Top} id="trigger" isConnectable={false}>
        <AddButton describedAs="Add a trigger" label="trigger" />
      </ButtonHandle>
      <BaseNode className="text-[13px]">
        <BaseNodeHeader>
          <span aria-hidden="true" className="text-accent">
            ƒx
          </span>
          <BaseNodeHeaderTitle>
            <span className="sr-only">Node</span>
            <Skeleton className="h-4 w-36" />
          </BaseNodeHeaderTitle>
        </BaseNodeHeader>
        <BaseNodeContent className="shrink-0 gap-y-0.5 px-0 py-2">
          {data.inputs.map((input) => (
            <LabeledHandle
              key={input.name}
              type="target"
              position={Position.Left}
              id={input.name}
              isConnectable={false}
              title={input.name}
              label={<Field name={input.name} dtype={input.takes} ink="text-input-ink" />}
              className="min-h-[22px]"
            />
          ))}
        </BaseNodeContent>
        {/* `nowheel` and `nodrag` let the list scroll under the wheel and by its bar, and not move the canvas or the box. */}
        <div className="nowheel nodrag min-h-0 flex-1 overflow-y-auto border-t px-3 py-3">
          <StepStandIns lines={STEP_STAND_INS} />
        </div>
        <BaseNodeFooter className="flex-row justify-between py-2">
          <Skeleton className="h-3 w-20" />
          <Skeleton className="h-3 w-16" />
        </BaseNodeFooter>
      </BaseNode>
      <ButtonHandle type="source" position={Position.Right} id="next" isConnectable={false}>
        <AddButton describedAs="Add a following node" label="node" />
      </ButtonHandle>
    </>
  );
}

/**
 * @file The box each node of the graph is drawn as, built on React Flow UI's
 * node and pins. Its slots have pins on the left edge and its outputs on the
 * right; its header names it, and double-clicking the name renames it. What
 * the host reported of it is shown as it came: what it is set to, the columns
 * it gives, its values and what it still lacks. Its buttons ask the canvas for
 * an edit: a file chosen, a node added after it, a value set, a list read from
 * a column, a slot emptied, the node removed, or a transform's steps opened.
 * Any edge or corner resizes it.
 */

import { NodeResizer, Position } from '@xyflow/react';
import type { Node, NodeProps } from '@xyflow/react';
import { useState } from 'react';

import { Popover, cn } from '@dageist/design';
import type { NodeKind, SlotView } from '@dageist/host';

import { useGraphActions } from './actions';
import { FieldList } from './FieldList';
import type { BoxOutput, BoxSlot, CanvasBox } from './graph';
import { NEW_INPUT_PIN } from './graph';
import {
  BaseNode,
  BaseNodeContent,
  BaseNodeFooter,
  BaseNodeHeader,
  BaseNodeHeaderTitle,
} from './react-flow-ui/base-node';
import { LabeledHandle } from './react-flow-ui/labeled-handle';

// React Flow requires a node's data to be an object type that has an index
// signature, which an interface does not, so this is a type alias.

/** A node's box, as React Flow holds it. */
export type GraphBoxNode = Node<{ readonly box: CanvasBox }, 'box'>;

/** The least size a box can be resized to. */
export const BOX_LEAST = { width: 260, height: 120 } as const;

/** The glyph beside a node's name, by its kind. */
const GLYPHS: Readonly<Record<NodeKind, string>> = {
  source: '▦',
  transform: 'ƒx',
  output: '⇥',
  parameters: '≔',
};

/** A small button in a box, which neither drags the box nor pans the canvas. */
function BoxButton({
  label,
  onPress,
  title,
  className,
  ref,
}: {
  readonly label: React.ReactNode;
  readonly onPress: () => void;
  readonly title: string;
  readonly className?: string;
  readonly ref?: React.Ref<HTMLButtonElement>;
}): React.JSX.Element {
  return (
    <button
      ref={ref}
      type="button"
      title={title}
      aria-label={title}
      onClick={onPress}
      className={cn(
        'nodrag cursor-pointer rounded-md border border-line-strong bg-panel px-2 py-0.5 font-mono text-[11px] text-muted hover:bg-hover hover:text-ink',
        className,
      )}
    >
      {label}
    </button>
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
      <span className={cn('truncate', ink)}>{name}</span>
      <span className="shrink-0 text-dtype">{dtype}</span>
    </span>
  );
}

/** The node's name, which double-clicking turns into a field that renames it. */
function Name({ box }: { readonly box: CanvasBox }): React.JSX.Element {
  const { rename } = useGraphActions();
  const [editing, setEditing] = useState(false);
  if (!editing) {
    return (
      <BaseNodeHeaderTitle
        title="Double-click to rename"
        onDoubleClick={() => {
          setEditing(true);
        }}
      >
        {box.name}
      </BaseNodeHeaderTitle>
    );
  }
  const finish = (field: HTMLInputElement, keep: boolean): void => {
    setEditing(false);
    const name = field.value.trim();
    if (keep && name !== '' && name !== box.name) rename(box.id, name);
  };
  return (
    <input
      // The field is shown because the name was double-clicked, so it takes the keys at once.
      // oxlint-disable-next-line jsx-a11y/no-autofocus
      autoFocus
      aria-label="Name"
      defaultValue={box.name}
      // The whole name is selected, so that typing replaces it.
      onFocus={(event) => {
        event.currentTarget.select();
      }}
      className="nodrag min-w-0 flex-1 rounded border border-line-strong bg-canvas px-1 text-[13px] font-semibold text-ink outline-none"
      onBlur={(event) => {
        finish(event.currentTarget, true);
      }}
      onKeyDown={(event) => {
        if (event.key === 'Enter') finish(event.currentTarget, true);
        if (event.key === 'Escape') finish(event.currentTarget, false);
      }}
    />
  );
}

/** What fills a slot, in words: the node it reads, and the column or value. */
function fromText(slot: BoxSlot, nameOf: (node: string) => string): string {
  const { from } = slot;
  if (from === null) return 'not connected';
  switch (from.kind) {
    case 'table':
      return nameOf(from.node);
    case 'column':
      return `${nameOf(from.node)} · ${from.column}`;
    case 'parameter':
      return `${nameOf(from.node)} · ${from.name}`;
    default:
      return from satisfies never;
  }
}

/** The columns a list slot may be read from, each table that fits it by its columns. */
function ColumnPicker({
  box,
  slot,
  anchor,
  onDismiss,
}: {
  readonly box: CanvasBox;
  readonly slot: BoxSlot;
  readonly anchor: HTMLElement;
  readonly onDismiss: () => void;
}): React.JSX.Element {
  const { connectColumn, columnsOf, nameOf } = useGraphActions();
  const tables = slot.fits.flatMap((fit) => (fit.kind === 'table' ? [fit.node] : []));
  return (
    <Popover anchor={anchor} onDismiss={onDismiss}>
      <div className="flex max-h-(--popover-room) min-w-56 flex-col overflow-y-auto rounded-lg border border-line bg-panel py-1 text-[12px] shadow-lg">
        {tables.length === 0 && <p className="m-0 px-3 py-1.5 text-muted">No table fits.</p>}
        {tables.map((table) => (
          <div key={table} className="flex flex-col">
            <p className="m-0 px-3 pt-1.5 pb-0.5 text-muted">{nameOf(table)}</p>
            {(columnsOf(table) ?? []).map((column) => (
              <button
                key={column.name}
                type="button"
                className="cursor-pointer px-5 py-1 text-left hover:bg-hover"
                onClick={() => {
                  connectColumn(box.id, slot.slot, table, column.name);
                  onDismiss();
                }}
              >
                <Field name={column.name} dtype={column.dtype} ink="text-column-ink" />
              </button>
            ))}
          </div>
        ))}
      </div>
    </Popover>
  );
}

/** A slot's line: its pin on the left edge, its name, what fills it, and its buttons. */
function SlotLine({
  box,
  slot,
}: {
  readonly box: CanvasBox;
  readonly slot: BoxSlot;
}): React.JSX.Element {
  const { disconnect, removeInput, nameOf } = useGraphActions();
  // The button the column picker opens against, kept as state so that drawing never reads a ref.
  const [picker, setPicker] = useState<HTMLButtonElement | null>(null);
  const [picking, setPicking] = useState(false);
  const input: SlotView = slot.slot;
  return (
    <LabeledHandle
      type="target"
      position={Position.Left}
      id={slot.pin}
      title={`${slot.label}, takes a ${slot.takes}`}
      className="min-h-[24px]"
      labelClassName="flex flex-1 min-w-0 items-center gap-2 pr-3"
      label={
        <>
          <Field name={slot.label} dtype={slot.takes} ink="text-input-ink" />
          <span
            className={cn(
              'min-w-0 flex-1 truncate text-right text-[11px]',
              slot.from === null ? 'text-danger' : 'text-muted',
            )}
          >
            {fromText(slot, nameOf)}
          </span>
          {slot.takes === 'list' && (
            <BoxButton
              ref={setPicker}
              label="column…"
              title={`Read ${slot.label} from a column`}
              onPress={() => {
                setPicking(true);
              }}
            />
          )}
          {slot.from !== null && (
            <BoxButton
              label="×"
              title={`Disconnect ${slot.label}`}
              onPress={() => {
                disconnect(box.id, input);
              }}
            />
          )}
          {input.kind === 'input' && slot.from === null && (
            <BoxButton
              label="−"
              title={`Remove the input ${slot.label}`}
              onPress={() => {
                removeInput(box.id, input.name);
              }}
            />
          )}
          {picking && picker !== null && (
            <ColumnPicker
              box={box}
              slot={slot}
              anchor={picker}
              onDismiss={() => {
                setPicking(false);
              }}
            />
          )}
        </>
      }
    />
  );
}

/** An output's line: what it gives, with its pin on the right edge. */
function OutputLine({ output }: { readonly output: BoxOutput }): React.JSX.Element {
  if (output.kind === 'table') {
    return (
      <LabeledHandle
        type="source"
        position={Position.Right}
        id={output.pin}
        title="the table it gives"
        className="min-h-[22px] pl-3"
        // The table's pin stays level with its first line, the header of its fields.
        handleClassName="top-[11px]!"
        labelClassName="flex-1 px-0 pr-3"
        label={
          output.columns === null ? (
            <span className="text-[12px] text-muted">table · columns not known yet</span>
          ) : (
            <FieldList fields={output.columns} />
          )
        }
      />
    );
  }
  return (
    <LabeledHandle
      type="source"
      position={Position.Right}
      id={output.pin}
      title={`the value ${output.name}`}
      className="min-h-[22px] pl-3"
      labelClassName="flex-1 px-0 pr-3"
      label={<Field name={output.name} dtype={output.dtype} ink="text-input-ink" />}
    />
  );
}

/** A Parameters node's values, each editable where it stands, and a line that adds one. */
function ParameterEditor({ box }: { readonly box: CanvasBox }): React.JSX.Element {
  const { setParameter, removeParameter, parameterDtypes } = useGraphActions();
  const [adding, setAdding] = useState(false);
  return (
    <div className="nodrag flex flex-col gap-1 px-3 py-2 text-[12px]">
      {box.parameters.map((value) => (
        <div key={value.name} className="flex items-center gap-2">
          <span className="w-24 truncate font-mono text-input-ink">{value.name}</span>
          <input
            // A text the host changed is drawn afresh.
            key={value.text}
            aria-label={`The text of ${value.name}`}
            defaultValue={value.text}
            className="min-w-0 flex-1 rounded border border-line bg-canvas px-1 font-mono text-value outline-none"
            onBlur={(event) => {
              const text = event.currentTarget.value;
              if (text !== value.text) setParameter(box.id, value.name, value.dtype, text);
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter') event.currentTarget.blur();
            }}
          />
          <span className="shrink-0 font-mono text-dtype">{value.dtype}</span>
          <BoxButton
            label="×"
            title={`Remove ${value.name}`}
            onPress={() => {
              removeParameter(box.id, value.name);
            }}
          />
        </div>
      ))}
      {adding ? (
        <form
          className="flex items-center gap-1"
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            const text = (field: string): string => {
              const value = form.get(field);
              return typeof value === 'string' ? value : '';
            };
            const name = text('name').trim();
            if (name === '') return;
            setParameter(box.id, name, text('dtype'), text('text'));
            setAdding(false);
          }}
        >
          <input
            name="name"
            aria-label="The new value's name"
            placeholder="name"
            className="w-20 rounded border border-line bg-canvas px-1 font-mono outline-none"
          />
          <select
            name="dtype"
            aria-label="The new value's dtype"
            className="rounded border border-line bg-canvas font-mono text-dtype"
          >
            {parameterDtypes.map((dtype) => (
              <option key={dtype}>{dtype}</option>
            ))}
          </select>
          <input
            name="text"
            aria-label="The new value's text"
            placeholder="value"
            className="w-20 min-w-0 flex-1 rounded border border-line bg-canvas px-1 font-mono outline-none"
          />
          <button
            type="submit"
            className="nodrag rounded-md border border-line-strong px-2 text-muted hover:text-ink"
          >
            add
          </button>
        </form>
      ) : (
        <BoxButton
          label="+ value"
          title="Add a value"
          className="self-start"
          onPress={() => {
            setAdding(true);
          }}
        />
      )}
    </div>
  );
}

/** Draws a node of the graph. */
export function GraphBox({ data, selected }: NodeProps<GraphBoxNode>): React.JSX.Element {
  const { box } = data;
  const actions = useGraphActions();
  const hasFile = box.kind === 'source' || box.kind === 'output';
  return (
    <>
      <NodeResizer
        isVisible
        minWidth={BOX_LEAST.width}
        minHeight={BOX_LEAST.height}
        onResizeEnd={(_event, params) => {
          actions.resized(box.id, {
            x: Math.round(params.x),
            y: Math.round(params.y),
            width: Math.round(params.width),
            height: Math.round(params.height),
          });
        }}
      />
      <BaseNode
        className={cn(
          'flex h-full min-w-[260px] flex-col text-[13px]',
          selected && 'ring-1 ring-accent',
        )}
      >
        <BaseNodeHeader>
          <span aria-hidden="true" className="text-accent">
            {GLYPHS[box.kind]}
          </span>
          <Name box={box} />
          <BoxButton
            label="×"
            title={`Remove ${box.name}`}
            onPress={() => {
              actions.remove(box.id);
            }}
          />
        </BaseNodeHeader>
        <p
          className="m-0 truncate border-b px-3 py-1.5 text-[12px] text-muted"
          title={box.file?.path}
        >
          {box.summary}
        </p>
        {box.problems.length > 0 && (
          <ul className="m-0 list-none border-b px-3 py-1.5 text-[12px] text-danger">
            {box.problems.map((problem) => (
              <li key={problem}>{problem}</li>
            ))}
          </ul>
        )}
        {box.slots.length > 0 && (
          <BaseNodeContent className="gap-y-0.5 px-0 py-2">
            {box.slots.map((slot) => (
              <SlotLine key={slot.pin} box={box} slot={slot} />
            ))}
            {box.newInput !== null && (
              <LabeledHandle
                type="target"
                position={Position.Left}
                id={NEW_INPUT_PIN}
                title="drop a wire here to add an input"
                className="min-h-[22px]"
                label={<span className="text-[12px] text-muted">+ input</span>}
              />
            )}
          </BaseNodeContent>
        )}
        {box.kind === 'parameters' && <ParameterEditor box={box} />}
        {box.outputs.length > 0 && (
          <BaseNodeContent className="nowheel nodrag min-h-0 flex-1 gap-y-0.5 overflow-y-auto border-t px-0 py-2">
            {box.outputs.map((output) => (
              <OutputLine key={output.pin} output={output} />
            ))}
          </BaseNodeContent>
        )}
        {box.kind === 'output' && box.columns !== null && (
          <div className="nowheel nodrag min-h-0 flex-1 overflow-y-auto border-t px-3 py-2">
            <FieldList fields={box.columns} />
          </div>
        )}
        <BaseNodeFooter className="flex-row flex-wrap gap-1.5 py-2">
          {hasFile && (
            <BoxButton
              label={box.file === null ? 'Choose file…' : 'Change file…'}
              title={box.kind === 'source' ? 'Choose the file to read' : 'Choose the file to write'}
              onPress={() => {
                actions.chooseFile(box.id, box.kind === 'source' ? 'open' : 'save');
              }}
            />
          )}
          {box.kind === 'transform' && (
            <BoxButton
              label="Edit steps"
              title={`Edit the steps of ${box.name}`}
              className="text-accent"
              onPress={() => {
                actions.editSteps(box.id);
              }}
            />
          )}
          {(box.kind === 'source' || box.kind === 'transform') && (
            <>
              <BoxButton
                label="+ transform"
                title="Add a transform that starts from this table"
                onPress={() => {
                  actions.addFollowing(box.id, 'transform');
                }}
              />
              <BoxButton
                label="+ output"
                title="Add an output that writes this table"
                onPress={() => {
                  actions.addFollowing(box.id, 'output');
                }}
              />
            </>
          )}
        </BaseNodeFooter>
      </BaseNode>
    </>
  );
}

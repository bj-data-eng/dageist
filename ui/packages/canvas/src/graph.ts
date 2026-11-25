/**
 * @file The graph the canvas draws, prepared from the graph view the host
 * reports: one box for each node, with a pin for each of its slots and
 * outputs, and a wire for each reference a node holds. A connection dragged
 * between two pins is turned into the edit of the graph it asks for, only
 * where the host reported that the output fits the slot.
 */

import type {
  FieldView,
  FileView,
  GraphEdit,
  GraphFitView,
  GraphParameterView,
  GraphView,
  NodeBoxView,
  NodeKind,
  ReferenceView,
  SlotView,
  Takes,
} from '@dageist/host';

/** The pin of the table a source or a transform gives. */
export const TABLE_PIN = 'table';
/** The pin on a transform that a wire is dropped on to add an input. */
export const NEW_INPUT_PIN = 'new-input';

/** Returns the pin of the value `name` of a Parameters node. */
export function parameterPin(name: string): string {
  return `parameter:${name}`;
}

/** Returns the pin of a slot. */
export function slotPin(slot: SlotView): string {
  switch (slot.kind) {
    case 'start':
      return 'start';
    case 'input':
      return `input:${slot.name}`;
    case 'written':
      return 'written';
    default:
      return slot satisfies never;
  }
}

/** Returns the pin a reference leaves from: a table, or a column of it, from its table. */
export function outputPin(reference: ReferenceView): string {
  switch (reference.kind) {
    case 'table':
    case 'column':
      return TABLE_PIN;
    case 'parameter':
      return parameterPin(reference.name);
    default:
      return reference satisfies never;
  }
}

/** One slot of a box, with its pin. */
export interface BoxSlot {
  readonly pin: string;
  readonly slot: SlotView;
  /** What the slot is called beside its pin. */
  readonly label: string;
  readonly takes: Takes;
  readonly from: ReferenceView | null;
  readonly fits: readonly GraphFitView[];
}

/** One output of a box, with its pin. */
export type BoxOutput =
  | { readonly kind: 'table'; readonly pin: string; readonly columns: readonly FieldView[] | null }
  | {
      readonly kind: 'value';
      readonly pin: string;
      readonly name: string;
      readonly dtype: string;
      readonly text: string;
    };

/** A node as the canvas draws it. */
export interface CanvasBox {
  readonly id: string;
  readonly kind: NodeKind;
  readonly name: string;
  readonly summary: string;
  readonly slots: readonly BoxSlot[];
  readonly outputs: readonly BoxOutput[];
  /** For a transform, what may feed a new input; `null` for other kinds. */
  readonly newInput: readonly GraphFitView[] | null;
  /** The columns the node gives or would write, when they are known. */
  readonly columns: readonly FieldView[] | null;
  readonly parameters: readonly GraphParameterView[];
  readonly file: FileView | null;
  readonly problems: readonly string[];
}

/** A wire from an output's pin to the slot it fills. */
export interface CanvasWire {
  readonly id: string;
  readonly source: string;
  readonly sourcePin: string;
  readonly target: string;
  readonly targetPin: string;
  /** The column a list is read from, which the wire says. */
  readonly column: string | null;
}

/** Everything the canvas draws. */
export interface CanvasGraph {
  readonly revision: number;
  readonly layoutRevision: number;
  /** Every box, each after the boxes it reads. */
  readonly boxes: readonly CanvasBox[];
  readonly wires: readonly CanvasWire[];
  readonly parameterDtypes: readonly string[];
  /** Where each box the host has placed stands. */
  readonly placed: ReadonlyMap<string, Place>;
}

/** Returns what a slot is called beside its pin. */
function slotLabel(slot: SlotView): string {
  switch (slot.kind) {
    case 'start':
      return 'starts from';
    case 'input':
      return slot.name;
    case 'written':
      return 'writes';
    default:
      return slot satisfies never;
  }
}

/** Returns the outputs of a node. */
function outputsOf(node: NodeBoxView): BoxOutput[] {
  switch (node.kind) {
    case 'source':
    case 'transform':
      return [{ kind: 'table', pin: TABLE_PIN, columns: node.columns }];
    case 'parameters':
      return node.parameters.map((value) => ({
        kind: 'value',
        pin: parameterPin(value.name),
        name: value.name,
        dtype: value.dtype,
        text: value.text,
      }));
    case 'output':
      return [];
    default:
      return node.kind satisfies never;
  }
}

/** Returns the graph the canvas draws of `view`. */
export function canvasGraph(view: GraphView): CanvasGraph {
  const boxes = view.nodes.map((node): CanvasBox => ({
    id: node.id,
    kind: node.kind,
    name: node.name,
    summary: node.summary,
    slots: node.slots.map((state) => ({
      pin: slotPin(state.slot),
      slot: state.slot,
      label: slotLabel(state.slot),
      takes: state.takes,
      from: state.from,
      fits: state.fits,
    })),
    outputs: outputsOf(node),
    newInput: node.newInput,
    columns: node.columns,
    parameters: node.parameters,
    file: node.file,
    problems: node.problems,
  }));
  const wires = boxes.flatMap((box) =>
    box.slots.flatMap((slot): CanvasWire[] => {
      const from = slot.from;
      if (from === null) return [];
      const sourcePin = outputPin(from);
      return [
        {
          id: `${from.node}/${sourcePin}->${box.id}/${slot.pin}`,
          source: from.node,
          sourcePin,
          target: box.id,
          targetPin: slot.pin,
          column: from.kind === 'column' ? from.column : null,
        },
      ];
    }),
  );
  const placed = new Map<string, Place>();
  for (const node of view.nodes) {
    if (node.frame !== null) placed.set(node.id, { x: node.frame.x, y: node.frame.y });
  }
  return {
    revision: view.revision,
    layoutRevision: view.layoutRevision,
    boxes,
    wires,
    parameterDtypes: view.parameterDtypes,
    placed,
  };
}

/** Returns the reference an output's pin on `node` gives, or `null` for a pin it lacks. */
function referenceOf(node: string, pin: string): ReferenceView | null {
  if (pin === TABLE_PIN) return { kind: 'table', node };
  if (pin.startsWith('parameter:')) {
    return { kind: 'parameter', node, name: pin.slice('parameter:'.length) };
  }
  return null;
}

/** Returns whether `reference` is among `fits`. A list is filled from a column, so a dragged table never fits it. */
function fitsAmong(fits: readonly GraphFitView[], reference: ReferenceView): boolean {
  return fits.some((fit) =>
    fit.kind === 'table'
      ? reference.kind === 'table' && reference.node === fit.node
      : reference.kind === 'parameter' &&
        reference.node === fit.node &&
        reference.name === fit.name,
  );
}

/** A connection dragged from an output's pin to a slot's pin. */
export interface Dragged {
  readonly source: string | null;
  readonly sourcePin: string | null;
  readonly target: string | null;
  readonly targetPin: string | null;
}

/**
 * Returns the edit a connection dragged on `graph` asks for, or `null` when
 * the host did not report that the output fits the slot. A wire dropped on a
 * transform's new-input pin adds an input named after what feeds it.
 */
export function connectEdit(graph: CanvasGraph, dragged: Dragged): GraphEdit | null {
  const { source, sourcePin, target, targetPin } = dragged;
  if (source === null || sourcePin === null || target === null || targetPin === null) return null;
  const from = referenceOf(source, sourcePin);
  const box = graph.boxes.find((candidate) => candidate.id === target);
  if (from === null || box === undefined) return null;
  if (targetPin === NEW_INPUT_PIN) {
    return box.newInput !== null && fitsAmong(box.newInput, from)
      ? { edit: 'connectNew', node: target, from }
      : null;
  }
  const slot = box.slots.find((candidate) => candidate.pin === targetPin);
  if (slot === undefined || slot.takes === 'list' || !fitsAmong(slot.fits, from)) return null;
  return { edit: 'connect', node: target, slot: slot.slot, from };
}

/** Where a box's top-left corner sits on the canvas. */
export interface Place {
  readonly x: number;
  readonly y: number;
}

/** The room between two boxes in a column. */
const GAP = 32;
/** The room from one column of boxes to the next. */
const COLUMN = 400;

/**
 * Returns where each box stands. A box the host has placed stands there. The
 * others are set out in columns by how many boxes lie before them along the
 * wires, each column one box under another, so a box stands to the right of
 * everything it reads. `height` gives each box's height.
 */
export function placesOf(
  graph: CanvasGraph,
  height: (id: string) => number,
): ReadonlyMap<string, Place> {
  const depth = new Map<string, number>();
  // The boxes come after the boxes they read, so each depth is known when it is needed.
  for (const box of graph.boxes) {
    const before = graph.wires
      .filter((wire) => wire.target === box.id)
      .map((wire) => (depth.get(wire.source) ?? 0) + 1);
    depth.set(box.id, Math.max(0, ...before));
  }
  const places = new Map<string, Place>();
  const columnBottom = new Map<number, number>();
  for (const box of graph.boxes) {
    const placed = graph.placed.get(box.id);
    if (placed !== undefined) {
      places.set(box.id, placed);
      continue;
    }
    const column = depth.get(box.id) ?? 0;
    const y = columnBottom.get(column) ?? 0;
    places.set(box.id, { x: column * COLUMN, y });
    columnBottom.set(column, y + height(box.id) + GAP);
  }
  return places;
}

/** The room a header, a summary and the buttons take, and each listed line, before a box is measured. */
const HEADER = 96;
const LINE = 22;

/** Returns a guess at a box's height, from the lines it lists, for the first frame before it is measured. */
export function guessedHeight(box: CanvasBox): number {
  const lines =
    box.slots.length +
    box.parameters.length +
    box.problems.length +
    (box.columns === null ? 0 : box.columns.length + 1);
  return HEADER + LINE * lines;
}

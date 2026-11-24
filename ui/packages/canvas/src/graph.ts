/**
 * @file The graph the canvas draws, prepared from what the host reports of
 * the node: one box for each thing that supplies an input, one box for the
 * node, and a wire from each supplier's output to the input it feeds.
 */

import type { FieldView, InputView, NodeView, OriginView } from '@dageist/host';

/** What one output of a supplier gives. */
export type Gives =
  /** The whole table a node gives, with its columns when the host reports them. */
  | { readonly kind: 'table'; readonly columns: readonly FieldView[] }
  /** One column of the table a node gives. */
  | { readonly kind: 'column'; readonly column: string; readonly dtype: string }
  /** A value set on the graph itself, as the engine writes it. */
  | {
      readonly kind: 'parameter';
      readonly name: string;
      readonly dtype: string;
      readonly value: string;
    };

/** One output of a supplier, which wires leave from. */
export interface Output {
  /** Unique within its box. */
  readonly id: string;
  readonly gives: Gives;
}

/** Something that supplies the node: another node, or the graph's parameters. */
export interface Supplier {
  /** Unique within the graph. */
  readonly id: string;
  readonly kind: 'node' | 'parameters';
  /** The node's name, or `Parameters`. */
  readonly title: string;
  readonly outputs: readonly Output[];
}

/** One input of the node, by the name its steps use for it. */
export interface Input {
  readonly name: string;
  /** What the input takes, in a few words. */
  readonly takes: string;
}

/** A wire from a supplier's output to one of the node's inputs. */
export interface Wire {
  readonly supplier: string;
  readonly output: string;
  readonly input: string;
}

/** Everything the canvas draws. The node's own box is the one target of every wire. */
export interface CanvasGraph {
  readonly suppliers: readonly Supplier[];
  /** The table the steps start from first, then every other input. */
  readonly inputs: readonly Input[];
  readonly wires: readonly Wire[];
}

/** The name of the box that holds the graph's parameters. */
const PARAMETERS = 'Parameters';

/** Returns the shown values of a list input as one line, with how many more follow. */
function listText(values: InputView & { readonly kind: 'list' }): string {
  const shown = values.values.shown.map((value) => value ?? 'null').join(', ');
  return values.values.more > 0 ? `${shown}, … ${values.values.more} more` : shown;
}

/** Returns `count` columns in words. */
function columnsText(count: number): string {
  return count === 1 ? '1 column' : `${count} columns`;
}

/** Returns what an input takes, in a few words. */
function takes(input: InputView): string {
  switch (input.kind) {
    case 'table':
      return `table · ${columnsText(input.columns.length)}`;
    case 'value':
      return input.dtype;
    case 'list':
      return `list of ${input.dtype}`;
    default:
      return input satisfies never;
  }
}

/** Returns the output an input is fed from, given where it comes from. */
function outputFor(input: InputView, from: OriginView): Output {
  switch (from.kind) {
    case 'node':
      return {
        id: 'table',
        gives: { kind: 'table', columns: input.kind === 'table' ? input.columns : [] },
      };
    case 'column':
      return {
        id: `column:${from.column}`,
        gives: {
          kind: 'column',
          column: from.column,
          dtype: input.kind === 'table' ? 'table' : input.dtype,
        },
      };
    case 'parameter':
      return {
        id: `parameter:${input.name}`,
        gives: {
          kind: 'parameter',
          name: input.name,
          dtype: input.kind === 'table' ? 'table' : input.dtype,
          value:
            input.kind === 'value'
              ? (input.value ?? 'null')
              : input.kind === 'list'
                ? listText(input)
                : columnsText(input.columns.length),
        },
      };
    default:
      return from satisfies never;
  }
}

/** Returns the id and title of the supplier an origin belongs to. */
function supplierOf(from: OriginView): { readonly kind: Supplier['kind']; readonly title: string } {
  switch (from.kind) {
    case 'node':
      return { kind: 'node', title: from.name };
    case 'column':
      return { kind: 'node', title: from.node };
    case 'parameter':
      return { kind: 'parameters', title: PARAMETERS };
    default:
      return from satisfies never;
  }
}

/**
 * Returns the graph of `node`. Suppliers are in the order their first input
 * is reported, the table the steps start from first. An output that feeds
 * several inputs is drawn once, with a wire to each.
 */
export function canvasGraph(node: NodeView): CanvasGraph {
  const source: InputView = {
    kind: 'table',
    name: node.source.name,
    from: node.source.from,
    columns: node.source.columns,
  };
  const all = [source, ...node.inputs];
  const suppliers = new Map<string, { kind: Supplier['kind']; title: string; outputs: Output[] }>();
  const wires: Wire[] = [];
  for (const input of all) {
    const owner = supplierOf(input.from);
    const id = `${owner.kind}:${owner.title}`;
    const box = suppliers.get(id) ?? { ...owner, outputs: [] };
    suppliers.set(id, box);
    const output = outputFor(input, input.from);
    if (!box.outputs.some((known) => known.id === output.id)) box.outputs.push(output);
    wires.push({ supplier: id, output: output.id, input: input.name });
  }
  return {
    suppliers: [...suppliers].map(([id, box]) => ({ id, ...box })),
    inputs: all.map((input) => ({ name: input.name, takes: takes(input) })),
    wires,
  };
}

/** Where a box's top-left corner sits on the canvas. */
export interface Place {
  readonly x: number;
  readonly y: number;
}

/** The height of each box, in canvas units. */
export interface Heights {
  /** The height of the supplier's box with this id. */
  readonly supplier: (id: string) => number;
  readonly node: number;
}

/** The room between two suppliers, one above the other. */
const GAP = 28;
/** How far right of the suppliers' left edge the node's box stands. */
const NODE_X = 440;
/** The room a header and the stand-ins take, and each listed line, before a box is measured. */
const HEADER = 44;
const STAND_INS = 70;
const LINE = 22;

/**
 * Returns a guess at the height of each box, from the lines it lists, for
 * the first frame before the boxes are measured.
 */
export function guessedHeights(graph: CanvasGraph): Heights {
  const lines = new Map(
    graph.suppliers.map((supplier) => [
      supplier.id,
      supplier.outputs.reduce(
        (count, output) =>
          count + 1 + (output.gives.kind === 'table' ? output.gives.columns.length : 0),
        0,
      ),
    ]),
  );
  return {
    supplier: (id) => HEADER + STAND_INS + LINE * (lines.get(id) ?? 0),
    node: HEADER + STAND_INS * 2 + LINE * graph.inputs.length,
  };
}

/**
 * Returns where each box stands: the suppliers in a column, one under
 * another, and the node's box to their right, level with their middle. The
 * boxes can be moved from there.
 */
export function placesOf(
  graph: CanvasGraph,
  heights: Heights,
): {
  readonly suppliers: ReadonlyMap<string, Place>;
  readonly node: Place;
} {
  const suppliers = new Map<string, Place>();
  let y = 0;
  for (const supplier of graph.suppliers) {
    suppliers.set(supplier.id, { x: 0, y });
    y += heights.supplier(supplier.id) + GAP;
  }
  const column = Math.max(0, y - GAP);
  return { suppliers, node: { x: NODE_X, y: Math.max(0, (column - heights.node) / 2) } };
}

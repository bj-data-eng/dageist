/**
 * @file The flow canvas: the node and its suppliers as boxes on a surface
 * that pans and zooms, with a wire from each output to the input it feeds.
 * The boxes can be moved; where they stand is the view's own and is not
 * kept. The wires are the host's: none can be drawn or cut here.
 */

import '@xyflow/react/dist/style.css';
import './canvas.css';

import { useEffect, useRef, useState } from 'react';

import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
} from '@xyflow/react';
import type { Edge, NodeChangeset, NodeTypes } from '@xyflow/react';

import { Skeleton } from '@dageist/design';
import type { Host } from '@dageist/host';

import { StepsBox, SupplierBox } from './Boxes';
import type { StepsNode, SupplierNode } from './Boxes';
import { canvasGraph, guessedHeights, placesOf } from './graph';
import type { CanvasGraph, Heights } from './graph';

/** The id of the node's own box, which every wire ends at. */
const NODE = 'node';

/**
 * The room left around the boxes when the view is fitted to them. The top
 * leaves room for the bar that floats over the canvas.
 */
const FIT_PADDING = { top: '84px', bottom: '48px', x: '64px' } as const;

/** How React Flow draws each kind of box. */
const NODE_TYPES: NodeTypes = { supplier: SupplierBox, steps: StepsBox };

/** A box of either kind, as React Flow holds it. */
type Box = SupplierNode | StepsNode;

/** Returns the boxes of `graph`, each placed for the given heights. */
function boxesOf(graph: CanvasGraph, heights: Heights): Box[] {
  const places = placesOf(graph, heights);
  return [
    ...graph.suppliers.map((supplier): SupplierNode => ({
      id: supplier.id,
      type: 'supplier',
      position: places.suppliers.get(supplier.id) ?? { x: 0, y: 0 },
      data: { supplier },
    })),
    { id: NODE, type: 'steps', position: places.node, data: { inputs: graph.inputs } },
  ];
}

/** Returns a wire for each input that a supplier feeds. */
function wiresOf(graph: CanvasGraph): Edge[] {
  return graph.wires.map((wire) => ({
    id: `${wire.supplier}/${wire.output}->${wire.input}`,
    source: wire.supplier,
    sourceHandle: wire.output,
    target: NODE,
    targetHandle: wire.input,
  }));
}

/**
 * Draws `graph` on a surface that pans and zooms. The boxes first stand where
 * guessed heights put them; once React Flow has measured them, they are
 * stacked again by what they measure and the view is fitted to them.
 */
function Flow({ graph }: { readonly graph: CanvasGraph }): React.JSX.Element {
  const [boxes, setBoxes] = useState<Box[]>(() => boxesOf(graph, guessedHeights(graph)));
  const stacked = useRef(false);
  const { fitView } = useReactFlow();

  const onBoxesChange = (changes: NodeChangeset<Box>): void => {
    const next = changes.applyTo(boxes);
    if (stacked.current || !next.every((box) => box.measured?.height !== undefined)) {
      setBoxes(next);
      return;
    }
    // Every box has now been measured: stack them by what they measure.
    stacked.current = true;
    const height = (id: string): number => next.find((box) => box.id === id)?.measured?.height ?? 0;
    const places = boxesOf(graph, { supplier: height, node: height(NODE) });
    setBoxes(
      next.map((box) => ({
        ...box,
        position: places.find((placed) => placed.id === box.id)?.position ?? box.position,
      })),
    );
    // The new places reach React Flow's store on the next frame.
    requestAnimationFrame(() => {
      void fitView({ padding: FIT_PADDING });
    });
  };

  return (
    <ReactFlow
      className="dageist-flow"
      forceColorMode="dark"
      nodes={boxes}
      edges={wiresOf(graph)}
      nodeTypes={NODE_TYPES}
      onNodesChange={onBoxesChange}
      nodesConnectable={false}
      edgesFocusable={false}
      fitView
      fitViewOptions={{ padding: FIT_PADDING }}
      minZoom={0.2}
    >
      <Background variant={BackgroundVariant.Dots} gap={20} size={1.2} />
      <Controls showInteractive={false} />
      <MiniMap pannable zoomable />
    </ReactFlow>
  );
}

/** What the canvas has of the node so far. */
type Loaded =
  | { readonly kind: 'loading' }
  | { readonly kind: 'failed'; readonly reason: string }
  | { readonly kind: 'ready'; readonly graph: CanvasGraph };

/** What the canvas needs from the shell. */
export interface FlowCanvasProps {
  readonly host: Host;
}

/**
 * Asks the host what the node is connected to and draws it, filling the box
 * it is given. The shell gives it the whole window, as the window's background.
 */
export function FlowCanvas({ host }: FlowCanvasProps): React.JSX.Element {
  const [loaded, setLoaded] = useState<Loaded>({ kind: 'loading' });
  useEffect(() => {
    let live = true;
    host.nodeInputs().then(
      (node) => {
        if (live) setLoaded({ kind: 'ready', graph: canvasGraph(node) });
      },
      (error: unknown) => {
        const reason = error instanceof Error ? error.message : String(error);
        if (live) setLoaded({ kind: 'failed', reason });
      },
    );
    return () => {
      live = false;
    };
  }, [host]);

  if (loaded.kind === 'ready') {
    return (
      <ReactFlowProvider>
        <Flow graph={loaded.graph} />
      </ReactFlowProvider>
    );
  }
  // Until the host answers, the canvas is its dots, with stand-ins where the boxes will be.
  return (
    <div className="relative h-full bg-canvas bg-dots">
      {loaded.kind === 'loading' ? (
        <div className="flex h-full items-center justify-center gap-40">
          <div className="flex flex-col gap-6">
            <Skeleton className="h-40 w-[280px] rounded-xl" />
            <Skeleton className="h-24 w-[280px] rounded-xl" />
          </div>
          <Skeleton className="h-72 w-[320px] rounded-xl" />
        </div>
      ) : (
        <p className="absolute bottom-6 left-1/2 m-0 -translate-x-1/2 rounded-lg border border-line bg-panel px-3 py-1.5 text-[13px] text-muted">
          The host did not answer: {loaded.reason}
        </p>
      )}
    </div>
  );
}

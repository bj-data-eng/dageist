/**
 * @file The flow canvas: the graph the host holds, as boxes on a surface that
 * pans and zooms, with a wire from each output to the slot it fills. Every
 * change is an edit the host makes or refuses: a wire dragged between two
 * pins, checked while it is dragged against what the host reported fits; a
 * wire or a box selected and deleted; a box added, renamed, given a file or
 * removed; a box moved or resized, which the host keeps in the layout. The
 * canvas then draws the graph the host answers with.
 */

import '@xyflow/react/dist/style.css';
import './canvas.css';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  Panel,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
} from '@xyflow/react';
import type { Connection, EdgeTypes, NodeChangeset, NodeTypes } from '@xyflow/react';

import { Skeleton } from '@dageist/design';
import type { FrameView, GraphEdit, GraphView, Host, NodeKind, SlotView } from '@dageist/host';

import { ActionsProvider } from './actions';
import type { GraphActions } from './actions';
import { GraphBox } from './Boxes';
import type { GraphBoxNode } from './Boxes';
import { canvasGraph, connectEdit, guessedHeight, placesOf } from './graph';
import type { CanvasGraph, Place } from './graph';
import { RoutedWire } from './RoutedWire';
import type { Wire } from './RoutedWire';
import type { Box as RouteBox } from './route';
// Named apart from this file's own boxes, which React Flow places.
import { SettledBoxes, anyMoving, boxesOf as obstaclesOf } from './settled';

/**
 * The room left around the boxes when the view is fitted to them. The top
 * leaves room for the bar that floats over the canvas.
 */
const FIT_PADDING = { top: '84px', bottom: '48px', x: '64px' } as const;

/** How React Flow draws a box. */
const NODE_TYPES: NodeTypes = { box: GraphBox };

/** How React Flow draws each kind of wire. */
const EDGE_TYPES: EdgeTypes = { routed: RoutedWire };

/** The kinds of node the canvas adds, with what each button says. */
const ADDABLE: ReadonlyArray<readonly [NodeKind, string]> = [
  ['source', 'Source'],
  ['transform', 'Transform'],
  ['output', 'Output'],
  ['parameters', 'Parameters'],
];

/** How far right of the box it follows a new box stands. */
const FOLLOWING_X = 420;

/** Returns the boxes of `graph`, each standing at `places`, with `kept` boxes' places and sizes kept. */
function boxesOf(
  graph: CanvasGraph,
  places: ReadonlyMap<string, Place>,
  kept: readonly GraphBoxNode[],
): GraphBoxNode[] {
  return graph.boxes.map((box): GraphBoxNode => {
    const before = kept.find((held) => held.id === box.id);
    return {
      ...before,
      id: box.id,
      type: 'box',
      position: before?.position ?? places.get(box.id) ?? { x: 0, y: 0 },
      data: { box },
    };
  });
}

/** Returns a wire for each reference, each on a lane of its own by its place among the wires. */
function wiresOf(graph: CanvasGraph): Wire[] {
  return graph.wires.map((wire, lane) => ({
    id: wire.id,
    source: wire.source,
    sourceHandle: wire.sourcePin,
    target: wire.target,
    targetHandle: wire.targetPin,
    type: 'routed',
    label: wire.column ?? undefined,
    data: { lane, lanes: graph.wires.length },
  }));
}

/** Returns the slot a wire's target pin names on its box, or `null`. */
function slotOfWire(graph: CanvasGraph, wire: Wire): SlotView | null {
  const box = graph.boxes.find((candidate) => candidate.id === wire.target);
  return box?.slots.find((slot) => slot.pin === wire.targetHandle)?.slot ?? null;
}

/** What the canvas tells the shell. */
interface FlowProps {
  readonly host: Host;
  readonly view: GraphView;
  /** Replaces the graph the canvas draws with what the host answered. */
  readonly onView: (view: GraphView) => void;
  /** Asks the host for the graph again, as after an edit made on an old one. */
  readonly onStale: () => void;
  readonly onEditSteps: (node: string, name: string) => void;
}

/**
 * Draws `view` on a surface that pans and zooms. The boxes the host has not
 * placed first stand where guessed heights put them; once React Flow has
 * measured them, they are set out again by what they measure.
 */
function Flow({ host, view, onView, onStale, onEditSteps }: FlowProps): React.JSX.Element {
  const graph = useMemo(() => canvasGraph(view), [view]);
  const [boxes, setBoxes] = useState<GraphBoxNode[]>(() =>
    boxesOf(
      graph,
      placesOf(graph, (id) => guess(graph, id)),
      [],
    ),
  );
  // The boxes as last shown, which each batch of changes applies to.
  const latest = useRef(boxes);
  const [settled, setSettled] = useState<readonly RouteBox[]>([]);
  const stacked = useRef(false);
  // Where a box added after another is to stand, until the graph that has it arrives.
  const pending = useRef(new Map<string, Place>());
  const [notice, setNotice] = useState<string | null>(null);
  const { fitView } = useReactFlow();
  const wires = useMemo(() => wiresOf(graph), [graph]);

  const show = useCallback((next: GraphBoxNode[]): void => {
    latest.current = next;
    setBoxes(next);
    if (!anyMoving(next)) setSettled(obstaclesOf(next));
  }, []);

  // A new graph keeps every box where it stands, and places the boxes that are new.
  useEffect(() => {
    const height = (id: string): number =>
      latest.current.find((box) => box.id === id)?.measured?.height ?? guess(graph, id);
    const places = new Map(placesOf(graph, height));
    for (const [id, place] of pending.current) places.set(id, place);
    show(boxesOf(graph, places, latest.current));
  }, [graph, show]);

  /** Sends `edit` and draws what the host answers, or says why it was refused. */
  const send = useCallback(
    (edit: GraphEdit, place?: (added: string) => Place): void => {
      host.editGraph(view.revision, edit).then(
        (answer) => {
          if (answer.kind === 'refused') {
            setNotice(answer.message);
            return;
          }
          setNotice(null);
          const { added } = answer;
          if (added === null || place === undefined) {
            onView(answer.graph);
            return;
          }
          // The added box stands where it was asked to, and the host keeps that in its layout.
          const at = place(added);
          pending.current.set(added, at);
          onView(answer.graph);
          host
            .placeNode(answer.graph.layoutRevision, added, { ...at, width: null, height: null })
            .then(
              (placed) => {
                pending.current.delete(added);
                onView({ ...answer.graph, layoutRevision: placed.layoutRevision });
              },
              () => {
                onStale();
              },
            );
        },
        (error: unknown) => {
          setNotice(error instanceof Error ? error.message : String(error));
          onStale();
        },
      );
    },
    [host, view.revision, onView, onStale],
  );

  /** Keeps where a box stands, and its size, in the host's layout. */
  const keep = useCallback(
    (node: string, frame: FrameView): void => {
      host.placeNode(view.layoutRevision, node, frame).then(
        (placed) => {
          pending.current.delete(node);
          onView({ ...view, layoutRevision: placed.layoutRevision });
        },
        () => {
          onStale();
        },
      );
    },
    [host, view, onView, onStale],
  );

  const actions = useMemo(
    (): GraphActions => ({
      rename: (node, name) => {
        send({ edit: 'rename', node, name });
      },
      remove: (node) => {
        send({ edit: 'remove', node });
      },
      chooseFile: (node, purpose) => {
        host.chooseFile(purpose).then(
          (file) => {
            if (file !== null) send({ edit: 'setFile', node, file });
          },
          (error: unknown) => {
            setNotice(error instanceof Error ? error.message : String(error));
          },
        );
      },
      editSteps: (node) => {
        onEditSteps(node, graph.boxes.find((box) => box.id === node)?.name ?? node);
      },
      addFollowing: (node, kind) => {
        send({ edit: 'add', kind, follows: node, name: null }, () =>
          followingPlace(latest.current, node),
        );
      },
      setParameter: (node, name, dtype, text) => {
        send({ edit: 'setParameter', node, name, dtype, text });
      },
      removeParameter: (node, name) => {
        send({ edit: 'removeParameter', node, name });
      },
      connectColumn: (node, slot, from, column) => {
        send({ edit: 'connect', node, slot, from: { kind: 'column', node: from, column } });
      },
      disconnect: (node, slot) => {
        send({ edit: 'disconnect', node, slot });
      },
      removeInput: (node, name) => {
        send({ edit: 'removeInput', node, name });
      },
      resized: keep,
      parameterDtypes: graph.parameterDtypes,
      columnsOf: (node) => graph.boxes.find((box) => box.id === node)?.columns ?? null,
      nameOf: (node) => graph.boxes.find((box) => box.id === node)?.name ?? node,
    }),
    [graph, host, send, keep, onEditSteps],
  );

  const onBoxesChange = (changes: NodeChangeset<GraphBoxNode>): void => {
    const next = changes.applyTo(latest.current);
    if (stacked.current || !next.every((box) => box.measured?.height !== undefined)) {
      show(next);
      return;
    }
    // Every box has now been measured: set out the boxes the host has not placed by what they measure.
    stacked.current = true;
    const places = placesOf(
      graph,
      (id) => next.find((box) => box.id === id)?.measured?.height ?? guess(graph, id),
    );
    show(next.map((box) => ({ ...box, position: places.get(box.id) ?? box.position })));
    // The new places reach React Flow's store on the next frame.
    requestAnimationFrame(() => {
      void fitView({ padding: FIT_PADDING });
    });
  };

  const isValidConnection = (connection: Connection | Wire): boolean =>
    connectEdit(graph, {
      source: connection.source,
      sourcePin: connection.sourceHandle ?? null,
      target: connection.target,
      targetPin: connection.targetHandle ?? null,
    }) !== null;

  return (
    <ActionsProvider value={actions}>
      <SettledBoxes value={settled}>
        <ReactFlow
          className="dageist-flow"
          forceColorMode="dark"
          nodes={boxes}
          edges={wires}
          edgeTypes={EDGE_TYPES}
          nodeTypes={NODE_TYPES}
          onNodesChange={onBoxesChange}
          isValidConnection={isValidConnection}
          onConnect={(connection) => {
            const edit = connectEdit(graph, {
              source: connection.source,
              sourcePin: connection.sourceHandle ?? null,
              target: connection.target,
              targetPin: connection.targetHandle ?? null,
            });
            if (edit !== null) send(edit);
          }}
          onNodeDragStop={(_event, _box, moved) => {
            for (const box of moved) {
              keep(box.id, {
                x: Math.round(box.position.x),
                y: Math.round(box.position.y),
                width: box.width === undefined ? null : Math.round(box.width),
                height: box.height === undefined ? null : Math.round(box.height),
              });
            }
          }}
          // A deleted wire empties its slot, and a deleted box is removed; the host decides both.
          onEdgesDelete={(deleted) => {
            for (const wire of deleted) {
              const slot = slotOfWire(graph, wire);
              if (slot !== null) send({ edit: 'disconnect', node: wire.target, slot });
            }
          }}
          onNodesDelete={(deleted) => {
            for (const box of deleted) send({ edit: 'remove', node: box.id });
          }}
          fitView
          fitViewOptions={{ padding: FIT_PADDING }}
          minZoom={0.2}
        >
          <Background variant={BackgroundVariant.Dots} gap={20} size={1.5} />
          <Controls showInteractive={false} />
          <MiniMap pannable zoomable />
          <Panel position="top-right" className="mt-14! flex gap-1.5">
            {ADDABLE.map(([kind, label]) => (
              <button
                key={kind}
                type="button"
                className="cursor-pointer rounded-md border border-line-strong bg-panel px-2.5 py-1 text-[12px] text-muted hover:bg-hover hover:text-ink"
                onClick={() => {
                  send({ edit: 'add', kind, follows: null, name: null });
                }}
              >
                + {label}
              </button>
            ))}
          </Panel>
          {notice !== null && (
            <Panel position="bottom-center">
              <button
                type="button"
                title="Dismiss"
                className="cursor-pointer rounded-lg border border-line bg-panel px-3 py-1.5 text-[13px] text-danger"
                onClick={() => {
                  setNotice(null);
                }}
              >
                {notice}
              </button>
            </Panel>
          )}
        </ReactFlow>
      </SettledBoxes>
    </ActionsProvider>
  );
}

/** The width a box is taken to have before it is measured. */
const UNMEASURED_WIDTH = 300;
/** The room left between a new box and the box above it. */
const BELOW = 32;

/**
 * Returns where a box added after `node` stands: to its right, level with it,
 * or below the lowest of the boxes already standing there.
 */
function followingPlace(boxes: readonly GraphBoxNode[], node: string): Place {
  const followed = boxes.find((box) => box.id === node)?.position ?? { x: 0, y: 0 };
  const x = Math.round(followed.x + FOLLOWING_X);
  const below = boxes
    .filter((box) => {
      const width = box.measured?.width ?? UNMEASURED_WIDTH;
      return box.position.x < x + UNMEASURED_WIDTH && box.position.x + width > x;
    })
    .map((box) => box.position.y + (box.measured?.height ?? 0) + BELOW);
  return { x, y: Math.round(Math.max(followed.y, ...below)) };
}

/** Returns a guess at the height of the box `id` of `graph`. */
function guess(graph: CanvasGraph, id: string): number {
  const box = graph.boxes.find((candidate) => candidate.id === id);
  return box === undefined ? 0 : guessedHeight(box);
}

/** What the canvas has of the graph so far. */
type Loaded =
  | { readonly kind: 'loading' }
  | { readonly kind: 'failed'; readonly reason: string }
  | { readonly kind: 'ready'; readonly view: GraphView };

/** What the canvas needs from the shell. */
export interface FlowCanvasProps {
  readonly host: Host;
  /** Opens the steps of the transform `node`, called `name`. */
  readonly onEditSteps: (node: string, name: string) => void;
}

/**
 * Asks the host for the graph and draws it, filling the box it is given. The
 * shell gives it the whole window, as the window's background.
 */
export function FlowCanvas({ host, onEditSteps }: FlowCanvasProps): React.JSX.Element {
  const [loaded, setLoaded] = useState<Loaded>({ kind: 'loading' });
  // Whether the canvas is still shown, so that an answer that comes after it is gone is dropped.
  const live = useRef(true);
  const load = useCallback(() => {
    host.graph().then(
      (view) => {
        if (live.current) setLoaded({ kind: 'ready', view });
      },
      (error: unknown) => {
        const reason = error instanceof Error ? error.message : String(error);
        if (live.current) setLoaded({ kind: 'failed', reason });
      },
    );
  }, [host]);
  useEffect(() => {
    live.current = true;
    load();
    return () => {
      live.current = false;
    };
  }, [load]);

  const onView = useCallback((view: GraphView) => {
    setLoaded({ kind: 'ready', view });
  }, []);

  if (loaded.kind === 'ready') {
    return (
      <ReactFlowProvider>
        <Flow
          host={host}
          view={loaded.view}
          onView={onView}
          onStale={load}
          onEditSteps={onEditSteps}
        />
      </ReactFlowProvider>
    );
  }
  // Until the host answers, the canvas is its dots, with stand-ins where the boxes will be.
  return (
    <div className="dageist-canvas-surface relative h-full">
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

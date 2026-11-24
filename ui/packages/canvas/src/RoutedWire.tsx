/**
 * @file A wire drawn in straight runs and right angles, on a lane of its own
 * and around the boxes on the canvas. React Flow gives it its two ends; the
 * boxes it keeps clear of are the ones the canvas last took when nothing was
 * moving.
 *
 * So wires are routed around a box only once it has settled, after it is
 * dropped or a resize ends, not on every frame while it moves: a route found
 * against a box still moving flips from one side of it to the other as it
 * passes. While a box moves, a wire that belongs to it follows its pin on its
 * plain lane, and every other wire keeps its route, since neither its ends
 * nor the boxes it is routed around change.
 */

import { useContext, useMemo } from 'react';

import { BaseEdge, useNodes } from '@xyflow/react';
import type { Edge, EdgeProps } from '@xyflow/react';

import { laneAt, lanePath, orthogonalRoute, pathOf } from './route';
import { SettledBoxes } from './settled';

// React Flow requires a wire's data to be an object type that has an index
// signature, which an interface does not, so this is a type alias.

/** A wire as React Flow holds it: its place among the wires, which sets its lane. */
export type Wire = Edge<{ readonly lane: number; readonly lanes: number }, 'routed'>;

/** Draws a wire around the boxes, turning on its lane where nothing is in the way. */
export function RoutedWire({
  id,
  source,
  target,
  sourceX,
  sourceY,
  targetX,
  targetY,
  data,
  style,
}: EdgeProps<Wire>): React.JSX.Element {
  const boxes = useContext(SettledBoxes);
  const ownMoving = useNodes().some(
    (node) =>
      (node.id === source || node.id === target) &&
      (node.dragging === true || node.resizing === true),
  );
  const path = useMemo(() => {
    const from = { x: sourceX, y: sourceY };
    const to = { x: targetX, y: targetY };
    const lane = data === undefined ? (from.x + to.x) / 2 : laneAt(from, to, data.lane, data.lanes);
    return pathOf(
      ownMoving ? lanePath(from, to, lane) : orthogonalRoute({ from, to, boxes, laneX: lane }),
    );
  }, [sourceX, sourceY, targetX, targetY, data, ownMoving, boxes]);
  return <BaseEdge id={id} path={path} {...(style === undefined ? {} : { style })} />;
}

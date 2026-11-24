/**
 * @file The boxes as they stood when last nothing was moving, which the wires
 * are routed around. The canvas takes them again after a box is dropped or a
 * resize ends, and leaves them as they were while one moves, so the wires
 * keep their routes until the boxes have settled.
 */

import { createContext } from 'react';

import type { Node } from '@xyflow/react';

import type { Box } from './route';

/** The boxes the wires are routed around. */
export const SettledBoxes = createContext<readonly Box[]>([]);

/** Reports whether any of `nodes` is being dragged or resized. */
export function anyMoving(nodes: readonly Node[]): boolean {
  return nodes.some((node) => node.dragging === true || node.resizing === true);
}

/** Returns the box each of `nodes` takes up, leaving out any not yet measured. */
export function boxesOf(nodes: readonly Node[]): Box[] {
  return nodes.flatMap((node) => {
    const width = node.measured?.width ?? node.width;
    const height = node.measured?.height ?? node.height;
    return width === undefined || height === undefined
      ? []
      : [{ x: node.position.x, y: node.position.y, width, height }];
  });
}

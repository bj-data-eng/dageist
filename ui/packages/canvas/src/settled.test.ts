import { describe, expect, it } from 'vitest';

import type { Node } from '@xyflow/react';

import { anyMoving, boxesOf } from './settled';

/** A box at `x`, `y`, with whatever else is given. */
function node(id: string, extra: Partial<Node> = {}): Node {
  return { id, position: { x: 10, y: 20 }, data: {}, ...extra };
}

describe('whether any box is moving', () => {
  it('is so while one is dragged or resized, and not otherwise', () => {
    expect(anyMoving([node('a'), node('b', { dragging: true })])).toBe(true);
    expect(anyMoving([node('a', { resizing: true })])).toBe(true);
    expect(anyMoving([node('a', { dragging: false, resizing: false }), node('b')])).toBe(false);
  });
});

describe('the boxes the wires are routed around', () => {
  it('take the size React Flow measured, before the size the box was given', () => {
    expect(
      boxesOf([node('a', { width: 300, height: 400, measured: { width: 280, height: 120 } })]),
    ).toEqual([{ x: 10, y: 20, width: 280, height: 120 }]);
  });

  it('take the size the box was given while it is not yet measured', () => {
    expect(boxesOf([node('a', { width: 300, height: 400 })])).toEqual([
      { x: 10, y: 20, width: 300, height: 400 },
    ]);
  });

  it('leave out a box of no known size', () => {
    expect(boxesOf([node('a')])).toEqual([]);
  });
});

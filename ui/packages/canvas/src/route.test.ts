import { describe, expect, it } from 'vitest';

import { laneAt, lanePath, orthogonalRoute, pathOf } from './route';
import type { Box, Point } from './route';

/** Reports whether every stretch of `points` runs straight across or straight along. */
function rightAngled(points: readonly Point[]): boolean {
  return points.slice(1).every((point, at) => {
    const before = points[at];
    return before !== undefined && (before.x === point.x || before.y === point.y);
  });
}

/** Reports whether any stretch of `points` passes through the inside of `box`, tried every pixel. */
function passesThrough(points: readonly Point[], box: Box): boolean {
  return points.slice(1).some((point, at) => {
    const before = points[at];
    if (before === undefined) return false;
    const length = Math.abs(point.x - before.x) + Math.abs(point.y - before.y);
    for (let step = 0; step <= length; step += 1) {
      const share = length === 0 ? 0 : step / length;
      const x = before.x + (point.x - before.x) * share;
      const y = before.y + (point.y - before.y) * share;
      if (x > box.x && x < box.x + box.width && y > box.y && y < box.y + box.height) return true;
    }
    return false;
  });
}

describe('a wire with nothing in its way', () => {
  it('runs out, turns once on its lane, and runs in', () => {
    expect(
      orthogonalRoute({ from: { x: 0, y: 0 }, to: { x: 200, y: 100 }, boxes: [], laneX: 120 }),
    ).toEqual([
      { x: 0, y: 0 },
      { x: 120, y: 0 },
      { x: 120, y: 100 },
      { x: 200, y: 100 },
    ]);
  });

  it('runs straight when its two ends are level', () => {
    expect(
      orthogonalRoute({ from: { x: 0, y: 40 }, to: { x: 200, y: 40 }, boxes: [], laneX: 100 }),
    ).toEqual([
      { x: 0, y: 40 },
      { x: 200, y: 40 },
    ]);
  });
});

describe('a wire with a box across its lane', () => {
  // A box standing between the two ends, across the whole height the wire would turn in.
  const wall: Box = { x: 80, y: -60, width: 40, height: 220 };
  const route = orthogonalRoute({
    from: { x: 0, y: 0 },
    to: { x: 300, y: 100 },
    boxes: [wall],
    laneX: 100,
  });

  it('goes around the box and not through it', () => {
    expect(passesThrough(route, wall)).toBe(false);
  });

  it('still runs only across and along, from its output to its input', () => {
    expect(rightAngled(route)).toBe(true);
    expect(route[0]).toEqual({ x: 0, y: 0 });
    expect(route.at(-1)).toEqual({ x: 300, y: 100 });
  });
});

describe('the boxes the wire leaves and enters', () => {
  it('are left and entered straight, not cut across', () => {
    const source: Box = { x: -200, y: -50, width: 200, height: 100 };
    const target: Box = { x: 300, y: 50, width: 200, height: 100 };
    const route = orthogonalRoute({
      from: { x: 0, y: 0 },
      to: { x: 300, y: 100 },
      boxes: [source, target],
      laneX: 150,
    });
    expect(passesThrough(route, source)).toBe(false);
    expect(passesThrough(route, target)).toBe(false);
    expect(route).toHaveLength(4);
  });
});

describe('a wire that leaves close beside another box', () => {
  // The output is 16 from a box that stands beside it, closer than the room a wire keeps from a
  // box, and the box reaches above and below the output and across the wire's lane.
  const beside: Box = { x: 16, y: -150, width: 300, height: 250 };
  const route = orthogonalRoute({
    from: { x: 0, y: 0 },
    to: { x: 500, y: 150 },
    boxes: [{ x: -200, y: -40, width: 200, height: 80 }, beside],
    laneX: 250,
  });

  it('goes around that box and not through it', () => {
    expect(passesThrough(route, beside)).toBe(false);
  });

  it('still runs only across and along, from its output to its input', () => {
    expect(rightAngled(route)).toBe(true);
    expect(route[0]).toEqual({ x: 0, y: 0 });
    expect(route.at(-1)).toEqual({ x: 500, y: 150 });
  });
});

/** A repeatable stream of numbers from 0 to 1, so a failure can be run again. */
function numbers(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state * 1_103_515_245 + 12_345) % 2_147_483_648;
    return state / 2_147_483_648;
  };
}

describe('wires among boxes placed at random', () => {
  it('never pass through a box, wherever the boxes stand', () => {
    const next = numbers(7);
    const between = (low: number, high: number): number => Math.round(low + next() * (high - low));
    const failures: string[] = [];
    for (let trial = 0; trial < 300; trial += 1) {
      const from = { x: 0, y: between(-200, 200) };
      const to = { x: between(150, 600), y: between(-200, 200) };
      // The box the wire leaves, ending at its output, and the box it enters, starting at its input.
      const source: Box = { x: -200, y: from.y - 40, width: 200, height: 80 };
      const target: Box = { x: to.x, y: to.y - 40, width: 200, height: 80 };
      // Others anywhere, but never over the two ends or the boxes they belong to.
      const others: Box[] = [];
      while (others.length < 4) {
        const box: Box = {
          x: between(-150, 700),
          y: between(-350, 350),
          width: between(30, 220),
          height: between(30, 220),
        };
        const touches = (other: Box): boolean =>
          box.x < other.x + other.width + 4 &&
          box.x + box.width + 4 > other.x &&
          box.y < other.y + other.height + 4 &&
          box.y + box.height + 4 > other.y;
        if (![source, target, ...others].some(touches)) others.push(box);
      }
      const boxes = [source, target, ...others];
      const route = orthogonalRoute({ from, to, boxes, laneX: (from.x + to.x) / 2 });
      const through = boxes.filter((box) => passesThrough(route, box));
      if (through.length > 0 || !rightAngled(route)) {
        failures.push(`trial ${String(trial)}: ${JSON.stringify({ from, to, others })}`);
      }
    }
    expect(failures).toEqual([]);
  });
});

describe('the lanes of wires that run side by side', () => {
  const from = { x: 0, y: 0 };
  const below = { x: 200, y: 100 };
  const above = { x: 200, y: -100 };

  it('stand ten apart about the middle', () => {
    expect([0, 1, 2].map((lane) => laneAt(from, below, lane, 3))).toEqual([110, 100, 90]);
  });

  it('put a wire that runs down further right the earlier it comes', () => {
    expect(laneAt(from, below, 0, 2)).toBeGreaterThan(laneAt(from, below, 1, 2));
  });

  it('put a wire that runs up further right the later it comes', () => {
    expect(laneAt(from, above, 1, 2)).toBeGreaterThan(laneAt(from, above, 0, 2));
  });
});

describe('the path of a wire as if nothing were in its way', () => {
  it('runs out, along its lane and in, whatever stands between', () => {
    expect(lanePath({ x: 0, y: 0 }, { x: 200, y: 100 }, 80)).toEqual([
      { x: 0, y: 0 },
      { x: 80, y: 0 },
      { x: 80, y: 100 },
      { x: 200, y: 100 },
    ]);
  });
});

describe('the drawn path', () => {
  it('moves to the first point and draws a line to each of the rest', () => {
    expect(
      pathOf([
        { x: 0, y: 0 },
        { x: 10, y: 0 },
        { x: 10, y: 5 },
      ]),
    ).toBe('M 0 0 L 10 0 L 10 5');
  });
});

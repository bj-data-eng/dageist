/**
 * @file The path a wire takes: straight runs that turn at right angles, kept
 * clear of the boxes on the canvas. A wire leaves its output to the right and
 * comes into its input from the left. Where nothing is in the way it turns
 * once on its own lane, a line of its own between the two ends, so wires that
 * run side by side do not lie on top of each other. Where a box is in the
 * way, it goes around it, by the path that is shortest once each turn is
 * counted as a stretch of length too.
 *
 * The search runs over a sparse grid: the lines along each box's edges, at
 * its clearance and hugging its margin, the lines through the two ends and
 * the wire's lane. A path along those lines that touches no box is found
 * with Dijkstra's search, its state the point and the direction the wire last
 * ran in. Running within a box's clearance is allowed but costs more, so
 * wires keep their distance where there is room and squeeze through where
 * boxes stand close.
 */

/** A point on the canvas. */
export interface Point {
  readonly x: number;
  readonly y: number;
}

/** A box on the canvas, by its top left corner and size. */
export interface Box {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** What a wire is routed between, and around. */
export interface RouteRequest {
  /** Where the wire leaves its output, which faces right. */
  readonly from: Point;
  /** Where the wire comes into its input, which faces left. */
  readonly to: Point;
  /** The boxes the wire keeps clear of. */
  readonly boxes: readonly Box[];
  /** Where the wire turns when nothing is in the way. */
  readonly laneX: number;
}

/** How far a wire keeps from a box's edge where it has room: running closer costs more. */
const CLEARANCE = 12;
/** How much more running within a box's clearance costs than running in the open, per length. */
const CLEARANCE_WEIGHT = 3;
/** How close to a box's edge a wire may run at all. */
const MARGIN = 2;
/**
 * How far a wire runs straight out of its output, and into its input, before
 * it may turn, unless a box stands nearer: then half the way to it.
 */
const STUB = 20;
/** What a turn costs, as a length. */
const TURN_COST = 24;
/** What turning onto a line other than the wire's lane costs, as a length. */
const OFF_LANE_COST = 40;

/** The way a wire runs along a stretch. */
type Heading = 'across' | 'along';

/** Returns `box` grown by `by` on every side. */
function grown(box: Box, by: number): Box {
  return { x: box.x - by, y: box.y - by, width: box.width + 2 * by, height: box.height + 2 * by };
}

/** Returns the outlines along which a path may run past `box`: at its clearance, and hugging its margin. */
function edgesOf(box: Box): readonly Box[] {
  return [grown(box, CLEARANCE), grown(box, MARGIN)];
}

/**
 * Returns how far a wire runs straight from `point` before it turns, going
 * right when `rightward` and left otherwise: the stub, or half the way to the
 * nearest box that stands across its way, if that is nearer.
 */
function stubFrom(point: Point, rightward: boolean, walls: readonly Box[]): number {
  let room = Number.POSITIVE_INFINITY;
  for (const wall of walls) {
    if (point.y <= wall.y || point.y >= wall.y + wall.height) continue;
    const gap = rightward ? wall.x - point.x : point.x - (wall.x + wall.width);
    if (gap > 0) room = Math.min(room, gap);
  }
  return Math.min(STUB, room / 2);
}

/** Reports whether `point` lies strictly inside `box`. */
function inside(point: Point, box: Box): boolean {
  return (
    point.x > box.x &&
    point.x < box.x + box.width &&
    point.y > box.y &&
    point.y < box.y + box.height
  );
}

/** Returns `values` sorted, each once. */
function distinct(values: readonly number[]): number[] {
  const sorted = [...new Set(values)];
  sorted.sort((a, b) => a - b);
  return sorted;
}

/** Returns `points` without the ones that lie on a straight line between their neighbours. */
function corners(points: readonly Point[]): Point[] {
  const kept: Point[] = [];
  for (const point of points) {
    const last = kept.at(-1);
    if (last !== undefined && last.x === point.x && last.y === point.y) continue;
    const before = kept.at(-2);
    if (
      before !== undefined &&
      last !== undefined &&
      ((before.x === last.x && last.x === point.x) || (before.y === last.y && last.y === point.y))
    ) {
      kept[kept.length - 1] = point;
    } else {
      kept.push(point);
    }
  }
  return kept;
}

/**
 * Returns the wire's path as if nothing were in the way: out, along its lane,
 * and in. It is what a wire is drawn as while a box it belongs to is moved,
 * and when no path around the boxes can be found.
 */
export function lanePath(from: Point, to: Point, laneX: number): Point[] {
  return corners([from, { x: laneX, y: from.y }, { x: laneX, y: to.y }, to]);
}

/** Returns the request's path as if nothing were in the way. */
function direct(request: RouteRequest): Point[] {
  return lanePath(request.from, request.to, request.laneX);
}

/** A point of the search, by its place on the grid and the way the wire came into it. */
interface State {
  readonly column: number;
  readonly row: number;
  readonly heading: Heading;
}

/** Returns a key that names `state` in a map. */
function keyOf(state: State): string {
  return `${String(state.column)},${String(state.row)},${state.heading}`;
}

/** The open states of the search, cheapest first: a binary heap. */
class Heap {
  private readonly entries: Array<readonly [number, State]> = [];

  /** Adds `state`, reached at `cost`. */
  push(cost: number, state: State): void {
    const entries = this.entries;
    entries.push([cost, state]);
    let at = entries.length - 1;
    while (at > 0) {
      const parent = (at - 1) >> 1;
      const above = entries[parent];
      const here = entries[at];
      if (above === undefined || here === undefined || above[0] <= here[0]) break;
      entries[parent] = here;
      entries[at] = above;
      at = parent;
    }
  }

  /** Removes and returns the cheapest state with its cost, or nothing when none is left. */
  pop(): readonly [number, State] | undefined {
    const entries = this.entries;
    const top = entries[0];
    const last = entries.pop();
    if (top === undefined || last === undefined || entries.length === 0) return top;
    entries[0] = last;
    let at = 0;
    for (;;) {
      const left = 2 * at + 1;
      const right = left + 1;
      let least = at;
      const leastCost = (index: number): number => entries[index]?.[0] ?? Infinity;
      if (left < entries.length && leastCost(left) < leastCost(least)) least = left;
      if (right < entries.length && leastCost(right) < leastCost(least)) least = right;
      if (least === at) break;
      const here = entries[at];
      const below = entries[least];
      if (here === undefined || below === undefined) break;
      entries[at] = below;
      entries[least] = here;
      at = least;
    }
    return top;
  }
}

/**
 * Returns the points of the path from `request.from` to `request.to` that
 * runs only across and along, touches no box, and costs least, counting
 * length, length run close to a box, turns and turns off the wire's lane.
 * Falls back to the direct path only when the wire's ends are inside boxes.
 */
export function orthogonalRoute(request: RouteRequest): Point[] {
  const { from, to, laneX } = request;
  // A wire may not touch a box, less its margin. Within a box's clearance it may run, at a cost,
  // so a path is found between boxes that stand close together.
  const walls = request.boxes.map((box) => grown(box, MARGIN));
  const zones = request.boxes.map((box) => grown(box, CLEARANCE));
  const blocked = (point: Point): boolean => walls.some((wall) => inside(point, wall));
  const crowded = (point: Point): boolean => zones.some((zone) => inside(point, zone));
  // Measured to the boxes themselves, not their margins, so that between two boxes a few pixels
  // apart the turning point stands on the line between their margins, which neither claims.
  const start: Point = { x: from.x + stubFrom(from, true, request.boxes), y: from.y };
  const end: Point = { x: to.x - stubFrom(to, false, request.boxes), y: to.y };

  // The lines a path may follow: along each box's clearance and each box's margin, through the
  // two ends, and the wire's lane.
  const xs = distinct([
    start.x,
    end.x,
    laneX,
    ...request.boxes.flatMap(edgesOf).flatMap((box) => [box.x, box.x + box.width]),
  ]);
  const ys = distinct([
    start.y,
    end.y,
    ...request.boxes.flatMap(edgesOf).flatMap((box) => [box.y, box.y + box.height]),
  ]);
  const startColumn = xs.indexOf(start.x);
  const startRow = ys.indexOf(start.y);
  const endColumn = xs.indexOf(end.x);
  const endRow = ys.indexOf(end.y);
  const pointAt = (column: number, row: number): Point => ({ x: xs[column] ?? 0, y: ys[row] ?? 0 });
  if (blocked(start) || blocked(end)) return direct(request);

  // Dijkstra's search, the open states kept in a heap by what reaching them cost. A state can be
  // in the heap more than once, with a smaller cost found later; the dearer copies are skipped.
  const first: State = { column: startColumn, row: startRow, heading: 'across' };
  const cost = new Map<string, number>([[keyOf(first), 0]]);
  const cameFrom = new Map<string, State>();
  const open = new Heap();
  open.push(0, first);
  let reached: State | null = null;
  for (let entry = open.pop(); entry !== undefined; entry = open.pop()) {
    const [paid, state] = entry;
    if (paid > (cost.get(keyOf(state)) ?? Infinity)) continue;
    if (state.column === endColumn && state.row === endRow) {
      reached = state;
      break;
    }
    const here = pointAt(state.column, state.row);
    const steps: ReadonlyArray<readonly [number, number, Heading]> = [
      [1, 0, 'across'],
      [-1, 0, 'across'],
      [0, 1, 'along'],
      [0, -1, 'along'],
    ];
    for (const [dx, dy, heading] of steps) {
      const column = state.column + dx;
      const row = state.row + dy;
      if (column < 0 || column >= xs.length || row < 0 || row >= ys.length) continue;
      const there = pointAt(column, row);
      const middle = { x: (here.x + there.x) / 2, y: (here.y + there.y) / 2 };
      if (blocked(there) || blocked(middle)) continue;
      const length = Math.abs(there.x - here.x) + Math.abs(there.y - here.y);
      let step = crowded(middle) ? length * CLEARANCE_WEIGHT : length;
      if (heading !== state.heading) {
        step += TURN_COST;
        if (heading === 'along' && here.x !== laneX) step += OFF_LANE_COST;
      }
      // The wire goes on into its input across, so arriving along costs one more turn.
      if (column === endColumn && row === endRow && heading === 'along') step += TURN_COST;
      const next: State = { column, row, heading };
      const total = paid + step;
      if (total < (cost.get(keyOf(next)) ?? Infinity)) {
        cost.set(keyOf(next), total);
        cameFrom.set(keyOf(next), state);
        open.push(total, next);
      }
    }
  }
  if (reached === null) return direct(request);

  const path: Point[] = [];
  for (
    let state: State | undefined = reached;
    state !== undefined;
    state = cameFrom.get(keyOf(state))
  ) {
    path.push(pointAt(state.column, state.row));
  }
  path.reverse();
  return corners([from, ...path, to]);
}

/** Returns an SVG path that draws `points` as straight runs. */
export function pathOf(points: readonly Point[]): string {
  return points
    .map((point, at) => `${at === 0 ? 'M' : 'L'} ${String(point.x)} ${String(point.y)}`)
    .join(' ');
}

/** How far apart neighbouring lanes stand. */
const LANE_GAP = 10;

/**
 * Returns where the wire numbered `lane` of `lanes` turns, between `from` and
 * `to`: the lanes are spread about the middle, `LANE_GAP` apart. A wire that
 * runs down turns further right the higher it starts, and one that runs up
 * the lower it starts, so wires that run the same way do not cross on their
 * way in.
 */
export function laneAt(from: Point, to: Point, lane: number, lanes: number): number {
  const middle = (from.x + to.x) / 2;
  const offset = (lane - (lanes - 1) / 2) * LANE_GAP;
  return middle + (to.y >= from.y ? -offset : offset);
}

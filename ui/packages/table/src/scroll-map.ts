/**
 * @file Maps a scroller onto a table taller than a page element can be.
 *
 * Browsers cap an element's height, so millions of rows cannot be given their
 * true height. The scroller gets a capped height (its physical range) and this
 * module translates between it and the table's true height (the virtual
 * range). Small movements scroll one pixel of table per pixel of scroller;
 * large jumps, such as dragging the thumb, land proportionally.
 *
 * The two ranges match pixel for pixel in a zone at each end, so the first and
 * last rows scroll like an ordinary page. Between the zones the table is
 * compressed evenly.
 */

/** The stretch at each end where the two ranges match pixel for pixel. */
const END_ZONE = 20_000;
/** How far the thumb may drift from its place between the end zones, in pixels. */
const DRIFT_LIMIT = 1500;

/** The two ranges and the size of one page of rows, all in pixels. */
export interface ScrollGeometry {
  /** The largest `scrollTop` the scroller can reach. */
  readonly physicalMax: number;
  /** The largest scroll position of the table at its true height. */
  readonly virtualMax: number;
  /** A movement larger than this is a jump, not a scroll. */
  readonly page: number;
}

/** Where the scroller is and which part of the table that shows. */
export interface ScrollPosition {
  /** The scroller's `scrollTop`. */
  readonly top: number;
  /** The table's scroll position at its true height. */
  readonly virtual: number;
}

function isScaled(geometry: ScrollGeometry): boolean {
  return geometry.virtualMax > geometry.physicalMax && geometry.physicalMax > 2 * END_ZONE;
}

/** Converts a position in one range to the other; `from` and `to` are the ranges' ends. */
function convert(value: number, from: number, to: number): number {
  if (value <= END_ZONE) return value;
  if (from - value <= END_ZONE) return to - (from - value);
  const share = (value - END_ZONE) / (from - 2 * END_ZONE);
  return END_ZONE + share * (to - 2 * END_ZONE);
}

/** Returns the position after the scroller has moved from `previous` to `top`. */
export function nextPosition(
  previous: ScrollPosition,
  top: number,
  geometry: ScrollGeometry,
): ScrollPosition {
  if (!isScaled(geometry)) return { top, virtual: top };
  const moved = top - previous.top;
  const virtual =
    Math.abs(moved) > geometry.page
      ? convert(top, geometry.physicalMax, geometry.virtualMax)
      : previous.virtual + moved;
  return { top, virtual: Math.min(Math.max(virtual, 0), geometry.virtualMax) };
}

/**
 * Returns where to move the scroller so that its thumb shows how far through
 * the table `position` is, or `null` when it is close enough already.
 *
 * Moving it keeps one-to-one scrolling from running out of scroller before it
 * runs out of table.
 */
export function anchoredTop(position: ScrollPosition, geometry: ScrollGeometry): number | null {
  if (!isScaled(geometry)) return null;
  const { virtual } = position;
  const ideal = convert(virtual, geometry.virtualMax, geometry.physicalMax);
  const inEndZone = virtual <= END_ZONE || geometry.virtualMax - virtual <= END_ZONE;
  return Math.abs(ideal - position.top) > (inEndZone ? 1 : DRIFT_LIMIT) ? ideal : null;
}

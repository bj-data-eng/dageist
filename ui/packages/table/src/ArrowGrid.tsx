/**
 * @file Draws a table as a scrolling grid. Only the block of rows and columns
 * around the visible area exists in the page; scrolling past it fetches the
 * next block, which arrives as Arrow IPC bytes.
 */

import type { Table } from '@uwdata/flechette';
import { Type, tableFromIPC } from '@uwdata/flechette';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';

import { cx } from '@dageist/design';
import type { BlockRequest, TableView } from '@dageist/host';

import type { ScrollGeometry, ScrollPosition } from './scroll-map';
import { anchoredTop, nextPosition } from './scroll-map';

const ROW_HEIGHT = 26;
const HEADER_HEIGHT = 46;
const COLUMN_WIDTH = 184;
const GUTTER_WIDTH = 92;
/** Rows fetched beyond each edge of the visible area. */
const ROW_OVERSCAN = 60;
/** Columns fetched beyond each edge of the visible area. */
const COLUMN_OVERSCAN = 8;
/** How many fetched rows must remain beyond the visible area before the next fetch. */
const ROW_MARGIN = 20;
/** How many fetched columns must remain beyond the visible area before the next fetch. */
const COLUMN_MARGIN = 2;
/** The tallest the rows area is made; browsers cap element heights not far above this. */
const MAX_ROWS_HEIGHT = 8_000_000;

/** A rectangle of the table. Both ends are exclusive. */
interface Block {
  readonly rowStart: number;
  readonly rowEnd: number;
  readonly columnStart: number;
  readonly columnEnd: number;
}

/** A fetched block and the decoded rows that fill it. */
interface Drawn {
  readonly block: Block;
  readonly table: Table;
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}

function covers(outer: Block, inner: Block): boolean {
  return (
    outer.rowStart <= inner.rowStart &&
    outer.rowEnd >= inner.rowEnd &&
    outer.columnStart <= inner.columnStart &&
    outer.columnEnd >= inner.columnEnd
  );
}

/** Returns the rows and columns inside the scroller's visible area. */
function visibleBlock(scroller: HTMLElement, view: TableView, virtualTop: number): Block {
  const rows = view.rows;
  const columns = view.columns.length;
  const firstRow = Math.floor(virtualTop / ROW_HEIGHT);
  const rowCount = Math.ceil((scroller.clientHeight - HEADER_HEIGHT) / ROW_HEIGHT) + 1;
  const firstColumn = Math.floor(scroller.scrollLeft / COLUMN_WIDTH);
  const columnCount = Math.ceil((scroller.clientWidth - GUTTER_WIDTH) / COLUMN_WIDTH) + 1;
  return {
    rowStart: clamp(firstRow, 0, rows),
    rowEnd: clamp(firstRow + rowCount, 0, rows),
    columnStart: clamp(firstColumn, 0, columns),
    columnEnd: clamp(firstColumn + columnCount, 0, columns),
  };
}

/** Widens `block` by `rows` and `columns` on every side, within the table. */
function widened(block: Block, view: TableView, rows: number, columns: number): Block {
  return {
    rowStart: clamp(block.rowStart - rows, 0, view.rows),
    rowEnd: clamp(block.rowEnd + rows, 0, view.rows),
    columnStart: clamp(block.columnStart - columns, 0, view.columns.length),
    columnEnd: clamp(block.columnEnd + columns, 0, view.columns.length),
  };
}

function scrollGeometry(scroller: HTMLElement, view: TableView): ScrollGeometry {
  const physicalMax = Math.max(scroller.scrollHeight - scroller.clientHeight, 0);
  const hidden = Math.max(view.rows * ROW_HEIGHT - MAX_ROWS_HEIGHT, 0);
  return { physicalMax, virtualMax: physicalMax + hidden, page: scroller.clientHeight };
}

function isNumeric(typeId: number): boolean {
  return typeId === Type.Int || typeId === Type.Float || typeId === Type.Decimal;
}

/** Returns the text for one value, or `null` for a missing value. */
function formatValue(value: unknown, typeId: number): string | null {
  if (value === null || value === undefined) return null;
  if (value instanceof Date) {
    const text = value.toISOString();
    return typeId === Type.Date ? text.slice(0, 10) : `${text.slice(0, 10)} ${text.slice(11, 19)}`;
  }
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'bigint' || typeof value === 'boolean') {
    return String(value);
  }
  // Nested values, such as lists and structs. JSON has no form for BigInt.
  return JSON.stringify(value, (_key, nested: unknown) =>
    typeof nested === 'bigint' ? String(nested) : nested,
  );
}

function describeSize(bytes: number): string {
  return bytes < 1024 * 1024
    ? `${(bytes / 1024).toFixed(1)} KB`
    : `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function BodyRows({ table }: { readonly table: Table }): React.JSX.Element {
  const typeIds = table.schema.fields.map((field) => field.type.typeId);
  const columns = typeIds.map((_typeId, index) => table.getChildAt<unknown>(index));
  const rows = Array.from({ length: table.numRows }, (_unused, row) => row);
  return (
    <>
      {rows.map((row) => (
        <div
          key={row}
          className="absolute left-0 flex border-b border-line hover:bg-row"
          style={{
            top: row * ROW_HEIGHT,
            height: ROW_HEIGHT,
            lineHeight: `${String(ROW_HEIGHT)}px`,
          }}
        >
          {columns.map((column, index) => {
            const typeId = typeIds[index] ?? Type.Null;
            const text = formatValue(column.at(row), typeId);
            return (
              <span
                // Cells never move within a row, so their position identifies them.
                key={index}
                className={cx(
                  'shrink-0 overflow-hidden border-r border-line px-2.5 text-ellipsis whitespace-nowrap',
                  isNumeric(typeId) && 'text-right',
                  text === null && 'text-muted',
                )}
                style={{ width: COLUMN_WIDTH }}
              >
                {text ?? '—'}
              </span>
            );
          })}
        </div>
      ))}
    </>
  );
}

/** What the grid shows and how it gets its rows. */
export interface ArrowGridProps {
  /** The table's shape. */
  readonly view: TableView;
  /** Fetches a block of the table as Arrow IPC bytes. */
  readonly fetchBlock: (request: BlockRequest) => Promise<ArrayBuffer>;
  /** Receives a one-line description of each block as it is drawn, or of a failure. */
  readonly onReport: (text: string) => void;
}

/** Draws `view` as a grid and keeps it filled as it scrolls. */
export function ArrowGrid({ view, fetchBlock, onReport }: ArrowGridProps): React.JSX.Element {
  const scroller = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const numbers = useRef<HTMLDivElement>(null);
  /** Where the scroller is and which part of the table that shows. */
  const position = useRef<ScrollPosition>({ top: 0, virtual: 0 });
  /** The most recently fetched block, which may not be on screen yet. */
  const fetched = useRef<Block | null>(null);
  /** The block whose rows are in the page right now. */
  const rendered = useRef<Block | null>(null);
  const [drawn, setDrawn] = useState<Drawn | null>(null);

  const rowsHeight = Math.min(view.rows * ROW_HEIGHT, MAX_ROWS_HEIGHT);

  /** Moves the rendered rows to where the current scroll position puts them. */
  const place = useCallback((): void => {
    if (rendered.current === null) return;
    const shift =
      rendered.current.rowStart * ROW_HEIGHT - (position.current.virtual - position.current.top);
    const transform = `translateY(${String(shift)}px)`;
    if (body.current !== null) body.current.style.transform = transform;
    if (numbers.current !== null) numbers.current.style.transform = transform;
  }, []);

  useLayoutEffect(() => {
    rendered.current = drawn?.block ?? null;
    place();
  });

  useEffect(() => {
    const element = scroller.current;
    if (element === null) return undefined;
    let live = true;
    let loading = false;

    /** Reads the scroller, works out which part of the table it shows and moves the rows there. */
    const follow = (): void => {
      const geometry = scrollGeometry(element, view);
      let next = nextPosition(position.current, element.scrollTop, geometry);
      const anchored = anchoredTop(next, geometry);
      if (anchored !== null) {
        element.scrollTop = anchored;
        next = { top: element.scrollTop, virtual: next.virtual };
      }
      position.current = next;
      place();
    };

    /**
     * Returns the block to fetch, or `null` while the fetched block still
     * reaches well past the visible area. Fetching before the edge comes into
     * view keeps blank rows off the screen.
     */
    const needed = (): Block | null => {
      const visible = visibleBlock(element, view, position.current.virtual);
      const comfortable = widened(visible, view, ROW_MARGIN, COLUMN_MARGIN);
      return fetched.current !== null && covers(fetched.current, comfortable)
        ? null
        : widened(visible, view, ROW_OVERSCAN, COLUMN_OVERSCAN);
    };

    const load = async (block: Block): Promise<void> => {
      const names = view.columns.slice(block.columnStart, block.columnEnd).map((info) => info.name);
      const started = performance.now();
      const bytes = await fetchBlock({
        offset: block.rowStart,
        length: block.rowEnd - block.rowStart,
        columns: names,
      });
      const table = tableFromIPC(bytes, { useDate: true });
      if (!live) return;
      fetched.current = block;
      setDrawn({ block, table });
      onReport(
        `Last block: ${String(table.numRows)} rows × ${String(table.numCols)} columns` +
          ` (${describeSize(bytes.byteLength)}), fetched and decoded in` +
          ` ${(performance.now() - started).toFixed(1)} ms`,
      );
    };

    const refresh = async (): Promise<void> => {
      // With no room for rows the grid is not on screen; leave everything as it is.
      if (element.clientHeight <= HEADER_HEIGHT) return;
      follow();
      if (loading) return;
      loading = true;
      try {
        // One request at a time: by the time a block arrives the view may have
        // scrolled on, so ask again until the fetched block covers it.
        for (let block = needed(); block !== null; block = needed()) {
          await load(block);
          if (!live) return;
        }
      } catch (error) {
        if (live) onReport(String(error));
      } finally {
        loading = false;
      }
    };

    const onScroll = (): void => {
      void refresh();
    };
    element.addEventListener('scroll', onScroll, { passive: true });
    const observer = new ResizeObserver(onScroll);
    observer.observe(element);
    return () => {
      live = false;
      element.removeEventListener('scroll', onScroll);
      observer.disconnect();
    };
  }, [view, fetchBlock, onReport, place]);

  const block = drawn?.block ?? null;
  const headColumns = block === null ? [] : view.columns.slice(block.columnStart, block.columnEnd);
  const rowNumbers =
    block === null
      ? []
      : Array.from({ length: block.rowEnd - block.rowStart }, (_unused, at) => block.rowStart + at);

  return (
    <div
      ref={scroller}
      className="min-h-0 flex-1 overflow-auto text-[13px] tabular-nums [overflow-anchor:none]"
    >
      <div
        className="relative"
        style={{
          width: GUTTER_WIDTH + view.columns.length * COLUMN_WIDTH,
          height: HEADER_HEIGHT + rowsHeight,
        }}
      >
        <div
          className="sticky top-0 z-20 border-b border-line-strong bg-raised"
          style={{ height: HEADER_HEIGHT }}
        >
          <div
            className="sticky left-0 z-10 flex h-full items-center justify-end border-r border-line-strong bg-raised px-2.5 text-muted"
            style={{ width: GUTTER_WIDTH }}
          >
            #
          </div>
          {headColumns.map((column, at) => (
            <div
              key={column.name}
              className="absolute top-0 flex h-full flex-col justify-center overflow-hidden border-r border-line px-2.5 whitespace-nowrap"
              style={{
                left: GUTTER_WIDTH + ((block?.columnStart ?? 0) + at) * COLUMN_WIDTH,
                width: COLUMN_WIDTH,
              }}
            >
              <span className="overflow-hidden font-semibold text-ellipsis">{column.name}</span>
              <span className="font-mono text-[11px] text-muted">{column.dataType}</span>
            </div>
          ))}
        </div>
        <div
          className="sticky left-0 z-10 overflow-clip border-r border-line-strong bg-raised"
          style={{ width: GUTTER_WIDTH, height: rowsHeight }}
        >
          <div ref={numbers} className="absolute inset-x-0 top-0">
            {rowNumbers.map((row, at) => (
              <div
                key={row}
                className="absolute inset-x-0 px-2.5 text-right text-muted"
                style={{
                  top: at * ROW_HEIGHT,
                  height: ROW_HEIGHT,
                  lineHeight: `${String(ROW_HEIGHT)}px`,
                }}
              >
                {(row + 1).toLocaleString('en-US')}
              </div>
            ))}
          </div>
        </div>
        {/*
          Rows of the previous block stay in place until the next one arrives,
          and a long jump can put them far outside the table. Clipping keeps
          them from stretching the scroller's height, which would move the
          scrollbar's thumb under the pointer.
        */}
        <div className="absolute inset-x-0 bottom-0 overflow-clip" style={{ top: HEADER_HEIGHT }}>
          <div
            ref={body}
            className="absolute top-0"
            style={{ left: GUTTER_WIDTH + (block?.columnStart ?? 0) * COLUMN_WIDTH }}
          >
            {drawn !== null && <BodyRows table={drawn.table} />}
          </div>
        </div>
      </div>
    </div>
  );
}

/**
 * @file The rows the engine gave after a step, every cell as the engine wrote
 * it, with each column's exact dtype under its name and the columns the step
 * added or replaced marked. The view formats no value: a cell is the text it was sent, and
 * only a null, which has no text, is drawn as a dash.
 */

import { cx } from '@dageist/design';
import type { ColumnChangeView } from '@dageist/host';

import type { RowsDisplay } from './model/report';
import { cellText } from './model/values';

/** What the preview shows. */
export interface RowsPreviewProps {
  /** The rows, or why there are none; `null` while the engine has not answered. */
  readonly display: RowsDisplay | null;
  /** The columns to mark as new: the ones the step added or replaced, as the engine reports them. */
  readonly changes: readonly ColumnChangeView[];
  /** Whether the rows are those of the steps as they are now. */
  readonly settled: boolean;
}

/** Draws the rows after a step as a table, or says why there are none. */
export function RowsPreview({ display, changes, settled }: RowsPreviewProps): React.JSX.Element {
  if (display === null) {
    return <p className="m-0 text-xs text-muted">Waiting for the engine…</p>;
  }
  if (display.kind === 'message') {
    return <p className={cx('m-0 text-xs text-wrong', !settled && 'opacity-60')}>{display.text}</p>;
  }
  const { columns, rows } = display;
  return (
    <div className={cx('overflow-x-auto', !settled && 'opacity-60')}>
      <table className="w-full border-collapse text-[13px]">
        <thead>
          <tr>
            {columns.map((column) => (
              <th
                key={column.name}
                className={cx(
                  'border-b border-line px-2.5 py-1.5 text-left whitespace-nowrap',
                  changes.some((change) => change.name === column.name) && 'bg-fresh',
                )}
              >
                <span className="block text-muted">{column.name}</span>
                <span className="block font-mono text-[11px] font-normal text-muted">
                  {column.dtype}
                </span>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((cells, row) => (
            <tr key={row}>
              {columns.map((column, at) => (
                <td
                  key={column.name}
                  className={cx(
                    'border-b border-line px-2.5 py-1.5 whitespace-nowrap',
                    changes.some((change) => change.name === column.name) && 'bg-fresh',
                  )}
                >
                  {cellText(cells[at] ?? null)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

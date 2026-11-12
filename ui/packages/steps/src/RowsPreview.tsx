/**
 * @file The rows a step produced, with each column's exact dtype under its
 * name and the column the step added marked.
 */

import { cx } from '@dageist/design';

import { formatCell } from './format';
import type { Frame } from './model/frame';

/** What the preview shows. */
export interface RowsPreviewProps {
  readonly frame: Frame;
  /** The column to mark as new, if the step added one. */
  readonly added: string | null;
}

/** Draws `frame` as a table. */
export function RowsPreview({ frame, added }: RowsPreviewProps): React.JSX.Element {
  const rows = Array.from({ length: frame[0]?.cells.length ?? 0 }, (_unused, row) => row);
  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-[13px]">
        <thead>
          <tr>
            {frame.map((column) => (
              <th
                key={column.name}
                className={cx(
                  'border-b border-line px-2.5 py-1.5 text-left whitespace-nowrap',
                  column.name === added && 'bg-fresh',
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
          {rows.map((row) => (
            <tr key={row}>
              {frame.map((column) => (
                <td
                  key={column.name}
                  className={cx(
                    'border-b border-line px-2.5 py-1.5 whitespace-nowrap',
                    column.name === added && 'bg-fresh',
                  )}
                >
                  {formatCell(column.cells[row] ?? null, column.dtype)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

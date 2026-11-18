/**
 * @file The lines the table viewer writes under its heading. The figures in
 * them are text the host wrote, and are put into the line as they are. The one
 * figure the view measures itself, how long a block took to arrive, is the only
 * number written here.
 */

import type { FiguresView } from '@dageist/host';

/** The line about a table that was opened: its size, where its rows are held and how long opening took. */
export function describeShape(figures: FiguresView, source: string): string {
  return `${figures.rows} × ${figures.columns} · ${source} · opened in ${figures.opened}`;
}

/**
 * The line about the block that was drawn last: the report the host wrote of it, and how many
 * milliseconds the view waited for it, which only the view can measure.
 */
export function describeBlock(report: string, waitedMillis: number): string {
  return `${report} · fetched and decoded in ${waitedMillis.toFixed(1)} ms`;
}

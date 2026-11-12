/**
 * @file The other tables a step can look values up in, while the editor is a
 * storyboard. In the real application these are the results of other nodes.
 */

import type { Frame } from './frame';

/** A table a step can join to, by the name the step refers to it with. */
export interface NamedTable {
  readonly name: string;
  readonly frame: Frame;
}

/** The lookup tables the sample offers. */
export const TABLES: readonly NamedTable[] = [
  {
    name: 'step_sla',
    frame: [
      { name: 'step', dtype: 'String', cells: ['Intake', 'Review', 'Pending', 'Complete'] },
      { name: 'sla_days', dtype: 'Int64', cells: [1, 1, 2, 3] },
    ],
  },
  {
    name: 'owner_teams',
    frame: [
      { name: 'owner', dtype: 'String', cells: ['Company A', 'Company B'] },
      { name: 'team', dtype: 'String', cells: ['Claims North', 'Claims South'] },
      { name: 'owner_id', dtype: 'Int64', cells: [101, 102] },
    ],
  },
];

/** Returns the table called `name`, if there is one. */
export function findTable(name: string): NamedTable | null {
  return TABLES.find((table) => table.name === name) ?? null;
}

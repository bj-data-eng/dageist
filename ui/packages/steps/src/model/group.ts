/**
 * @file A step that groups, as the editor lays it out: the call that groups, the predicate that
 * keeps some of the groups if there is one, and the one call that finishes the grouping. What each
 * call of the step is comes from the catalogue, which says it for every entry that applies to a
 * group by, and never from the identifier of an entry.
 */

import type { CallView, DocumentStepView } from '@dageist/host';

import type { Catalogue } from './catalogue';
import { groupedRoleOf } from './catalogue';

/** A call after `group_by`, with the place it stands at among the calls after it. */
export interface LinkedCall {
  /** Its place among the links of the step: the number a path to it names it by. */
  readonly at: number;
  readonly call: CallView;
}

/** How a step that groups is laid out. */
export interface GroupLayout {
  /** The call that keeps the groups a predicate keeps, or `null` where the step has none. */
  readonly having: LinkedCall | null;
  /** The call that finishes the grouping, or `null` where the document holds none, which the engine does not allow. */
  readonly finishing: LinkedCall | null;
}

/** Returns how the step `step` is laid out, which is of a step that groups. */
export function groupLayoutOf(catalogue: Catalogue, step: DocumentStepView): GroupLayout {
  const linked: LinkedCall[] = step.links.map((call, at) => ({ at, call }));
  const having = linked.find((link) => groupedRoleOf(catalogue, link.call.entry) === 'having');
  const finishing = linked.find((link) => groupedRoleOf(catalogue, link.call.entry) === 'finishes');
  return { having: having ?? null, finishing: finishing ?? null };
}

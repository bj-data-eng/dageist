/**
 * @file The rows of the editor's lists that come from the engine: what it
 * says while it has not answered, the calls it offers, and the verdict on a
 * candidate. Every reason is the engine's or the editor's own limit, said as
 * such; no rule about a dtype is worked out here.
 */

import type { Offer, OfferGroup } from '@dageist/design';
import type { CandidateRequest, LiteralView, NodeView, OffersView } from '@dageist/host';

import type { Asked, CandidateFits, Fits } from './ask';
import { anyField, counted, fieldOffers, matches, offer, titled } from './offers';
import type { Catalogue } from './model/catalogue';
import { methodNameOf } from './model/catalogue';
import type { MethodName } from './model/expr';
import type { Frame } from './model/frame';
import type { PreparedFamily, PreparedOffer, PreparedReading } from './model/offers';
import { candidateKey, prepareOffers, refusalWords } from './model/offers';

/** What a list says while the engine has not answered. */
const WAITING = 'waiting for the engine';

/** What a list says where the place it would ask about is not known yet. */
const NOT_KNOWN = 'the engine has not reported on this yet';

/** Returns one row that says why the list has no offers, for a list that has none to show. */
function saying(label: string, reason: string): Offer {
  return offer(label, 'value', '', () => undefined, { id: 'engine', look: 'none', reason });
}

/**
 * Returns a group that says why there are no offers while the engine has not
 * answered, or `null` when it has. A place that is not known says so, and does
 * not show offers for another.
 */
export function unlessAnswered(asked: Asked<unknown>, known: boolean): OfferGroup | null {
  if (!known) return { title: null, counted: false, offers: [saying('…', NOT_KNOWN)] };
  switch (asked.state) {
    case 'idle':
    case 'waiting':
      return { title: null, counted: false, offers: [saying('…', WAITING)] };
    case 'failed':
      return { title: null, counted: false, offers: [saying('…', asked.reason)] };
    case 'ready':
      return null;
    default:
      return asked satisfies never;
  }
}

/** What a row says of a candidate: why it cannot be picked, and the dtype the engine gave it. */
export interface Verdict {
  readonly reason: string | null;
  readonly dtype: string | null;
}

/**
 * Returns what the engine said of `candidate`, from what a list asked of it.
 * Nothing is refused where nothing was asked: the slot's place is not known,
 * and the steps are judged when they run.
 */
export function verdictOn(asked: Asked<Fits>, candidate: CandidateRequest): Verdict {
  switch (asked.state) {
    case 'idle':
      return { reason: null, dtype: null };
    case 'waiting':
      return { reason: WAITING, dtype: null };
    case 'failed':
      return { reason: asked.reason, dtype: null };
    case 'ready': {
      const fit = asked.answer.get(candidateKey(candidate));
      return { reason: fit?.reason ?? null, dtype: fit?.dtype ?? null };
    }
    default:
      return asked satisfies never;
  }
}

/** What a call says of where a chain may end, as a few words for its row, or none. */
function endNote(offered: PreparedOffer): string {
  return offered.ends.kind === 'refused' ? 'cannot end here' : '';
}

/** Returns the note of a row for `offered`: its hint, and where it leaves the chain. */
function noteOf(offered: PreparedOffer): string {
  const end = endNote(offered);
  return end === '' ? offered.hint : `${offered.hint} · ${end}`;
}

/**
 * Returns the groups of calls the engine offers, one for each family and
 * counted, narrowed to what was typed. `onPick` is given the call that was
 * picked, so that the caller has what the engine said of it.
 */
export function offerGroups(
  families: readonly PreparedFamily[],
  typed: string,
  onPick: (offered: PreparedOffer) => void,
): OfferGroup[] {
  return families
    .map((family) =>
      counted(
        family.title,
        family.offers
          .filter((offered) => matches(typed, offered.name, offered.hint))
          .map((offered) =>
            offer(
              offered.name,
              'function',
              noteOf(offered),
              () => {
                onPick(offered);
              },
              { id: offered.id, reason: offered.reason },
            ),
          ),
      ),
    )
    .filter((group) => group.offers.length > 0);
}

/** What the rows that offer a value, an input or a field are made of. */
export interface CandidateRows {
  readonly frame: Frame;
  readonly node: NodeView;
  /** What is typed into the list. */
  readonly typed: string;
  /** What the engine said of the candidates. */
  readonly fits: CandidateFits;
  /** Which connected inputs are offered: values, lists, or both. */
  readonly inputs: ReadonlyArray<'value' | 'list'>;
  readonly onInput: (name: string) => void;
  readonly onField: (name: string) => void;
}

/** What the rows that offer what was typed in are made of, besides the rest. */
export interface ValueRows extends CandidateRows {
  /** How the engine read what is typed. */
  readonly readings: Asked<readonly PreparedReading[]>;
  readonly onLiteral: (literal: LiteralView) => void;
}

/**
 * Returns the rows that offer what was typed as the engine read it, each with
 * the dtype the engine gave it, or why the engine did not read it as what the
 * slot takes. Until the engine has answered there is one row for the text as
 * it was typed, which waits. `onPick` is given the literal of the row that was
 * picked. Nothing typed has no rows.
 */
export function readingRows(
  typed: string,
  readings: Asked<readonly PreparedReading[]>,
  onPick: (literal: LiteralView) => void,
): Offer[] {
  const typedRow = (reason: string): Offer =>
    offer(typed, 'value', '', () => undefined, { id: 'text', look: 'quoted', reason });
  switch (readings.state) {
    case 'idle':
      return [];
    case 'waiting':
      return [typedRow(WAITING)];
    case 'failed':
      return [typedRow(readings.reason)];
    case 'ready':
      return readings.answer.map((reading) => {
        if (reading.kind === 'unread') return typedRow(reading.reason);
        const text = reading.literal.kind === 'text';
        return offer(
          reading.literal.shown,
          'value',
          reading.dtype ?? '',
          () => {
            onPick(reading.literal);
          },
          { id: text ? 'text' : 'number', look: text ? 'quoted' : 'plain', reason: reading.reason },
        );
      });
    default:
      return readings satisfies never;
  }
}

/** Returns the rows that offer what was typed as the engine read it, under the heading of the values. */
export function valueGroup(rows: ValueRows): OfferGroup {
  return titled('Value', readingRows(rows.typed, rows.readings, rows.onLiteral));
}

/** Returns the rows that offer the connected values and lists the engine allows, each with its dtype. */
export function inputGroup(rows: CandidateRows): OfferGroup {
  return counted(
    'Inputs',
    rows.node.inputs
      .flatMap((input) => (input.kind === 'table' ? [] : [input]))
      .filter((input) => rows.inputs.includes(input.kind) && matches(rows.typed, input.name))
      .map((input) =>
        offer(
          input.name,
          'input',
          input.kind === 'list' ? `list of ${input.dtype}` : input.dtype,
          () => {
            rows.onInput(input.name);
          },
          {
            reason: verdictOn(rows.fits.listed, {
              kind: 'input',
              dtype: input.dtype,
              list: input.kind === 'list',
            }).reason,
          },
        ),
      ),
  );
}

/** Returns the rows that offer the fields of the rows, each with its dtype and why the engine does not take it, if it does not. */
export function fieldGroup(rows: CandidateRows): OfferGroup {
  return counted(
    'Fields',
    fieldOffers(rows.frame, rows.typed, {
      ...anyField(rows.onField),
      reasonFor: (dtype) => verdictOn(rows.fits.listed, { kind: 'field', dtype }).reason,
    }),
  );
}

/**
 * Returns the groups of methods the engine offers at the place it was asked
 * about, or the one row that says why there are none yet. `known` says whether
 * the engine has reported on the place at all. `onPick` is given the method
 * and what the engine said of it.
 */
export function methodGroups(
  catalogue: Catalogue,
  asked: Asked<OffersView>,
  known: boolean,
  typed: string,
  onPick: (name: MethodName, offered: PreparedOffer) => void,
): OfferGroup[] {
  const unanswered = unlessAnswered(asked, known);
  if (unanswered !== null) return [unanswered];
  if (asked.state !== 'ready') return [];
  return offerGroups(prepareOffers(catalogue, asked.answer, ['expression']), typed, (offered) => {
    const name = methodNameOf(offered.id);
    if (name !== null) onPick(name, offered);
  });
}

/** The functions the editor starts a chain from besides a field, an input and a value typed in. */
const START_FUNCTIONS: ReadonlySet<string> = new Set(['polars.when', 'polars.element']);

/**
 * Returns the group of functions the engine offers to start a chain at the
 * place it was asked about, or the one row that says why there are none yet.
 * `onPick` is given the identifier of the function that was picked.
 */
export function startFunctions(
  catalogue: Catalogue,
  asked: Asked<OffersView>,
  known: boolean,
  typed: string,
  onPick: (id: string) => void,
): OfferGroup[] {
  const unanswered = unlessAnswered(asked, known);
  if (unanswered !== null) return [unanswered];
  if (asked.state !== 'ready') return [];
  const rows = prepareOffers(catalogue, asked.answer, ['function', 'condition'])
    .flatMap((family) => family.offers)
    .filter(
      (offered) => START_FUNCTIONS.has(offered.id) && matches(typed, offered.name, offered.hint),
    )
    .map((offered) =>
      offer(
        offered.name,
        'function',
        noteOf(offered),
        () => {
          onPick(offered.id);
        },
        { id: offered.id, reason: offered.reason },
      ),
    );
  return [titled('Functions', rows)];
}

/**
 * Returns why the engine refuses the call `id` at the place it was asked
 * about, or `null` where it allows it or has not said.
 */
export function refusalOf(asked: Asked<OffersView>, id: string): string | null {
  if (asked.state !== 'ready') return null;
  const found = asked.answer.entries.find((entry) => entry.entry === id);
  return found?.verdict.kind === 'refused' ? refusalWords(found.verdict.refusal) : null;
}

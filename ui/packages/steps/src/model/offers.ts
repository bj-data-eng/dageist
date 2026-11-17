/**
 * @file Prepares what the engine says may stand at a place for the editor's
 * lists, and nothing else: the calls it offers grouped by family with their
 * hints and reasons, which candidates fit a slot, and what the options of an
 * entry may be set to. Every verdict is read off the engine's answer; no rule
 * about a dtype is worked out here.
 */

import type {
  CandidateRequest,
  ChosenOption,
  DefaultArgumentView,
  EndingView,
  EntryView,
  FitsView,
  OffersView,
  OptionsView,
  RefusalView,
} from '@dageist/host';

import type { Catalogue } from './catalogue';
import { editorLimit } from './limits';
import type { Options } from './settings';

/** Returns the first line of `reason`, which is what Polars says before it prints more. */
export function shortReason(reason: string): string {
  return (reason.split('\n')[0] ?? reason).trim();
}

/** Returns the words for a refusal, briefly. */
export function refusalWords(refusal: RefusalView): string {
  return shortReason(refusal.reason);
}

/** Whether a chain may end where a call leaves it. */
export type Ends =
  | { readonly kind: 'allowed' }
  | { readonly kind: 'refused'; readonly reason: string }
  | { readonly kind: 'notKnown' };

/** Returns whether the chain may end, from the engine's answer. */
export function endsOf(ending: EndingView): Ends {
  switch (ending.kind) {
    case 'allowed':
      return { kind: 'allowed' };
    case 'refused':
      return { kind: 'refused', reason: refusalWords(ending.refusal) };
    case 'notKnown':
      return { kind: 'notKnown' };
    default:
      return ending satisfies never;
  }
}

/** Who stopped a row from being picked. */
export type Stopped = 'engine' | 'editor';

/** One call the engine offers at a place, as a row of a list. */
export interface PreparedOffer {
  /** The identifier the catalogue gives the entry. */
  readonly id: string;
  /** The name the catalogue shows it by. */
  readonly name: string;
  readonly hint: string;
  readonly family: string;
  /** Why it cannot be picked, or `null` when it can. */
  readonly reason: string | null;
  /** Who stopped it: the engine refuses the call there, or the editor cannot draw it. */
  readonly stoppedBy: Stopped | null;
  /** Whether a chain may end at the call. `notKnown` for a call that is refused. */
  readonly ends: Ends;
  /** What a method that may stand starts with for its first parameter, if the tables allow Polars' default. */
  readonly argument: DefaultArgumentView | null;
}

/** The calls of one family. */
export interface PreparedFamily {
  readonly key: string;
  /** What the catalogue calls the family. */
  readonly title: string;
  readonly offers: readonly PreparedOffer[];
}

/**
 * Prepares the calls of `view` whose entries apply to one of `applies`,
 * grouped by family. The groups go from the shortest to the longest, so a long
 * one never hides a short one, and the families of a length keep the
 * catalogue's order. The calls of a family are in the engine's order. A call
 * the engine refuses carries the engine's reason, and one it allows but the
 * editor cannot draw carries the editor's.
 */
export function prepareOffers(
  catalogue: Catalogue,
  view: OffersView,
  applies: ReadonlyArray<EntryView['applies']>,
): PreparedFamily[] {
  const families = catalogue.view.families.map((family) => ({
    key: family.key,
    title: family.title,
    offers: [] as PreparedOffer[],
  }));
  for (const offered of view.entries) {
    const entry = catalogue.entries.get(offered.entry);
    if (entry === undefined || !applies.includes(entry.applies)) continue;
    const family = families.find((found) => found.key === entry.family);
    if (family === undefined) continue;
    const { verdict } = offered;
    const limit = verdict.kind === 'allowed' ? editorLimit(entry) : null;
    family.offers.push({
      id: entry.id,
      name: entry.name,
      hint: entry.hint,
      family: entry.family,
      reason: verdict.kind === 'refused' ? refusalWords(verdict.refusal) : limit,
      stoppedBy: verdict.kind === 'refused' ? 'engine' : limit === null ? null : 'editor',
      ends: verdict.kind === 'allowed' ? endsOf(verdict.ending) : { kind: 'notKnown' },
      argument: verdict.kind === 'allowed' ? offered.argument : null,
    });
  }
  const present = families.filter((family) => family.offers.length > 0);
  present.sort((left, right) => left.offers.length - right.offers.length);
  return present;
}

/** What the engine says of a candidate in a slot. */
export interface PreparedFit {
  /** The dtype the engine gave the candidate, or `null` where the tables cover none. */
  readonly dtype: string | null;
  /** Why the candidate cannot be picked, or `null` when it can. */
  readonly reason: string | null;
  /** Whether a chain may end at the candidate, where it stands as the start of an expression. */
  readonly ends: Ends | null;
}

/** Returns what tells a candidate from the others in a request. */
export function candidateKey(candidate: CandidateRequest): string {
  return JSON.stringify(candidate);
}

/**
 * Prepares what the engine said of `candidates`, in the order they were
 * asked, by the key of each. A candidate the tables cover no class of is not
 * refused: the engine says nothing of it, and the step it is in is judged when
 * it runs. `starts` says whether the candidates stand as the start of an
 * expression, which is when the verdict says whether a chain may end there.
 */
export function prepareFits(
  candidates: readonly CandidateRequest[],
  view: FitsView,
  starts: boolean,
): ReadonlyMap<string, PreparedFit> {
  const prepared = new Map<string, PreparedFit>();
  for (const [index, candidate] of candidates.entries()) {
    const fit = view.fits[index];
    if (fit === undefined) continue;
    if (fit.kind === 'unjudged') {
      prepared.set(candidateKey(candidate), { dtype: null, reason: null, ends: null });
      continue;
    }
    const { verdict } = fit;
    prepared.set(candidateKey(candidate), {
      dtype: fit.dtype,
      reason: verdict.kind === 'refused' ? refusalWords(verdict.refusal) : null,
      ends: starts && verdict.kind === 'allowed' ? endsOf(verdict.ending) : null,
    });
  }
  return prepared;
}

/** One option of an entry: its default, and what it may be set to. */
export interface PreparedOption {
  readonly parameter: string;
  /** The word the option has when it is not set. */
  readonly default: string | null;
  /** The words it may be set to, with why Polars refuses each, or `null`. */
  readonly words: ReadonlyArray<{ readonly word: string; readonly reason: string | null }>;
  /**
   * The word an option starts with when it is added: the first that is not the
   * default and that Polars allows. `null` where there is none.
   */
  readonly starter: string | null;
  /** Why the option cannot be added: Polars refuses every word but the default. */
  readonly stuck: string | null;
}

/** Prepares the options of an entry by the parameter's name. */
export function prepareOptions(view: OptionsView): ReadonlyMap<string, PreparedOption> {
  const prepared = new Map<string, PreparedOption>();
  for (const option of view.options) {
    const words = option.words.map((word) => ({
      word: word.word,
      reason: word.refusal === null ? null : refusalWords(word.refusal),
    }));
    const others = words.filter((word) => word.word !== option.default);
    const starter = others.find((word) => word.reason === null)?.word ?? null;
    prepared.set(option.parameter, {
      parameter: option.parameter,
      default: option.default,
      words,
      starter,
      stuck: starter === null ? (others[0]?.reason ?? 'there is nothing else to set it to') : null,
    });
  }
  return prepared;
}

/**
 * Returns the options of `entry` that `options` sets to a word the engine's
 * option table knows: a flag, or a choice. The others are not words of that
 * table, and the table is asked only of these.
 */
export function chosenOptions(entry: EntryView, options: Options): ChosenOption[] {
  return entry.parameters.flatMap((parameter) => {
    const setting = Object.hasOwn(options, parameter.name) ? options[parameter.name] : undefined;
    if (setting === undefined) return [];
    if (parameter.kind.kind === 'flag' && setting.kind === 'flag') {
      return [{ parameter: parameter.name, word: String(setting.value) }];
    }
    if (parameter.kind.kind === 'choice' && setting.kind === 'word') {
      return [{ parameter: parameter.name, word: setting.value }];
    }
    return [];
  });
}

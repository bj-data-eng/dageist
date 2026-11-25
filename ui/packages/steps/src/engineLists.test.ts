import { describe, expect, it } from 'vitest';

import type { OfferGroup } from '@dageist/design';

import type { Maker, Makers } from './engineLists';
import { listedGroups } from './engineLists';
import type { ListedWord } from './model/words';

/** `true` where a drawing that gives `Given` is accepted for the list of a word of `Kind`. */
type Accepts<Kind extends ListedWord['word'], Given> = Given extends Makers<Kind> ? true : false;

// The groups a list is made of are the ones the model says its word lists, each made by a function
// the drawing gives. There is no earlier version of this helper to have failed first.

/** A group that says which maker made it. */
function made(title: string): () => OfferGroup[] {
  return () => [{ title, counted: false, offers: [] }];
}

/** The titles of the groups, in order. */
function titles(groups: readonly OfferGroup[]): Array<string | null> {
  return groups.map((group) => group.title);
}

describe('the groups a list is made of', () => {
  it('are those the model lists for the word, in its order, each made by its own function', () => {
    const groups = listedGroups(
      { word: 'chainStart' },
      { fields: made('fields'), functions: made('functions'), inputs: made('inputs') },
    );

    expect(titles(groups)).toEqual(['functions', 'inputs', 'fields']);
  });

  it('leave out a group the word does not list, though the drawing has a function for it', () => {
    const makers = { options: made('options'), methods: made('methods') };

    expect(titles(listedGroups({ word: 'method', options: true }, makers))).toEqual([
      'options',
      'methods',
    ]);
    expect(titles(listedGroups({ word: 'method', options: false }, makers))).toEqual(['methods']);
  });

  it('are none from a function that makes none, as a function with nothing to offer makes', () => {
    const groups = listedGroups(
      { word: 'value', fromPath: true },
      {
        chain: made('chain'),
        values: made('values'),
        inputs: made('inputs'),
        fields: made('fields'),
        functions: () => [],
      },
    );

    expect(titles(groups)).toEqual(['chain', 'values', 'inputs', 'fields']);
  });

  it('are not made where the drawing gives no function for a group the word may list, which does not compile', () => {
    // A drawing that lacks the function for a group is refused by the type, so it cannot fail while it draws.
    const lacking: Accepts<'chainStart', { readonly inputs: Maker }> = false;
    const lackingSome: Accepts<'newStep', { readonly fields: Maker }> = false;
    const complete: Accepts<
      'chainStart',
      { readonly functions: Maker; readonly inputs: Maker; readonly fields: Maker }
    > = true;
    expect([lacking, lackingSome, complete]).toEqual([false, false, true]);
  });
});

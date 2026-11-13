/**
 * @file Stamps one card for every version of a piece and lets a click hold a
 * button open. A version is one entry in the list here and one block in
 * pieces.css under `.v-<id>`.
 */

/**
 * @typedef {object} Version
 * @property {string} id What its block in pieces.css is called.
 * @property {string} name What its card is headed.
 * @property {string} note One line on what sets it apart.
 * @property {'fmt-lists' | 'cond' | 'cond-chain' | 'nest' | 'fold' | 'pick' | 'type' | 'list-settled' | 'head' | 'button' | 'function' | 'seam'} kind
 *   What its card shows.
 */

/** @type {Version[]} */
const VERSIONS = [
  {
    id: 'fmt-lists',
    name: 'Lists under the step',
    note: 'A condition is a list where all of the lines must hold, or any of them, and a group is a list inside a list, so nothing is left to precedence. when owns a list of cases: a condition, then a value, and otherwise last. A chain in a value is three dots, and its list sits under its own line, numbered from it. What a line holds depends on what owns the list.',
    kind: 'fmt-lists',
  },
  {
    id: 'cmp-plain',
    name: 'Plain',
    note: 'A value, a comparison and a value as three blocks a hair apart. Click the comparison in the middle, such as > or ==. The top of its list adds another comparison after it, with and or or, or removes this one. Below that it changes the comparison. Click and to make it or.',
    kind: 'cond',
  },
  {
    id: 'cmp-wait',
    name: 'Plain, with a waiting word',
    note: "The same, with a faint and waiting after the last comparison. Click it, pick and or or, and the next comparison is there to fill in. The comparison's list only changes or removes it.",
    kind: 'cond',
  },
  {
    id: 'cmp-recess',
    name: 'Recess',
    note: 'The three blocks lie in a recess in whatever takes them, a little smaller than it.',
    kind: 'cond',
  },
  {
    id: 'chain-only',
    name: 'Methods',
    note: 'Kept beside the comparison: the same steps with each comparison written as a method in a chain, as gt is. A condition that a function takes is a chain behind three dots. A test with one side, such as is_null, can only be written this way.',
    kind: 'cond-chain',
  },
  {
    id: 'nest',
    name: 'Nested chain',
    note: 'A chain that is a value is a list of its own under the step. The three dots in the function own it: click them to open it, which shuts any other list of that step.',
    kind: 'nest',
  },
  {
    id: 'fold',
    name: 'Fold',
    note: 'A list or dictionary shows as many entries as its line has room for, then a count of the rest. Any of them opens the list.',
    kind: 'fold',
  },
  {
    id: 'pick',
    name: 'Grouped list',
    note: 'Click a value. Search at the top, then a chain, what was typed as a value, the inputs and the fields.',
    kind: 'pick',
  },
  {
    id: 'type',
    name: 'Typed block',
    note: 'A name or a value that is typed. A block like any other at rest. Typing empties its fill and rings it.',
    kind: 'type',
  },
  {
    id: 'head',
    name: 'Step head',
    note: 'Bracketed like a function, in the verb’s colour: the verb, then the name it makes.',
    kind: 'head',
  },
  {
    id: 'list',
    name: 'Values',
    note: 'Each value is one more block a hair apart, and a pair is two that touch. Click one to open its list. Point at the function for its own tools.',
    kind: 'list-settled',
  },
  {
    id: 'tint',
    name: 'Button',
    note: 'A column. A flat fill in the colour of what it names.',
    kind: 'button',
  },
  {
    id: 'fn',
    name: 'Function',
    note: 'Bracketed, in two blocks a hair apart: the function, then its field.',
    kind: 'function',
  },
  {
    id: 'seam',
    name: 'Seam',
    note: 'A heavy chevron in a darker shade of the brackets’ colour.',
    kind: 'seam',
  },
];

/**
 * The field each function shown works on, and whether it is a column or a
 * plain value. A function that is not listed takes no field.
 * @type {Map<string, { text: string, kind: 'column' | 'value' }>}
 */
const FIELDS = new Map([
  ['over', { text: 'dcn', kind: 'column' }],
  ['shift', { text: '1', kind: 'value' }],
  ['sub', { text: 'resolved_at', kind: 'column' }],
]);

/** The template each kind of card is stamped from. */
const CARDS = {
  'fmt-lists': '#card-fmt-lists',
  cond: '#card-cond',
  'cond-chain': '#card-chain-only',
  nest: '#card-nest',
  fold: '#card-fold',
  pick: '#card-pick',
  type: '#card-type',
  'list-settled': '#card-list',
  head: '#card-head',
  seam: '#card-seam',
  button: '#card',
  function: '#card',
};

/** The function the Function card shows. */
const SHOWN_FUNCTION = 'over';

const pieceTemplate = document.querySelector('#piece');

/**
 * Writes `text` into the part of `card` that `selector` finds.
 * @param {HTMLElement} card
 * @param {string} selector
 * @param {string} text
 */
function fill(card, selector, text) {
  const part = card.querySelector(selector);
  if (part !== null) part.textContent = text;
}

/**
 * @typedef {object} Spec What a piece is to be.
 * @property {string | null} state `live`, or the one state it is a picture of.
 * @property {string | null} name What a column is called, where it is not `dcn`.
 * @property {string | null} fn The function it is, or null for a column.
 * @property {string | null} typed A value the function takes that is typed, as `kind:text`.
 * @property {string | null} slot A value of any kind the function takes, as `kind:text`.
 * @property {string | null} many The fields a function takes several of, parted by commas.
 */

/**
 * Reads what the piece for `slot` is to be.
 * @param {Element} slot
 * @param {Version} version
 * @returns {Spec}
 */
function specOf(slot, version) {
  // A slot in a row says what it is. The others are the piece the version is about.
  const inRow = slot.closest('.row, .lines') !== null;
  const shown = version.kind === 'function' ? SHOWN_FUNCTION : null;
  return {
    state: slot.getAttribute('data-piece'),
    name: slot.getAttribute('data-name'),
    fn: inRow ? slot.getAttribute('data-fn') : shown,
    typed: slot.getAttribute('data-type'),
    slot: slot.getAttribute('data-slot'),
    many: slot.getAttribute('data-many'),
  };
}

/**
 * Makes `piece` a function: bracketed, its name in the first segment, and
 * after it whatever it takes: several fields, a value of any kind, a value
 * that is typed, or the one field it works on.
 * @param {HTMLElement} piece
 * @param {HTMLButtonElement} button
 * @param {Spec} spec
 * @param {string} fn
 */
function makeFunction(piece, button, spec, fn) {
  piece.classList.add('fn', 'is-bracketed');
  button.textContent = fn;
  if (spec.many !== null) {
    const slot = document.createElement('span');
    slot.className = 'slot is-many';
    slot.dataset.many = spec.many;
    // Several fields are picked from the fields alone.
    slot.dataset.takes = 'field';
    button.after(slot);
    return;
  }
  if (spec.slot !== null) {
    const [kind = 'text', value = ''] = spec.slot.split(':');
    const slot = document.createElement('span');
    slot.className = 'slot';
    slot.dataset.kind = kind;
    slot.dataset.value = value;
    button.after(slot);
    return;
  }
  if (spec.typed !== null) {
    const [kind = 'text', text = ''] = spec.typed.split(':');
    const block = document.createElement('label');
    block.className = `type is-${kind}`;
    const input = document.createElement('input');
    input.value = text;
    input.spellcheck = false;
    input.setAttribute('aria-label', kind === 'number' ? 'A whole number' : 'A text value');
    block.append(input);
    button.after(block);
    return;
  }
  const field = FIELDS.get(fn);
  if (field === undefined) return;
  const segment = document.createElement('button');
  segment.type = 'button';
  segment.className = field.kind === 'value' ? 'btn field is-value' : 'btn field';
  segment.textContent = field.text;
  button.after(segment);
}

/**
 * Returns a copy of the piece that `spec` describes. A live one answers the
 * pointer and the keyboard. Any other is a picture of one state, and answers
 * neither.
 * @param {HTMLTemplateElement} source
 * @param {Spec} spec
 * @returns {HTMLElement | null}
 */
function pieceFor(source, spec) {
  const piece = source.content.firstElementChild?.cloneNode(true);
  if (!(piece instanceof HTMLElement)) return null;
  const button = piece.querySelector('.btn');
  if (!(button instanceof HTMLButtonElement)) return null;
  if (spec.name !== null) button.textContent = spec.name;
  if (spec.fn !== null) makeFunction(piece, button, spec, spec.fn);

  if (spec.state === 'live') {
    for (const segment of piece.querySelectorAll('.btn')) segment.classList.add('live');
    return piece;
  }
  for (const control of piece.querySelectorAll('button')) control.tabIndex = -1;
  if (spec.state === 'hover') {
    piece.classList.add('is-hover');
    button.classList.add('is-hover');
  }
  if (spec.state === 'pressed') button.classList.add('is-active');
  if (spec.state === 'open') button.setAttribute('aria-expanded', 'true');
  if (spec.state === 'focus') button.classList.add('is-focus');
  if (spec.state === 'off') {
    piece.classList.add('is-off');
    for (const control of piece.querySelectorAll('button')) control.disabled = true;
  }
  return piece;
}

/** How many nested chains have been given a list, so that each list knows its dots. */
let nestsLowered = 0;

/**
 * Joins each caret of a chain to the block after it, so that a chain too
 * long for its line breaks before a caret and never between a caret and the
 * block it points at.
 * @param {HTMLElement} card
 */
function linkChains(card) {
  for (const seam of card.querySelectorAll('.chain > .seam')) {
    const next = seam.nextElementSibling;
    const link = document.createElement('span');
    link.className = 'link';
    seam.before(link);
    link.append(seam);
    if (next !== null) link.append(next);
  }
}

/**
 * Moves every nested chain in `card` out of its function and into a list of
 * its own under the step it belongs to. Three dots stay in the function and
 * own that list: it holds nothing of any other function, and its lines are
 * numbered from one after the step, as 2.1 is the first line of a list under
 * step 2. One list of a step is open at a time, the first to begin with.
 * @param {HTMLElement} card
 */
function lowerNests(card) {
  for (const row of card.querySelectorAll('.row')) {
    let last = row;
    for (const [index, nest] of [...row.querySelectorAll('.nest')].entries()) {
      const chain = nest.querySelector('.nest-chain');
      if (!(nest instanceof HTMLElement) || chain === null) continue;
      nestsLowered += 1;
      nest.dataset.nest = String(nestsLowered);
      nest.classList.toggle('is-open', index === 0);
      const list = document.createElement('div');
      list.className = index === 0 ? 'subrow' : 'subrow is-shut';
      list.dataset.nest = nest.dataset.nest;
      const mark = document.createElement('span');
      mark.className = 'mark';
      mark.textContent = `${row.querySelector('.num')?.textContent ?? ''}.1`;
      list.append(mark, chain);
      last.after(list);
      last = list;
    }
  }
}

if (pieceTemplate instanceof HTMLTemplateElement) {
  for (const [index, version] of VERSIONS.entries()) {
    const template = document.querySelector(CARDS[version.kind]);
    if (!(template instanceof HTMLTemplateElement)) continue;
    const card = template.content.firstElementChild?.cloneNode(true);
    if (!(card instanceof HTMLElement)) continue;
    card.classList.add(`v-${version.id}`);
    fill(card, '.letter', String.fromCodePoint(65 + index));
    fill(card, '.name', version.name);
    fill(card, '.note', version.note);
    card.querySelector(version.kind === 'button' ? '.row-function' : '.row-button')?.remove();
    // The buttons written out in a row answer a click as the stamped pieces do.
    for (const part of card.querySelectorAll(':is(.row, .lines) .body > .btn')) {
      part.classList.add('live');
    }
    for (const slot of card.querySelectorAll('[data-piece]')) {
      const piece = pieceFor(pieceTemplate, specOf(slot, version));
      if (piece !== null) slot.replaceWith(piece);
    }
    if (version.id === 'cmp-wait') {
      // A word waits after the last comparison of every function that has one.
      for (const body of card.querySelectorAll('.body:has(> .cmp)')) {
        [...body.querySelectorAll(':scope > .cmp')]
          .at(-1)
          ?.after(slotOf('join', 'and', 'join waits'));
      }
    }
    linkChains(card);
    lowerNests(card);
    const section = version.kind.startsWith('fmt')
      ? '#versions-format'
      : version.kind.startsWith('cond')
        ? '#versions-cond'
        : '#versions-settled';
    document.querySelector(section)?.append(card);
  }
}

/**
 * Opens or shuts the list that the dots of `nest` own.
 * @param {Element} nest
 * @param {boolean} open
 */
function showList(nest, open) {
  if (!(nest instanceof HTMLElement)) return;
  nest.classList.toggle('is-open', open);
  const list = document.querySelector(
    `:is(.subrow, .lines)[data-nest="${nest.dataset.nest ?? ''}"]`,
  );
  list?.classList.toggle('is-shut', !open);
}

// A click on three dots opens the list they own, or shuts it if it is open. It also shuts any
// other list of the same step or line, so that neither shows two functions' lists at once.
document.addEventListener('click', (event) => {
  if (!(event.target instanceof Element)) return;
  const nest = event.target.closest('.nest-token')?.closest('.nest');
  // Dots on a line of a list own a list under that line, so the line is what they share.
  const owner = nest?.closest('.line, .row');
  if (nest === null || nest === undefined || owner === null || owner === undefined) return;
  const open = !nest.classList.contains('is-open');
  for (const other of owner.querySelectorAll('.nest')) showList(other, false);
  showList(nest, open);
});

/**
 * Closes every open button except `keep`.
 * @param {Element | null} keep
 */
function closeOthers(keep) {
  for (const open of document.querySelectorAll('.btn.live[aria-expanded="true"]')) {
    if (open !== keep) open.setAttribute('aria-expanded', 'false');
  }
}

document.addEventListener('click', (event) => {
  const button = event.target instanceof Element ? event.target.closest('.btn.live') : null;
  closeOthers(button);
  if (button === null) return;
  const open = button.getAttribute('aria-expanded') === 'true';
  button.setAttribute('aria-expanded', String(!open));
});

/*
 * The slot that takes a value. It shows its value as a block of the value's
 * kind, or several fields as several blocks. A click opens what it can take
 * beneath it, with a search box at the top: what is typed there narrows the
 * offer and is itself offered as a value. Whatever is picked becomes the
 * value. A slot that takes several fields stays open while they are ticked.
 */

/**
 * @typedef {object} Named Something on offer that has a name.
 * @property {string} name What it is called.
 * @property {string} dtype The dtype it holds.
 */

/**
 * Makes up the fields of a wide table: the six the steps use, then a few
 * hundred more, so the offer can be judged at the size it has to work at.
 * @returns {Named[]}
 */
function manyFields() {
  const stems = [
    'claim',
    'member',
    'provider',
    'service',
    'payment',
    'adjustment',
    'diagnosis',
    'procedure',
    'line',
    'authorization',
    'plan',
    'group',
    'facility',
    'referral',
    'appeal',
  ];
  /** @type {[string, string][]} */
  const ends = [
    ['id', 'String'],
    ['code', 'String'],
    ['status', 'String'],
    ['type', 'String'],
    ['name', 'String'],
    ['date', 'Date'],
    ['received_at', 'Datetime'],
    ['amount', 'Float64'],
    ['paid_amount', 'Float64'],
    ['count', 'Int64'],
    ['days', 'Int64'],
    ['units', 'Int64'],
    ['is_final', 'Boolean'],
    ['is_void', 'Boolean'],
    ['note', 'String'],
    ['region', 'String'],
    ['tier', 'Int64'],
    ['closed_at', 'Datetime'],
    ['due_date', 'Date'],
    ['rate', 'Float64'],
  ];
  return [
    { name: 'dcn', dtype: 'String' },
    { name: 'timestamp', dtype: 'Datetime' },
    { name: 'step_to', dtype: 'String' },
    { name: 'owner', dtype: 'String' },
    { name: 'row_days', dtype: 'Int64' },
    { name: 'resolved_at', dtype: 'Datetime' },
    ...stems.flatMap((stem) => ends.map(([end, dtype]) => ({ name: `${stem}_${end}`, dtype }))),
  ];
}

const FIELDS_ON_OFFER = manyFields();

/** @type {Named[]} */
const INPUTS_ON_OFFER = [
  { name: 'sla_cutoff', dtype: 'Int64' },
  { name: 'as_of', dtype: 'Date' },
];

/**
 * Reports whether `slot` takes `what`: a value, an input, a field or a chain.
 * @param {HTMLElement} slot
 * @param {string} what
 */
function takes(slot, what) {
  return (slot.dataset.takes ?? 'chain value input field').split(' ').includes(what);
}

/** The comparisons on offer, and a word on each. */
/** @type {[string, string][]} */
const COMPARISONS_ON_OFFER = [
  ['==', 'equal to'],
  ['!=', 'not equal to'],
  ['>', 'greater than'],
  ['>=', 'at least'],
  ['<', 'less than'],
  ['<=', 'at most'],
  ['is_in', 'one of'],
];

/** The words that join two comparisons, and a word on each. */
/** @type {[string, string][]} */
const JOINS_ON_OFFER = [
  ['and', 'both must hold'],
  ['or', 'either may hold'],
];

/**
 * Returns a slot that holds `value` as a value of `kind`. A comparison and a
 * joining word are slots too: `role` names which, and whether the word waits.
 * @param {string} kind
 * @param {string} value
 * @param {string} role
 */
function slotOf(kind, value, role = '') {
  const slot = document.createElement('span');
  slot.className = `slot ${role}`.trim();
  if (role !== '') slot.dataset.takes = kind;
  slot.dataset.kind = kind;
  slot.dataset.value = value;
  return slot;
}

/** Returns a comparison with nothing in it yet: a field, equal to, a value. */
function emptyComparison() {
  const comparison = document.createElement('span');
  comparison.className = 'cmp';
  comparison.append(slotOf('none', 'field'), slotOf('op', '==', 'op'), slotOf('none', 'value'));
  return comparison;
}

/**
 * Draws the slots just put on the page, and opens the first of them so that
 * the new comparison's field is picked straight away.
 * @param {HTMLElement[]} parts
 */
function drawNew(parts) {
  const slots = parts.flatMap((part) =>
    part.classList.contains('slot') ? [part] : [...part.querySelectorAll('.slot')],
  );
  for (const slot of slots) {
    if (slot instanceof HTMLElement) showSlot(slot);
  }
  const first = parts.find((part) => part.classList.contains('cmp'))?.querySelector('.slot');
  if (first instanceof HTMLElement) openSlot(first);
}

/**
 * Puts an empty comparison after `comparison`, joined to it by `join`.
 * @param {HTMLElement} comparison
 * @param {string} join
 */
function addComparison(comparison, join) {
  const parts = [slotOf('join', join, 'join'), emptyComparison()];
  comparison.after(...parts);
  drawNew(parts);
}

/**
 * Reports whether `part` is a word that joins two comparisons, not one that waits.
 * @param {Element | null} part
 */
function isJoin(part) {
  return part !== null && part.classList.contains('join') && !part.classList.contains('waits');
}

/**
 * Takes `comparison` away, with the word that joined it to the one before,
 * or to the one after when it was the first.
 * @param {HTMLElement} comparison
 */
function removeComparison(comparison) {
  const before = comparison.previousElementSibling;
  const after = comparison.nextElementSibling;
  if (isJoin(before)) before?.remove();
  else if (isJoin(after)) after?.remove();
  comparison.remove();
}

/** The words for a list of comparisons taken as a group, and a word on each. */
/** @type {[string, string][]} */
const GROUPS_ON_OFFER = [
  ['all of', 'every line must hold'],
  ['any of', 'one line is enough'],
];

/** The room a count takes at the end of a folded slot, and the room a bracket takes after it. */
const COUNT_ROOM = 44;
const BRACKET_ROOM = 8;

/**
 * Returns the pairs a dictionary slot holds: a field, and what it is given.
 * @param {HTMLElement} slot
 * @returns {[string, string][]}
 */
function pairsOf(slot) {
  const pairs = slot.dataset.pairs ?? '';
  if (pairs === '') return [];
  return pairs.split(',').map((pair) => {
    const [field = '', given = ''] = pair.split(':');
    return [field, given];
  });
}

/**
 * Writes the pairs a dictionary slot holds.
 * @param {HTMLElement} slot
 * @param {[string, string][]} pairs
 */
function setPairs(slot, pairs) {
  slot.dataset.pairs = pairs.map(([field, given]) => `${field}:${given}`).join(',');
}

/**
 * Returns the fields ticked in a slot that takes several. A page that wants
 * many of them writes `first:60` for the first sixty fields of the table.
 * @param {HTMLElement} slot
 * @returns {string[]}
 */
function ticked(slot) {
  if (slot.classList.contains('is-dict')) return pairsOf(slot).map(([field]) => field);
  const many = slot.dataset.many ?? '';
  if (many.startsWith('first:')) {
    const count = Number(many.slice('first:'.length));
    slot.dataset.many = FIELDS_ON_OFFER.slice(0, count)
      .map((field) => field.name)
      .join(',');
    return ticked(slot);
  }
  return many === '' ? [] : many.split(',');
}

/**
 * Returns the block that shows `value` as a value of `kind`.
 * @param {string} kind
 * @param {string} value
 * @returns {HTMLButtonElement}
 */
function blockOf(kind, value) {
  const block = document.createElement('button');
  block.type = 'button';
  block.className = `btn shown is-${kind}`;
  block.textContent = kind === 'chain' ? '…' : value;
  return block;
}

/**
 * Draws what `slot` holds: its value as a block, or its fields as blocks.
 * They are kept in a part of their own, so that they can be redrawn while
 * the offer beneath them stays as it is.
 * @param {HTMLElement} slot
 */
function drawHeld(slot) {
  const held = slot.querySelector('.held') ?? document.createElement('span');
  held.className = 'held';
  if (!slot.classList.contains('is-many')) {
    held.replaceChildren(blockOf(slot.dataset.kind ?? 'text', slot.dataset.value ?? ''));
  } else {
    // An entry is a field, or in a dictionary a field and what it is given, as two blocks that touch.
    /** @type {HTMLElement[]} */
    const entries = slot.classList.contains('is-dict')
      ? pairsOf(slot).map(([field, given]) => {
          const pair = document.createElement('span');
          pair.className = 'pair';
          // What a field is given is a name, unless the page says it is a plain value.
          const kind = slot.dataset.gives === 'value' ? 'number' : 'name';
          pair.append(blockOf('column', field), blockOf(kind, given));
          return pair;
        })
      : ticked(slot).map((name) => blockOf('column', name));
    if (entries.length === 0) entries.push(blockOf('none', 'no fields'));
    held.replaceChildren(...entries);
  }
  if (!held.isConnected) slot.prepend(held);
  fold(slot);
}

/**
 * Returns the room a slot's entries have: from where the slot starts to the
 * end of its step, less what the step returns, the bracket that closes the
 * function, and whatever follows the slot inside the function. A word that
 * follows keeps its width, and another slot is allowed the room of a count.
 * @param {HTMLElement} slot
 */
function roomFor(slot) {
  const row = slot.closest('.row');
  if (row === null) return Number.POSITIVE_INFINITY;
  const returns = row.querySelector('.returns');
  const kept = returns === null ? 0 : returns.getBoundingClientRect().width + 14;
  let after = 0;
  for (let next = slot.nextElementSibling; next !== null; next = next.nextElementSibling) {
    after += next.classList.contains('slot') ? COUNT_ROOM : next.getBoundingClientRect().width;
  }
  const end = row.getBoundingClientRect().right - kept - BRACKET_ROOM - after;
  return end - slot.getBoundingClientRect().left;
}

/**
 * Folds a slot that holds several entries to the room it has. The entries
 * are measured, not counted, so a dictionary of long names folds sooner than
 * a list of short ones.
 * @param {HTMLElement} slot
 */
function fold(slot) {
  if (!slot.classList.contains('is-many')) return;
  const held = slot.querySelector('.held');
  if (!(held instanceof HTMLElement)) return;
  let count = slot.querySelector(':scope > .fold-count');
  if (count === null) {
    count = blockOf('chain', '');
    count.classList.add('fold-count');
    held.after(count);
  }
  const entries = [...held.children];
  for (const entry of entries) entry.classList.remove('is-extra');
  slot.classList.remove('is-folded');
  // A hair stands between two entries.
  const widths = entries.map((entry) => entry.getBoundingClientRect().width + 1);
  const needed = widths.reduce((sum, width) => sum + width, 0);
  // With every entry out of the way the function sits where it will once it is folded, which is
  // where its room has to be measured from.
  for (const entry of entries) entry.classList.add('is-extra');
  slot.classList.add('is-folded');
  count.textContent = `+${String(entries.length)}`;
  const room = roomFor(slot);
  if (needed <= room) {
    for (const entry of entries) entry.classList.remove('is-extra');
    slot.classList.remove('is-folded');
    return;
  }
  let used = 0;
  let shown = 0;
  for (const width of widths) {
    if (used + width > room - COUNT_ROOM) break;
    used += width;
    shown += 1;
  }
  for (const entry of entries.slice(0, shown)) entry.classList.remove('is-extra');
  count.textContent = `+${String(entries.length - shown)}`;
}

/**
 * Closes `slot`: what it holds, and no offer.
 * @param {HTMLElement} slot
 */
function showSlot(slot) {
  slot.classList.remove('is-open');
  slot.querySelector('.offers')?.remove();
  drawHeld(slot);
}

/**
 * Returns one row of the offer: a tick box, the value as a block, and a word
 * on what it is. In a dictionary a ticked row has, in place of the word, a
 * field for what the value is given.
 * @param {string} kind
 * @param {string} value
 * @param {string} word
 * @param {boolean} isTicked
 * @param {string | null} given What a ticked field is given in a dictionary, or null elsewhere.
 */
function offerRow(kind, value, word, isTicked, given = null) {
  const row = document.createElement('button');
  row.type = 'button';
  row.tabIndex = -1;
  row.className = isTicked ? 'offer is-ticked' : 'offer';
  row.dataset.kind = kind;
  row.dataset.value = value;
  const box = document.createElement('span');
  box.className = 'tick';
  const what = document.createElement('i');
  what.textContent = given === null ? word : '→';
  row.append(box, blockOf(kind, value), what);
  if (given !== null) {
    const field = document.createElement('input');
    field.className = 'given';
    field.value = given;
    field.spellcheck = false;
    field.setAttribute('aria-label', `New name for ${value}`);
    row.append(field);
  }
  return row;
}

/**
 * Returns a row of the offer that does something, where the others are
 * something to take: its block says what it does in `label`.
 * @param {string} kind
 * @param {string} value
 * @param {string} label
 * @param {string} word
 */
function actRow(kind, value, label, word) {
  const row = offerRow(kind, value, word, false);
  row.classList.add('offer-act');
  const block = row.querySelector('.shown');
  if (block !== null) block.textContent = label;
  return row;
}

/**
 * Returns one group of the offer: its heading, how many rows it has when
 * that is worth saying, and the rows.
 * @param {string} title
 * @param {number | null} count
 * @param {HTMLElement[]} rows
 */
function offerGroup(title, count, rows) {
  const group = document.createElement('span');
  group.className = 'group';
  const heading = document.createElement('span');
  heading.className = 'group-head';
  heading.textContent = title;
  if (count !== null) {
    const number = document.createElement('i');
    number.textContent = String(count);
    heading.append(number);
  }
  group.append(heading, ...rows);
  return group;
}

/**
 * Draws what an open slot offers for what has been searched for. The groups
 * go from the shortest to the longest: a chain, what was typed as a value,
 * the inputs, then the fields. A group with nothing in it is left out. The
 * row at `chosen` is marked, or the first row after the chain when `chosen`
 * is null, since a chain is seldom what Enter is meant for.
 * @param {HTMLElement} slot
 * @param {string} typed
 * @param {number | null} chosen
 */
function listOffers(slot, typed, chosen) {
  const list = slot.querySelector('.offer-list');
  if (list === null) return;
  const sought = typed.trim().toLowerCase();
  const isTicked = new Set(ticked(slot));
  /** @type {HTMLElement[]} */
  const groups = [];
  /** @type {(row: [string, string]) => boolean} */
  const isSought = ([name, word]) => name.includes(sought) || word.includes(sought);
  if (takes(slot, 'op')) {
    // First, what can be done with the whole comparison, as the shortest group: add another
    // after it, or take it away. Where a word waits to add the next one, adding is left to it.
    const taker = slot.closest('.cmp')?.parentElement ?? null;
    const inBody = taker?.classList.contains('body') ?? false;
    const others = taker?.querySelectorAll(':scope > .cmp').length ?? 0;
    const acts = [];
    if (inBody && slot.closest('.v-cmp-wait') === null) {
      acts.push(
        ...JOINS_ON_OFFER.map(([name]) =>
          actRow('add', name, `+ ${name}`, 'add another comparison'),
        ),
      );
    }
    if (inBody && others > 1) acts.push(actRow('remove', 'remove', '× remove', 'this comparison'));
    if (sought === '' && acts.length > 0) groups.push(offerGroup('This condition', null, acts));
    const rows = COMPARISONS_ON_OFFER.filter(isSought).map(([name, word]) =>
      offerRow('op', name, word, false),
    );
    if (rows.length > 0) groups.push(offerGroup('Comparison', null, rows));
  }
  if (takes(slot, 'join')) {
    const rows = JOINS_ON_OFFER.filter(isSought).map(([name, word]) =>
      offerRow('join', name, word, false),
    );
    const title = slot.classList.contains('waits') ? 'Add a comparison' : 'Joined by';
    if (rows.length > 0) groups.push(offerGroup(title, null, rows));
  }
  if (takes(slot, 'group')) {
    const rows = GROUPS_ON_OFFER.filter(isSought).map(([name, word]) =>
      offerRow('join', name, word, false),
    );
    if (rows.length > 0) groups.push(offerGroup('The lines below', null, rows));
  }
  if (takes(slot, 'chain')) {
    const chain = offerRow('chain', 'chain', 'a chain of its own', false);
    chain.classList.add('offer-chain');
    groups.push(chain);
  }
  if (takes(slot, 'value')) {
    const rows = [];
    if (/^-?\d+$/.test(sought)) rows.push(offerRow('number', sought, 'Int64', false));
    if (typed !== '') rows.push(offerRow('text', typed, 'String', false));
    rows.push(offerRow('none', 'None', 'no value', false));
    groups.push(offerGroup('Value', null, rows));
  }
  if (takes(slot, 'input')) {
    const inputs = INPUTS_ON_OFFER.filter((input) => input.name.includes(sought));
    const rows = inputs.map((input) => offerRow('input', input.name, input.dtype, false));
    if (rows.length > 0) groups.push(offerGroup('Inputs', inputs.length, rows));
  }
  if (takes(slot, 'field')) {
    const fields = FIELDS_ON_OFFER.filter((field) => field.name.includes(sought));
    // The fields already ticked come first, so they are in view however long the list is.
    fields.sort(
      (left, right) => Number(isTicked.has(right.name)) - Number(isTicked.has(left.name)),
    );
    const given = new Map(slot.classList.contains('is-dict') ? pairsOf(slot) : []);
    const rows = fields.map((field) =>
      offerRow(
        'column',
        field.name,
        field.dtype,
        isTicked.has(field.name),
        given.get(field.name) ?? null,
      ),
    );
    if (rows.length > 0) groups.push(offerGroup('Fields', fields.length, rows));
  }
  if (groups.length === 0) {
    const nothing = document.createElement('span');
    nothing.className = 'nothing';
    nothing.textContent = 'Nothing by that name.';
    groups.push(nothing);
  }
  list.replaceChildren(...groups);
  const rows = [...list.querySelectorAll('.offer')];
  // Enter is not meant for a chain, nor for a row that adds or removes.
  const first = rows.findIndex(
    (row) => !row.classList.contains('offer-chain') && !row.classList.contains('offer-act'),
  );
  const marked = Math.max(0, Math.min(chosen ?? first, rows.length - 1));
  rows[marked]?.classList.add('is-chosen');
  slot.dataset.chosen = String(marked);
}

/**
 * Opens `slot`: beneath what it holds, a search box and what it can take.
 * @param {HTMLElement} slot
 */
function openSlot(slot) {
  // One list at a time: whatever else was open is closed first.
  for (const other of document.querySelectorAll('.slot.is-open')) {
    if (other instanceof HTMLElement && other !== slot) showSlot(other);
  }
  slot.classList.add('is-open');
  const many = slot.classList.contains('is-many');
  const offers = document.createElement('span');
  offers.className = 'offers';
  const search = document.createElement('label');
  search.className = 'search';
  const input = document.createElement('input');
  input.spellcheck = false;
  input.className = 'sought';
  const isValue = !takes(slot, 'op') && !takes(slot, 'join');
  input.placeholder = many
    ? 'Search fields'
    : isValue && !takes(slot, 'group')
      ? 'Type a value, or search'
      : 'Search';
  input.setAttribute('aria-label', input.placeholder);
  // A value that was typed can be changed where it is typed.
  const kind = slot.dataset.kind;
  if (!many && (kind === 'text' || kind === 'number')) input.value = slot.dataset.value ?? '';
  search.append(input);
  const list = document.createElement('span');
  list.className = 'offer-list';
  offers.append(search, list);
  slot.append(offers);
  // What is there was the value before, so nothing is narrowed until something is typed.
  listOffers(slot, '', null);
  input.focus();
  input.select();
}

/**
 * Takes the offer in `row`. A slot that takes one value becomes it and
 * closes. A slot that takes several fields ticks or unticks it and stays open.
 * @param {HTMLElement} slot
 * @param {Element | undefined} row
 */
function takeOffer(slot, row) {
  if (!(row instanceof HTMLElement)) return;
  const value = row.dataset.value ?? '';
  const comparison = slot.closest('.cmp');
  if (row.dataset.kind === 'add' || row.dataset.kind === 'remove') {
    // The row acts on the whole comparison, and leaves what it compares by as it was.
    showSlot(slot);
    if (!(comparison instanceof HTMLElement)) return;
    if (row.dataset.kind === 'add') addComparison(comparison, value);
    else removeComparison(comparison);
    return;
  }
  if (slot.classList.contains('waits')) {
    // The word that waited now joins a new comparison, and another word waits after it.
    slot.classList.remove('waits');
    slot.dataset.value = value;
    showSlot(slot);
    const parts = [emptyComparison(), slotOf('join', 'and', 'join waits')];
    slot.after(...parts);
    drawNew(parts);
    return;
  }
  if (!slot.classList.contains('is-many')) {
    slot.dataset.kind = row.dataset.kind;
    slot.dataset.value = value;
    showSlot(slot);
    return;
  }
  const fields = ticked(slot);
  if (slot.classList.contains('is-dict')) {
    // A field newly ticked in a dictionary is given its own name until another is typed.
    const pairs = pairsOf(slot);
    setPairs(
      slot,
      fields.includes(value)
        ? pairs.filter(([field]) => field !== value)
        : [...pairs, [value, value]],
    );
  } else {
    const next = fields.includes(value)
      ? fields.filter((name) => name !== value)
      : [...fields, value];
    slot.dataset.many = next.join(',');
  }
  drawHeld(slot);
  const list = slot.querySelector('.offer-list');
  const scrolled = list?.scrollTop ?? 0;
  listOffers(slot, slot.querySelector('input')?.value ?? '', Number(slot.dataset.chosen ?? 0));
  if (list !== null) list.scrollTop = scrolled;
}

for (const slot of document.querySelectorAll('.slot')) {
  if (slot instanceof HTMLElement) showSlot(slot);
}

/** Works out again what every slot folds away. */
function foldAll() {
  for (const slot of document.querySelectorAll('.slot.is-many')) {
    if (slot instanceof HTMLElement) fold(slot);
  }
}

// The room changes with the window, so what is folded is worked out again.
window.addEventListener('resize', foldAll);

document.addEventListener('click', (event) => {
  if (!(event.target instanceof Element)) return;
  const slot = event.target.closest('.slot');
  if (!(slot instanceof HTMLElement)) return;
  // A click in the field where a new name is typed is not a tick.
  if (event.target instanceof HTMLInputElement) return;
  const row = event.target.closest('.offer');
  if (row !== null) {
    takeOffer(slot, row);
  } else if (!slot.classList.contains('is-open')) {
    openSlot(slot);
  } else if (event.target.closest('.offers') === null) {
    // A click on what an open slot holds closes it again.
    showSlot(slot);
  }
});

// A press on an open slot must not take the search box's focus, or the slot would close before
// the click that follows.
document.addEventListener('pointerdown', (event) => {
  if (!(event.target instanceof Element) || event.target instanceof HTMLInputElement) return;
  if (event.target.closest('.slot.is-open') !== null) event.preventDefault();
});

// Leaving the slot closes it. Moving between its search box and its other fields does not.
document.addEventListener('focusout', (event) => {
  if (!(event.target instanceof HTMLInputElement)) return;
  const slot = event.target.closest('.slot.is-open');
  if (!(slot instanceof HTMLElement)) return;
  if (event.relatedTarget instanceof Node && slot.contains(event.relatedTarget)) return;
  showSlot(slot);
});

document.addEventListener('keydown', (event) => {
  if (!(event.target instanceof HTMLInputElement)) return;
  const slot = event.target.closest('.slot.is-open');
  if (!(slot instanceof HTMLElement)) return;
  if (event.target.classList.contains('given')) {
    // Enter or Escape in a new name goes back to the search box.
    if (event.key === 'Enter' || event.key === 'Escape') {
      event.preventDefault();
      const sought = slot.querySelector('.sought');
      if (sought instanceof HTMLElement) sought.focus();
    }
    return;
  }
  const rows = slot.querySelectorAll('.offer');
  const chosen = Number(slot.dataset.chosen ?? 0);
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const next = (chosen + (event.key === 'ArrowDown' ? 1 : rows.length - 1)) % rows.length;
    rows[chosen]?.classList.remove('is-chosen');
    rows[next]?.classList.add('is-chosen');
    rows[next]?.scrollIntoView({ block: 'nearest' });
    slot.dataset.chosen = String(next);
  } else if (event.key === 'Enter') {
    event.preventDefault();
    takeOffer(slot, rows[chosen]);
  } else if (event.key === 'Escape') {
    showSlot(slot);
  }
});

/**
 * Keeps a typed block as wide as what is in it, and marks it when what is
 * typed cannot be taken: a number that is not whole, or a name left empty.
 * @param {HTMLInputElement} input
 */
function tend(input) {
  input.style.width = `${Math.max(input.value.length, 1)}ch`;
  const block = input.closest('.type');
  if (block === null || block.classList.contains('is-wrong')) return;
  const wholeNumber = /^-?\d+$/.test(input.value);
  const wrong =
    (block.classList.contains('is-number') && !wholeNumber) ||
    (block.classList.contains('is-name') && input.value.trim() === '');
  input.setAttribute('aria-invalid', String(wrong));
}

for (const input of document.querySelectorAll('.type input')) {
  if (input instanceof HTMLInputElement) tend(input);
}

// The typed blocks have only now taken their widths, which moves what follows them.
foldAll();

document.addEventListener('input', (event) => {
  if (!(event.target instanceof HTMLInputElement)) return;
  const slot = event.target.closest('.slot.is-open');
  if (!(slot instanceof HTMLElement)) {
    tend(event.target);
  } else if (event.target.classList.contains('given')) {
    // A new name is written into its pair as it is typed. The offer is left as it is.
    const field = event.target.closest('.offer')?.getAttribute('data-value');
    const typed = event.target.value;
    setPairs(
      slot,
      pairsOf(slot).map(([old, given]) => [old, old === field ? typed : given]),
    );
    drawHeld(slot);
  } else {
    listOffers(slot, event.target.value, null);
  }
});

document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') closeOthers(null);
  // Enter ends the typing, as leaving the block does. An open slot has its own answer to Enter.
  const typing = event.target instanceof HTMLInputElement ? event.target : null;
  if (event.key === 'Enter' && typing !== null && typing.closest('.slot') === null) typing.blur();
});

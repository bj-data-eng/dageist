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
 * @property {'add' | 'fmt-lists' | 'cond' | 'cond-chain' | 'nest' | 'fold' | 'pick' | 'type' | 'list-settled' | 'head' | 'button' | 'function' | 'seam'} kind
 *   What its card shows.
 */

/** @type {Version[]} */
const VERSIONS = [
  {
    id: 'add-wait',
    name: 'Adding',
    note: "A faint method waits at the end of every chain, and a faint step after the last step. Click one for its list. Point at a block in a chain: its + puts a method after it and its × takes it away. On a step's head, + adds a step after it and × removes the step. A function such as when or coalesce can only start a chain: pick it where a new chain starts, or under Wrap in a function in the method list, which puts the chain so far inside it. What a function takes is a list under the step.",
    kind: 'add',
  },
  {
    id: 'fmt-lists',
    name: 'Lists under the step',
    note: "A condition is a list where all of the lines must hold, or any of them, and a group is a list inside a list. when owns a list of cases. A chain in a value is three dots, with its list under its own line. A faint word waits where the next thing goes: and or or at the end of a list adds a comparison or a group, when adds a case, and the and after a case's one comparison turns that condition into a list. Click a comparison such as > to remove its line.",
    kind: 'fmt-lists',
  },
  {
    id: 'cmp',
    name: 'Comparison',
    note: 'A value, a comparison and a value as three blocks a hair apart. A row holds one comparison, with a faint and waiting after it. Click the word and the condition moves into a list under the step, headed all of, with the next comparison there to fill in. A chain the comparison holds goes with it, under its line.',
    kind: 'cond',
  },
  {
    id: 'chain-only',
    name: 'Methods',
    note: 'Nothing special: every Polars method is there to use in a chain, so a comparison can also be written as one, as gt is. A test with one side, such as is_null, can only be written this way.',
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
  add: '#card-add',
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
    // A word waits after a comparison that is a whole condition: in a step's head, and in a case.
    for (const comparison of card.querySelectorAll('.body > .cmp')) {
      comparison.after(slotOf('join', 'and', 'join waits'));
    }
    for (const comparison of card.querySelectorAll('.lines.is-cases > .line > .cmp')) {
      if (comparison instanceof HTMLElement) conditionOf(comparison);
    }
    linkChains(card);
    if (version.kind === 'add') {
      // A method waits at the end of every chain, and a step after the last step.
      for (const chain of card.querySelectorAll('.chain')) chain.append(waitingMethod());
      card.append(waitingStep());
    }
    lowerNests(card);
    const section = version.kind.startsWith('add')
      ? '#versions-add'
      : version.kind.startsWith('fmt')
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

/**
 * The methods on offer for a chain, in groups: each with a word on what it
 * gives, and what it takes: nothing, a `slot` for any value, `many` fields,
 * a `number` or a `text`. They stand for all the methods there are.
 */
/** @type {[string, [string, string, string][]][]} */
const METHODS_ON_OFFER = [
  [
    'Null',
    [
      ['fill_null', 'a value where there is none', 'slot'],
      ['is_null', 'whether there is no value', ''],
      ['drop_nulls', 'without the missing values', ''],
    ],
  ],
  [
    'Text',
    [
      ['str.to_uppercase', 'in capitals', ''],
      ['str.len_chars', 'how many characters', ''],
      ['str.contains', 'whether it holds a text', 'text'],
      ['str.strip_chars', 'without spaces at the ends', ''],
    ],
  ],
  [
    'Date',
    [
      ['dt.date', 'the date alone', ''],
      ['dt.year', 'the year', ''],
      ['dt.total_days', 'a duration in whole days', ''],
      ['dt.truncate', 'to the start of a period', 'text'],
    ],
  ],
  [
    'Window',
    [
      ['over', 'within each group', 'many'],
      ['shift', 'the value some rows away', 'number'],
      ['cum_sum', 'a running total', ''],
      ['cum_count', 'a running count', ''],
      ['rank', 'its place in order', ''],
    ],
  ],
  [
    'Arithmetic',
    [
      ['add', 'plus', 'slot'],
      ['sub', 'minus', 'slot'],
      ['mul', 'times', 'slot'],
      ['truediv', 'divided by', 'slot'],
      ['abs', 'without its sign', ''],
      ['round', 'to a number of decimals', 'number'],
    ],
  ],
  [
    'Compare',
    [
      ['gt', 'greater than', 'slot'],
      ['ge', 'at least', 'slot'],
      ['lt', 'less than', 'slot'],
      ['le', 'at most', 'slot'],
      ['eq', 'equal to', 'slot'],
      ['is_in', 'one of', 'slot'],
    ],
  ],
  [
    'Aggregate',
    [
      ['sum', 'the total', ''],
      ['mean', 'the average', ''],
      ['min', 'the smallest', ''],
      ['max', 'the largest', ''],
      ['count', 'how many', ''],
      ['first', 'the first', ''],
      ['last', 'the last', ''],
    ],
  ],
];

/** What each method on offer takes, by its name. */
/** @type {Map<string, string>} */
const METHOD_TAKES = new Map();
for (const [, methods] of METHODS_ON_OFFER) {
  for (const [name, , what] of methods) METHOD_TAKES.set(name, what);
}

/** The steps on offer: the verb, a word on what it does, and the key its colour goes by. */
/** @type {[string, string, string][]} */
const STEPS_ON_OFFER = [
  ['Add column', 'a new column from a chain', 'add'],
  ['Filter table', 'the rows where a condition holds', 'filter'],
  ['Sort table', 'the rows in the order of some fields', 'sort'],
  ['Select', 'some of the columns', 'select'],
  ['Rename', 'columns under new names', 'rename'],
];

/**
 * The functions on offer. A function is not called on a value, so it can
 * only stand where a chain starts. Each has a word on what it gives, and
 * what its list under the step holds: `cases`, or `values`, one to a line.
 */
/** @type {[string, string, string][]} */
const FUNCTIONS_ON_OFFER = [
  ['when', 'one value or another, by cases', 'cases'],
  ['coalesce', 'the first value there is', 'values'],
  ['concat_str', 'texts joined into one', 'values'],
  ['sum_horizontal', 'the values added up', 'values'],
  ['max_horizontal', 'the largest of the values', 'values'],
  ['min_horizontal', 'the smallest of the values', 'values'],
];

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
  const first = parts
    .map((part) => (part.classList.contains('cmp') ? part : part.querySelector('.cmp')))
    .find((part) => part !== null)
    ?.querySelector('.slot');
  if (first instanceof HTMLElement) openSlot(first);
}

/**
 * Returns `comparison` as the whole condition of a case: the comparison, and
 * the word that waits to turn the condition into a list.
 * @param {HTMLElement} comparison
 */
function conditionOf(comparison) {
  const condition = document.createElement('span');
  condition.className = 'cond';
  // Where the comparison is on the page already, the condition takes its place.
  comparison.replaceWith(condition);
  condition.append(comparison, slotOf('join', 'and', 'join waits'));
  return condition;
}

/**
 * Returns a line of a list: its number, written in later, then `parts`.
 * @param {HTMLElement[]} parts
 * @param {boolean} waits Whether it is the faint line that waits to add the next one.
 */
function lineOf(parts, waits = false) {
  const line = document.createElement('div');
  line.className = waits ? 'line waits' : 'line';
  const mark = document.createElement('span');
  mark.className = 'mark';
  line.append(mark, ...parts);
  return line;
}

/**
 * Numbers the lines of `list` from `prefix`, starting after `start`, and
 * the lists under each line from that line's number. Two lists under one
 * line are numbered as one, so no two lines share a number. A line that
 * waits has no number.
 * @param {Element} list
 * @param {string} prefix
 * @param {number} start How many lines under `prefix` are numbered already.
 * @returns {number} How many are numbered now.
 */
function numberLines(list, prefix, start = 0) {
  let count = start;
  let last = prefix;
  let under = 0;
  for (const part of list.children) {
    if (part.classList.contains('lines')) {
      under = numberLines(part, last, under);
    } else if (part.classList.contains('line') && !part.classList.contains('waits')) {
      count += 1;
      under = 0;
      last = `${prefix}.${String(count)}`;
      const mark = part.querySelector(':scope > .mark');
      if (mark !== null) mark.textContent = last;
    }
  }
  return count;
}

/**
 * Numbers again every line under the step that `part` belongs to.
 * @param {Element} part
 */
function renumber(part) {
  const card = part.closest('.card');
  if (card !== null) numberSteps(card);
}

/**
 * Returns the list that the group word in `slot` heads: under its step when
 * it is in the step's head, or under its own line.
 * @param {HTMLElement} slot
 */
function listUnder(slot) {
  const next = (slot.closest('.line') ?? slot.closest('.row'))?.nextElementSibling ?? null;
  return next !== null && next.classList.contains('lines') ? next : null;
}

/**
 * Makes the word that waits at the end of a group's list say what the group
 * means: and where all of its lines must hold, or where any may.
 * @param {HTMLElement} slot The group word.
 */
function suitWaiting(slot) {
  const waiting = listUnder(slot)?.querySelector(':scope > .line.waits > .slot');
  if (!(waiting instanceof HTMLElement)) return;
  waiting.dataset.value = slot.dataset.value === 'any of' ? 'or' : 'and';
  showSlot(waiting);
}

/**
 * Puts a new line before the line that waits in `slot`: a comparison, or a
 * group of the other kind with a comparison and a waiting word of its own.
 * @param {HTMLElement} slot The word that waits.
 * @param {string | undefined} kind `line-group` for a group, or a comparison.
 */
function addLine(slot, kind) {
  const waiting = slot.closest('.line');
  if (waiting === null) return;
  /** @type {HTMLElement[]} */
  let parts = [lineOf([emptyComparison()])];
  if (kind === 'line-group') {
    const inner = slot.dataset.value === 'and' ? 'or' : 'and';
    const group = slotOf('join', inner === 'or' ? 'any of' : 'all of', 'join');
    group.dataset.takes = 'group';
    const list = document.createElement('div');
    list.className = 'lines';
    const word = slotOf('join', inner, 'join waits');
    word.dataset.takes = 'line';
    list.append(lineOf([emptyComparison()]), lineOf([word], true));
    parts = [lineOf([group]), list];
  }
  waiting.before(...parts);
  renumber(waiting);
  drawNew(parts);
}

/**
 * Returns a word in a list that is not a block, such as then.
 * @param {string} text
 */
function labelOf(text) {
  const label = document.createElement('span');
  label.className = 'label';
  label.textContent = text;
  return label;
}

/** Returns a case with nothing in it yet: a comparison, then a value. */
function caseLine() {
  return lineOf([conditionOf(emptyComparison()), labelOf('then'), slotOf('none', 'value')]);
}

/**
 * Returns the faint line that waits at the end of a list to add the next
 * one, with `word` in it.
 * @param {string} word
 * @param {string} adds What a click on it adds.
 * @param {number} cells How many empty cells follow, in a list laid out in columns.
 */
function waitingLine(word, adds, cells = 0) {
  const slot = slotOf('join', word, 'join waits');
  slot.dataset.takes = adds;
  const parts = [slot];
  for (let cell = 0; cell < cells; cell += 1) parts.push(document.createElement('span'));
  return lineOf(parts, true);
}

/** Returns the slot a chain starts from, with nothing picked: a value, an input, a field or a function. */
function sourceSlot() {
  const slot = slotOf('none', 'field');
  slot.dataset.takes = 'function value input field';
  return slot;
}

/** Returns a chain with nothing in it yet: where it starts, and the method that waits. */
function freshChain() {
  const chain = document.createElement('span');
  chain.className = 'chain';
  chain.append(sourceSlot(), waitingMethod());
  return chain;
}

/**
 * Returns the function called `name` as a block with three dots, and the
 * list those dots own, still empty.
 * @param {string} name
 */
function functionOf(name) {
  if (!(pieceTemplate instanceof HTMLTemplateElement)) return null;
  const piece = pieceFor(pieceTemplate, {
    state: 'live',
    name: null,
    fn: name,
    typed: null,
    slot: null,
    many: null,
  });
  const body = piece?.querySelector('.body') ?? null;
  if (piece === null || body === null) return null;
  nestsLowered += 1;
  const nest = document.createElement('span');
  nest.className = 'nest is-open';
  nest.dataset.nest = String(nestsLowered);
  const token = document.createElement('button');
  token.type = 'button';
  token.className = 'btn shown is-chain nest-token';
  token.setAttribute('aria-label', `What ${name} takes`);
  nest.append(token);
  body.append(nest);
  const list = document.createElement('div');
  list.className = 'lines';
  list.dataset.nest = nest.dataset.nest;
  return { piece, list };
}

/**
 * Starts a chain from the function called `name`, in place of `slot`. What
 * the function takes is a list under the step or line the chain is on:
 * cases for when, and for the others values, one to a line.
 * @param {HTMLElement} slot Where the chain starts.
 * @param {string} name
 */
function startWith(slot, name) {
  const made = functionOf(name);
  const owner = slot.closest('.line, .row, .subrow');
  if (made === null || owner === null) return;
  if (FUNCTIONS_ON_OFFER.find(([known]) => known === name)?.[2] === 'cases') {
    made.list.classList.add('is-cases');
    const last = lineOf([
      labelOf('otherwise'),
      document.createElement('span'),
      slotOf('none', 'value'),
    ]);
    made.list.append(caseLine(), waitingLine('when', 'case', 2), last);
  } else {
    made.list.append(
      lineOf([freshChain()]),
      lineOf([freshChain()]),
      waitingLine('value', 'value-line'),
    );
  }
  slot.replaceWith(made.piece);
  owner.after(made.list);
  renumber(made.list);
  settle(made.list);
}

/**
 * Wraps the chain so far in the function called `name`. What stands before
 * the method's place in `slot` goes inside the function as its first value,
 * and whatever follows carries on after the function.
 * @param {HTMLElement} slot The method that waits, or the one that is pending.
 * @param {string} name
 */
function wrapIn(slot, name) {
  const chain = slot.closest('.chain');
  const here = slot.closest('.link');
  const owner = slot.closest('.line, .row, .subrow');
  const made = functionOf(name);
  if (chain === null || here === null || owner === null || made === null) return;
  const inner = document.createElement('span');
  inner.className = 'chain';
  while (chain.firstElementChild !== null && chain.firstElementChild !== here) {
    inner.append(chain.firstElementChild);
  }
  inner.append(waitingMethod());
  const next = lineOf([freshChain()]);
  made.list.append(lineOf([inner]), next, waitingLine('value', 'value-line'));
  if (slot.classList.contains('waits')) {
    showSlot(slot);
    chain.prepend(made.piece);
  } else {
    slot.classList.remove('is-pending');
    here.replaceWith(made.piece);
  }
  owner.after(made.list);
  renumber(made.list);
  draw(made.list);
  const first = next.querySelector('.slot');
  if (first instanceof HTMLElement) openSlot(first);
}

/**
 * Puts a new value before the word that waits in `slot`, in the list of a
 * function that takes several: a chain to start and carry on.
 * @param {HTMLElement} slot The word that waits.
 */
function addValueLine(slot) {
  const waiting = slot.closest('.line');
  if (waiting === null) return;
  const line = lineOf([freshChain()]);
  waiting.before(line);
  renumber(waiting);
  settle(line);
}

/**
 * Puts a new case before the `when` that waits in `slot`: a comparison, then
 * a value, both to fill in.
 * @param {HTMLElement} slot The word that waits.
 */
function addCase(slot) {
  const waiting = slot.closest('.line');
  if (waiting === null) return;
  const line = caseLine();
  waiting.before(line);
  renumber(waiting);
  drawNew([line]);
}

/**
 * Returns the lists that the dots in `comparison` own, as lists of lines. A
 * list that was a single line under a step is made into one.
 * @param {HTMLElement} comparison
 */
function listsOwnedBy(comparison) {
  /** @type {HTMLElement[]} */
  const lists = [];
  for (const nest of comparison.querySelectorAll('.nest')) {
    const name = nest.getAttribute('data-nest') ?? '';
    const owned = document.querySelector(`:is(.subrow, .lines)[data-nest="${name}"]`);
    if (!(owned instanceof HTMLElement)) continue;
    if (owned.classList.contains('lines')) {
      lists.push(owned);
      continue;
    }
    const chain = owned.querySelector('.chain');
    const list = document.createElement('div');
    list.className = owned.classList.contains('is-shut') ? 'lines is-shut' : 'lines';
    list.dataset.nest = name;
    if (chain instanceof HTMLElement) list.append(lineOf([chain]));
    owned.remove();
    lists.push(list);
  }
  return lists;
}

/**
 * Moves the one comparison before the waiting word in `slot` into a list of
 * its own: the word becomes all of, and under the step, or under the case,
 * are the comparison, a new one to fill in, and a word that waits.
 * @param {HTMLElement} slot The word that waits after the comparison.
 */
function intoList(slot) {
  const comparison = slot.previousElementSibling;
  const owner = slot.closest('.line') ?? slot.closest('.row');
  if (!(comparison instanceof HTMLElement) || owner === null) return;
  const group = slotOf('join', 'all of', 'join');
  group.dataset.takes = 'group';
  const word = slotOf('join', 'and', 'join waits');
  word.dataset.takes = 'line';
  const owned = listsOwnedBy(comparison);
  const added = lineOf([emptyComparison()]);
  const list = document.createElement('div');
  list.className = 'lines';
  list.append(lineOf([comparison]), ...owned, added, lineOf([word], true));
  slot.replaceWith(group);
  owner.after(list);
  renumber(list);
  for (const part of [group, word]) showSlot(part);
  drawNew([added]);
}

/**
 * Takes `line` away, with any list under it.
 * @param {HTMLElement} line
 */
function removeLine(line) {
  const list = line.parentElement;
  while (line.nextElementSibling?.classList.contains('lines') === true) {
    line.nextElementSibling.remove();
  }
  line.remove();
  if (list !== null) renumber(list);
}

/**
 * Returns a link of a chain: a caret, then `part`.
 * @param {HTMLElement} part
 */
function linkOf(part) {
  const link = document.createElement('span');
  link.className = 'link';
  const seam = document.createElement('span');
  seam.className = 'seam';
  link.append(seam, part);
  return link;
}

/** Returns the faint method that waits at the end of a chain, behind its caret. */
function waitingMethod() {
  const slot = slotOf('none', 'method', 'method waits');
  slot.dataset.takes = 'method';
  const link = linkOf(slot);
  link.classList.add('waits');
  return link;
}

/**
 * Returns a row that holds only a word for a step: the faint one that waits
 * after the last step, or one that is there until a step is picked.
 * @param {string} state `waits` or `is-pending`.
 */
function stepWord(state) {
  const row = document.createElement('div');
  row.className = `row step ${state}`;
  const num = document.createElement('span');
  num.className = 'num';
  const slot = slotOf('none', 'step', `verb ${state}`);
  slot.dataset.takes = 'verb';
  row.append(num, slot);
  return row;
}

/** Returns the faint step that waits after the last step. */
function waitingStep() {
  return stepWord('waits');
}

/**
 * Draws what was just put on the page: its slots, and its typed blocks.
 * @param {HTMLElement} part
 */
function draw(part) {
  for (const slot of part.querySelectorAll('.slot')) {
    if (slot instanceof HTMLElement) showSlot(slot);
  }
  for (const input of part.querySelectorAll('.type input')) {
    if (input instanceof HTMLInputElement) tend(input);
  }
  foldAll();
}

/**
 * Draws what was just put on the page, and goes to the first thing in it
 * that is to be filled in: a name or value to type, or a slot to pick for.
 * @param {HTMLElement} part
 */
function settle(part) {
  draw(part);
  const first = part.querySelector('.type input, .slot:not(.waits)');
  if (first instanceof HTMLInputElement) {
    first.focus();
    first.select();
  } else if (first instanceof HTMLElement) {
    openSlot(first);
  }
}

/**
 * Puts the method called `name` into the chain of `slot`: before the method
 * that waits, or in place of the one that was pending.
 * @param {HTMLElement} slot
 * @param {string} name
 */
function addMethod(slot, name) {
  if (!(pieceTemplate instanceof HTMLTemplateElement)) return;
  const what = METHOD_TAKES.get(name) ?? '';
  const typed = new Map([
    ['number', 'number:1'],
    ['text', 'text:'],
  ]);
  const piece = pieceFor(pieceTemplate, {
    state: 'live',
    name: null,
    fn: name,
    typed: typed.get(what) ?? null,
    slot: what === 'slot' ? 'none:value' : null,
    many: what === 'many' ? '' : null,
  });
  if (piece === null) return;
  if (slot.classList.contains('waits')) {
    showSlot(slot);
    slot.closest('.link')?.before(linkOf(piece));
  } else {
    slot.classList.remove('is-pending');
    slot.replaceWith(piece);
  }
  settle(piece);
}

/**
 * Opens a list of methods after `piece`, a block in a chain. The place is
 * held by a pending method, which goes again if nothing is picked.
 * @param {Element} piece
 */
function pendMethod(piece) {
  const slot = slotOf('none', 'method', 'method is-pending');
  slot.dataset.takes = 'method';
  (piece.closest('.link') ?? piece).after(linkOf(slot));
  showSlot(slot);
  openSlot(slot);
}

/**
 * Takes `piece` out of its chain, with its caret and any list its dots own.
 * The block a chain starts from stays.
 * @param {Element} piece
 */
function removeMethod(piece) {
  const link = piece.parentElement;
  const starts = link !== null && !link.classList.contains('link');
  // The block a chain starts from stays, unless it is a function, which leaves an empty start.
  if (link === null || (starts && piece.querySelector('.nest') === null)) return;
  for (const nest of piece.querySelectorAll('.nest')) {
    const name = nest.getAttribute('data-nest') ?? '';
    document.querySelector(`:is(.subrow, .lines)[data-nest="${name}"]`)?.remove();
  }
  if (starts) {
    const slot = sourceSlot();
    piece.replaceWith(slot);
    showSlot(slot);
  } else {
    link.remove();
  }
  renumber(link);
  foldAll();
}

/**
 * Numbers the steps of `card` in order, and the lists under each from it.
 * @param {Element} card
 */
function numberSteps(card) {
  let count = 0;
  let under = 0;
  for (const part of card.children) {
    if (part.classList.contains('step')) {
      if (part.classList.contains('waits') || part.classList.contains('is-pending')) continue;
      count += 1;
      under = 0;
      const num = part.querySelector(':scope > .num');
      if (num !== null) num.textContent = String(count);
    } else if (part.classList.contains('subrow')) {
      under += 1;
      const mark = part.querySelector(':scope > .mark');
      if (mark !== null) mark.textContent = `${String(count)}.${String(under)}`;
    } else if (part.classList.contains('lines')) {
      under = numberLines(part, String(count), under);
    }
  }
}

/**
 * Returns a new step for `verb`, with what it takes still to be filled in.
 * @param {string} verb
 */
function stepOf(verb) {
  const source = document.querySelector('#step-head');
  const head =
    source instanceof HTMLTemplateElement
      ? source.content.firstElementChild?.cloneNode(true)
      : null;
  if (!(head instanceof HTMLElement)) return null;
  const body = head.querySelector('.body');
  const button = head.querySelector('.btn.verb');
  if (body === null || button === null) return null;
  button.textContent = verb;
  button.classList.add('live');
  const key = STEPS_ON_OFFER.find(([name]) => name === verb)?.[2] ?? 'add';
  const row = document.createElement('div');
  row.className = 'row step';
  row.dataset.verb = key;
  const num = document.createElement('span');
  num.className = 'num';
  row.append(num, head);
  if (key === 'add') {
    // A column: a name to type, then a chain that starts from a field to pick.
    const label = document.createElement('label');
    label.className = 'type is-name';
    const input = document.createElement('input');
    input.value = 'new_column';
    input.spellcheck = false;
    input.setAttribute('aria-label', 'Name of the new column');
    label.append(input);
    body.append(label);
    const equals = document.createElement('span');
    equals.className = 'equals';
    const chain = document.createElement('span');
    chain.className = 'chain';
    chain.append(sourceSlot(), waitingMethod());
    row.append(equals, chain);
  } else if (key === 'filter') {
    body.append(emptyComparison(), slotOf('join', 'and', 'join waits'));
  } else {
    if (key === 'sort') {
      const by = document.createElement('span');
      by.className = 'word';
      by.textContent = 'by';
      body.append(by);
    }
    const slot = document.createElement('span');
    slot.className = key === 'rename' ? 'slot is-many is-dict' : 'slot is-many';
    slot.dataset.takes = 'field';
    if (key === 'rename') slot.dataset.pairs = '';
    else slot.dataset.many = '';
    body.append(slot);
  }
  return row;
}

/**
 * Puts a step for `verb` where the word in `slot` is: before the step that
 * waits, or in place of the one that was pending.
 * @param {HTMLElement} slot
 * @param {string} verb
 */
function addStep(slot, verb) {
  const word = slot.closest('.row');
  const row = stepOf(verb);
  if (word === null || row === null) return;
  const card = word.closest('.card');
  if (slot.classList.contains('waits')) {
    showSlot(slot);
    word.before(row);
  } else {
    slot.classList.remove('is-pending');
    word.replaceWith(row);
  }
  if (card !== null) numberSteps(card);
  settle(row);
}

/**
 * Opens a list of steps after `row` and the lists under it. The place is
 * held by a pending step, which goes again if nothing is picked.
 * @param {Element} row
 */
function pendStep(row) {
  let last = row;
  while (last.nextElementSibling !== null && !last.nextElementSibling.classList.contains('row')) {
    last = last.nextElementSibling;
  }
  const word = stepWord('is-pending');
  last.after(word);
  const slot = word.querySelector('.slot');
  if (!(slot instanceof HTMLElement)) return;
  showSlot(slot);
  openSlot(slot);
}

/**
 * Takes the step in `row` away, with the lists under it.
 * @param {Element} row
 */
function removeStep(row) {
  const card = row.closest('.card');
  while (row.nextElementSibling !== null && !row.nextElementSibling.classList.contains('row')) {
    row.nextElementSibling.remove();
  }
  row.remove();
  if (card !== null) numberSteps(card);
}

// The panels at the sides of a block. On a block in a chain, + puts a method after it and × takes
// it away. On a step's head they add a step after it and take the step away.
document.addEventListener('click', (event) => {
  const side = event.target instanceof Element ? event.target.closest('.side') : null;
  const piece = side?.closest('.piece');
  if (side === null || side === undefined || piece === null || piece === undefined) return;
  const adds = side.classList.contains('side-add');
  if (piece.classList.contains('head')) {
    const row = piece.closest('.row.step');
    if (row === null) return;
    if (adds) pendStep(row);
    else removeStep(row);
  } else if (piece.closest('.chain') !== null) {
    if (adds) pendMethod(piece);
    else removeMethod(piece);
  }
});

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
  const wasOpen = slot.classList.contains('is-open');
  slot.classList.remove('is-open');
  slot.querySelector('.offers')?.remove();
  if (wasOpen && slot.classList.contains('is-pending')) {
    // A place that was held for something to be added goes when nothing was picked for it.
    (slot.closest('.link, .row') ?? slot).remove();
    return;
  }
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
    // First, what can be done with the comparison's line in a list: take it away, where it is
    // not the only one. Adding is left to the word that waits.
    const line = slot.closest('.cmp')?.parentElement ?? null;
    const isLine = line?.classList.contains('line') ?? false;
    const lines = line?.parentElement?.querySelectorAll(':scope > .line:not(.waits)').length ?? 0;
    if (sought === '' && isLine && lines > 1) {
      const acts = [actRow('remove', 'remove', '× remove', 'this line')];
      groups.push(offerGroup('This condition', null, acts));
    }
    const rows = COMPARISONS_ON_OFFER.filter(isSought).map(([name, word]) =>
      offerRow('op', name, word, false),
    );
    if (rows.length > 0) groups.push(offerGroup('Comparison', null, rows));
  }
  if (takes(slot, 'method')) {
    // The groups go from the shortest to the longest, as they do for a value.
    const found = [];
    for (const [title, methods] of METHODS_ON_OFFER) {
      const rows = methods
        .filter(([name, word]) => isSought([name, word]))
        .map(([name, word]) => offerRow('method', name, word, false));
      if (rows.length > 0) found.push(offerGroup(title, rows.length, rows));
    }
    found.sort((left, right) => left.childElementCount - right.childElementCount);
    groups.push(...found);
    // Last, the functions that take several values: one of them wraps the chain so far.
    const wraps = FUNCTIONS_ON_OFFER.filter(
      ([name, word, holds]) => holds === 'values' && isSought([name, word]),
    ).map(([name, word]) => offerRow('wrap', name, word, false));
    if (wraps.length > 0) groups.push(offerGroup('Wrap in a function', null, wraps));
  }
  if (takes(slot, 'verb')) {
    const rows = STEPS_ON_OFFER.filter(([name, word]) => isSought([name, word])).map(
      ([name, word]) => offerRow('verb', name, word, false),
    );
    if (rows.length > 0) groups.push(offerGroup('Step', null, rows));
  }
  if (takes(slot, 'line')) {
    const group = slot.dataset.value === 'and' ? 'any of' : 'all of';
    const rows = [
      actRow('line-cmp', 'comparison', 'field == value', 'a comparison'),
      actRow('line-group', group, group, 'a group with lines of its own'),
    ];
    groups.push(offerGroup('Add a line', null, rows));
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
  if (takes(slot, 'function')) {
    const rows = FUNCTIONS_ON_OFFER.filter(([name, word]) => isSought([name, word])).map(
      ([name, word]) => offerRow('function', name, word, false),
    );
    if (rows.length > 0) groups.push(offerGroup('Functions', rows.length, rows));
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
  const isValue = takes(slot, 'value');
  input.placeholder = many ? 'Search fields' : isValue ? 'Type a value, or search' : 'Search';
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
  if (takes(slot, 'method')) {
    if (row.dataset.kind === 'wrap') wrapIn(slot, value);
    else addMethod(slot, value);
    return;
  }
  if (row.dataset.kind === 'function') {
    startWith(slot, value);
    return;
  }
  if (takes(slot, 'verb')) {
    addStep(slot, value);
    return;
  }
  if (row.dataset.kind === 'remove') {
    // The row acts on the line the comparison stands on.
    showSlot(slot);
    const line = comparison?.parentElement ?? null;
    if (line instanceof HTMLElement && line.classList.contains('line')) removeLine(line);
    return;
  }
  if (takes(slot, 'line')) {
    showSlot(slot);
    addLine(slot, row.dataset.kind);
    return;
  }
  if (!slot.classList.contains('is-many')) {
    slot.dataset.kind = row.dataset.kind;
    slot.dataset.value = value;
    showSlot(slot);
    if (takes(slot, 'group')) suitWaiting(slot);
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
  } else if (takes(slot, 'case')) {
    // There is one thing a waiting when can add, so it adds it without a list.
    addCase(slot);
  } else if (takes(slot, 'value-line')) {
    addValueLine(slot);
  } else if (takes(slot, 'join')) {
    // Nor is there a choice after a lone comparison: the condition becomes a list.
    intoList(slot);
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

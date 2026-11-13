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
 * @property {'pick' | 'type' | 'list-settled' | 'head' | 'button' | 'function' | 'seam'} kind
 *   What its card shows.
 */

/** @type {Version[]} */
const VERSIONS = [
  {
    id: 'pick-list',
    name: 'Grouped list',
    note: 'One list in groups, the shortest first: what was typed as a value, then inputs, then fields.',
    kind: 'pick',
  },
  {
    id: 'pick-panel',
    name: 'Panel',
    note: 'The groups stand side by side under the step, and fields can be cut down by dtype.',
    kind: 'pick',
  },
  {
    id: 'type-block',
    name: 'Block',
    note: 'A block like any other at rest. Typing empties its fill and rings it in its own colour.',
    kind: 'type',
  },
  {
    id: 'type-well',
    name: 'Well',
    note: 'A recess, always: what is typed sits in the surface, what is picked sits on it.',
    kind: 'type',
  },
  {
    id: 'type-line',
    name: 'Line',
    note: 'No fill, a dashed line beneath, as a blank to be written on.',
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
    note: 'Each value is one more block a hair apart. Point at one and the run parts around it. Point at a bracket and the function opens.',
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
 * @property {string[]} list The columns a function takes as a list, if it takes one.
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
  const inRow = slot.closest('.row') !== null;
  const shown = version.kind === 'function' ? SHOWN_FUNCTION : null;
  const list = slot.getAttribute('data-list');
  return {
    state: slot.getAttribute('data-piece'),
    name: slot.getAttribute('data-name'),
    fn: inRow ? slot.getAttribute('data-fn') : shown,
    list: list === null ? [] : list.split(','),
    typed: slot.getAttribute('data-type'),
    slot: slot.getAttribute('data-slot'),
    many: slot.getAttribute('data-many'),
  };
}

/**
 * Makes `piece` a function: bracketed, its name in the first segment, and
 * after it either the list it takes or the one field it works on.
 * @param {HTMLTemplateElement} source
 * @param {HTMLElement} piece
 * @param {HTMLButtonElement} button
 * @param {Spec} spec
 * @param {string} fn
 */
function makeFunction(source, piece, button, spec, fn) {
  piece.classList.add('fn', 'is-bracketed');
  button.textContent = fn;
  if (spec.list.length > 0) {
    const list = document.createElement('span');
    list.className = 'list';
    for (const name of spec.list) {
      const item = pieceFor(source, {
        state: spec.state,
        name,
        fn: null,
        list: [],
        typed: null,
        slot: null,
        many: null,
      });
      if (item !== null) list.append(item);
    }
    button.after(list);
    return;
  }
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
  if (spec.fn !== null) makeFunction(source, piece, button, spec, spec.fn);

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
    for (const part of card.querySelectorAll('.row .body > .btn')) part.classList.add('live');
    for (const slot of card.querySelectorAll('[data-piece]')) {
      const piece = pieceFor(pieceTemplate, specOf(slot, version));
      if (piece !== null) slot.replaceWith(piece);
    }
    const sectioned = version.kind === 'type' || version.kind === 'pick';
    const section = sectioned ? `#versions-${version.kind}` : '#versions-settled';
    document.querySelector(section)?.append(card);
  }
}

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
 * kind, or several fields as several blocks. A click turns it into a field
 * with what it can take offered beneath: what is typed narrows the offer, and
 * whatever is picked becomes the value. A slot that takes several fields
 * stays open while they are ticked.
 */

/**
 * @typedef {object} Named Something on offer that has a name.
 * @property {string} name What it is called.
 * @property {string} dtype The dtype it holds.
 */

/** The dtypes the fields come in, for cutting a long offer down. */
const DTYPES = ['String', 'Int64', 'Float64', 'Date', 'Datetime', 'Boolean'];

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
  return (slot.dataset.takes ?? 'value input field chain').split(' ').includes(what);
}

/**
 * Returns the fields ticked in a slot that takes several.
 * @param {HTMLElement} slot
 * @returns {string[]}
 */
function ticked(slot) {
  const many = slot.dataset.many ?? '';
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
 * Draws `slot` at rest: its value as a block, or its fields as blocks.
 * @param {HTMLElement} slot
 */
function showSlot(slot) {
  slot.classList.remove('is-open');
  // A dtype the fields were cut down to lasts only while the slot is open.
  delete slot.dataset.dtype;
  if (!slot.classList.contains('is-many')) {
    slot.replaceChildren(blockOf(slot.dataset.kind ?? 'text', slot.dataset.value ?? ''));
    return;
  }
  const fields = ticked(slot);
  slot.replaceChildren(
    ...(fields.length > 0
      ? fields.map((name) => blockOf('column', name))
      : [blockOf('none', 'no fields')]),
  );
}

/**
 * Returns one row of the offer: a tick box, the value as a block, and a word
 * on what it is.
 * @param {string} kind
 * @param {string} value
 * @param {string} word
 * @param {boolean} isTicked
 */
function offerRow(kind, value, word, isTicked) {
  const row = document.createElement('button');
  row.type = 'button';
  row.tabIndex = -1;
  row.className = isTicked ? 'offer is-ticked' : 'offer';
  row.dataset.kind = kind;
  row.dataset.value = value;
  const box = document.createElement('span');
  box.className = 'tick';
  const what = document.createElement('i');
  what.textContent = word;
  row.append(box, blockOf(kind, value), what);
  return row;
}

/**
 * Returns one group of the offer: its heading, how many rows it has when
 * that is worth saying, and the rows.
 * @param {string} id
 * @param {string} title
 * @param {number | null} count
 * @param {HTMLElement[]} rows
 * @param {HTMLElement | null} above Something to stand between the heading and the rows.
 */
function offerGroup(id, title, count, rows, above) {
  const group = document.createElement('span');
  group.className = `group group-${id}`;
  const heading = document.createElement('span');
  heading.className = 'group-head';
  heading.textContent = title;
  if (count !== null) {
    const number = document.createElement('i');
    number.textContent = String(count);
    heading.append(number);
  }
  const body = document.createElement('span');
  body.className = 'group-body';
  body.append(...rows);
  group.append(heading, ...(above === null ? [] : [above]), body);
  return group;
}

/**
 * Returns the row of dtypes that cuts the fields down, with the one in force marked.
 * @param {string} inForce
 */
function dtypeChips(inForce) {
  const chips = document.createElement('span');
  chips.className = 'dtypes';
  for (const dtype of ['', ...DTYPES]) {
    const chip = document.createElement('button');
    chip.type = 'button';
    chip.tabIndex = -1;
    chip.className = dtype === inForce ? 'chip is-on' : 'chip';
    chip.dataset.dtype = dtype;
    chip.textContent = dtype === '' ? 'All' : dtype;
    chips.append(chip);
  }
  return chips;
}

/**
 * Draws what an open slot offers for what has been typed, with the row at
 * `chosen` marked. The groups go from the shortest to the longest: what was
 * typed as a value, then the inputs, then the fields.
 * @param {HTMLElement} slot
 * @param {string} typed
 * @param {number} chosen
 */
function listOffers(slot, typed, chosen) {
  const list = slot.querySelector('.offers');
  if (list === null) return;
  const sought = typed.trim().toLowerCase();
  const isTicked = new Set(ticked(slot));
  /** @type {HTMLElement[]} */
  const groups = [];
  if (takes(slot, 'value')) {
    const rows = [];
    if (/^-?\d+$/.test(sought)) rows.push(offerRow('number', sought, 'Int64', false));
    if (typed !== '') rows.push(offerRow('text', typed, 'String', false));
    rows.push(offerRow('none', 'None', 'no value', false));
    groups.push(offerGroup('value', 'Value', null, rows, null));
  }
  if (takes(slot, 'input')) {
    const inputs = INPUTS_ON_OFFER.filter((input) => input.name.includes(sought));
    const rows = inputs.map((input) => offerRow('input', input.name, input.dtype, false));
    // A group with nothing in it is left out.
    if (rows.length > 0) groups.push(offerGroup('input', 'Inputs', inputs.length, rows, null));
  }
  if (takes(slot, 'field')) {
    const dtype = slot.dataset.dtype ?? '';
    const fields = FIELDS_ON_OFFER.filter(
      (field) => field.name.includes(sought) && (dtype === '' || field.dtype === dtype),
    );
    // The fields already ticked come first, so they are in view however long the list is.
    fields.sort(
      (left, right) => Number(isTicked.has(right.name)) - Number(isTicked.has(left.name)),
    );
    const rows = fields.map((field) =>
      offerRow('column', field.name, field.dtype, isTicked.has(field.name)),
    );
    // The fields stay even when none match while a dtype is in force, so the dtype can be undone.
    if (rows.length > 0 || dtype !== '') {
      groups.push(offerGroup('field', 'Fields', fields.length, rows, dtypeChips(dtype)));
    }
  }
  if (takes(slot, 'chain')) {
    const chain = offerRow('chain', 'chain', 'a chain of its own', false);
    chain.classList.add('offer-chain');
    groups.push(chain);
  }
  list.replaceChildren(...groups);
  const rows = list.querySelectorAll('.offer');
  const marked = Math.max(0, Math.min(chosen, rows.length - 1));
  rows[marked]?.classList.add('is-chosen');
  slot.dataset.chosen = String(marked);
}

/**
 * Opens `slot`: a field to type in, and what it can take offered beneath. In
 * the panel the offer is as wide as the step it belongs to.
 * @param {HTMLElement} slot
 */
function openSlot(slot) {
  // The field starts as wide as what it takes the place of, so nothing beside it jumps.
  const shown = slot.getBoundingClientRect();
  slot.classList.add('is-open');
  const field = document.createElement('label');
  field.className = 'type';
  const input = document.createElement('input');
  input.spellcheck = false;
  const many = slot.classList.contains('is-many');
  input.setAttribute('aria-label', many ? 'Find a field' : 'A value: type it, or pick one');
  const kind = slot.dataset.kind;
  const typed = many || kind === 'none' || kind === 'chain' ? '' : (slot.dataset.value ?? '');
  input.value = typed;
  field.append(input);
  const list = document.createElement('span');
  list.className = 'offers';
  slot.replaceChildren(field, list);
  field.style.minWidth = `${shown.width}px`;
  input.style.width = `${Math.max(input.value.length, 1)}ch`;
  const row = slot.closest('.row')?.getBoundingClientRect();
  if (row !== undefined && slot.closest('.v-pick-panel') !== null) {
    list.style.left = `${row.left - shown.left}px`;
    list.style.width = `${row.width}px`;
  }
  // What is there was the value before, so nothing is narrowed until something is typed.
  listOffers(slot, '', 0);
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
  if (!slot.classList.contains('is-many')) {
    slot.dataset.kind = row.dataset.kind;
    slot.dataset.value = value;
    showSlot(slot);
    return;
  }
  const fields = ticked(slot);
  const next = fields.includes(value)
    ? fields.filter((name) => name !== value)
    : [...fields, value];
  slot.dataset.many = next.join(',');
  const typed = slot.querySelector('input')?.value ?? '';
  const scrolled = slot.querySelector('.group-field .group-body')?.scrollTop ?? 0;
  listOffers(slot, typed, Number(slot.dataset.chosen ?? 0));
  const body = slot.querySelector('.group-field .group-body');
  if (body !== null) body.scrollTop = scrolled;
}

for (const slot of document.querySelectorAll('.slot')) {
  if (slot instanceof HTMLElement) showSlot(slot);
}

document.addEventListener('click', (event) => {
  if (!(event.target instanceof Element)) return;
  const slot = event.target.closest('.slot');
  if (!(slot instanceof HTMLElement)) return;
  const chip = event.target.closest('.chip');
  if (chip instanceof HTMLElement) {
    slot.dataset.dtype = chip.dataset.dtype ?? '';
    listOffers(slot, slot.querySelector('input')?.value ?? '', 0);
    return;
  }
  const row = event.target.closest('.offer');
  if (row !== null) {
    takeOffer(slot, row);
  } else if (!slot.classList.contains('is-open')) {
    openSlot(slot);
  }
});

// A press on the offer must not take the field's focus, or the slot would close before the click.
document.addEventListener('pointerdown', (event) => {
  if (event.target instanceof Element && event.target.closest('.offers') !== null) {
    event.preventDefault();
  }
});

// Leaving the field closes the slot. Only the field counts: the block a click replaces loses the
// focus too, while the slot is opening.
document.addEventListener('focusout', (event) => {
  if (!(event.target instanceof HTMLInputElement)) return;
  const slot = event.target.closest('.slot.is-open');
  if (slot instanceof HTMLElement) showSlot(slot);
});

document.addEventListener('keydown', (event) => {
  if (!(event.target instanceof HTMLInputElement)) return;
  const slot = event.target.closest('.slot.is-open');
  if (!(slot instanceof HTMLElement)) return;
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

document.addEventListener('input', (event) => {
  if (!(event.target instanceof HTMLInputElement)) return;
  const slot = event.target.closest('.slot.is-open');
  if (slot instanceof HTMLElement) {
    event.target.style.width = `${Math.max(event.target.value.length, 1)}ch`;
    listOffers(slot, event.target.value, 0);
  } else {
    tend(event.target);
  }
});

document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') closeOthers(null);
  // Enter ends the typing, as leaving the block does. An open slot has its own answer to Enter.
  const typing = event.target instanceof HTMLInputElement ? event.target : null;
  if (event.key === 'Enter' && typing !== null && typing.closest('.slot') === null) typing.blur();
});

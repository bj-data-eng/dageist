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
 * @property {'slot' | 'type' | 'list-settled' | 'head' | 'button' | 'function' | 'seam'} kind
 *   What its card shows.
 */

/** @type {Version[]} */
const VERSIONS = [
  {
    id: 'slot',
    name: 'Type to pick',
    note: 'Click a value and type. What is typed can stay as text or a number, or pick a field, an input, nothing or a chain.',
    kind: 'slot',
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
  slot: '#card-slot',
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
      });
      if (item !== null) list.append(item);
    }
    button.after(list);
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
    const sectioned = version.kind === 'type' || version.kind === 'slot';
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
 * The slot that takes a value of any kind. It shows its value as a block of
 * the value's kind. A click turns it into a field with a list beneath: what
 * is typed narrows the list, and whatever is picked becomes the value.
 */

/** The fields of the table and the inputs connected to the step, to pick from. */
const FIELDS_ON_OFFER = ['dcn', 'timestamp', 'step_to', 'owner', 'row_days', 'resolved_at'];
const INPUTS_ON_OFFER = ['sla_cutoff', 'as_of'];

/** What the list says a kind is. */
const KIND_WORDS = {
  text: 'text',
  number: 'number',
  none: 'no value',
  column: 'column',
  input: 'input',
  chain: 'a chain of its own',
};

/**
 * @typedef {object} Offer One thing a slot can become.
 * @property {keyof typeof KIND_WORDS} kind The kind of value it is.
 * @property {string} value What the value reads as.
 */

/**
 * Lists what a slot can become for what has been typed: a number when it is
 * one, the fields and inputs it is part of, the text itself, nothing, and a
 * chain.
 * @param {string} typed
 * @returns {Offer[]}
 */
function offersFor(typed) {
  const sought = typed.trim().toLowerCase();
  /** @type {Offer[]} */
  const offers = [];
  if (/^-?\d+$/.test(sought)) offers.push({ kind: 'number', value: sought });
  for (const field of FIELDS_ON_OFFER) {
    if (field.includes(sought)) offers.push({ kind: 'column', value: field });
  }
  for (const input of INPUTS_ON_OFFER) {
    if (input.includes(sought)) offers.push({ kind: 'input', value: input });
  }
  if (typed !== '') offers.push({ kind: 'text', value: typed });
  if (sought === '' || 'none'.startsWith(sought)) offers.push({ kind: 'none', value: 'None' });
  offers.push({ kind: 'chain', value: 'chain' });
  return offers;
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
 * Draws `slot` at rest: its value as a block.
 * @param {HTMLElement} slot
 */
function showSlot(slot) {
  slot.classList.remove('is-open');
  const block = blockOf(slot.dataset.kind ?? 'text', slot.dataset.value ?? '');
  if (slot.classList.contains('is-picture')) block.tabIndex = -1;
  slot.replaceChildren(block);
}

/**
 * Draws the list under an open slot for what has been typed, with the offer
 * at `chosen` marked.
 * @param {HTMLElement} slot
 * @param {string} typed
 * @param {number} chosen
 */
function listOffers(slot, typed, chosen) {
  const list = slot.querySelector('.offers');
  if (list === null) return;
  const offers = offersFor(typed);
  list.replaceChildren(
    ...offers.map((offer, index) => {
      const row = document.createElement('button');
      row.type = 'button';
      row.tabIndex = -1;
      row.className = index === chosen ? 'offer is-chosen' : 'offer';
      row.dataset.kind = offer.kind;
      row.dataset.value = offer.value;
      const word = document.createElement('i');
      word.textContent = KIND_WORDS[offer.kind];
      row.append(blockOf(offer.kind, offer.value), word);
      return row;
    }),
  );
  slot.dataset.chosen = String(Math.min(chosen, offers.length - 1));
}

/**
 * Opens `slot`: a field holding its value, selected, and the list beneath.
 * @param {HTMLElement} slot
 */
function openSlot(slot) {
  // The field starts as wide as the block it takes the place of, so nothing beside it jumps.
  const shown = slot.getBoundingClientRect().width;
  slot.classList.add('is-open');
  const field = document.createElement('label');
  field.className = 'type';
  const input = document.createElement('input');
  input.spellcheck = false;
  input.setAttribute('aria-label', 'A value: type it, or pick from the list');
  const kind = slot.dataset.kind;
  input.value = kind === 'none' || kind === 'chain' ? '' : (slot.dataset.value ?? '');
  field.append(input);
  const list = document.createElement('span');
  list.className = 'offers';
  slot.replaceChildren(field, list);
  field.style.minWidth = `${shown}px`;
  input.style.width = `${Math.max(input.value.length, 1)}ch`;
  // What is there was the value before, so nothing is narrowed until something is typed.
  listOffers(slot, '', 0);
  input.focus();
  input.select();
}

/**
 * Makes the offer at `index` the value of `slot` and closes it.
 * @param {HTMLElement} slot
 * @param {number} index
 */
function takeOffer(slot, index) {
  const row = slot.querySelectorAll('.offer')[index];
  if (row instanceof HTMLElement) {
    slot.dataset.kind = row.dataset.kind;
    slot.dataset.value = row.dataset.value;
  }
  showSlot(slot);
}

for (const slot of document.querySelectorAll('.slot')) {
  if (slot instanceof HTMLElement) showSlot(slot);
}

document.addEventListener('click', (event) => {
  if (!(event.target instanceof Element)) return;
  const slot = event.target.closest('.slot:not(.is-picture)');
  if (!(slot instanceof HTMLElement)) return;
  const row = event.target.closest('.offer');
  if (row !== null) {
    takeOffer(slot, [...slot.querySelectorAll('.offer')].indexOf(row));
  } else if (!slot.classList.contains('is-open')) {
    openSlot(slot);
  }
});

// A press on the list must not take the field's focus, or the slot would close before the click.
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
  const chosen = Number(slot.dataset.chosen ?? 0);
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const count = slot.querySelectorAll('.offer').length;
    const next = (chosen + (event.key === 'ArrowDown' ? 1 : count - 1)) % count;
    listOffers(slot, event.target.value, next);
  } else if (event.key === 'Enter') {
    event.preventDefault();
    takeOffer(slot, chosen);
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

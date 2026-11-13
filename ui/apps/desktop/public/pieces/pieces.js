/**
 * @file Stamps one card for every version of the button and lets a click
 * hold a button open. A version is one entry here and one block in
 * pieces.css under `.v-<id>`.
 */

const VERSIONS = [
  { id: 'tint', name: 'Tint', note: 'A flat fill in the colour of what it names. Today’s look.' },
  {
    id: 'hairline',
    name: 'Hairline',
    note: 'An outline at rest. The fill arrives with the pointer.',
  },
  {
    id: 'underline',
    name: 'Underline',
    note: 'Reads as code. A dashed rule says it can be changed.',
  },
  { id: 'keycap', name: 'Keycap', note: 'Lit from above, and it sinks when pressed.' },
  { id: 'rail', name: 'Rail', note: 'Neutral body. A rail at the edge carries the colour.' },
  { id: 'corner', name: 'Corner', note: 'Tint, with a corner mark that says a menu is behind it.' },
];

const template = document.querySelector('#card');
const versions = document.querySelector('#versions');

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

if (template instanceof HTMLTemplateElement && versions !== null) {
  for (const [index, version] of VERSIONS.entries()) {
    const card = template.content.firstElementChild?.cloneNode(true);
    if (!(card instanceof HTMLElement)) continue;
    card.classList.add(`v-${version.id}`);
    fill(card, '.letter', String.fromCodePoint(65 + index));
    fill(card, '.name', version.name);
    fill(card, '.note', version.note);
    versions.append(card);
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

document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') closeOthers(null);
});

# Explanation

## Rust Owns The Application

Dageist presents dataframe pipelines as graphs: source nodes read files, inner
nodes join, transform, filter and aggregate, and output nodes render tables and
charts or write files. Everything that decides what the graph is, whether it is
valid, how it runs and what each node shows belongs to Rust. The view draws the
result and reports what the user did.

This split keeps the behavior testable without a window, and it keeps the
TypeScript small enough to replace.

## Core, Host And View

The repository has three layers, each depending only on the one below it.

| Layer | Package | Owns |
| --- | --- | --- |
| Core | `dageist` | Application behavior, independent of any window system. |
| Host | `dageist-tauri` | The native window, webview configuration, permissions and IPC commands. |
| View | `ui/` | Rendering and input inside the host's webview; see [How The View Is Divided](#how-the-view-is-divided). |

The core never depends on Tauri. The host depends on the core and translates its
values into serializable views, such as `IdentityView` for the `core_identity`
command. The view calls those commands, renders their results and forwards user
intent back through further commands. It holds no authoritative state: anything
it shows can be rebuilt by asking the host again.

Table data takes a shorter path than other values. The engine crate,
`dageist-engine`, runs Polars and offers a table two ways:

- A **working window** holds every row of a limited number of keys in memory.
  It is small enough to recompute in milliseconds, and because its keys are
  whole, grouped and windowed expressions give the same answer on it as on the
  full data.
- A **stored result** stays in its Parquet files, however large. The engine
  reads each requested block from disk, so memory use depends on the block and
  not on the table.

The engine builds Polars from the source of a Python Polars release, kept in
`vendor/polars`, with the same features, crate patches, dependency versions
and toolchain as that release. Polars' Rust crates are released less often
than Python Polars and from other commits, so this is the only way for a step
to behave in dageist as it does in Python Polars. The release's optimisation
profile and CPU settings are not copied; they change speed, not results.

Either way the view asks for only the rows and columns near what is on screen,
and the host passes them through as the raw bytes of an Arrow IPC stream. The
view decodes those bytes and draws them; it never holds the table.

As the product grows, new behavior lands in the core or in crates under
`crates/` that the core depends on. The host gains a command only when the view
needs to reach that behavior.

## Tauri Now, Surgeist Later

Tauri 2 is the interim host. The intended eventual host is
[Surgeist](https://github.com/bj-data-eng/surgeist), a Rust UI framework for
retained, CSS-styled document interfaces. Moving to it means writing a Surgeist
host beside or in place of `dageist-tauri` and porting the view; the core does
not change. Keeping Tauri-specific code inside the host is what makes that move
possible.

Building the view on web standards first also shows which capabilities a real
application needs from Surgeist. Those findings belong to Surgeist's own
repository.

## How The View Is Divided

The view is built with React, styled with Tailwind CSS and bundled by Vite. It
is an npm workspace of small packages, and what each part may depend on is
written down in its `package.json`. Nothing checks those boundaries
automatically yet, so they are also a review criterion.

| Part | Package | Responsibility |
| --- | --- | --- |
| Shell | `@dageist/desktop` | The composition root. It decides which host the view talks to and which feature is on screen, and owns the page, the bundler configuration and the design tokens. |
| Host bridge | `@dageist/host` | The contract of every IPC command and its implementation over Tauri. No other package knows Tauri exists. |
| Design system | `@dageist/design` | Primitive components. It knows nothing about dataframes, steps or tables. |
| Features | `@dageist/steps`, `@dageist/table` | One package per feature, each exporting one entry component. |

Three rules keep this workable as the application grows:

- **Features never import each other.** They meet only in the shell, which
  passes each one what it needs, such as the host.
- **Features depend on the host contract, not on Tauri.** That is why the view
  can run in a plain browser against a stand-in host while it is being worked
  on.
- **Rust stays the authority.** A feature holds interface state, such as what
  is selected or which menu is open. Values and validation come from the host.
  The step editor's model is a storyboard that works on fixed sample rows until
  the core provides that behavior.

The view reaches Tauri through the `window.__TAURI__` global that
`app.withGlobalTauri` injects. The host bridge declares the global's types by
hand, covering only the members it uses.

## The Catalogue Of Operations

The engine holds a catalogue of the Polars operations the step editor can
offer, as data, in `dageist_engine::catalog`. Each entry is one item of the
Python Polars API, such as `Expr.shift` or `LazyFrame.filter`, under an
identifier that saved steps keep. An entry records:

- what it applies to: an expression, a frame, nothing for a function such as
  `col`, or a chain of conditions for `when`, `then` and `otherwise`;
- the family it can be swapped within, such as the cumulative methods;
- its parameters, with Python's names and defaults, each of one kind. The
  engine reads each default from Python's text into a value, such as a whole
  number, a flag or unset, so nothing else parses Python;
- the dtypes it accepts on the value it follows;
- whether it works on a lazy frame;
- the function that builds it in Polars from filled-in arguments.

Those build functions are the start of the path from a saved document of steps
to a Polars plan. The arguments for an entry are checked against its
parameters, their kinds, shapes and choice words before it is built. Result
dtypes are not stored, because Polars works them out from a schema without
reading data.

Every parameter is one of ten kinds, and the view has one way of drawing
each, so a new operation needs data rather than a new look:

| Kind | Holds |
| --- | --- |
| expression | A chain: a field, a value or an input, then methods. A typed number or text is a chain of one value. |
| column | Names of input columns. |
| value | A literal of a stated type that cannot be an expression. |
| flag | True or false, and unset where Polars decides by default. |
| choice | One word of a closed set. |
| dtype | A Polars dtype. |
| duration | A Polars duration such as `1mo`. |
| table | A frame from the graph. |
| path | A file, folder or cloud location. |
| pairs | Key and value entries, each side of its own kind. |

A parameter holds one of its kind, an ordered list of them, or one for each
item of another parameter, such as sort's `descending` for each key.

Every word the editor shows comes from a language file beside the catalogue,
including Polars' own names. In English the shown names equal Polars' names,
and a test proves it unless a change is listed in the file. Renaming
something for the editor, or translating it, therefore changes no entry and
no saved step.

The catalogue is checked against the
[Python API audit](reference.md#public-api-audits).
Every item there is either an entry or on a list of items left out, with the
reason: deprecated, taking a Python function, meaningful only with a Python
object, never offered because it is not an operation a step could use (such
as running a query or printing a plan), or not yet catalogued. When Polars
moves, refreshing the audit makes that test name every item that changed.
The catalogue holds the seven steps, the expression methods the storyboard
uses, the aggregates `sum`, `min`, `max` and `len`, `col`, `lit`, `element`,
and the links of `when`, `then` and `otherwise`.

The host serves the catalogue with its English words to the view through the
`operation_catalogue` command. The step editor takes its method lists,
families, hints, step heads and the words `when`, `then` and `otherwise` from
it, and draws every parameter by its kind. A method's first expression
parameter is written inline. Any other parameter of a method or step is an
option: the list its name opens offers the rest by name, and an option is
written into the Polars text only when it differs from its default. A step's
own parameters are drawn by kind too: sort's keys and their per-item
`descending`, join's table, keys and `how`, a rename's pairs, and a pivot's
aggregate, which is a chain from `pl.element()`. A new column's name and chain
and a filter's conditions keep the lines the storyboard settled, which lay out
an expression list one item to a line, and a pivot's output columns stay
pinned values until the graph supplies tables. Previews still run on the
storyboard's sample rows, and a set option a preview cannot apply is named
under the step instead of being ignored.

## What The Step Editor Storyboard Has Settled

The step editor is where the design of a transform node is being worked out.
It runs on fixed sample rows inside the view, so none of it is application
behavior yet. The decisions below are the ones the core is expected to carry.

- **Two windows.** The table is the centre of the main window, and the steps
  are listed down its right side, each said by its head, such as "Sort table
  by dcn" or "Add column step_index". Choosing a step there shows the rows
  after it, so the table can be followed step by step, and a step is dragged
  to another place in the list. The steps themselves are edited in a window
  of their own, opened by a button or a double click on a step, which shows
  every step written out beside the same list. The step chosen in either
  window is the one whose rows the table shows, and a change in one window
  shows in the other at once.
- **Words.** A step is named for what it does to the table: Add column,
  Filter table, Sort table, Select, Rename, Join table, Pivot table. Methods
  keep their Polars names, such as `shift`, `cum_sum` and `over`, because a
  plainer word would only rename them. A type is always shown by its exact
  dtype name, such as `Int64`, because dtypes decide which tables and values
  can meet.
- **Text, not boxes.** A step is written as a line of code in a monospaced
  face. A word that can be changed is drawn as nothing but its letters, in
  the colour of what it names: a field, a method, a value, a comparison, a
  connected input. There are no fills, brackets, rails or animations. An
  earlier version drew every word as a filled block in brackets that parted
  when pointed at; all of it together was tiring to look at, and something
  moved wherever the pointer went. Now the only thing that answers to
  pointing is a faint flat fill on a word that can be pressed.
- **One click, one list.** A click marks a word and opens a searched list of
  what it can be changed to, in groups from the shortest to the longest, and
  a choice that does not fit says why. The top of the list says what is known
  about the word: a field's dtype and the step it comes from, what a method
  does and the dtypes it turns into each other, a step's Polars text. The
  foot of the list holds what can be done to the thing the word names: adding
  after it and taking it away. A step's name names the step, a method's name
  the method, a comparison's sign its line, and the word a chain starts from
  the chain. A faint `step` waits after the last step and adds one there.
- **Chains.** A chain is what it starts from, then methods, each behind a
  caret. It can stand wherever a value can: as what a new column holds, after
  `then` and `otherwise`, and as a line of a condition. A field becomes the
  start of a chain when a method is added after it. A chain too long for its
  row carries on set in, breaking before a caret. A method is moved by
  dragging its name. A function such as `when` is not called on a value, so
  it can only stand where a chain starts.
- **Lines under a step.** Whatever a step takes that is more than one thing
  is set out on lines beneath it, each set in under what it belongs to, as
  code is. A `when` is its condition, then `then` and `otherwise` with the
  values they give, then the rest of the chain. A `when` holds one condition;
  a further case is a `when` after `otherwise`. Several fields in one step,
  as in Sort table or Select, stay on the row and fold to the room there is,
  with a count of the rest.
- **Signs as they are typed.** The code face is set without ligatures, so
  two signs side by side are never drawn as one: `==` is two equals signs.
  In a step and in the list of steps, a comparison that has a sign of its
  own is written as it: `≠`, `≥` and `≤`, each one character wide. The
  Polars text of a step keeps `!=`, `>=` and `<=`.
- **Conditions.** A comparison is a field, a comparison and a value. A
  condition is always a list of lines under `all of` or `any of`, even when
  it has one line, and a group is a list set in again. Nothing depends on
  whether `and` or `or` binds tighter, because the two are never mixed in one
  list. A line can also be a chain of its own that ends in a comparison
  method, such as `gt`. A condition can be taken away whole, which leaves an
  empty place that offers how to start one again.
- **Missing things are marked at once.** What is still to be picked is
  written in the colour of a mistake as soon as it exists, and the step says
  what it lacks.
- **Values.** A value is typed into the search box of its list, where it is
  offered as a number or as text, or picked from the connected inputs and the
  fields. Text is shown between quotation marks that are drawn, not typed.
  The list never offers the values a field holds, since a column can hold
  millions of them.
- **Inputs, not variables.** A node declares no variables. What its steps can
  use arrives from the nodes connected to it: tables, single values and
  lists, each with a name, its dtypes and the node it comes from. Logic is
  reused by connecting several nodes to one node's output.
- **Leaving the lazy query.** A step that cannot run lazily carries a marker,
  and the choice is the user's: collect there, sink first in a node of its
  own, or change the step so that it stays lazy. Pivot is the only step
  marked so far, because its output columns depend on the data unless they
  are pinned.

## Current Limitations

The application opens its window on two features. The steps of a transform
are a storyboard that works on fixed sample rows in the view, offering the
operations of the engine's catalogue; its two windows
keep the steps the same by telling each other of every change, where the core
will later own them. The table viewer shows a
generated sample through the engine. The graph model, node types, pipeline
execution, previews from the core and canvas interactions are not implemented
yet.

A third tab shows the Pieces page. It is a design prototype, not a feature:
a static page in plain HTML, CSS and JavaScript where the way the step
editor is drawn is tried out in several versions before one is built. It
works on made-up samples of its own, shares no code with the view's
packages and calls no host command. The tab that is open is kept in the
page's address, so reloading the page returns to it.

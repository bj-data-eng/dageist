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

## What The Step Editor Storyboard Has Settled

The step editor is where the design of a transform node is being worked out.
It runs on fixed sample rows inside the view, so none of it is application
behavior yet. The decisions below are the ones the core is expected to carry.

- **Words.** A step is named for what it does to the table: Add column,
  Filter table, Sort table, Select, Rename, Join table, Pivot table. Methods
  keep their Polars names, such as `shift`, `cum_sum` and `over`, because a
  plainer word would only rename them. A type is always shown by its exact
  dtype name, such as `Int64`, because dtypes decide which tables and values
  can meet.
- **Blocks.** Everything that can be pressed is a block in the colour of what
  it names: a field, a method, a value, a comparison, a connected input. A
  click on a block opens a searched list of what it can hold, in groups from
  the shortest to the longest, and a choice that does not fit says why. A
  step's head and a method stand in square brackets. Pointing at one parts
  its brackets: a cross removes it and a plus adds one after it. A click is
  for what is inside; pointing is for the step or method itself.
- **Chains.** A column's expression is a chain: what it starts from, then
  methods, each behind a caret. A chain too long for its row breaks before a
  caret. A method is moved by dragging its name. A function such as `when`
  is not called on a value, so it can only stand where a chain starts.
- **Lists under a step.** A step's head is one row. Whatever it takes that is
  more than one thing is a numbered list beneath it, and a list inside a
  list is numbered from its line, as 9.1.2 is under 9.1. The cases of a
  `when` are lines of a condition, then a value, with `otherwise` last. A
  chain used as a value is three dots where it stands, with its own line in
  the list. Several fields in one step, as in Sort table or Select, stay on
  the row and fold to the room there is, with a count of the rest.
- **Conditions.** A comparison is a field, a comparison and a value. A row
  holds one comparison. More than one is a list headed `all of` or `any of`,
  and a group is a list inside the list. Nothing depends on whether `and` or
  `or` binds tighter, because the two are never mixed in one list.
- **Adding.** A faint word waits where the next thing goes: `step` after the
  last step, `method` at the end of a chain, `when` after the last case, and
  `and` or `or` at the end of a condition's list. What is added starts from
  stand-ins that say what is still to be picked.
- **Values.** A value is typed into the search box of its list, where it is
  offered as a number or as text. Text is shown between quotation marks that
  are drawn, not typed.
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

The application opens its window on two features. The step editor is a
storyboard that works on fixed sample rows in the view. The table viewer shows a
generated sample through the engine. The graph model, node types, pipeline
execution, previews from the core and canvas interactions are not implemented
yet.

A third tab shows the Pieces page. It is a design prototype, not a feature:
a static page in plain HTML, CSS and JavaScript where the blocks the step
editor is written with are tried out in several versions before one is
built. It works on made-up samples of its own, shares no code with the view's
packages and calls no host command. The tab that is open is kept in the
page's address, so reloading the page returns to it.

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
reading data; which combinations Polars allows is kept in
[tables](#which-combinations-polars-allows) beside it.

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
`descending`, select's expressions, join's table, keys and `how`, a rename's
pairs, and a pivot's aggregate, which is a chain from `pl.element()`. A new
column's name and chain and a filter's conditions keep the lines the
storyboard settled, which lay out an expression list one item to a line, and a
pivot's output columns stay pinned values until the graph supplies tables.
Previews still run on the storyboard's sample rows, and a set option a preview
cannot apply is named under the step instead of being ignored.

## The Step Document

A node's steps are saved as a step document, the source of truth that the
configuration repository will keep, in `dageist_engine::document`. It is JSON
with a version, and a version the reader does not know is refused: dageist is
before 1.0, so an older document is not upgraded. A step is a call of a frame
entry of the catalogue, with its arguments by Python parameter name, and a
parameter left out takes its default. An expression is a start, a function
such as `polars.col` or `polars.lit`, `polars.when`, or a connected input,
followed by a list of calls, one for each method or link of a condition, so
columns, literals and conditions are written the same way. Dtypes are written
as Python prints them, such as `Datetime(time_unit='us', time_zone=None)`.

Reading is strict. A field, entry or argument the catalogue does not know,
and a call where it cannot stand, such as a method before its start or a
`then` without its `when`, is refused with its place: the step, the path
through the parameter's lists and pairs, and the link. A repeated key is a
syntax error, reported by line and column. Writing a document gives text that
reads back as the same document, and text the writer produced is written back
exactly, each float to the bit. Nesting is limited to a fixed depth, which
reading and constructing a document both enforce.

The engine runs a document on a source table and the graph's inputs. It
builds each step through the catalogue and reports whether it is valid, with
the place and reason when it is not, its result schema, and the dtype after
each link of every expression, all without reading data. Steps after a
refused one are reported as not run. The rows of any step come from running
it on an in-memory working window. Polars' own message is the reason when
Polars refuses. Some mistakes, such as `dt.date` on a String column, are only
found when rows are read, not from the schema; the editor learns them in
advance from the [tables of allowed combinations](#which-combinations-polars-allows).
The runner asks the named rules beside those tables, and not the tables, for
where an expression may start: `element` starts a chain only in a pivot's
aggregate. A row of the context table was measured with the chain ending at one
entry, with one argument, such as a bare `element()` or an untyped `lit(1)`, so
it cannot judge a start that other arguments or more links follow. Polars decides
the rest as the step is built, and when it refuses a pivot because of a column in
its aggregate, it refuses the step, not the expression. The contexts the tables
are measured in also say which table a join's `right_on` is read against.

## Which Combinations Polars Allows

Polars' schema resolution does not refuse every invalid combination. The
schema of `dt.date` on a `String` column is a `Date`, and the call fails only
when rows are read. `cum_sum` on text, `dt.total_days` on something that is
not a duration, and a comparison of text with a whole number are the same.
So the step editor cannot learn what it may offer from a schema.

The engine instead runs every combination of the catalogue in Polars, once,
and keeps what it found as three text artifacts in
[api/catalogue/](../api/catalogue/dtypes.txt), one combination to a line,
sorted, so that a Polars upgrade shows as a readable diff. A combination counts
as allowed only when Polars resolved its schema, ran it on three rows that
include a null, and ran it on a table with no rows. Where Polars refuses, the
line records where it did and its own message, cut at the first blank line, with
no plan, address or timing in it. The stage is `schema`, `rows` (the schema
resolved and the rows were refused: a mistake only running shows) or `empty`.
An allowed line records the dtype of the result and whether it is `one` value
for the three rows or `many`.

| Table | One line for |
| --- | --- |
| `dtypes.txt` | A method of an expression on a receiver of each of the representative dtypes, and for each of its expression parameters, with an argument of each of them. Parameters are varied one at a time, the others at a fitting default. |
| `contexts.txt` | An entry in each context: a position that holds an expression, or such a position inside an expression that stands in one. Two lines for each, one for the chain going on from the entry and one for the chain ending at it. |
| `options.txt` | An entry with each pair of values of its options, the flags and the words of a choice; with one value of its option when it has only one. |

The representative dtypes are every integer and float width, `String`,
`Boolean`, `Date`, `Time`, `Datetime` with and without a zone, `Duration`,
`Null`, `List` of an integer and of `String`, `Struct`, `Array` and `Decimal`.
The contexts are a `select` item, a filter's predicate, a sort key, a join key
on each side, and a pivot's aggregate, each also with, inside the expression
that stands there, `over`'s `partition_by` or `order_by` or a condition's
predicate or value. That is two levels deep, and a place deeper than that is
judged by its outer position and the innermost nested one: an `over` key inside
a condition's value is a key, and a predicate inside one is a predicate.
A join's `right_on` is evaluated against the other table, whose columns are
named differently from the left table's, so a column of the wrong table is
found missing. A pivot's aggregate starts from `element()`.

Whether an entry may stand at a place is not whether it may end an expression
there, so the context table measures each entry in each context twice. On the
line of `continues`, the chain goes on from the entry with a neutral
continuation that the context needs, and the line says whether the entry may
stand there at all: `.is_not_null()` where a `Boolean` is required (a filter's
predicate, what a condition gives in a filter, a condition's predicate), a cast
to the dtype of the other table's key for the whole expression in a join key,
and `.first()` for the whole expression in a pivot's aggregate; nothing where
the context needs nothing. A window in a join key, or in a pivot's aggregate,
is refused however it goes on. On the line of `ends`, the chain ends at the
entry as it is, and the line says whether the entry may end an expression
there: a sum may stand in a filter's predicate, followed by a comparison, though
it may not end it.

Every call is built through the catalogue's own build functions and run on
fixed sample tables, by an engine test that checks the committed files are
current and, with `DAGEIST_REFRESH_TABLES=1`, rewrites them. The test runs
Polars on one thread in a process of its own, because Polars runs two sides of
some operations in parallel and reports whichever fails first, which would
give a different reason from run to run. The
[refresh procedure](how-to.md#refresh-the-tables-of-allowed-combinations)
says how to read the diff.

Some rules are not Polars' to hold, and are named in `dageist_engine::allowed::Rule`
and referenced from the lines of the context table they govern:

| Rule | Says |
| --- | --- |
| `pivot-aggregate-gives-one-value` | A pivot's aggregate gives one value for each group. Polars lets a running sum stand there. |
| `condition-predicate-is-boolean` | A condition's predicate is a `Boolean`. Polars finds a whole number there only when rows run. |
| `filter-predicate-is-boolean` | A filter's predicate is a `Boolean`, and so is what a condition in it gives. Polars refuses a chain that ends in anything else, but a method that is not a `Boolean` may stand partway through one. |
| `element-only-in-pivot-aggregate` | `polars.element` stands only in a pivot's aggregate. Polars refuses it elsewhere, but as a panic when the table is sorted. |

`dageist_engine::allowed::offers` answers which entries may stand at a place,
given the context and what the expression has so far: nothing, a value of a
representative dtype and a shape, or a condition that is not finished. `offer`
answers for one entry, with the dtypes of the arguments chosen for its
expression parameters, and `options` for a set of options. They read the
committed files, embedded in the engine, and the rules, and never run Polars.
Each refusal says who refused it, Polars at a stage or a rule or the shape of
an expression, and gives the reason. A rule that forbids an entry here
is applied first, so that its words explain the refusal. An entry may stand at
a place when the line of `continues` allows it, and a refusal of that line is a
refusal to stand. A rule about how a chain ends, and the line of `ends`, do not
refuse an entry, which may be continued, but are reported by
`Allowance::can_end`: the rule first, so that its words explain the refusal,
then Polars' reason. An end rule that is about a dtype refuses a dtype known
not to fit, and does not refuse one that is not known: a start takes the dtype
of a column, and some methods give a dtype that their argument decides, so the
result is unknown until it is chosen, and it does not break the rule.

The tables hold what they were measured on. A method's context was measured on
the receiver that fits it best, so a table cannot say that a predicate must be
`Boolean` for a receiver it was not measured on; the rule does. Arguments are
varied one parameter at a time, so a method with two expression parameters has
each judged apart, and what the arguments given decide is what the method gives:
a `fill_value` of text makes a `shift` give text, and where two arguments give
different results the result is not known. A `Struct` is two fields, an `Array`
of two, and a `Decimal` of ten digits: the tables say nothing of other sizes.

### What The Tables Are Checked Against

The tables are measured one combination at a time, so whether they predict what
Polars does with the combinations they were not measured as is itself checked.
A property target built on [dageist-fuzz](../crates/dageist-fuzz/README.md) makes
random step documents of one step from the catalogue: chains up to six links
deep that start from a column of any representative dtype or from a typed
literal, in each position that holds an expression (a select item, a filter's
predicate, a sort key, either side of a join key, a pivot's aggregate), with
conditions nested two deep and `over`'s keys inside them, and sets of the
options of each call. For each it asks [`offer`](#which-combinations-polars-allows),
at the place each call stands in, with the dtypes of the arguments the case chose,
whether the call may stand and what it gives; asks whether each expression a
position holds may end there; and asks `options` of the options set. From
those it predicts whether Polars allows the case and the dtype after each link.
It then runs the case through the step document, as the editor does, on three rows
with a null and on a table with no rows, and compares: a case is allowed only
when the schema and both runs succeed, as the tables count it, and the dtype
after every link that the tables gave is the class Polars gives. The case is the
document's own text, so a kept case replays exactly. The
[how-to](how-to.md#check-the-tables-against-polars) says how to run it for longer
and to replay a case.

A disagreement is either a defect of dageist or a combination the tables were not
measured on. The first found while building the target was `offer` giving a
`shift` the dtype of what it shifts when a `fill_value` of another dtype was
chosen; it is corrected, and its case is kept. The second kind is a limit. There are
fifteen. Each is named in the target, counted when it excuses a case, and has a
kept case that the normal tests replay, which only that limit excuses; each has
a test that the same feature elsewhere in the case excuses nothing.

A limit excuses a disagreement only when it is the cause of it. The tables always
say where they refused, and that place must lie in the expression that holds what
the limit is about. Polars says where only when it refused while it built the
step and worked out its schema, and then a refusal of the whole step, as a join's
keys are refused, has the step for its place, so the key that holds the cause is
found by what each key holds. A refusal when Polars collected rows, the empty
table's too, and a panic, have no place. For those the reason and the feature in the
case are what there is, and the limits that can only be gated so (the lengths of
the rows, an overflow, a panic, a window over a window, an explode, the empty table
of a pivot) say so below. Two limits may both explain one case, and it is then
excused by both.

Two things the target does not do. It stops predicting after a call given an option
that changes what the method gives, which `offer` is not told: `over`'s
`mapping_strategy` as `join` gives a `List`, and `dt.total_days`'s `fractional` as
true a `Float64`. `options.txt` records them. The dtype after such a call, and so
what follows it that needs one, is not asked of the tables, and the tally counts
the cases that stopped so; it is not a limit, which excuses a disagreement. And it
compares nothing more of a case that a named rule refuses and Polars allows: the
dtypes before the refusal are compared, but whether Polars allows the rest is not.
The rule `pivot-aggregate-gives-one-value` is stricter than Polars wherever it
refuses, which is about a third of the pivot cases (55 of the 153 of the fixed seed,
and the same share in longer campaigns), so those pivot aggregates are checked little;
the tally counts the pivot cases and those the rule refuses apart.

| Limit | Excuses |
| --- | --- |
| An option changes what a method gives | `Expr.over`'s `mapping_strategy` as `explode` gives other lengths, which meet those of what follows it. Polars refusing the rows, with a reason about lengths, in a case with an `explode` (no place: the reason and the `explode` gate it); and the tables refusing, with a reason about lengths, at the window or at a call after it, or in the arguments of a link after it, in the same chain, which Polars allows; not before the window, not in its own arguments, and not an `explode` in another operand or item. |
| A literal | An argument or a start is measured as a column of the dtype, except the values of `shift` and the arguments of a pivot's group. A literal, or a condition of literals, is one value, which Polars casts and broadcasts as it does not a column. A comparison or a subtraction with a null literal is null, so that Polars does not evaluate what it follows or what it is given, and does evaluate the arguments of the calls after it. The tables refusing at a call that follows a literal or is given one, or at what a null comparison follows, is given, or at the calls from it on, which Polars allows. |
| A single value | Polars does not evaluate what it does not need for one value. The tables refusing at a call whose receiver in its chain is one value (an aggregate with only single values after it), or inside a sort key or a window's key that is one value (a literal is one), which is dropped, or in what a `len` follows, or in the arguments of the links before it, which it does not count; Polars allows it. A `len` before an unrelated later argument excuses nothing there. |
| A pivot's aggregate | It is run on the values of each group, with kernels of its own, and on no groups when the table is empty. Polars refusing the empty table, with a reason that an operation is not supported or that a dtype is null (no place); and the tables refusing, with a reason about a dtype an operation does not support, a comparison, an operation, an evaluation, a cast, a struct field or lengths of a window over a group, at a call of the aggregate, which Polars allows. |
| A single value in a key | A join's or a window's key that holds a single value, an aggregate or a null literal, beside what is not one is not elementwise; the context table measured the entry as the whole key, and a key that is wholly single is that. Polars refusing with that reason, when a key of the case holds one beside a column, and, if the refusal has a place inside the step, inside that key; and the tables refusing, with that reason, a key that holds the single value, which Polars allows. A refusal of the rows of a window has no place. |
| The lengths of the rows | Whether lists, or other values, of different lengths combine depends on the rows. Polars refusing the rows with a reason about lengths, in a case with no `explode` (no place). |
| An overflow | A debug build of Polars panics where a release build wraps. Polars panicking so (no place). |
| Branches that are one | A condition whose values are one expression is simplified to it, and its predicates are dropped. The tables refusing inside a predicate of such a condition, which Polars allows. |
| A decided condition | A condition with a predicate made of literals alone is decided before it is run, and the other branches are dropped; a predicate that only starts from a literal and goes on to a column is not. The tables refusing inside such a condition, which Polars allows. |
| A value that runs on no rows | A value of a condition runs on the rows its predicate selects, and a predicate after the first on the rows left to it, which on the sample may be none. The tables refusing, at the stage of running rows, inside such a value or predicate of a condition that is not decided, which Polars allows. What they refused when they built the step is refused whatever the rows. |
| A panic | One is an internal check that failed on the plan Polars made in `select`; another place, or a combination of what was measured singly, may not reach it, or may reach another. Polars panicking where the tables allow (no place), and the tables recording a panic where Polars allows. |
| A `Boolean` is needed and the dtype unknown | A predicate needs a `Boolean`. The dtype of a column, one that the arguments decide, and one a condition's values give, is not one `offer` is told, so a predicate that is only a start, or ends in such a call or a condition, is not refused for being no `Boolean`; and a start is judged on the dtype it was measured on. Polars refusing for that, in a predicate whose dtype was not given, and, if the refusal is placed, inside it; and the tables refusing a start in a predicate, which Polars allows. |
| A hidden schema | The tables refuse a method whose plan says another dtype than its rows give, as the `cum_sum` of a `UInt128` does; inside an expression that goes on from it or compares it, the rows of the step do not show it. The tables refusing with that reason inside such an expression, which Polars allows. |
| A window over a window | A window whose receiver holds another window, in its chain before it, or in the arguments of those calls, is run in the aggregation context of the outer one, where a key that is not elementwise is refused; the tables measured one window. Polars refusing the rows with that reason (no place). A window in a key, or two windows apart, is not this. |
| A join key's dtype | The other table's key is made of the dtype the chain gives on its own, and the join may resolve the chain to another, as the precision of a decimal a null is taken from. Polars refusing the join, with a reason that names a decimal. |

A case that the tables cannot be asked about, because the dtype of an argument or
before a method is not known, is not predicted past that point and is counted. The
generator keeps to what the tables measured where a value, not a dtype, decides: a
`shift` by the number 1 of a dtype, durations that are text of `b`, two values of
a condition of one dtype, a join without `validate`.

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

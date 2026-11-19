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
| Host | `dageist-tauri` | The native window, webview configuration, permissions, IPC commands, and the bridge that serves the same commands to a browser. |
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

Building Polars as the release is built is not all the release does: when the
Python module loads, it also sets Polars up. [Polars Is Set Up As The Release
Sets It Up](#polars-is-set-up-as-the-release-sets-it-up) says which parts of that
the engine does.

Either way the view asks for only the rows and columns near what is on screen.
The engine writes every cell of a block as the text Polars writes the value,
the same text the step editor's previews show, with a null as a missing cell, in
an Arrow IPC stream whose columns are all text. The host puts the number of each
row and a report of the block in front of it, as the [frame](reference.md#the-block-frame)
the view reads. The view draws what it reads and formats nothing, neither a
cell nor a count, a size or a time: the one figure it writes is how long it
waited for a block, which only it can measure. A column says whether it holds
numbers, so that the view can align it, since the type of a value is not in the
block. It never holds the table.

As the product grows, new behavior lands in the core or in crates under
`crates/` that the core depends on. The host gains a command only when the view
needs to reach that behavior.

## Polars Is Set Up As The Release Sets It Up

The Python Polars release in `vendor/polars` runs a start-up function when its
module loads, `register_startup_deps` in
[on_startup.rs](../vendor/polars/crates/polars-python/src/on_startup.rs). Most of
it connects Polars to Python. Four parts do not, and without them Polars
behaves differently in dageist than in the release:

1. **Larger stacks in debug builds.** In a build with debug assertions the
   release raises the stack sizes of the `recursive` crate, which Polars uses
   to avoid overflowing the stack on deep plans, because stack frames get very
   large there. dageist's debug and test builds are debug builds; a build
   without debug assertions keeps the crate's defaults, as the release does.
2. **The SQL resolver.** A plan written in SQL is resolved when it is collected,
   by a resolver that must be registered. Polars registers it itself only when a
   SQL context is built in the process, and a plan read from text or built
   elsewhere has none.
3. **Unknown extension types.** What Polars does with an extension type that
   nothing registered when it reads one. It loads a generic extension type, or,
   when the environment variable `POLARS_UNKNOWN_EXTENSION_TYPE_BEHAVIOR` is
   `load_as_storage`, the storage type. The engine reads the same variable, with
   the same default.
4. **The spill-file cleaner.** When Polars' streaming engine spills data to disk
   it hands the files to a thread that deletes them. The release starts the
   thread when it loads, which also deletes what a process that ended abnormally
   left behind, and waits for it to finish when the process exits.

The engine does these in the release's order, once for the process. Every
engine entry that runs Polars sets Polars up before it runs it, so nothing that
goes through the engine finds Polars unprepared, however the engine is used.
The set-up reads its environment variables the first time it runs.

The host also holds a `PolarsSession` from its start to its end. Starting it
sets Polars up, and dropping it waits for the cleaner to finish, which is the
flush the release does at exit; holding it in `main` keeps the two together, so
no caller has to remember a call at the end. That covers the program ending
normally and a panic unwinding out of `main`. A process that is killed, ended by
a signal, aborted or ended by `std::process::exit` runs no destructor and does
not flush. Its spill files stay in a directory named for it under Polars' spill
directory, and the next start of Polars deletes the directories of processes
that no longer exist. The bridge ends this way, as it runs until it is
stopped.

The rest of the release's start-up is left out. Most of it only connects Polars
to Python, with nothing to connect here: the registries that carry Python
values in and out of Polars and call Python functions, the warning function
that goes through Python's `warnings`, allowing extension types for `Object`
columns that hold Python objects, the name and version of the Python package
that Polars writes into Parquet files, and registering the flush with Python's
`atexit`, which the session replaces. Two parts are not about Python and are
left out for other reasons:

- **The query abort mechanism.** The release hooks `SIGINT` and aborts the
  running query with a panic that catchers around the Python binding's queries
  turn into a `KeyboardInterrupt`; the same catchers turn a spill that runs out
  of disk into an error. The engine has no catchers. Without them a spill that
  runs out of disk panics as any panic does, and leaving the hook out leaves
  `SIGINT` to end the process, which is how Ctrl-C stops the bridge. The
  release's hook would take `SIGINT` over, and with no catcher it would do
  nothing.
- **The coloured backtrace printer** behind the `backtrace_filter` feature,
  which the release's Makefile turns on for development builds of Python
  Polars and the released wheels do not have.

The engine's [start-up module](../crates/dageist-engine/src/startup.rs) lists
each part with its reason. When `vendor/polars` moves to a new release, that
module is checked against the new release's `on_startup.rs`; see
[Move Polars To A New Release](how-to.md#move-polars-to-a-new-release).

## The Bridge

A plain browser has no Tauri, so it has no engine to ask. The one binary,
`dageist`, therefore has a second job besides opening the application: its
`bridge` subcommand serves the view's commands over HTTP on `127.0.0.1`, opens no
window and starts no Tauri. The browser preview then shows what the application
shows, without building and opening the application.

The bridge belongs to the host and adds no behavior. Every command is a plain
function apart from its Tauri binding, and the bridge calls the same function
with the arguments it decodes from the request, so the two cannot answer
differently. What Tauri keeps with `manage`, the gate that lets one command at
a time generate the sample and the table that is open, the bridge keeps for as
long as it runs, across requests. It finds the sample in the application's own
cache folder, so the browser and the application share one sample. It serves
only the commands that reach the core; opening a window is the application's,
so a browser opens a tab for it itself.

Because the bridge is a web server on the developer's machine, it is built to
be driven only by the view it was started for. It listens on the loopback
address and refuses a peer that is not loopback. A web page on another site in
the same browser could still send it requests, so it takes a request only if
its `Host` names the bridge's own loopback address and port, which defeats a
name that another site points at `127.0.0.1`, and only if the body is declared
`application/json`, which a page on another site cannot send without a
preflight that the bridge does not answer. It sends no cross-origin header, so
no such page can read an answer either. Vite's development server forwards the
bridge's path to it, so the view calls its own origin. The forwarded request
has to carry the bridge's own `Host`, or the bridge would refuse it, so behind
the development server the bridge's `Host` check no longer tells one page from
another. Safety through the development server rests on two other things: the
server's own `Host` check, which `server.allowedHosts` limits to `localhost` and
`127.0.0.1` in [vite.config.ts](../ui/apps/desktop/vite.config.ts), as it does
for `vite preview`, and the bridge's content-type rule, together with the
server's `cors: false`, which answers no cross-origin request. The bridge has no
authentication and is a development tool, not a server to expose. A later
server that answers queries to dashboards, as
[the exploration](https://github.com/bj-data-eng/dageist/issues/64) describes,
would need a smaller set of commands and real access control.

The view holds one host for each way it is reached. `@dageist/host` has the
Tauri host and the bridge host beside each other, and the shell uses the
bridge host when Tauri is absent.

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
| Host door | `@dageist/host` | The contract of every IPC command and its two implementations: over Tauri, and over the [bridge](#the-bridge) in a plain browser. No other package knows Tauri or the bridge's transport. |
| Design system | `@dageist/design` | Primitive components. It knows nothing about dataframes, steps or tables. |
| Features | `@dageist/steps`, `@dageist/table` | One package per feature, each exporting one entry component. |

Three rules keep this workable as the application grows:

- **Features never import each other.** They meet only in the shell, which
  passes each one what it needs, such as the host.
- **Features depend on the host contract, not on Tauri.** That is why the view
  can run in a plain browser against the [bridge](#the-bridge) while it is
  being worked on.
- **Rust stays the authority.** A feature holds interface state, such as what
  is selected or which menu is open. Values and validation come from the host.
  The step editor holds its steps and writes them as a step document; the
  engine runs the document and reports the columns, the dtypes and the
  refusals, the rows after a step as text, and the short summary of each step,
  and the view shows them.

The view reaches Tauri through the `window.__TAURI__` global that
`app.withGlobalTauri` injects. The host package declares the global's types by
hand, covering only the members it uses.

What the host sends is JSON, which TypeScript can only call `unknown`, so the
host package does not trust it with a type assertion. A decoder beside the
contract reads each answer and returns it typed, or refuses it with the place
and the reason, such as `$.steps[3].outcome.status`. Its objects are closed: a
missing field, a field the contract lacks and a tag it does not name are all
refused, so a change of shape on the host's side fails where it is read. The
decoders are pure TypeScript and are tested with Vitest against snapshots that
host tests write from Rust, `ui/packages/host/fixtures/`, with expectations
written by hand; a change of shape in the host shows up in those tests as a
failing snapshot check in Rust and a refusing decoder in the view. The words
of each closed list, such as the rules and the positions of an expression, are
constants in the contract that a test compares with the host's own list of
the engine's variants, so a variant the engine adds fails a test. The same
holds for the functions that prepare decoded data for display. Drawing is React
that only consumes prepared values, and it has no tests while the design
changes.

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

Python's operators are spellings of entries, not entries of their own. `Expr.__add__`
is `+`, which `Expr.add` is; the same holds for `-`, `*`, `/`, `//`, `%`, `**`, unary
`-`, `abs()`, `&`, `|`, `^`, `~` and the six comparisons, and for each reflected form,
which is the same operation with the sides swapped. Unary `+` returns the expression it is
given and is left out for that. A step puts a value on the left of an operator by starting
its chain from that value. Offering both would show one operation twice, so each dunder
method of `Expr` is left out as never offered, with the entry it spells.

`and_`, `or_`, `xor` and `not_` are logical on Booleans and bitwise on whole numbers, so
they have a family of their own, Logic, and are not swapped for a comparison, and the
editor shows `and`, `or` and `not` without Python's trailing underscore, which the
language file lists as changed. `and_` and `or_` take any number of operands and fold them
from the left as Python does; given none they are the expression they follow, which is no
use to a step, so the catalogue marks the operands as needed and the tables, the offers
and `fit` judge them with at least one: `and` is offered after a whole number or a Boolean
and refused after a float. The tables judge each operand beside what the method follows,
as they do for any method with more than one expression parameter, and not beside what the
operands before it gave. The mark is on any optional parameter that holds expressions and
of which a step always gives at least one, and `clip`'s two bounds carry it for the same
reason: a clip with neither is the expression as it was, which Polars allows after any
dtype, and is no use to a step. A bound given on its own is judged by its own lines of the
tables.

The maths of one value are three families beside Arithmetic, so that a method's list stays
short and a method is swapped for one that takes the same arguments. Maths holds `sqrt`,
`cbrt`, `exp`, `log`, `log1p`, `erf` and `erfc`; Trigonometry the sine, cosine, tangent and
cotangent, their inverses, the hyperbolic functions and their inverses, and `degrees` and
`radians`; and Rounding `ceil`, `floor`, `round`, `round_sig_figs` and `truncate`, of which
`round` and `truncate` take the same places. `sign` and `clip` join `abs` and `neg` in
Arithmetic. Every name is Polars' own, `cbrt`, `erf`, `erfc` and `round_sig_figs` among
them; the hints, such as "cube root" and "error function", are where the plain words go.

What Polars does with each is what the tables say, and some of it is not what a name suggests:

- The maths and the trigonometric functions give a float: a `Float32`, and a `Float16`, stays
  one, and every other dtype gives a `Float64`. The maths also take a `Boolean` and a
  `Decimal`, and the trigonometric functions take neither.
- `sqrt` and `cbrt` resolve on a column of any dtype, a date or text among them, to a schema
  that names the dtype they were given and rows that are a `Float64`. The tables refuse that as
  they do any call whose schema and rows differ, with the reason that the schema says one dtype
  and the rows give another, and the check of what each method accepts does the same.
- The roundings keep the dtype they are given, take whole numbers and leave them as they are,
  and refuse a `Boolean`. Rounding, flooring or ceiling a `Decimal` gives it one more digit of
  precision, which the class of its dtype does not show, and truncating it does not.
- `round`'s `mode` has the two words Python's binding takes, `half_to_even` and
  `half_away_from_zero`. Polars' documentation lists `to_zero` as a third, which the binding
  refuses and `truncate` does.
- `decimals` and `digits` are whole numbers, which Python passes to Polars as an unsigned and a
  signed 32-bit number, so a build refuses one outside that range, and `digits` must be given.
  Polars refuses fewer than one significant figure when it runs rows, whatever the dtype. The
  tables, measured with two, do not say so, and the generator gives `digits` one or more.
- `log`'s `base` defaults to `math.e`, which the engine reads as the number it is, as it reads
  no other name. Python makes the default a number of no dtype, and Polars leaves the logarithm
  of a whole number to such a base an unresolved float, which a join's key or a sort key does
  not resolve, so the build makes the default a `Float64`. A base that is given keeps Python's
  reading.
- `clip` casts a bound to the dtype it clips and refuses a bound that dtype cannot hold, so a
  number typed without a dtype does not widen the dtype as an operand's does.

The catalogue holds the seven steps, the expression methods the storyboard
uses, the aggregates `sum`, `min`, `max` and `len`, the six comparisons, the
arithmetic of `add`, `sub`, `mul`, `truediv`, `floordiv`, `mod`, `pow`, `neg`,
`abs`, `sign` and `clip`, the maths, trigonometry and rounding above, the logic of
`and_`, `or_`, `xor` and `not_`, `col`, `lit`, `element`, and the links of `when`,
`then` and `otherwise`.

The host serves the catalogue with its English words to the view through the
`operation_catalogue` command. The step editor takes its method lists,
families, hints, step heads and the words `when`, `then` and `otherwise` from
it, and draws every parameter by its kind. A method's first expression
parameter is written inline. Any other parameter of a method or step is an
option: the list its name opens offers the rest by name. The editor writes
every option that is set into the document; the engine decides which are at
their default. A step's own parameters are drawn by kind too: sort's keys and
their per-item `descending`, select's expressions, join's table, keys and
`how`, a rename's pairs, and a pivot's aggregate, which is a chain from
`pl.element()`. A new
column's name and chain and a filter's conditions keep the lines the
storyboard settled, which lay out an expression list one item to a line. A
pivot takes its output columns from a connected table, its `on_columns`.

The editor sends its steps to the engine as a step document and shows what the
engine reports of them, in
[ui/packages/steps/src/model/](../ui/packages/steps/src/model/document.ts).
Translating the steps into a document and preparing the report for display are
plain TypeScript with no React, tested against the snapshots the host writes.
What the editor shows of a run it reads off the report and works out none of it:
the columns a step added or replaced, the rows a filter keeps of those it was
given, the first values of a column a step added, and for each connected input
what supplies it and the value or values it holds. The view only lays the
engine's text out as a line, with a dash for a null and an ellipsis where more
values follow.
Where an expression is still to be picked, a value, the field or input a chain
starts from, a method's argument or a comparison's field, the editor writes a
hole, which the document reads as an expression that starts from nothing. The
engine walks the step up to the hole, so it reports the places before it, and
does not build it: the step is incomplete at the hole's place, and the steps
after it are not run, naming it. The editor reads the place back into what is to
be picked and the word that is the matter. The steps after an incomplete step have no walk, so their chains have no lists of
methods until the step before is finished. A new add-column step holds a hole, so
its chain starts from nothing and has no methods to list until a field is picked,
and a new pivot is held back until its three fields are picked, so its aggregate
has no lists of methods until then. A step is held back from the
document, with the reason, only where what is missing is not an expression to
pick, such as an empty name, a field of a pivot or a table, or where the catalogue
has no entry for it yet, such as a comparison with `is_in` or a condition of
which any line is enough; the steps after it are not run, as they are not after
a step the engine refuses. The
document keeps the editor's step identifiers, so the editor's starting steps
are the engine's claim-timing sample and translate to the same document. A
run is asked for after a short wait for the next edit, with one request out at
a time, and the answer to a request that a newer edit has replaced is dropped,
so what is shown is never older than the last answer that was wanted. A
request that failed because the host was not there, as a bridge that is not yet
running, is asked again after two seconds while it is still the newest; an
answer of the engine's, such as a refusal, is the same each time and is not. A report is shown only for the
steps that are still what it reported of: a step an edit has changed, and the
steps after it, wait for the engine. A `when` of more than one case is held
back too, as a further case is a `when` after `otherwise`. A refusal
carries the place of the engine's document it is at, which the translation maps
back to the word of the editor it marks.

The editor's lists offer what the engine allows and nothing of their own. A list
asks the engine only while it is open, through a scheduler of its own that sends
the newest request after a short wait, drops the answer to one that a newer
request replaced and asks again only a failure that may pass, as the runs do. A
list says that it is waiting, or that the engine has not reported on the place,
and never shows what was answered for another place. Five commands answer:

- `offers_at` says which calls may stand where an expression has what it has so
  far, each with the engine's verdict, whether the chain may end there and the
  argument a method starts with. A method's place is the one the engine's walk
  reported for its link, which a chain's prepared dtypes carry; a chain's start
  is the positions that hold it and nothing yet.
- `candidate_fits` says which fields and connected inputs fit a slot, as the
  start of an expression, as the argument of a method or both. A start is offered
  a connected input where the engine allows a literal of its dtype, which is what
  a run makes of an input.
- `read_typed` says how a text typed into a slot reads, which is the engine's
  to say and not the view's; see [How a typed text is read](#how-a-typed-text-is-read).
- `option_offers` says what the flags and choices of an entry may be set to
  beside those already chosen, and the word each starts with when it is added,
  so that an option is added with a word Polars allows, or not at all.
- `step_starts` says what each step of the list starts with on a table: the
  list of steps asks it while it is open, and a step is added or changed to
  with what the engine says, so it cannot be picked until the engine has said.
  A filter starts with a comparison for equality of a field and a value still
  to be given, which the editor draws as a test.

What a parameter holds when it is not given, what it is first set to and
whether an option is at its default are rules about Polars, so the engine
decides them and the editor applies them. The catalogue carries each
parameter's default and its first setting, and a step report names the
arguments of the step, and of the calls inside its expressions, that are what
their parameter holds when it is not given, which the editor shows as "same as
the default". Polars calls a method the same with a parameter left out or
given its default. A list for each item, such as sort's `descending`, is at its
default only when it holds one value for each key; Polars refuses a list of
another length, so it is not at its default. An option that cannot be added
says why, with the refusal of the first word it could be set to, or that there
is no other word. The catalogue also records which parameters Python takes only
by name and which it takes as starred, and an engine test holds those marks to
the signatures in `api/polars/python.txt`.

The catalogue gives each expression parameter the position the tables name for
it, so the editor says where a slot stands and the engine works out the context.
The offers are prepared for display by plain functions in
[model/offers.ts](../ui/packages/steps/src/model/offers.ts), tested against
snapshots written from Rust: calls grouped by family, the shortest group first,
with hints, reasons and where a chain may end.

A method that is swapped keeps its argument, and the engine refuses it when the
steps run if the new method does not take it, with its reason. A flag or a
choice that is already set shows a word Polars refuses beside the other options
as a refusal when the steps run, not in its list; only the list that adds an
option asks. The values of `is_in` are connected lists the engine has no entry
to judge yet, so that list offers them unjudged.

### How A Typed Text Is Read

The view sends the text it was given with what it is typed into, which is the
kind of the parameter as the catalogue writes it (an expression, a literal of a
type, a duration or a path) and, for an expression, the slot it stands in. It
holds no pattern of a number, a date or a duration and no word for a dtype. The
engine's `read_typed` answers with each way the slot reads the text, in the
order the slot offers them: the literal the document would hold, the dtype
Polars gives it and, where a slot was named, what the tables say of it there;
or, for a text the slot does not take, why not. Nothing typed reads as nothing.

- **A number** is read as the document's own JSON reader reads one, so what is
  read is what a document holds once it is written: `12` is a whole number,
  `1.5`, `1.0` and `1e3` are numbers with a fraction, and `01`, `.5` and `+1` are
  none. A whole number is the narrowest integer it fits, and a number with a
  fraction a `Float64`. A whole number past 2^53 - 1 is read as the number with a
  fraction nearest to it, because the host sends it as JSON and the document
  must read back the number that was read; a slot that takes a whole number
  refuses it. A slot that takes a number with a fraction reads a whole number as
  one. A slot that takes a number or text, and an expression, reads a number
  first and the text as typed after it.
- **A duration** is read as Polars reads a duration string, such as `1mo` or
  `2d12h`, by the parser that `dt.offset_by` uses. Given to an expression, as
  `dt.offset_by`'s `by`, `"1d"` is a `String` literal like any other, which the
  tables allow there and Polars reads when it runs rows; only a parameter that
  takes a duration itself reads the text as one, and the reading has no dtype.
  An amount whose units add up to more than an `i64` of nanoseconds (a day is 24
  hours and a week 7 days) or of months is refused, in every build: Polars adds
  the amounts without checking, so a build with overflow checks would panic and
  one without would wrap around to another duration.
- **A date** is read as Polars reads text given no format, by the patterns
  `str.to_date` infers from: year, month and day, or day, month and year, with
  `-`, `/` or `.`. Polars refuses to compare a date or a datetime with a string,
  so a typed date that is to be compared would have to be a date literal, and the
  document holds none: the reading carries the ISO text of the day, with the
  dtype `Date`, for a parameter that takes one. No entry of the catalogue has a
  parameter of that type yet. Where a slot is named, the tables judge the
  literal the document holds, so a date reading has the dtype `Date` and its
  `fit` is that of the `String` that carries it; this changes when the document
  holds a date.
- **Text** is read as typed, with the dtype `String`, or with none for a path.

Where a slot is both the start of an expression and the argument of a method, as
a window's key is, a reading is refused where either side refuses it. Where both
allow it, the result and the shape are the argument's, for they say what the call
it is given to gives, and whether a chain may end there is the start's, for the
mark on a reading is about the chain the reading starts and not about whether the
call's result may end the chain the call stands in.

The reading of a text is made by plain functions in the engine's
[allowed/typed.rs](../crates/dageist-engine/src/allowed/typed.rs), served by the
Tauri command and by the bridge through one function in the host's
[offers.rs](../crates/dageist-tauri/src/offers.rs). The view asks it with the
scheduler of the other lists and prepares the answer in
[model/offers.ts](../ui/packages/steps/src/model/offers.ts); an answer that has
not arrived shows the text as typed, waiting. A number the engine reads with a
fraction is held with that mark in the value or setting, so that it is written
as a float and has the dtype that was shown.

The engine also words each literal it reads, as `shown` beside its value, and the
view puts that text in a block and words no number or text of its own: the
reading of a typed text, the argument a method is added with and the settings of
the catalogue and of a new step's start carry it, and a value or setting the
editor holds keeps it beside the value, which is what the offer rows, the blocks
and the parameter drawings show. The words are those the engine writes
elsewhere ([Literal::shown](../crates/dageist-engine/src/allowed/literal.rs)):

- A whole number is its digits.
- A number with a fraction is written as Python's `repr` writes it, which is also
  how a summary writes it, so a whole one keeps its fraction (`1.0`, not `1`), the
  shortest digits that read back are used, and an exponent, with a sign and two
  digits at least, is written only below 1e-4 and from 1e16 on (`1e+20`,
  `1e-05`).
- Text is written as it is, with no quotation marks and nothing escaped. The
  editor draws a block that holds text between quotation marks of its own, so
  adding them to the words would show them twice, and escaping is for a Python
  string. A summary has no such drawing, and writes text as a Python string.

The comparisons a test makes are the catalogue's entries `Expr.eq`, `Expr.ne`,
`Expr.gt`, `Expr.ge`, `Expr.lt` and `Expr.le`, which the engine lists in the order
they are offered in the catalogue's `comparisons`, each with the sign the
language file gives it, the same sign a summary is written with. A test in the
editor holds the entry that compares, one the catalogue lists as a comparison and
checked as one wherever it is received, and a new test starts from the first the
engine lists. A comparison's sign and what it means, which is the
entry's hint, are the engine's. Membership in a list, `is_in`, has no entry yet,
so the editor shows it as its own name.

A list that closes when a row is picked keeps its rows in the order the list is
given them, and a row that arrives later, as an answer of the engine does, takes
its place among the rest, so a list can be reordered while it waits. A list that
stays open keeps each row where it stood when the list opened or was last
searched, so that the row under the pointer does not move when a row is ticked.

### What The Editor Cannot Offer

What the editor has no way to draw is written once, in
[model/limits.ts](../ui/packages/steps/src/model/limits.ts), and a list shows
such a call disabled with the reason. The host writes the grid of every call the
engine allows at every place of every kind, 30 contexts each with nothing, 26
dtypes in two shapes and 4 states of a condition, as
`ui/packages/host/fixtures/offers-grid.json`, and a view test lists every
combination the engine allows that the editor cannot draw against a written
list. Today that is a further case of a condition, which the editor draws as a
`when` after `otherwise`, in the 24 contexts where the engine allows one, and the
parameters of a start or a condition that it does not set: `col`'s further
names, `lit`'s dtype and its object flag, and the keyword constraints of a
condition. Changing either side changes the list, which the test then fails on. The check
covers entries and their parameters. It does not cover which classes of dtype
the editor can type as an argument: the document holds a number, text or a flag
as a literal, so a typed date, duration or Boolean literal, which the engine may
allow, is not on the list.

## The Step Document

A node's steps are saved as a step document, the source of truth that the
configuration repository will keep, in `dageist_engine::document`. It is JSON
with a version, and a version the reader does not know is refused: dageist is
before 1.0, so an older document is not upgraded. A step is a call of a frame
entry of the catalogue, with its arguments by Python parameter name, and a
parameter left out takes its default. An expression is a start, a function
such as `polars.col` or `polars.lit`, `polars.when`, a connected input, or a hole,
`{ "hole": null }`, where nothing is picked yet,
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
each link of every expression, all without reading data. For each expression it
also gives the context it stands in and where each link stands, as the
[walk](#which-combinations-polars-allows) of the tables finds them, so that a
caller can ask for offers at any link. Steps after a refused one are reported
as not run. The rows of any step come from running
it on an in-memory working window, and a preview gives them as text, every
cell written as Polars writes it and each dtype by the engine's short name,
such as `Datetime(us)`, so the view formats no value. Polars' own message is
the reason when Polars refuses.
A valid step also says which columns it added and which it replaced: a column
the table before it lacked is added, and one it had is replaced when the step
writes it, as `with_columns` and `select` do for an expression that is not a bare
column, or gives it another dtype; the catalogue says which parameters write.
Run on the working window, the report also counts for each step the rows it was
given and the rows it kept, with the first values of each column it changed
written as text. Each step is collected once and its table is the next step's
input, so the window is run through the steps once and not again for every step;
the first values are those of the rows the preview gives, except after a join that is
not told to keep the order of its rows, which Polars does not promise;
the window is the only table that is counted, because it is held in memory
and bounded. A step Polars accepts but cannot collect on those rows is reported
as failed, and the steps after it are not counted. A connected input is
reported with what supplies it, and a value or the first values of a list as
Polars writes them. Some mistakes, such as `dt.date` on a String
column, are only found when rows are read, not from the schema; the editor
learns them in advance from the
[tables of allowed combinations](#which-combinations-polars-allows).
A chain that starts from a connected input is walked as the run makes it, a
literal of the input's dtype, so the tables' lines for a literal judge it, and a
list gives many values where a value gives one. A name that is not connected
leaves what follows it not known.
A number typed without a dtype is walked as the dtype Polars gives it by
itself, a whole number an `Int32` and one with a fraction a `Float64`, which `fit`
reads the same way, and among the values of a condition that are numbers it
takes their dtype, so `when(f).then(a).otherwise(0)` gives what `a` holds.
A hole starts an expression that nothing is asked of.
The tables are measured with typed columns, and a number typed without a dtype is a
dyn literal that adapts to the operand it is used with, so `u8 - 1` is a `UInt8`.
Where such a number is an operand of arithmetic or a comparison, the value `shift`
fills with, or the receiver of arithmetic or a comparison, the walk and `fit` read it
as the dtype of the column beside it where it fits that dtype, as Polars does: a whole
number fits a whole number type that holds it and any float, and a number with a
fraction fits a float and beside whole numbers is the `Float64` it is alone. Where it
does not fit, or the other side is no number, Polars widens the column to a dtype that
depends on the number, as `u8 - 300` is a `UInt16`, or refuses the call, and the dtype
of what the call gives is not known; the call still gives what the tables say it gives
for every dtype the number could take, where that is the same for all of them, as a
comparison always gives a `Boolean`. Two operands are not met so. The base of a power
does not adapt to its exponent, which adapts to the base: `2 ** u8` is an `Int32`, as the
number is alone, and the walk says so. A number with a fraction that is divided by a float
column is a `Float64` beside it, where a whole number adapts to the float: `1.5 / f32` is a
`Float64` and `2 / f32` a `Float32`. An integer raised to a number with a fraction is the
unresolved float the number is, which a select resolves to a `Float64` where it ends and
a sort key or a join's key does not, and which an operand it meets adapts as it does the
number, so its dtype is not known. A number with a fraction beside a Boolean that is floor
divided is a Boolean, which Polars cannot floor divide, though it floor divides a Boolean
by a typed float, so `f // 1.5` is refused and `f // 1` and `f * 1.5` are not. The
catalogue gives `pow`'s `exponent`, `truediv`'s `other` and `floordiv`'s `other` roles of
their own for these. A logarithm of a whole number, a `Boolean` or a `Decimal` to a base
with a fraction is the unresolved float the base is, as a power is, and `log`'s `base` has
a role of its own for it. A bound of `clip` is cast to the dtype it clips, so a number
beside it takes that dtype where it fits, a number with a fraction is cut to a whole
number beside whole numbers, and a number the dtype cannot hold is refused when the rows
run, so the walk does not say what the call gives; what is clipped does not adapt to the
bound. Where it is an operand of another method, such as a key to group by, what the
call gives is not known. The walk and `fit` know it where it
stands alone, counts rows as `shift` does, or is a value of a condition beside typed
values that it fits: a whole number adapts to whole numbers and numbers with a
fraction it fits, a number with a fraction only to numbers with a fraction, and
numbers alone have no class. A condition with no `otherwise` that holds one is not
known to end, as Polars refuses it. The fuzz target draws such numbers, some that fit a
narrow column and some that do not, outside the values of a condition, so that its
campaigns hold these answers to Polars.
The runner asks the named rules beside those tables, and not the tables, for
where an expression may start: `element` starts a chain only in a pivot's
aggregate. A row of the context table was measured with the chain ending at one
entry, with one argument, such as a bare `element()` or an untyped `lit(1)`, so
it cannot judge a start that other arguments or more links follow. Polars decides
the rest as the step is built, and when it refuses a pivot because of a column in
its aggregate, it refuses the step, not the expression. The contexts the tables
are measured in also say which table a join's `right_on` is read against.

Each step is also said as a short summary by the engine; the view composes none.
A summary is what the step works on, such as `by dcn, timestamp` or
`step_sla on step_to`; its words are in the language file's `summary` table, and
what is worked out is `…`. A name is written as the document holds it, and a typed
value as Python writes it, as above. The report of each step carries its summary,
and the step list and the editor's step menu show it. The engine does not write a
step as Python Polars source: Dageist adds functions of its own beside Polars', and
a Python rendering of every step would either not run as written or give slightly
different results, so the blocks, the summaries and the previews are what say what
a step does.

The claim-timing steps the step editor opens with, the seven claim rows they run
on and the inputs connected to them are one sample node in the engine,
`dageist_engine::claim_timing`. The host serves it to the view and the engine's
tests run it, so there is one copy.

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
found missing. A join's `on` is the keys of both tables, so Polars evaluates it
against each, and the walk judges it in both contexts. A pivot's aggregate starts
from `element()`.

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
| `join-key-names-are-not-repeated` | A join pairs no two output column names twice. Polars names a key by the column its chain starts from, whatever methods follow it, `literal` for a literal and by its first value for a condition, and refuses a join in which the key of one side and the key it is paired with repeat the names of an earlier pair: `left_on=[k, k.sum()]` with `right_on=[rk, rk.sum()]` is refused, and with `right_on=[rk, ra]` is not, for the pairs differ. A name repeated on one side only is allowed. `on` pairs each key with itself. An alias is refused in a join's key by Polars, and the catalogue holds none. The rule is asked by the walk, which finds the names of the whole join and places the refusal at the later left key that makes the pair again. Polars pairs the keys after expanding each side, so a key that may expand to several columns (a `col` of several names or a pattern), a key that starts from an input or is not picked yet (a hole), and an item that is no expression shift every pair after them. The rule judges only the pairs before the first such key on either side, and none after it. |
| `key-is-elementwise` | A join's own key holds no aggregate beside a column and no column compared with a null literal. Polars refuses such a key as not elementwise: it folds a comparison with a null literal into a null, at any depth, in an operand, before a method or in a condition's predicate, and a key whose column has gone is not elementwise. A subtraction with a null does not drop an aggregate it follows or is given: `k != f.len() - null` is refused. Nor does a fold drop a side that is more than one value or literals alone: `(k - a.sum()) > (b.first() == null)` is refused, for Polars evaluates the side that holds a column beside the aggregate. It allows a key that is wholly an aggregate, a null alone, a comparison of literals, a null beside a column that is not compared, an aggregate compared with a null, a column compared with a null inside the one value that a comparison of a null is given (`null != (f != null).min()` is a null whatever it is given, and `null != f` leaves a null where a column was), a column compared with a null that a later comparison with a null drops with the one value it follows (`(null > h).last() >= null` drops `null > h`, which `(null > h) >= null` does not), and in the key of a window in a select, a filter or a sort an aggregate beside a column. `and_` and `or_` given no operand are the expression they follow, so a null followed by one is still the null a comparison is given, and a comparison that follows it is the first Polars meets. The rule holds what the evidence settles. What Polars does with a comparison with what an earlier one folded depends on dtypes that the document does not decide, so a limit covers it. The context table measured an entry as the whole key, and what a key holds is a matter of the whole expression, so the caller that walks it asks `rule_forbids_a_key_known_not_elementwise`. |

`dageist_engine::allowed::offers` answers which entries may stand at a place,
given the context and what the expression has so far: nothing, a value of a
representative dtype and a shape, or a condition that is not finished. `offer`
answers for one entry, with what has been chosen for it, `Chosen`: the dtypes of
the arguments chosen for its expression parameters, and the dtype of the value
a start or a condition gives where the call decides it, which the tables cannot
(`Chosen::giving`: the column `col` reads, a literal given a dtype, the values
of a condition that ends in `otherwise`), and the options set on the call
(`Chosen::with_options`). `options` answers for a set of options alone. They
read the committed files, embedded in the engine, and the rules, and never run
Polars. Each refusal says who refused it, Polars at a stage or a
rule or the shape of an expression, and gives the reason. A rule that forbids an
entry here is applied first, so that its words explain the refusal. An entry may
stand at a place when the line of `continues` allows it, and a refusal of that
line is a refusal to stand. A rule about how a chain ends, and the line of
`ends`, do not refuse an entry, which may be continued, but are reported by
`Allowance::can_end`, which answers yes, no with the reason, or not known: the
rule first, so that its words explain the refusal, then Polars' reason. An end
rule that is about a dtype refuses a dtype known not to fit. The line of `ends`
for a start was measured with one argument, an untyped literal that cannot end a
join key's condition value where a typed one can, and a column that ends a
filter's predicate only if it is a `Boolean`. So where a start's arguments
decide whether it may end, it is not known to end until the call is told the
dtype it gives, and the rules then judge it. A method whose result an argument
decides is likewise not known to end a place that needs a `Boolean` until the
argument is chosen.

What the options chosen give, `offer` reads from the option table: a window whose
`mapping_strategy` is `join` gives a `List` of what it follows, and `dt.total_days`
with `fractional` true a `Float64`. An option set to its default is not a choice.
One option chosen is read from the lines that set it with the others at their
defaults, and more from the line of each pair of them; where those lines give
different results the result is not known. The option table was measured on one
dtype, on a column, and for a window given a `partition_by` and an `order_by`, so
what an option gives is read as a change of what the method gives at its defaults:
the dtype it was measured on, and a `List` of what the method gives elsewhere.
Polars gives a window the same result with either of the two alone, so the table is
read for a window given one, and the dtype table measures `order_by` alone. A window
given neither is refused by Polars, bare or with options, so no dtype is known after
it. After a single value, an option that changes the result leaves it not known.

`Step::walk` finds where every call of a step stands without building it in
Polars. It follows the document as running it does: the arguments of a call
first, each expression start by link, in the named positions that hold it. At
each call it asks `offer` for the place the chain has come to, giving it the
dtypes of the arguments, the options, and for a start the dtype of its column in
the table the expression is read against (a join's `right_on` is read against
the other table), of a literal given a dtype, of what a pivot's group holds, and
for a condition that ends the dtype its values have. It asks whether an
expression that a position holds may end there, whether a join's key holds an
aggregate beside a column, and whether a join's keys pair the same two output names
twice. A join's `on` is read against both tables, the other's after the left's, at
the same places, so the report holds two walked expressions at the place of each key of
`on`, the left then the right, and the view reads the first, the left. A column it names
that one of the tables lacks leaves what follows it not known, which is Polars' to refuse
as not found. A value of a condition in a filter is asked to be a
`Boolean` only where the condition is all the predicate is, and not where a method
or an operand goes on from it. For each expression it reports the context, the place of
each link and where a link added at the end would stand, the verdict on each
call, and whether it may end; for the step the first refusal, and why the tables
could not be asked past a point, where they could not: it does not ask where the
dtype before a call is not known. The report of a run carries the walk, and
gives each expression it built the context and the places of its links.

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
conditions nested two deep and `over`'s keys inside them, a second key beside a join's
key (a copy of it, its column taken as one value or tested for a null, or a chain of
its own, paired with the same column of the other table or another), a join's `on`,
which reads the one column `u` that both tables hold, sets of the options of
each call, the whole numbers that a rounding keeps, and one bound or both for a `clip`. For each it takes the engine's
[walk](#which-combinations-polars-allows) of the case, which asks `offer` at the
place each call stands in whether the call may stand and what it gives, whether
each expression a position holds may end there, and `options` of the options a
step is given. From that it predicts whether Polars allows the case and the dtype
after each link. A case whose answer is not known, as whether a start may end
where it was not told its dtype, is not predicted past that point.
It then runs the case through the step document, as the editor does, on three rows
with a null and on a table with no rows, and compares: a case is allowed only
when the schema and both runs succeed, as the tables count it, and the dtype
after every link that the tables gave is the class Polars gives. The case is the
document's own text, so a kept case replays exactly. The
[how-to](how-to.md#check-the-tables-against-polars) says how to run it for longer
and to replay a case.

A disagreement is either a defect of dageist, a combination the tables were not
measured on, or a part of an expression that Polars does not evaluate. The first
found while building the target was `offer` giving a `shift` the dtype of what it
shifts when a `fill_value` of another dtype was chosen; it is corrected, and its case
is kept. Others were found where a limit had excused a gap of `offer`, and are
corrected: a place deeper than two positions judged by its first two, a start judged
on the dtype it was measured on, the options chosen not told, and a single value
beside a column in a key offered. Their cases are kept, as replays that check.

**Where Polars does not evaluate a part.** The editor holds every part of an
expression to its types, whether or not Polars evaluates it. Polars skips parts: it
folds a comparison with a null into a null, decides a condition by a constant,
drops a sort key that is one value, never reads what a `len` counts in a `select`
of one values, and runs a window's keys on groups where no row may reach a
predicate. The target does not model what Polars skips. Where the tables refuse a
part and Polars allows the step, it asks Polars about the part alone, with a
probe, a document in which the part must be evaluated:

- for a link, the chain through that link selected on its own, in the same input;
- for a condition's predicate, a condition with that predicate and literal values
  selected on its own, and for a filter's predicate the predicate as a filter;
- for a part of a pivot's aggregate, the same pivot with the probe as its
  aggregate, since `element` stands nowhere else.

Polars folds some of these as they are written, so that the chain alone is folded
the same way and allowed: the length of a literal over a window is a constant, and
a comparison with a null is a column of nulls whatever the rows are, so what follows
is not evaluated. What is not settled by the part as written is asked apart from
what surrounds it, after the limits, in these ways:

- for a link, the refused call, with its arguments as written, selected on a column of
  the dtype that Polars gave what reaches it, which Polars cannot fold as it does a
  literal. The dtype is Polars' as well as the tables', and a case where they differ is
  a disagreement about the dtype, found first. Where the call is given a literal, which
  Polars folds or broadcasts however the rest is, the call is asked again with each such
  literal a column of its dtype too, so that a part Polars skips for another cause, as
  a branch no row takes or a sort key of one value, is not hidden by the literal in it. A
  literal that a call inside an argument is given counts too. The call is asked a last time
  with its receiver and every argument a column of the dtype that reaches it, which Polars
  and the tables both gave, so that nothing an argument holds, as a comparison the tables
  allow without knowing the dtype of a number, is refused before the call is;
- for a refusal that a join's key is not elementwise, the part that holds the refused
  call as the whole key of the same join, the only one where the join has several, for the refusal is of the position and
  Polars skips it where the key is folded away; and, where that part is one value, as
  an aggregate and a running value of it are, which Polars allows as a key whatever it
  holds, the refused call on a column as the key;
- for a refusal that is the target's own comparison of a schema (the call's plan says
  one dtype and its rows give another), which is no refusal of Polars and which no
  probe of the call alone can be refused for, the refused call on a column in the
  position the case holds it. It counts only on the evidence of the mismatch,
  together: Polars' own plan and rows for the call on that column, selected on its
  own, differ exactly as the tables' reason says; Polars refuses the call on that
  column in the position (a sort key of one value is dropped, and the call on a column
  as the key panics); and the call on a column whose plan and rows agree is allowed
  there, and so is the column alone. A refusal that the control meets too, as a
  filter's predicate that is not a `Boolean`, or a panic that has no part in the
  mismatch, shows nothing;
- for a refusal by the rule that a predicate be a `Boolean`, where Polars refuses the predicate
  as written for another call it holds, one the tables allow without knowing the dtype of an
  argument, or does not evaluate it at all, as under the length of a condition, a column of the
  dtype that Polars gave the end of the predicate, as the predicate of the same kind. Polars
  refuses a column of that dtype as a predicate for the reason the rule gives wherever it
  evaluates it;
- for a call the tables refused without knowing the dtype of an argument (a window
  whose options the tables were not measured with, or a number given no dtype), the
  part as written, alone. The tables refuse such a call only when no dtype the
  argument might have lets it stand, with the reason Polars gave for one of them, so
  the reason of the probe, which has the argument's own dtype, is not the tables' in
  words and is not compared. It counts when Polars refuses the probe for itself at the
  refused call, and the part before the call, alone, is allowed. It shows that Polars
  refuses the part at that call, not why: a refusal there for another argument or
  option would count too, and the reason the editor shows for such a call is the
  tables' for a stand-in dtype, not Polars' for the argument's own. A window that
  joins after a single value, which gives that value again where the table says a
  list, has no known dtype, and is one thing that makes a call blind in this way.

The first two count when Polars refuses the probe for the reason the tables gave, and
the case then counts as **refused where Polars does not evaluate the part**: the
tables are stricter by design, and the tally names it. The replay and the outcome say
what the probe showed: that the probe was refused for the tables' reason, that the
call's plan and rows differ as the tables say, the call is refused in the position
and a call whose plan and rows agree is allowed, or that Polars refuses the call that
the tables refused for every dtype of an argument they did not know. The reasons are matched by what
Polars objected to and the call it names (the symbol of a comparison, a name in
backticks), without the dtypes and column names it quotes, and for a rule that a
predicate be a `Boolean` by Polars' words for it. A refusal that Polars places must
lie at the refused call or predicate. One it does not place, as the refusal of rows,
counts only when the part before the refused call, alone (for a key, as the key), is
not refused for the same reason; for a call in an argument of a key's call, the part
before it in the argument counts too, in the key with the argument cut there. So a probe is not settled by an objection to another
call. What the target says of a schema is neither the tables' reason nor a probe's
for these two, and is settled only by the evidence above. That evidence ties the
refusal to the mismatch by what goes with it, not by what Polars says: the mismatch,
the refusal in the position and the control allowed there are all observed, and the
words of the refusal are not read. Which of two failures of a probe Polars names depends
on which of its threads gets there first, so a probe refused for another reason is
run again, up to sixteen times, before it counts as one that is not. When Polars
allows the probe, refuses it for another reason or at another call, or the evidence
of the mismatch is missing, the disagreement stays, for nothing shows the tables
right. The probe is a document of the same kind as a case, so a kept case replays
it. A kept case of this kind is filed as `regressions/stricter-<what it shows>.case`,
and the normal run replays it and expects the outcome to say so.

What a probe shows is that the tables are right about the part or the call when it
is evaluated, and the target takes that for a part Polars skips. The part as written
shows it, for Polars refuses what it was asked to evaluate. The probes apart from the
case, those that match the tables' reason, do not tell a part Polars skips from one
it evaluates on a value that hides the refusal, as a literal it broadcasts or the
nulls that the sample's rows leave. The
limits say what Polars evaluates and accepts, so they are asked before those probes
and after the part as written: in that order the part as written, then the limits,
then the probes apart from the case.

One pair of reasons is matched though their words differ. The tables are measured on
one thread, where Polars refuses a bitwise call between a whole number and text by
failing to unpack the text as the number. On many threads its streaming engine does not
fail: it gives the column of the whole number where its schema says text, which the target
counts as a refusal in words of its own. A failure to unpack a series, as the tables'
reason, and that mismatch, as a probe's, are the same refusal of the same call. So is a
failure to unpack a list, or an array, as a number and a failure to cast it to that number,
which is how the streaming engine refuses a bitwise call between a number and a list or an
array.

The second kind is a limit, for what Polars evaluates and accepts, which no probe
of the part shows, and for where the tables allow and Polars refuses. A limit is
never for a part Polars skips: the editor holds that to its types, and a probe
settles it. There are nineteen. Each is named in the target, counted when it excuses a
case, and has a kept case that the normal tests replay, which only that limit
excuses, named `uncovered-<limit>.case`, with others
`uncovered-<limit>--<what it shows>.case` (a case that another limit explains too says so,
`--also-<limit>`); each has a test that the same feature elsewhere in the case excuses
nothing. A named rule's refusal is excused by no
limit: a probe settles it or it is a disagreement.

A limit excuses a disagreement only when it is the cause of it. The tables always
say where they refused, and that place must lie in the expression that holds what
the limit is about. Polars says where only when it refused while it built the
step and worked out its schema, and then a refusal of the whole step, as a join's
keys are refused, has the step for its place, so the key that holds the cause is
found by what each key holds. A refusal when Polars collected rows, the empty
table's too, and a panic, have no place. For those the reason and the feature in the
case are what there is, and the limits that can only be gated so (the lengths of
the rows, an overflow, a value beyond a decimal, a panic, a window over a window, an explode, the empty table
of a pivot) say so below. Where a refusal of the rows has more than one possible cause
in the case, the target asks Polars which: the case is run again with the suspected
cause undone, as for an `explode`, and the sample's rows are followed through a chain
for what a shift leaves null. Where the tables refuse and Polars allows, a literal at
the place is asked the same way: the case is run again with the literal a column of
its dtype, and the literal is why only where Polars then refuses it for the tables'
reason. A part that Polars skips and a part it refuses are told
apart the same way, by what Polars does with the part alone or without its cause, and
each has a case that holds the cause and one that differs from it in that alone. Two limits may both explain one case, and it is then
excused by both.

A named rule is dageist's, and may be stricter than Polars only where it is so by design,
which the target names: `pivot-aggregate-gives-one-value`, since Polars lets a running
value stand in a pivot's aggregate, and the rule that a predicate be a `Boolean` or a
rule a probe shows right where Polars does not evaluate the part. Any other rule that
refuses what Polars allows is a disagreement. The target compares nothing
more of a case that the rule by design refuses and Polars allows: the dtypes before the
refusal are compared, but whether Polars allows the rest is not. That rule is stricter
wherever it refuses, which is about half of the pivot cases (72 or 73 of the 157 or 158 of
the fixed seed, and the same share in longer campaigns), so those pivot aggregates are checked
little; the tally counts the pivot cases and those the rule refuses apart.

A case that the tables and Polars both refuse is evidence of agreement about the
refusal only where Polars refuses it for the reason the tables gave: the objection
and the call it names, as for a probe, or, for a named rule, what Polars says of what
the rule is about (that a predicate be a `Boolean`, that a join's key be elementwise,
that `element` stand only in a pivot's aggregate, in the engine's words or in
Polars' that it is not allowed in this context, that a join pair no key names
twice). A key that the tables refuse as not elementwise, which Polars refuses for a
column the other table lacks or for the key names it pairs twice, is a case where
Polars never judged what the tables did, and a rule that Polars does not hold has no
reason of Polars' to be refused for. Such a case is not a disagreement: both refuse,
and the allowance is right. It is counted apart, as refused by both for another
reason, with what each said, and is not counted among the agreeing; its outcome is
an expected rejection that names both reasons.

The tally ends with the totals of the cases that agreed, that both refused for another
reason, that a limit excused, that were refused where Polars does not evaluate the
part, that a rule stricter by design refused, and that disagreed. The fixed seed's
1000 cases give none disagreeing; 16 of those that agree are refused by the rule against a
repeated pair of key names, and the tables could not be asked past a point in 22 more. These
counts are the same at one thread, at two and at the machine's own, and so are the 10 cases
refused where Polars does not evaluate the part. The others are not: Polars' thread count
changes which reason it gives where the tables and Polars both refuse, and a case in which
Polars panics at some counts and not at others moves a case between them, so the cases that
agree are 806 to 808, those refused by both for another reason are 98 or 99, those a limit
excused are 6 or 7 and those refused by the rule that is stricter by design are 72 or 73
of 157 or 158 pivot cases. The test asserts none of those.

| Limit | Excuses |
| --- | --- |
| An `explode` changes the lengths | `Expr.over`'s `mapping_strategy` as `explode` gives other lengths, which meet those of what follows it. The tables do not measure what meets them. Polars refusing the rows, with a reason about lengths, in a case with an `explode` that is why: the case run again with each `explode` undone is not refused for lengths (no place: the reason gates it, and that run shows the cause); and the tables refusing, with a reason about lengths, at the window or at a call after it, or in the arguments of a link after it, in the same chain, which Polars allows; not before the window, not in its own arguments, and not an `explode` in another operand or item. |
| A literal | An argument or a start is measured as a column of the dtype, except the values of `shift` and the arguments of a pivot's group. A literal, or a condition of literals, is one value, which Polars casts and broadcasts as it does not a column. The tables refusing at a call that follows a literal or is given one, which Polars allows (a literal inside what the call is given counts at any depth, as the null of `b / null` in a comparison, which Polars folds into a null), and what a call after a literal is given, at any depth, is met by one value: the tables refusing it when they run rows, which Polars allows. A literal at the place is why only where Polars refuses the case for the tables' reason once each such literal is a column of its dtype (the class Polars gave it, the same case in all else): the case run again. A literal beside a part Polars skips for another cause is no cause, for the column is allowed as well, and the limit that is the cause, or the probe of the call on columns, excuses the case. A key that the tables refused for a call and not as a position is run again as the only item of a `select`, for a column beside an aggregate is not elementwise and a literal is. None in a pivot's aggregate, which reads no column. |
| A pivot's aggregate | It is run on the values of each group, with kernels of its own, and on no groups when the table is empty. Polars refusing the empty table, with a reason that an operation is not supported or that a dtype is null, in either quoting (no place); and the tables refusing, with a reason about a dtype an operation does not support, a comparison, an operation, an evaluation, a cast, a struct field or lengths of a window over a group, at a call of the aggregate, which Polars allows |
| A single value in a key | A key that is wholly a single value, an aggregate or a null, is not one that has to be elementwise in what it holds: Polars allows a running count under a `last` there. The context table measured the entry as the whole key. The tables refusing, with a reason about a key not being elementwise, inside a key that is wholly single, which Polars allows. A key that holds an aggregate beside a column, or a column compared with a null, is refused by the rule above, and is not this. |
| A null comparison in a key | Polars folds a comparison with a null literal into a null before it asks whether a join's key is elementwise, and what it leaves depends on the dtypes and on what the comparison follows, such as an aggregate. A column compared with a null literal is refused by the rule, and the rest, such as a comparison with what one folded, is found by running. Polars refusing the join as not elementwise, with no place, in a case where a join key, or the key of a window, holds a comparison with a null literal at any depth, and no other key of the join is a cause: a key that is not wholly one value and holds a running value, an aggregate or a window. A key that is wholly one value is allowed beside another key, so it is not. |
| A shift that leaves nulls | The tables ran every call on columns that hold values, so what Polars does with a call that has only nulls to meet was not measured: it does not meet the dtype that the tables refuse when they run rows. A shift fills with a null unless it is given a `fill_value`, and the sample's three rows decide what is left, so the target follows them through the chain. The start column holds a value in each row, and `a` and `b` a null in the middle one; a start that is not one column of the sample holds nothing known. A shift moves the rows down for a positive number and up for a negative one, and the rows it empties hold the `fill_value` (a null where there is none). A running sum or maximum and the parts of a date keep each row where it is and null where it was; `is_null`, `is_not_null` and `cum_count` give a value in every row; a comparison and arithmetic give a null in each row where the rows they follow hold one, or an operand does (a column with a null in the middle row, or an aggregate that is null, meets every row), and leave the others unknown; `and` and `or` carry none, for a null with a true is a true; `xor` does carry one, and is left unknown, which excuses less; `offset_by`, a condition and a window that is not an `explode` keep the rows in place and leave what they hold unknown, and give the one value that an aggregate leaves to each of the sample's three rows; any other call leaves nothing known. The first and the last take a row, or the first or last that is not null where they skip nulls; the minimum and the maximum are null of rows that are all null and a value of any that holds one; a sum of rows that are all null is not known, since it is zero of numbers and null of a column of `Null`. The tables refusing, when they run rows, at a call that follows a chain whose rows are all null, or is given one. Polars refuses a comparison of a list with a `Boolean` after none, one or two shifts, and allows it after three. Nulls that Polars folds whatever the rows are, as those of a comparison with a null, are not the sample's: a probe settles them. |
| A join key's dtype | The other table's key is made of the dtype the chain gives on its own, and the join may resolve the chain to another, as the precision of a decimal a null is taken from. Polars refusing the join, with a reason that names a decimal (no place). |
| The lengths of the rows | Whether lists, or other values, of different lengths combine depends on the rows. Polars refusing the rows with a reason about lengths, in a case with no `explode`, or one that is refused for lengths still when each `explode` is undone, as a window that explodes beside a literal over a window that Polars refuses for it (no place). |
| An overflow | A debug build of Polars panics where a release build wraps. Polars panicking so (no place). |
| A value beyond a decimal | Polars compares a `UInt128` with a `Decimal` by casting the `UInt128` to `decimal[38,0]`, which holds 38 digits, and a `UInt128` holds more. The sample's values are within the decimal, so the tables measured what a value within it meets, and a comparison of a `UInt128` with a `Decimal` is allowed by the dtypes alone; a value beyond it, which a subtraction wraps to below zero, fails the cast. Polars refusing the rows with a failed conversion of `u128` to a decimal, in a case that holds a subtraction (no place: the reason and the subtraction gate it). The table with no rows holds no value to fail, and another conversion is not this. |
| A panic | One is an internal check that failed on the plan Polars made in `select`; another place, or a combination of what was measured singly, may not reach it, or may reach another. Polars panicking where the tables allow (no place), and the tables recording a panic where Polars allows. |
| A window over a window | A window whose receiver holds another window, in its chain before it, or in the arguments of those calls, is run in the aggregation context of the outer one, where a key that is not elementwise is refused; the tables measured one window. Polars refusing the rows with that reason (no place). A window in a key, or two windows apart, is not this. |
| A hidden schema | The tables hold the schema of a method and what its rows give as one thing, and refuse a method whose plan says another dtype than its rows give. That is the target's own comparison, not a refusal of Polars, and it shows only where the method is the output of the step: inside an expression that goes on from it or compares it, Polars evaluates the method and the step's rows do not show the difference. The tables refusing, with the reason that the schema says one dtype and the rows give another, at a call that is not the last of the step's own expression, which Polars allows. A sort key and a join's key are no output of the step either, so the same reason at the last call of one is excused, unless a probe of the call on a column in the key settles it, which says more of it and is asked first. |
| A negative exponent | Polars raises an integer to a power by taking the exponent as a `UInt32`, so it refuses a negative whole number as the exponent of an integer when the rows run. The exponent is a value, which the tables measured as the dtype and the number 1. Polars refusing the rows with a failed conversion to `u32`, in a case with a `pow` whose exponent is a whole number typed below zero or holds a subtraction, a negation or a not, which can take the values of a column below zero (no place). The sample's rows are 1, a null and 3, so a sum or a product of them, a power of a positive number or of a column is not this. |
| A null in a bitwise call | A value of the dtype Null, a column or a literal (typed as it or not) or what a call gives that dtype, is an operand of `and_`, `or_` and `xor` that the tables allow, giving a `Boolean`, as `select` shows, and Polars meets it where `select` hides it. It negates what a bitwise call combines operand by operand, so a `not_` meets each operand and refuses the dtype Null: Polars refusing the rows with the dtype Null not supported in `not`, in a case with a `not_` after a run of `and_` and `or_`, any call of which meets a null, as the receiver or an operand, by the dtypes the tables give and as a null goes on through the calls after it, which the negation is pushed through (no place); an `xor` or any other call ends the run. It gives a filter's predicate the dtype Null: Polars refusing the rows with a predicate that must be a `Boolean`, got `Null`, in a filter with a predicate that ends in bitwise calls the first of which meets a null (no place). An `and_` with a null on either side gives Null, so does an `or_` of nulls, and the dtype goes on through an `and_`, and an `or_` of another null, that follows; an `or_` or an `xor` of a `Boolean` and a null is a `Boolean` and is not this, nor is a predicate that a call other than a bitwise one ends. |
| A division by zero | Polars refuses to divide a `Decimal` by zero, where a whole number divided by zero is null and a float is infinite. The divisor is a value, which the tables measured as the dtype and the sample's values, none of them zero; a remainder or a difference of them can be. Polars refusing the rows with the reason that it divides a `Decimal` by zero, in a case with a `Decimal` as a column that holds a division, a floor division or a remainder (no place). |
| A bound the dtype cannot hold | Polars casts a bound of `clip` to the dtype it clips as strictly as it can, and refuses the rows when the bound holds a value that dtype cannot hold: a number typed without a dtype that is out of its range or below zero beside an unsigned dtype, or a value of a float that is no whole number it can hold, such as the infinity and the not-a-number that a logarithm to the base 1 or a root of a negative number gives beside whole numbers. The tables measured a bound as a column of the sample's rows, whose numbers every number dtype holds. Polars refusing the rows, with a failed conversion that has no place, in a case whose bound is why: the case run again with the bounds of every `clip` undone is not refused for a failed conversion, which is also why a failed conversion that another call causes, and a clip with no bound, are not excused. The table with no rows holds no value to convert. |
| A bound with values of its own | Polars casts the values of a bound of `clip` to the dtype it clips, and a bound whose values that dtype holds is cast whatever its dtype is: text that reads as the number, or a list or another dtype that holds nothing but nulls. The tables measured a bound as a column of the sample's rows, whose text is a duration no number reads and whose lists no dtype holds, so they refuse the cast of a dtype that the values of another bound pass. The tables refusing, when they run rows, with a failed conversion or a failed cast of a dtype, at a `clip` that has a bound that is not a bare column, which Polars allows, and where the tables, asked again with the computed bounds of every clip undone, no longer refuse a clip: the computed bound is then why. A bound that is a bare column holds the sample's rows, as the tables measured it, so a refusal that is its own is not this, whatever the other bound is, and neither is a clip with no bound or a refusal at another call. |
| A window over a literal | A window whose partition holds no column, a literal or a running sum of one, gives Polars' streaming engine no rows, so what follows it in a chain that starts from a literal is never evaluated, and the rows do not show a plan that they contradict. The tables hold the schema of a method and what its rows give as one thing, and refuse a root of a date, whose schema names the date and whose rows are a float, which is the target's own comparison and shows only where rows are given. The tables refusing, when they run rows, with the reason that the schema says one dtype and the rows give another, at a call after such a window in a chain that starts from a literal, which Polars allows; the window is why only where Polars, asked again with each literal partition a column of the table, refuses the case for that reason, which a column start, a window over a column and no window do. None in a pivot's aggregate, which has no column to stand in. |


A case that the tables cannot be asked about, because the dtype of an argument or
before a method is not known, is not predicted past that point and is counted. An
expression that holds such a call has the dtype of its own calls, and Polars refusing it
at that call, when it works out the schema, is no dtype the tables got wrong. The
generator keeps to what the tables measured where a value, not a dtype, decides: a
`shift` by the number 1 of a dtype, or the number -1 of a signed integer dtype (the
tables measured the dtype, which the sign does not change), a number of significant figures
of one or more, durations that are text of `b`, two values of a condition of one dtype, a join
without `validate`.

## What The Step Editor Storyboard Has Settled

The step editor is where the design of a transform node is being worked out.
It was drawn first against fixed sample rows in the view, and now runs on the
engine. The decisions below are the ones the core is expected to carry.

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
  Polars text of a step writes the method, such as `.ne(`, `.ge(` and `.le(`,
  and no operator. The signs are the engine's, from the language file, so the
  editor and the list of steps write them the same.
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
  fields. Text is shown between quotation marks that are drawn, not typed, and
  a number as the engine writes it.
  The list never offers the values a field holds, since a column can hold
  millions of them.
- **Inputs, not variables.** A node declares no variables. What its steps can
  use arrives from the nodes connected to it: tables, single values and
  lists, each with a name, its dtypes and the node it comes from. Logic is
  reused by connecting several nodes to one node's output.
- **Leaving the lazy query.** A step the engine cannot keep in a lazy plan
  carries a marker, which the engine's report decides. Pivot is the only step
  marked so far, because it collects the rows before it to build its columns.
  Its columns are the values of a connected table.

## Current Limitations

The application opens its window on two features. The steps of a transform
are edited on the engine's claim-timing sample node, offering the operations of
the engine's catalogue; its two windows
keep the steps the same by telling each other of every change, where the core
will later own them. The step editor runs its steps on the engine through the
host and shows the engine's columns, dtypes, refusals and rows. The table
viewer shows a generated sample through the engine. The graph model, node
types, pipeline execution and canvas interactions are not implemented yet.

A third tab shows the Pieces page. It is a design prototype, not a feature:
a static page in plain HTML, CSS and JavaScript where the way the step
editor is drawn is tried out in several versions before one is built. It
works on made-up samples of its own, shares no code with the view's
packages and calls no host command. The tab that is open is kept in the
page's address, so reloading the page returns to it.

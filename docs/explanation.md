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
| Shell | `@dageist/desktop` | The composition root. It decides which host the view talks to and which feature is on screen, and owns the page, the window's frame, the bundler configuration and the design tokens. |
| Host door | `@dageist/host` | The contract of every IPC command and its two implementations: over Tauri, and over the [bridge](#the-bridge) in a plain browser. No other package knows Tauri or the bridge's transport. |
| Design system | `@dageist/design` | Primitive components. It knows nothing about dataframes, steps or tables. |
| Features | `@dageist/canvas`, `@dageist/steps`, `@dageist/table` | One package per feature, each exporting one entry component. |

Some components are copied in as source rather than installed, and changed
to fit the view's lint and its colours. The design package's
`src/shadcn/` holds components from [shadcn/ui](https://ui.shadcn.com),
built on Radix Primitives, such as the side panel. The canvas package's
`src/react-flow-ui/` holds the node and pin components of
[React Flow UI](https://reactflow.dev/ui), which draw the canvas's boxes on
[React Flow](https://reactflow.dev). A README beside each lists where its
files came from and every change made to them. The shell's stylesheet sets
shadcn/ui's colour names, such as `sidebar` and `card`, to the view's tokens.

The shell draws the window's frame. The main window's title bar is drawn over
the page, so macOS draws its window controls at the top left, where the
`trafficLightPosition` in `tauri.conf.json` puts them, and the side panel's
toggle stands beside them; a browser has no such controls and shows the
application's name there instead. The window behaves as an application, not a
page: a click selects no text, the pointer stays an arrow, and the edges do
not stretch when scrolled past. In the application's window the browser's own
menu and keys, such as reload and print, are turned off as well; in a
browser they are left on, for the page is being worked on.

Three rules keep this workable as the application grows:

- **Features never import each other.** They meet only in the shell, which
  passes each one what it needs, such as the host.
- **Features depend on the host contract, not on Tauri.** That is why the view
  can run in a plain browser against the [bridge](#the-bridge) while it is
  being worked on.
- **Rust stays the authority.** A feature holds interface state, such as what
  is selected or which menu is open. Values and validation come from the host.
  The host holds the step editor's document and the engine makes every edit of
  it; the engine runs the document and reports the columns, the dtypes and the
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

The dtypes an entry accepts are named by groups of them, in the order the language file lists them:
every dtype, the numeric and the integer dtypes, the temporal ones and each of `Datetime`, `Date`,
`Time` and `Duration`, `String`, `Boolean`, and `List` and `Array` of any element dtype. A group says
what Polars takes of some dtype in it, and a test that runs the method on a column of each group
holds an entry to it, with a column of lists and one of arrays: a method that claims every dtype
must take a list and an array, and the ordering comparisons, `pct_change`, `replace` and
`replace_strict`, which Polars refuses lists for, name the groups they take instead. Which members
of a group, and which element dtypes, a call takes is the tables' to say.

Those build functions are the start of the path from a saved document of steps
to a Polars plan. The arguments for an entry are checked against its
parameters, their kinds, shapes and choice words before it is built. Result
dtypes are not stored, because Polars works them out from a schema without
reading data; which combinations Polars allows is kept in
[tables](#which-combinations-polars-allows) beside it.

Every parameter is one of twelve kinds, and the view has one way of drawing
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
| pattern | A regular expression, in the syntax of Rust's `regex` crate that Polars uses, typed in; see [the pattern kind](#the-pattern-kind). |
| time zone | A name of the time zone database such as `Europe/Berlin`, or a fixed offset, typed in, and for a method that takes a zone off, none; see [the time zone kind](#the-time-zone-kind). |
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

The nulls, the checks and the conversions are three families beside these, and every name
is Polars' own. Nulls hold what fills, drops and counts what is missing: `fill_null`,
`fill_nan`, `forward_fill`, `backward_fill`, `drop_nulls`, `drop_nans`, `has_nulls` and
`null_count`. They are one family and not an aggregate family's `null_count` beside `len`,
because a person looking for what is empty looks for it by that word, and the hints say which
give one value. Checks hold the tests of what a value is or of whether it is repeated:
`is_nan`, `is_not_nan`, `is_finite`, `is_infinite`, `is_duplicated`, `is_unique`,
`is_first_distinct` and `is_last_distinct`. Conversion holds `cast`, `replace` and
`replace_strict`. `is_between` and `is_close` stay in Comparison with `is_null`, `eq_missing`
and `ne_missing`, for each compares a value with others.

`eq_missing` and `ne_missing` have no sign. A sign belongs to a comparison that Python spells
with an operator, `==` for `eq`, and the editor writes it as the summary does; Python has no
operator for these two, and a sign of the editor's own would be a name Polars does not have.
They are plain methods, listed beside the six, and a test in the editor does not offer them.

What Polars does with each is what the tables say, and some of it is not what a name suggests:

- `fill_null` takes a value or a strategy, and Python refuses both and neither, so the catalogue
  marks `value` as one of which a step gives at least one and `strategy` as given in place of it.
  The build refuses the two it refuses as Python does, in Python's words, and a `limit` with
  anything but `forward` or `backward`; `limit` is a whole number Python passes on as an
  unsigned 32-bit number. The seven words of `strategy` are those of Python's binding. Which
  dtypes a strategy runs on depends on the word, `mean` refusing text, booleans, lists and
  arrays, `one` text, `Null`, lists and arrays, `zero` `Null`, lists and arrays and `min` and
  `max` lists and arrays, so the option table is measured on every dtype, and Polars calls
  only a fill by `zero` or `one` elementwise, which a join's key must be, so the context table
  holds the method in the form each word gives it. The sample holds no null `Struct`, so the
  tables do not say that Polars refuses a strategy for one that does, which a limit covers.
  `forward_fill` and `backward_fill` are the strategies of those names, with a `limit` of their
  own.
- `fill_nan` is a condition on `is_not_nan`, so it takes numbers and refuses text, a date and a
  list, except where its value is the expression it follows, for Polars drops the predicate of
  a condition whose two values are one expression. `is_nan`, `is_not_nan`, `is_finite` and
  `is_infinite` take whole numbers too, for which they give false, and `Null`, and refuse a
  `Decimal`.
- `drop_nulls` and `drop_nans` change how many values there are: `drop_nulls` drops what is
  null, and `drop_nans` the NaN of a float. Whether a column has fewer values than the table
  has rows depends on its rows, which the tables do not measure, so they measure these two in a
  context on a column that holds no null; a column that holds one meets the rows of the table
  with a length that Polars refuses.
- `is_duplicated`, `is_unique`, `is_first_distinct` and `is_last_distinct` look at the whole
  column, and count a null as a value; `has_nulls` and `null_count` give one value for it. The
  first four are not elementwise, which a join's key must be, and the tables say so; the last
  two are aggregates, so a join's key may be one of them alone and not beside a column.
- `is_between` is two comparisons, of its value with each bound, so a bound that is a null
  literal with no dtype is a comparison with a null, which Polars folds into a null; a null
  that it follows is not, and a null bound of a dtype is one Polars folds for some dtypes of
  what it bounds and not for others, as is a whole number below zero, which Polars decides at
  once for what is unsigned, as a count is, which a limit covers. The two bounds and what the interval
  follows meet in one dtype, so where the bounds are of different dtypes the tables, which ask
  each bound beside a companion of the receiver's dtype, are not asked about either. Its
  `closed` has the four words of Python's. `is_close` takes numbers, and a date, a time or a
  duration beside a number, and its two tolerances are numbers from zero, which Polars refuses
  below it when it runs rows. A number typed without a dtype that is given to it adapts to a
  number it follows only: beside a date, a time or a duration Polars refuses it as ambiguous,
  where it takes a number of a dtype, so the walk says nothing of what the test gives of them.
- `cast` is given the dtype it gives, which is the one parameter of the kind of a dtype that a
  method has, and `strict` and `wrap_numerical`, of which Python lets wrapping win. What it
  accepts depends on the dtype it follows and the one it gives, and on the two flags, so the
  option table has a line for every dtype a method takes and every dtype a document can hold,
  alone and with each flag that is not at its default; a cast that is not given a dtype gives
  none. A `Decimal`, a `Struct` and an `Array` are dtypes the document cannot hold, and so
  are no dtype a cast gives; one of them that a cast follows is a column's.
- `replace` and `replace_strict` take `old` and `new` as expressions: a typed value, a column
  with no value twice, or a connected list. Python's mapping and its sequences are sugar for two
  such expressions, and the editor has no kind that draws a mapping of values to values whose
  dtypes the tables could judge: a pairs parameter holds literals, which the tables measure as
  columns, so the mapping is left out, and so is a sequence typed in, which a connected list
  stands for. `new` is given with `old`, and a strict one is given a `default`, which Python does
  not need but a step does, for a value that is not listed is refused when the rows run. Polars
  casts `old` and `new` of `replace` to the dtype it follows, as `clip` casts a bound, so a
  number out of range is refused there, and `replace_strict` casts `old` and gives the dtype of
  `new`, or `return_dtype`, to which it converts `new` strictly where no `default` is given, and
  as a cast that gives a null where a value does not convert where one is, so the option table
  consults the strict cast only for the first. `new` and `default` meet in the supertype of
  their dtypes, so where they differ the walk does not say whether Polars finds one. Polars
  refuses `new` of many values beside an `old` of one, and a `default` of many beside a value of
  one, which the walk knows from the shapes and holds to a rule.

The methods of text are three families beside these, and three tests stand in Comparison. Each is
Polars' own name with `str.` in front of it, as `dt.` is for dates and times, and the editor shows
it so: `str.to_lowercase`, `str.strip_chars`, `str.zfill`, and the rest. Text holds the methods
that give text: the case (`str.to_lowercase`, `str.to_uppercase`, `str.to_titlecase`), the trims
(`str.strip_chars`, `str.strip_chars_start`, `str.strip_chars_end`, `str.strip_prefix`,
`str.strip_suffix`), the padding (`str.pad_start`, `str.pad_end`, `str.zfill`), `str.reverse`, and,
with the methods that take a pattern [below](#the-pattern-kind), `str.replace`, `str.replace_all` and
`str.escape_regex`. Pieces holds the methods that take text apart or measure it: `str.slice`, `str.head` and
`str.tail`, the lengths `str.len_chars` and `str.len_bytes`, `str.split` and `str.join`. The
tests, `str.starts_with` and `str.ends_with`, give true or false, so they stand in Comparison with
`is_between` and `is_close`, where a person who wants a filter looks first, as does `str.contains`.
One family of the methods that give text, take it apart or measure it would be the longest list the
popover shows, which is how a method is found by its name; Text of fifteen, Pieces of seven and Search
of five keep each short, with what changes text apart from what measures or splits it and from what
finds a pattern in it, and a method is swapped for another of its family that takes the same
arguments, as the three cases take none and the three trims take `characters`. The hints are
where the plain words go: "length in characters", "add zeros at the start up to a length".

What Polars does with each is what the tables say, and some of it is not what a name suggests:

- A method of text takes text and refuses every other dtype, a `Null` included: Polars resolves
  the schema and refuses the rows, which the tables record as a refusal at the rows. `str.join`
  is the exception: it casts what it joins to text
  and refuses only what it cannot cast, a duration, a list or an array, which the tables show.
  `str.join` gives one value, so it is an aggregate for what the walk holds a join's key to, as
  `sum` is, and `delimiter` and `ignore_nulls` are its options: a null is skipped unless it is
  asked to make the whole a null.
- The lengths give a `UInt32`, in characters or in bytes, and a test gives a `Boolean`. A trim
  and `str.split` take text; `characters` not given is null, which trims whitespace. A test for
  a start or an end, which takes a prefix or a suffix, casts what it is given to text and refuses
  only what it cannot cast.
- A count of characters, which `str.head`, `str.tail`, `str.slice`, `str.pad_start`,
  `str.pad_end` and `str.zfill` take as an expression, has the role of the rows `shift` counts:
  a whole number typed without a dtype is that dtype. Polars casts it to a whole number, so a
  float or a `Boolean` is taken, text is not (the offset of a slice, which reads it as it does a
  date, is), and a value the whole number cannot hold is refused when the rows run, which a limit
  covers. How long a text is made, the length of the two pads, of `str.zfill` and of a slice, is
  a count of a role of its own, for Polars converts it to an unsigned whole number: a number typed
  without a dtype that is below zero is refused when the schema is worked out, so the walk gives
  the call no dtype, and the editor does not say what it gives. The head, the tail and the offset
  of a slice are signed: a negative count of `str.head` leaves out that many from the end, and a
  negative offset of `str.slice` counts from the end.
- `characters` of the three trims is optional and Python's default is `None`, which trims
  whitespace. The engine says what a method starts with, and a method whose first parameter a
  step need not give and Python leaves to `None` starts without it: the offers name it
  (`leftOut`), the step is complete as it starts, the document does not mention the parameter,
  and the slot shows its label, which a value or a field is picked in. The view follows what the
  engine names, and reads nothing off the parameter itself. A first parameter that is needed, or
  that has a value of its own by default, as the rows `shift` counts, is still to be picked or
  starts with the default. `over` is the one earlier method this holds for: it starts without
  `partition_by`, and Polars refuses a window that has neither it nor an order, with its reason.
- Polars meets a receiver of one value with each value of an expression operand of many, so a
  text method of one value (`lit("abc").str.head(col a)`) gives many values. The catalogue holds
  that for each expression parameter, as a fact about the method that is apart from the role of the
  parameter, which says how a number typed without a dtype adapts; the walk reads it. The
  parameters that do not meet it are the ones that take one value only (`shift`'s rows and fill) and
  the values a replacement lists, and an engine test measures every parameter against Polars.
- `fill_char` is one character, as Python's binding takes it, so the build refuses text of
  another length with that reason, and the document keeps it as a value, not an expression.
- A split gives a list of text for each row, and Polars joins on a list, so a split may be a join's
  key, with a list of text on the other side. The tables measure a call in a join's key with a
  continuation that casts the key to the dtype of the other table's key, which a list, an array or
  a struct cannot be, so such a call is measured as the end of the chain, with the other key the
  dtype the key has.
- `str.split` splits at plain text where `literal` is true, as Python's default has it, and at each
  match of a pattern where it is false, which Polars builds as its regex split, with the match kept
  by `inclusive`. `strict` is for a pattern: where the separator is no regex, a strict split is
  refused when rows run and one that is not strict gives a null, so beside plain text `strict`
  changes nothing, as in Python. The separator is an expression, which may be a column of patterns, so
  it is not a parameter of the pattern kind. Where it is text typed in and `literal` is false it is held to
  a pattern as one is: refused at the separator, with Polars' words, by the build and by the rule
  `pattern-is-valid`, whatever `strict` says. A separator that is a column is Polars' to check, row by row,
  when the rows run and not when the step is built, and `strict` decides there whether a row that is no
  regex is refused or gives a null. The table has no line for an option of a method that has several alone, only
  for each pair of them, so an option set alone is judged by the lines that pair its word with the
  others at their defaults, and refused when Polars refuses the pair; a step's lines are not read
  so, for they are measured with the arguments the harness gives the step.

The methods that put values in another order, and those that compare a value with its neighbours or count
what is distinct, are a family of their own and parts of three that exist. Order holds `sort`, `sort_by`,
`reverse`, `arg_sort` and `rank`, which are swapped for one another where they take the same arguments, as
`sort` and `arg_sort` take `descending` and `nulls_last`. Rows gains `diff`, `pct_change` and `rle_id`
beside `shift`, which also look at the rows around a value, and Aggregate `n_unique` and `count`. Every
name is Polars' own.

What Polars does with each is what the tables say, and some of it is not what a name suggests:

- Each of the five of Order, `diff`, `pct_change` and `rle_id` gives as many values as the value it
  follows, one value for one, which the tables measure; `n_unique` and `count` give one. `n_unique` counts
  a null as a value and `count` counts the values that are not null, and both are aggregates, so a join's
  key may hold either alone and not beside a column, which `key-is-elementwise` asks of every aggregate the
  walk knows. `sort`, `sort_by`, `reverse`, `arg_sort`, `rank`, `diff`, `pct_change` and `rle_id` look at the
  whole column, so none of them is elementwise, which a join's key must be, and the tables say so.
- `rank` gives a `Float64` for `average` and a `UInt32` for every other `method`, and `arg_sort` and `rle_id`
  a `UInt32`. A `rank` of `random` takes a `seed`, a whole number from zero that Python passes as an unsigned
  64-bit number, and is another order each time it runs without one. `diff` of dates and moments is a length
  of time, in nanoseconds for a time of day, and of an unsigned whole number up to `UInt64` the signed one
  that holds every difference (`UInt8` gives `Int16`, `UInt64` gives `Int64`), where a `UInt128` stays a
  `UInt128`; it takes what a subtraction takes, and refuses text, a `Boolean`, a list of text and a struct
  that holds text, and `pct_change` is the quotient of a value and the one `n` rows before it,
  less one, a float.
- `n` of `diff` and `pct_change` is a count of rows as `shift`'s is: one value, which Polars refuses many
  values of with `'n' must be a scalar value` or `n must be a single value`, and a whole number typed
  without a dtype is that dtype. The tables measure it as one value, as they measure `shift`'s, and the
  fuzz target draws it so.
- `sort_by` takes the keys it sorts by as `by` and `more_by`, as a sort step does, a flag of `descending`
  and of `nulls_last` for each of them or one for all. In a lazy plan, which is how a step runs, Polars
  meets a receiver of one value with each value of the keys of many (a `DataFrame.select` run at once
  refuses it), so the sum of a column sorted by a column is as many values as the column; a column sorted
  by a key of one value is the column.
- `diff` has a `null_behavior`, whose `drop` leaves out the first `n` values and so gives as many values as the
  rows less `n`, a count the data decide. The catalogue holds both of Python's words, `ignore` by default, and the
  option table measures `drop` as counted.
- The methods of an expression that give as many values as the data or the arguments decide, `filter`,
  `unique`, `head`, `tail`, `slice`, `limit`, `top_k`, `bottom_k`, `gather_every`, `gather`, `arg_unique`, `mode`
  and `unique_counts`, are catalogued with the shape of [a count](#a-count-of-values), and so is `diff` that is told
  to drop the first values, `null_behavior` of `drop`. The steps of a table that share some of their names are
  [below](#the-steps-that-cut-and-number-a-table). `search_sorted` gives a place for each value of `element`, so its
  count is the argument's, and so is `gather`'s of its `indices`. `is_in` takes a column of the dtype of the values it
  follows as their set, whole, so it gives the count of the values it follows, and a list column row by row, as the
  operand of a method is met, so it gives as many as the two meet; a Python list given as `other` waits for a list kind.
  A join may key on `is_in` of a list or an array, which Polars meets row by row, so the context table measures it as a
  form of its own, `Expr.is_in[other=list]`. `interpolate` waits, `Expr.item` and the functions that give a list of ranges
  are left out, and `left_out.rs` gives the reason of each; the functions that start a range or a repeat are
  [below](#ranges-and-repeats).

### A Count Of Values

A chain gives one value, as many values as the table has rows, or another number of them. The tables measure
the three, and the walk follows them as the shape of a value: `one`, `many` and `counted`. The first values of
a column, the rows that pass a filter, the values that differ and the places of a range are counted: the data
or the arguments decide how many, which is neither one nor the rows. A function that starts an expression may give one
too: the [ranges and repeats](#ranges-and-repeats), whose count is the arguments' own. A method that takes some of the
values it follows says so in the catalogue, as [`Counting`](../crates/dageist-engine/src/catalog/counting.rs) names what decides:
the count a parameter holds, a place and a length, every nth value, the data, a condition, or an argument's
values. Three rows decide the count of such a method by chance, so the dtype and option tables write `counted`
for it whatever the sample gives, and the context table measures it going on to one value, which every place
takes, and ends it as the table of counts says.

The table of counts, `counts.txt`, measures a chain of each shape ending in each context, of the values of a
column, one value of it, or its first two. Polars takes one value or the rows in a filter's predicate, a sort
key, the key of a window and a join's key, and refuses another count there with a message about lengths; a
select takes a count beside no column; a condition's value takes a count where the condition's predicate and
values are as many. A chain of a count that ends where the table of counts refuses it is refused, in the words
of the count and Polars'. The table cannot say when the count is by chance the rows, as a filter that passes
every row is, so a count is taken for not the rows, which is how a step of a table of many rows runs.

What the walk says of a count, where the arguments fix it, is a `Count`: the first values of a column,
`head(2)`, are at most two; the first five taken every second are at most three; the values that differ of a
column are not known, of the first five at most five. A method that gives a value for each of the values it
follows keeps the count, and an aggregate of a count is one value. What a method takes of one value is worked
out from that one value: `sum().head(2)` is the one value, and so are `tail(5)`, `slice(0)`, `unique` and
`mode`, but `head(0)`, `slice(1)`, `top_k(0)`, `gather_every(1, 1)` and a difference that drops the first value
give none, which is a count of no values and not one value, and a select refuses it beside a column. The one
value that such a method took is the value of no row of a group, so a window refuses it, where `sum()` and
`sum().first()` are the value of every row of theirs. The value stays taken wherever a call reads it: as
an operand, a bound, a fill, a power, a part of a moment or of a length of time, the share of an approximate
quantile, the default of a strict replacement, or the value or the predicate of a condition. It is not taken
again once an aggregate of it is, and the call of a method that takes some of the values is taken whatever it
is given. A few parameters do not carry it, for the call only looks up in it or by it: the keys of a sort,
which order the values and are no part of them; what a replacement lists, a table of values it looks up; the
values `is_in` is given as a column of their own dtype, which is their set, whole; and the share of the exact
quantile, which Polars reads once, before the groups. What a parameter that must be a scalar refuses is a
second state of the same value, carried differently: Polars asks only whether what a call is made of is a
scalar, so the value stays taken for it through every parameter, the keys of a sort, what a replacement
lists and the set `is_in` is given included, and only the share of the exact quantile leaves a call that is
given one a scalar. A test asks Polars and the walk of every expression parameter of the methods and of the
functions that start an expression, apart from those of `over`, of the methods that take some of the values,
whose call is a count and is refused whatever it is given, and of the links of a condition, which tests of
their own ask, with such a value given to it, in a window and as the fill of a `shift`, so a parameter is not
left to a case that came to mind. The parameters that Polars reads as one value are asked as well in a
select, with the rows, a count and such a value, and refuse what `parameter-takes-one-value` says. A
`filter` of one value is a count, for its condition meets the one value with each of its own.

The values a call meets must be as many as one another, or one, which is met by each of the others: the rows are
no count, and two counts that are fixed and differ are not the same. The rule `values-meet-in-count` says so,
and the walk applies it to the operands of a method, to the predicates and values of a condition, and to the items
of a select, where a step that adds columns, `with_columns`, meets them with the rows. A window takes a value as long
as its group, so `window-input-is-not-counted` refuses a count unless the window joins the values of a group into a
list or explodes them. A join's key that holds a method that takes some of the values is not elementwise, unless it
goes on to an aggregate: the key `a.head(2).sum()` is one value.

The statistics of a column, the places of its largest and smallest values, the tests of truth, the
approximations and the bitwise methods are more of three families that exist. Aggregate gains `mean`, `median`,
`std`, `var`, `quantile`, `product`, `skew`, `kurtosis`, `entropy`, `arg_max`, `arg_min`, `nan_max`, `nan_min`,
`all`, `any`, `approx_n_unique` and `approx_quantile`, and the folds `bitwise_and`, `bitwise_or` and
`bitwise_xor`; Cumulative gains `cum_min` and `cum_prod`; and Logic the six counts of bits,
`bitwise_count_ones`, `bitwise_count_zeros`, `bitwise_leading_ones`, `bitwise_leading_zeros`,
`bitwise_trailing_ones` and `bitwise_trailing_zeros`. Every name is Polars' own.

What Polars does with each is what the tables say, and some of it is not what a name suggests:

- Each aggregate gives one value, so a join's key may hold one alone and not beside a column, which
  `key-is-elementwise` asks of every aggregate the walk knows. The walk holds them in its list of the methods
  that give one value, and a test holds that list to the tables in both directions. The six counts of bits keep the
  rows, so a join may key on one.
- `mean` and `median` give a `Float64` for whole numbers, a `Boolean` (the share of the true) and a `Decimal`,
  keep a `Float16`, a `Float32`, a moment, a time of day and a length of time as they are, and give a `Datetime`
  for a `Date`. `std` and `var` give a `Float64`, or the narrow float they follow, take numbers, booleans
  and a `Decimal` and refuse every temporal dtype. `ddof` is a whole number that Python passes on as an unsigned
  byte, so the build refuses one below 0 or above 255, and a count of values that `ddof` leaves nothing to divide
  by gives a null.
- `skew` and `kurtosis` convert what they follow to a `Float64`, so they also take a `Date`, a `Datetime`, a
  time of day and a length of time, and refuse text, lists, arrays and structs. `bias` and `fisher` are flags that
  default to true: an unbiased skew or kurtosis is corrected for few values, and `fisher` subtracts 3 so that a
  bell curve is 0. `entropy` takes numbers and booleans and gives a float: `base` is a number with a fraction,
  `math.e` unless it is given, and `normalize`, true unless it is not, takes each value as a share of the total.
  An entropy of values that include a zero is not a number, as `0 * log(0)` is.
- `product` gives an `Int64` for the whole numbers narrower than 64 bits and for a `Boolean`, a `UInt64`,
  an `Int128` and a `UInt128` as they are, and the dtype of a float. A `UInt128`'s product has the schema `Int64`
  where it runs as a `UInt128`, which the tables refuse as they do any call whose schema and rows differ, and
  `cum_prod` does the same. `cum_min` takes what `cum_max` takes, and `cum_prod` numbers and booleans.
- `quantile` takes a share and an `interpolation`, `nearest` unless it is given one of its six words. The share
  is an expression of one value: Polars refuses many values with "polars does not support varying quantiles yet",
  and a value outside 0 to 1 or a null when the rows run, so the tables measure it as the number 1 of the dtype it
  has, as they do `shift`'s rows, and the fuzz target draws it so. A number typed in as the share is the number it
  is, whatever the value the method follows is, so the share has a role of its own. The share takes the numeric
  dtypes only: Polars reads a text or a moment as the share it writes in a select, and refuses it where a window or
  a pivot's aggregate evaluates the values of a group, which the tables do not measure, so the editor offers a
  number and the fuzz target draws one. The value the quantile follows takes numbers and the temporal dtypes, and
  the quantile gives a `Float64` or the narrow float it follows, and a moment, a time of day or a length of time
  for one of those; it refuses a `Boolean` and text. A quantile of a `Date` panics in Polars when the table has no
  rows, which the tables run too, so they refuse it. Python also takes a list of shares, for which the quantile is
  a list of values: a list of literals is a kind that waits, and a share that is a list is refused by the tables,
  which measured it as a number.
- `approx_quantile` takes the same share, with `method` of five words, `error`, which Polars refuses unless it is
  from 2^-32 up to but not including 1, and `error_tightness`. It keeps the dtype of the value it follows, whole
  numbers, booleans, text and moments included, and refuses a `Null`, lists, arrays and structs. Unlike the exact
  quantile, on the streaming engine, which is the engine steps run on, `approx_quantile` meets a column of shares
  and gives one value for each of them, so it broadcasts, which the broadcast test measures with a column of shares
  made from the sample's, for the sample's own numbers are no share. Python's in-memory engine refuses a column of
  shares for it as it does for the exact quantile. The tables measure the share as the number 1, as they do the
  exact one's. On the streaming engine the approximate quantile of a literal is refused, with a null quantile, where
  it stands in a value of a condition's branch or in the predicate of a `when` after the first, and not in a select, a
  filter, a sort key, a join key, a window or the predicate of the first `when`: a limit covers it.
- `arg_max` and `arg_min` give the row of a largest or smallest value as a `UInt32` and take every
  dtype; where several rows hold it Polars does not say which it gives, and gives another one at another
  number of threads and for text in memory; a null is neither. `nan_max` and `nan_min` give what `max` and `min` give, except that a NaN is the
  largest or the smallest where the values hold one. `any` and `all` give a `Boolean`
  and take numbers, which are true where they are not zero, and booleans; `ignore_nulls`, true unless it is not,
  skips the nulls, and without it a null is unknown, so an `all` of values that are true and null is null.
  `approx_n_unique` gives a `UInt32` of the distinct values, a null counted, and takes every dtype but a list, an
  array and a struct.
- `bitwise_and`, `bitwise_or` and `bitwise_xor` fold the values bit by bit into one of the dtype they follow,
  whole numbers and booleans, and the counts give a `UInt32` for each value, of the bits that are set or not, and
  of those before the first, or after the last, that is not. A float is refused by all nine.
- `item` is not catalogued: it gives the single value and Polars refuses a column of more than one, or none. The shapes say it
  is accepted after one value, which it gives again, and after a count of exactly one, and refused after a count of more; what
  it is for, to find that a table holds one row, is no shape, and the editor, which is strict where the data decide,
  would refuse it there, so what it would accept is a call that gives back what it is given. `left_out.rs` says so.

### Ranges And Repeats

`polars.int_range`, `date_range`, `datetime_range`, `time_range`, `linear_space` and `repeat` start an expression
with a count of values their arguments decide, under Polars' own names, in the Start family. Each is built as Python
Polars 2.1.0 builds it: `int_range` given one bound reads it as the end, counted from 0; `time_range` makes the bounds
it is not given of the day, from midnight to the last microsecond; `datetime_range` counts in nanoseconds where its
interval says them and it is given no unit; `repeat` converts its value to the dtype it is given as a cast that is not
strict; and `closed` says whether each end is in. Python's `eager`, which makes a series where a step makes a plan, is in
the signature and is never offered, and a step that sets it is refused where it is built. The interval is a
[duration](#the-duration-string) typed in, a text Polars reads.

Every bound, the number of samples, the value and the number of times are read by Polars as one value that it can tell
is one before the rows run: the rows, a count of values and the one value that `head` took of one value are all refused
with "non-scalar start passed" or "must be a scalar value", which the rule `parameter-takes-one-value` says. The tables
measure each with the number 1 of its dtype, as they do `shift`'s count. A range takes the numbers and, where Polars does,
the dates, moments and times of day; `repeat` takes a value of any dtype, a list, an array and a struct included, which
the tables measure with the first value of a column of that dtype where no number casts to it.

How many values the call gives is worked out where the arguments that decide it are typed in: a whole number written
out, or a date, a moment or a time of day made of numbers written out with `polars.date`, `datetime` and `time`. A
column, an aggregate or any other chain is not, and the count is then not known.

- `int_range` counts from the bounds and the step with whole numbers wider than an `i64`: from `start` up to but not
  including `end`, by `step`, so `0` to `10` by 2 is five, a step down counts down, a range that goes the other way from
  its step is none, and a step past the end is one. A step of nothing is refused, and so is a bound that the dtype does not
  hold: 300 is no `UInt8`, and Polars refuses it of a range that is empty too.
- `linear_space` and `repeat` give as many values as the number of samples and the number of times: none for 0, and a
  number below nothing is refused.
- A value made of literals, ranges and connected values and lists alone, which Polars meets as literals of their dtype, with no
  column anywhere in it, is not cut by `head`, `tail`, `limit` or the
  length of a slice given a number below nothing, whatever methods come between: a chain that starts at a range or a repeat of
  literals, one that meets a range as an operand, a branch, a predicate, a fill or an exponent, and a literal alone, as `lit(5).head(-2)`
  is. Polars' slice of a series of literals casts the length to an unsigned number and refuses it with "unable to extract
  length", and the engine of steps and Python's engines fold the cut away only after a chain of methods that give a value for
  each value, or the first values, which they refuse with their optimizations off, so the editor, which is strict where the
  optimizer decides, refuses those too; a test pins them. Where a column is what is cut, and a range is an operand of it,
  `(a.head(4) + int_range(0, 4)).cum_sum().head(-2)` runs in the engine of steps, and Python's in-memory engine refuses what the
  streaming one takes, as it does `a.tail(-2)` of a column, so the cut is judged by what has no column and not by the range.
  A range that a column is added to, `int_range(0, 4).add(a.head(4))`, is no value of literals alone, and its cut runs. The rule
  `range-is-valid` says it, whose words are about a range, a repeat and a value of literals alone.
- The dates, moments and times of day of `date_range`, `datetime_range` and `time_range` are counted by Polars, which the
  engine asks with the arguments typed in, as it asks of a duration: how many months fit between two dates, the last day
  of a month, a clock put back or forward in a time zone and a range counted in nanoseconds are Polars' to say. A range of
  more than a million values is not asked, and nor is one Polars panics on, and the count of either is not known. A build of
  Polars with debug assertions asserts on a range of days in a zone that crosses the clocks going forward, which the
  release builds.
- No column of Polars holds more than 4294967295 values in the build the engine is, so a range of more is refused.

What no arguments make a range of is refused by the rule `range-is-valid`, with Polars' words where Polars says it: a step
of nothing, a bound that the dtype does not hold, an interval that is no duration, is not whole days for a date, has a unit
of days, weeks, months or years for a time of day, or is shorter than the unit it is counted in, bounds that are no
dates, moments or times, a date that is no day of a calendar, a clock time that a zone has none of or two of, a number of
samples or times below nothing, and, for a linear space, bounds of dtypes that have none in common. Two moments in different
time zones are refused by `moments-share-a-zone`.

Some of what the editor refuses Polars takes, on purpose:

- A repeat of a struct that is cast to another dtype is refused, though Polars casts a struct of one field to the dtype of
  its field: the tables measure a struct of two, and the editor does not know how many fields a column's struct has.
- An interval of nothing or below nothing is refused whatever the bounds are, though Polars gives no values where the end is
  before the start and refuses it only where it is not: the editor is strict where the data decide.
- A count of exactly one beside the rows, an operand, a fill, an exponent or a share is refused, as `values-meet-in-count`
  and `parameter-takes-one-value` say of every count. The streaming engine, which a step runs on, broadcasts it where
  Python's in-memory engine refuses it. A test pins one case of each.

What the rows hold is not known to the editor: a bound that holds a null, a text that is no number given as the number of
samples, and a number below nothing that a time of day is made of are refused when the rows run.

A condition whose two values are the same one value may have its predicate dropped by Polars, so it is not judged; where they
are the same rows or the same count, as a range is, the predicate is met by them as by any two values, and a condition
whose predicate is not as many is refused.

A range is a count and not one value: the rule `window-input-is-not-counted` refuses it in a window, and a join's key or a
sort key takes it only where the table of counts says so. Python's `*_ranges` functions, which give a list for each row,
wait for a list kind, and `Expr.item` is not catalogued; `left_out.rs` gives the reasons.

### The Steps That Cut And Number A Table

Thirteen more steps of a lazy frame are in the family Steps beside `filter`, `sort` and `rename`, each
under Polars' own name: `reverse`, `head`, `limit`, `tail`, `slice`, `first`, `last` and `gather_every`
keep some of the rows, `drop` removes columns, `drop_nulls` and `drop_nans` drop the rows that hold a
null or a NaN, `fill_nan` fills the NaN of the floating-point columns and `with_row_index` puts a count of
the rows first. They are built as Python Polars 2.1.0 builds them, and the hints are where the plain
words go.

A step's result is a table, so the number of its rows is not stated before the rows run, as it is
for the methods of an expression that give as many values as the data decide: the engine reads the
schema from Polars and counts the rows a step kept of those it was given when it runs the working
window, and the next step is given the table that one gave. A `tail` of three rows then a `head` of two
is two rows, as the report counts them.

What Polars does with each is what the document tests say, and some of it is not what a name suggests:

- `head`, `limit` and `tail` take `n`, which Python's binding passes on as an unsigned 32-bit number, so a
  build refuses a negative one and one above it; Python refuses them as `ValueError` and `OverflowError`.
  `limit` is `head` under another name, and both are offered because both are Polars' names. `slice`
  takes a signed `offset`, counted from the end where it is negative, and a `length` that is left out
  as the most rows an index holds; a negative length is refused. `first` is the slice of the first row
  and `last` the tail of one, so a table with no rows has none.
- `gather_every` takes `n` and `offset` as unsigned numbers and gathers the rows `offset`, `offset + n`,
  and so on, of every column. Polars refuses an `n` of zero when it runs rows. Python runs it, as it runs
  every step that selects over every column, beside a column of its own that holds an empty struct for each
  row and is dropped after, so that a table of no columns keeps the height it gathers, and the build does
  the same. It runs `reverse` that way too, so a reverse of no columns keeps its rows.
- `drop` takes its columns by name, as Python's `*columns` does, each name read as a pattern where it is
  one, and refuses a name the table lacks only where `strict` is true. Dropping every column leaves a
  table with no column and the rows it had.
- `drop_nulls` looks at every column unless it is given a `subset`, and `drop_nans` at the floating-point
  columns; a name the table lacks is refused. Given a `subset`, `drop_nans` takes a column Polars can test
  for a NaN, and refuses another when it runs rows, with the words of `is_nan`: a whole number holds no
  NaN and is no reason to drop a row, text is refused. The walk says so beforehand, by asking the tables
  of `is_nan` for each column the `subset` names, which is the one step the tables judge by the
  method that Polars makes of it. A `subset` that is empty looks at no column.
- `fill_nan` takes `value` as Python makes it of a number, a text, a flag or none: a literal with no
  dtype, which Polars fills the NaN of each floating-point column with, and where it is text the column
  becomes text. A whole number or a text column holds no NaN and keeps its dtype. Python also takes an
  expression, which the step does not: its place is no position of the tables.
- `with_row_index` gives a `UInt32` column named `name`, which is `index` unless it is told, first, counting
  from `offset`, which Python's binding passes on as an unsigned 32-bit number, so it refuses one below
  zero or above it. Polars refuses a name that a column of the table has already, with `duplicate column
  name`.

The other methods of a lazy frame that cut or reshape a table are not catalogued yet, each with its
reason in `left_out.rs`:

- `unique` keeps one of the rows that repeat, and which one is the choice of `keep`, whose default `any` is
  whichever Polars meets first, in no promised order without `maintain_order`; a step that shows its rows
  cannot state them twice, which waits for a decision on what such a step shows. Its `subset` also takes
  expressions, a position no table is measured in.
- `fill_null` and `shift` run a method of an expression over every column, `fill_null` choosing the
  columns by the Python type of its value, and both take expressions where no table is measured;
  `cast` takes a dtype for each column, a pair no view draws yet, and is judged by the tables of `Expr.cast`
  for every pair; `top_k` and `bottom_k` take their keys as expressions. A step that is judged by the table
  of a method for each of its columns, with the arguments it gives each, is a shape of its own, of which
  `drop_nans` is the case with no argument.
- The frame aggregates `max`, `min`, `sum`, `mean`, `median`, `std`, `var`, `quantile`, `count` and
  `null_count` are functions of the table of their own, and not the methods of an expression that
  share their names: the sum of text is a null, which `Expr.sum` refuses, so no table says what each gives
  for each dtype. `explode` refuses any column that is no list or array, which no table measures, and
  `polars.concat` takes several tables, which no step takes.

### The Pattern Kind

The methods of text that look for or replace what matches a regular expression take their pattern as a
parameter of a kind of its own, so that the editor knows which parameters hold one, as the designer
of patterns will ([issue 96](https://github.com/bj-data-eng/dageist/issues/96)). Polars takes a pattern as an
expression, which may be a column of them, and the catalogue holds one that is typed in: a column of
patterns is not offered. The pattern is Polars' own regex: Rust's `regex` crate, compiled with the settings
Polars compiles it with, which the engine does not repeat.

How the engine knows a pattern is valid is to ask Polars. `str.extract_groups` is the one method
that compiles its pattern when it is built, and not when rows run, for the groups of the pattern name
the fields of the struct it gives. `check_pattern` builds that call on the text and gives Polars'
reason where it refuses, in Polars' words, which place the error in the pattern: an unclosed group,
an unclosed class, a repetition whose end comes before its start, a group named twice, a flag or a
property that does not exist, an escape with nothing after it, or a regex that compiles to more than
Polars' size limit. The same compile is what a method of text that takes a pattern runs when it meets the
rows, so what it refuses is, nearly always, what each would refuse, and the dtypes and the rows are not
asked. Not always: `contains` takes a fast path for a pattern that is a chain of literals, which it never
compiles, so a very large chain written with a flag such as `(?s)` that Polars accepts there is refused here
for the size limit of the compile it does not make. A dependency on `regex` or `regex-syntax` was not needed.

- A pattern is refused where it is built, at the parameter that holds it, with Polars' reason, and
  where the walk asks, by the named rule `pattern-is-valid`, which says it in its own words followed by
  Polars': a step that holds a half-typed pattern is a document, and is reported as refused at the
  pattern, whatever follows it. A pattern is refused whatever `strict` says: that flag is for a
  column of patterns, where a row's may be none, and a typed one is held to what Polars accepts. The `strict` of `str.contains` and `str.find` is
  therefore kept in the catalogue for Python's signature and not offered, which a parameter's `offered` says, and
  the editor does not list it; that of `str.split` is offered, for a separator that is a column.
- The flag `literal`, where Polars has one (`str.contains`, `str.count_matches`, `str.find`,
  `str.replace` and `str.replace_all`), makes the pattern plain text. The catalogue names it as the
  pattern parameter's plain-text flag, the build reads the pattern as a regular expression only
  where the flag is not true, and the walk tells `offer` a pattern only where it is one. A pattern
  that no flag makes plain text, as that of `str.extract`, is always a regular expression.
- The kind is read from a typed text as the text typed, with no dtype. Whether it is a regular
  expression depends on the flag, which the step knows and the slot does not, so the step says
  it, with its reason, where it is built.
- `str.split` takes its separator as an expression, which may be a column of patterns, so the
  separator is not of the pattern kind. Text typed in as the separator is held to a pattern where
  `literal` is false, as one is; a column of them is Polars' to check when the rows run.

What the methods are, and the families that hold them. Each is Polars' own name with `str.` in front of it.
`str.contains`, a test that gives true or false, stands in Comparison with `str.starts_with`, where a person
who wants a filter looks first. `str.find`, `str.count_matches`, `str.extract`, `str.extract_all` and
`str.extract_groups` are a family of their own, Search, since each takes the pattern and gives what it
found, a place, a count, a piece, the pieces or the groups, and a method is swapped for another of its
family that takes the same arguments, so the pattern stays when what is wanted of it changes. `str.replace`,
`str.replace_all` and `str.escape_regex` give text, so they are in Text, which has fifteen methods, as many as
Trigonometry and Comparison, and fewer than the longest list one family of all the methods of text would be.

What Polars does with each is what the tables say, and some of it is not what a name suggests:

- `str.find` gives the place in bytes from the start, as Polars does, so a character of two bytes before the
  match makes it two further than the characters say, and nothing where there is no match. `str.count_matches` gives a
  `UInt32`, and every method of text refuses what is no text, a `Null` among them, at the stage the tables show.
- `str.extract` takes a group number, `group_index`, whose default is 1, 0 the whole match and a group
  the pattern lacks a null. Python's binding takes it as an unsigned whole number, so a negative one is refused by the build.
- `str.replace` replaces the first `n` matches and `str.replace_all` every one. Where the pattern is a regular
  expression, `$1` and `${name}` in what replaces stand for the text of a group of the match and `$$` for a
  dollar sign, which the hints of both say; given as plain text, by `literal`, the text means what it says.
  `n` is a whole number; below zero Polars replaces them all. Polars refuses a count above one of a pattern
  that has an ASCII punctuation mark, which it takes for a regular expression (one with none is taken as plain
  text, as `literal` makes it), or of a value that is many values, and the tables measured the first match, so a
  limit holds it.
- What replaces is an expression, and Polars meets a text of one value that is an aggregate with each of the
  many values of it in a select, as it does the operand of any other method of text, but not in the order key of
  a window or a key inside it, where its streaming engine refuses the lengths at every thread count, which a limit
  holds. A literal text beside many is refused by Polars for a table with no rows, and for the rows where a piece of them
  has more rows than the literal's one (see the limit below), which the tables count as refused where they
  measure a column beside a literal, so the broadcast test measures the value with an aggregate of the column as the receiver.
  `str.replace_all` refuses a value that is null where Polars reads the value as
  one value, a literal, and in a piece of one row where the streaming engine splits the rows into pieces, which
  depends on how many threads share them and how many rows there are; a limit holds it.
- `str.extract_groups` gives a struct whose fields are named by the groups of the pattern, the name a group has or its number where it has none, each of
  text. What it gives depends on the pattern and not on the dtype it follows alone, so
  the dtype table, measured with one pattern, does not say it: `offer` gives no dtype until it is told the
  pattern, and then the dtype Polars works out from the schema of the call with that pattern, such as
  `Struct(year: String, 2: String)`, so the report, the walk and the offers name the fields. A pattern with no
  groups gives a struct with no fields, which Polars cannot group by: it refuses a test for a value that is
  unique or duplicated of one, though the tables measured the struct on two fields, so a limit holds it, and
  the first and the last of one panic on the streaming engine, which the limit for a panic holds. The fields are
  read with `struct.field`, which is not catalogued yet: the walk knows what the call gives, and a chain that
  goes on from it is judged as one that follows a struct of two fields.
- `str.escape_regex` puts a backslash before each character that has a meaning in a pattern. Python's
  `polars.escape_regex` gives a Python string, and not an expression, so it is no start the catalogue has, and
  is left out as never offered: `literal` and the method give a step what it needs.
- Left for later: `str.contains_any`, `str.replace_many`, `str.extract_many` and `str.find_many`, which take a
  list of literal patterns that Polars matches all at once, wait for a kind that holds a list of texts, typed or
  connected; `str.split_exact` and `str.splitn` give a struct that waits for `struct.field`.

The catalogue holds the twenty steps, the expression methods the storyboard
uses, the aggregates `sum`, `min`, `max` and `len`, the six comparisons and the six
others that compare, the arithmetic of `add`, `sub`, `mul`, `truediv`, `floordiv`, `mod`,
`pow`, `neg`, `abs`, `sign` and `clip`, the maths, trigonometry and rounding above, the nulls,
checks and conversions above, the methods of text above, the logic of `and_`, `or_`, `xor` and `not_`, `col`, `lit`,
`element`, and the links of `when`, `then` and `otherwise`.

The host serves the catalogue with its English words to the view through the
`operation_catalogue` command. The step editor takes its method lists,
families, hints, step heads and the words `when`, `then` and `otherwise` from
it, and draws every parameter by its kind. A method's first expression
parameter is written inline. Any other parameter of a method or step is an
option: the list its name opens offers the rest by name. The editor writes
every option that is set into the document; the engine decides which are at
their default. A parameter that a step must give and Polars gives no default, beyond that first one, is not
left to be found in that list: the engine names it as a slot the method starts with (`slots` in the
offers, from `slots_at_start`), and the editor starts the method with an option for each, set to what is
still to be given. They are the pattern of the methods that take one, what `str.replace` puts in place of
a match, the dtype a `cast` gives and `is_between`'s upper bound. A slot that takes an expression is a hole
until it is picked, so the engine walks the step up to it; one that takes a literal has nothing to stand for
it in the document, so the step waits, with the reason, until it is given. A parameter that has a default, or that
Python leaves to `None`, stays an option to add. A step's own parameters are drawn by kind too: sort's keys and
their per-item `descending`, select's expressions, join's table, keys and
`how`, a rename's pairs, and a pivot's aggregate, which is a chain from
`pl.element()`. A new
column's name and chain and a filter's conditions keep the lines the
storyboard settled, which lay out an expression list one item to a line. A
pivot takes its output columns from a connected table, its `on_columns`.

The host holds the node's step document and its revision, and the editor draws
it and changes it only by sending the engine's edits through the host, in
[ui/packages/steps/src/](../ui/packages/steps/src/editor.tsx). A window asks for
the document, which comes with the report of running it, and says what the
person did as an edit made against the revision it drew. One command takes every
edit, tagged by its kind, so that the check of the revision, the run of the edited
document and the answer are written once and not for each of the engine's
nineteen edits. The host refuses an edit made against another revision, so a place
taken from an earlier document never lands on a step that has since taken its
identifier; the window then asks for the document again and the person does it
again against what is there. An edit the engine refuses is an answer, not a
failure: the place, a kind of problem from a closed list the editor can decide on,
and the engine's words. The editor never builds or changes a document, and keeps no
copy of the rules for one; a new step, link, option or test starts as the engine
starts it. What the engine says the document is arrives as a tree of calls whose
numbers are the words the engine shows them as, which `ui/packages/host` decodes
and refuses where it has the wrong shape.

An edit is made against the revision of the document the person saw when they did it,
not the newest one the window holds, because the places in it are built from what was
drawn: against a newer document they would land on a step the person never saw, and the
host answers it as stale. An action that is several edits, such as a join's table and then
its right keys, is one action: its edits are sent in order, each against the revision the
one before gave, and it stops at the first that is refused or stale, so no later edit goes
against a document it was not made for. The queue, the revisions and the loading of the
document are plain TypeScript in
[ui/packages/steps/src/editSession.ts](../ui/packages/steps/src/editSession.ts), tested
with a stand-in for the host. The document the host gives on being asked is the window's
whatever its revision, since a host that has been restarted is at a lower one; only an
answer that a later request has overtaken is dropped. Another window learns of an edit
from a message on the `document` topic that names the revision, and asks for the document
again; which step is chosen travels on a topic of its own. What is not yet an edit stays in
the window as a draft: an option, an item of a list or an entry of a pair that has no value
is not in the document until it has one, and the document never holds a part that is only
half made. A draft belongs to the parameter it would give a value to, and stays through
changes elsewhere in the document until that step or call is gone, the document gives the
parameter, or the person gives it its value.

A revision belongs to one lifetime of a host. It goes up by one with each edit, and a
host starts at the milliseconds since the Unix epoch when it started, so a page that
outlives its host, as one open across a restart does, holds a revision a later host is not
at as long as edits are slower than a millisecond and the clock does not step back, and its
next edit is stale, not made on a document it never saw. The window asks for the
document when it gains focus or becomes visible, and after an action fails, so it draws the
new host's document before the person acts, and takes it whatever its revision is.

The functions that remain in
[ui/packages/steps/src/model/](../ui/packages/steps/src/model/doc.ts) read the
document's shapes, such as which step is a sort, which expression is a test or a
`when`, and prepare the report for display; they are plain TypeScript with no React,
tested against the snapshots the host writes.
What the editor shows of a run it reads off the report and works out none of it:
the columns a step added or replaced, the rows a filter keeps of those it was
given, the first values of a column a step added, and for each connected input
what supplies it and the value or values it holds. The view only lays the
engine's text out as a line, with a dash for a null and an ellipsis where more
values follow.

Where an expression is still to be picked, a value, the field or input a chain
starts from, a method's argument or a comparison's field, the document holds a
hole, which it reads as an expression that starts from nothing. The engine walks
the step up to the hole, so it reports the places before it, and does not build
it: the step is incomplete at the hole's place, and the steps after it are not
run, naming it. The steps after an incomplete step have no walk, so their chains
have no lists of methods until the step before is finished. A new add-column step
holds a hole, so its chain starts from nothing and has no methods to list until a
field is picked. What the engine cannot hold yet is not offered: a comparison with
`is_in`, a condition of which any line is enough, nested groups of conditions, a
literal `None` and a `when` of more than one case have no place in the editor
until the catalogue has them.

The report is the host's of the very document, so a place in it is a place in the
document the editor draws, and a report is never shown for a document that is not
the one drawn. A window asks for the rows after a step with the revision it drew,
and the answer to a request that a newer document has replaced is dropped. A
request that failed because the host was not there, as a bridge that is not yet
running, is asked again after two seconds while it is still the newest; an answer
of the engine's, such as a refusal, is the same each time and is not. A refusal
carries the place it is at, which the editor maps back to the word it marks.

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
- `step_starts` says which steps of the list can be added to a table: the
  list of steps asks it while it is open, so a step cannot be picked until the
  engine has said. What a new step holds, a filter's comparison for equality of
  a field and a value still to be given among it, is the engine's to say when the
  step is added as an edit.

What a parameter holds when it is not given, what it is first set to and
whether an option is at its default are rules about Polars, so the engine
decides them and the editor shows what it says. The catalogue carries each
parameter's default, and a step report names the
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

### The Time Zone Kind

`dt.replace_time_zone` and `dt.convert_time_zone` take a time zone, which is a parameter of a kind
of its own: a name of the time zone database, such as `Europe/Berlin` or `UTC`, or a fixed offset
such as `+01:00`, which Polars reads as the zone of the database that has it. It is typed in, and
the engine does not know the database: it asks Polars, which reads a zone before it builds a
method of a moment, and `check_time_zone` gives the zone or Polars' reason, up to the first blank
line, where the text is none: the name, how to look the names up, and nothing of the hint at what was
meant, which is not about the text.

- A text that is no time zone is refused where the step is built, at the parameter, with Polars'
  reason, and where the walk asks, by the named rule `time-zone-is-valid`, whatever follows it.
- Python's `replace_time_zone` takes `None` for the zone, which takes the zone off a moment and keeps
  its clock. The parameter says so, `takes_no_time_zone`, and the document writes it as a time
  zone that is null, which the editor draws as the word `None` below the typed zones. `convert_time_zone`
  takes a zone only, and a document that gives it none is refused at the parameter. Polars reads the
  empty text as no zone, so `replace_time_zone` given it takes the zone off, and `convert_time_zone` given it
  gives the zone UTC, which the walk says.
- A moment is held in a zone or in none, so what the method gives depends on the zone it is given:
  a moment in the zone, and in none where it is given none. The tables, which were measured with the
  zone `UTC`, say only that a moment is given, so the walk says the rest: the dtype of the call is
  not known until the zone is, for a method that takes none, and is then a `Datetime` with the
  zone as Polars reads it, or without one.
- Polars finds no dtype that a moment in one zone and a moment in another have in common, so it refuses to
  compare them (`eq`, `ne`, `lt`, `le`, `gt`, `ge`, `eq_missing`, `ne_missing`), to subtract them, to fill one
  with the other (`fill_null`), to bound one by the other (`is_between`), to give both as the values of a
  condition, `then` and `otherwise` and each `then` of a chain of them, and to give a replacement new values
  and a default of two zones. A shift filled with a moment of another zone it cannot place either: Polars
  accepts it eagerly and, on the streaming engine, panics when the rows run for a column of moments, and shifts a moment of one value, which
  the fuzz target excuses by a [limit](#what-the-tables-are-checked-against). It refuses a moment in a zone and one in none too, which
  the tables, measured on `UTC` and on no zone, say for the methods they measure. They measure no
  condition, so the values of a condition with one of each are not refused yet (#121). The tables cannot say the rest, so the walk follows the
  zone of a moment along its chain, from the column or the literal to each call that gives one or keeps it,
  and of each value of a condition, and refuses an operand that is in another zone than what it is met with,
  by the named rule `moments-share-a-zone`, with the two zones. What an operand is met with is the value
  the method follows for a comparison, a difference, a fill and the bounds of an interval, and the new values
  for the default of a replacement, whose own receiver is no matter: its zone is the new values'. `clip`,
  `replace`, `is_in` and a fill by a strategy are not refused it: Polars keeps the zone of what they follow.
- `base_utc_offset` and `dst_offset` read the offsets of the zone a moment has, so Polars refuses
  a moment that has none, when the rows run.
- `ambiguous` of `replace_time_zone` says which of the two times a clock that is read twice stands
  for, where the clocks go back, or that it is refused or none, and `non_existent` what a clock that
  is never read, where they go forward, gives. Both are choices of Polars' words. The tables
  measured them with a zone that has neither gap nor repeat, so what they say of the words is that
  Polars takes them; a step that meets a clock Polars refuses is refused when the rows run.

### The Parts Of A Moment

The `dt` namespace is a family of its own, Temporal, under Polars' own names, which the tables say
what each takes and gives.

- **Taking a moment apart.** `year`, `iso_year`, `quarter`, `month`, `week`, `weekday`, `day`,
  `ordinal_day`, `century`, `millennium`, `days_in_month`, `is_leap_year` and `month_end` take a date or
  a moment, and `hour`, `minute`, `second`, `millisecond`, `microsecond` and `nanosecond` a moment or a
  time of day. `weekday` is Monday 1, `week` the ISO week, and `second` has the fraction of the second
  where `fractional` is set, as Python writes it, which is the nanoseconds over a billion added to the
  second.
- **Counting a length of time.** `total_days`, `total_hours`, `total_minutes`, `total_seconds`,
  `total_milliseconds`, `total_microseconds` and `total_nanoseconds` take a duration and give whole
  numbers, or with a fraction where `fractional` is set.
- **Counting a moment.** `timestamp` gives the count since the start of 1970 in a unit and `epoch` also
  in seconds and in days, which Python works out of the others, so the catalogue does. Polars counts a
  number or a `Boolean` as the moment it stands for, so the tables allow `timestamp` of them; a time of
  day, a duration and text it refuses. `cast_time_unit` holds a moment or a duration in another unit.
- **Combining, cutting and moving.** `combine` puts a time of day on a date or a moment, `truncate`
  and `round` cut and round to a period, `offset_by` moves, and `replace` sets the parts it is given
  and keeps the rest. A part not given is left as it was, so the method starts with all of them left
  out. Polars meets a value of one row, a literal, with each of the many values of a part where it is
  selected, but not where a window follows: the window maps what it is given to as many values as it has
  rows, and the value a part widened is refused for the length of its output. The catalogue says so of each
  part, `broadcasts_in_a_window`, and the walk follows whether the value a chain has was widened by one,
  through the calls that keep it many, and refuses a window that is given it, by the named rule
  `window-input-is-not-widened`.
- **Writing.** `strftime` and `to_string` write a moment as text by a format, or, for `to_string` given
  none, by ISO, which separates the date and the time of day by a space and by `T` where the format is
  `iso:strict`. A format is text typed in, and Polars says when it can write it: a year of a time of
  day, an hour of a date, a field that does not exist and anything but `iso` and `polars` for a
  duration are refused when the rows run, and the tables, which measured `iso`, do not say so, so a
  limit holds it. `strftime` takes a format, which Python's `to_string` takes optionally.

### Making A Moment

`polars.date`, `time`, `datetime`, `duration` and `from_epoch` start a chain of a moment or a length of
time that they make of parts or of a count, under Polars' own names, in the Start family beside `col`
and `lit`. Python builds each of them out of other calls, and the catalogue builds them as it does:

- `date` and `time` make a moment of their parts and keep its date or its time of day, named `date` and
  `time`; a part not given is the number 0, and a time is of the first of January 1970.
- `datetime` makes a moment in the `time_unit` and the `time_zone` it is given, none by default,
  and `ambiguous` says which of the two times of a clock read twice it stands for.
- `duration` makes a length of time of weeks, days, hours and the rest, in microseconds, or in
  nanoseconds where `nanoseconds` is given and no `time_unit` is, as Python infers it.
- `from_epoch` reads a count since the start of 1970 as a date for `d`, as a moment in microseconds for
  seconds and milliseconds, which it scales by an `Int64`, and as a moment in the unit for the others.

The tables hold them with no receiver, as [above](#which-combinations-polars-allows) says. The
walk judges each argument by its own line, and a start gives one value where each argument is one
value and a value for each row where one is not, so a date made of numbers typed in is one value that a
select puts beside a column, and a date of a column has a date for each row. A `from_epoch` reads
what it is given by the `time_unit`, so its options are measured on each dtype of its column: a count of
days reads a date that a count of seconds, which multiplies it, refuses. What Polars says of a part that is
no part of a calendar, a month of 13 or a day of 0, it says when the rows run, so the tables, which
measured the values of one column, do not say it, and a limit holds it; a Boolean is a part, for
true is one, and a row of false is the rows' to say. Polars names a moment and a length of time by the
first of their parts, so they are named so for the keys of a join, and not known where every part is a
number typed in, which Polars folds into a literal where the values fit.

### The Duration String

The methods of the `dt` namespace that move, cut and round a moment by a length of time, `dt.offset_by`,
`dt.truncate` and `dt.round`, take it as an expression that holds a duration string such as `1mo`,
`2d12h` or `-3h`, which may be a column of them, so it is not a parameter of a kind of its own. The
catalogue names the parameter as one that holds a duration string, and the method that reads it: the
move, the cut or the round.

Where the expression is text typed in and nothing more, the engine asks Polars whether it reads it,
and does not parse one itself. `check_duration` runs the method on one moment with the text and
gives Polars' reason where it refuses, in Polars' words, up to the first blank line: a text with no
amount or no unit, a unit that does not exist, a second sign, or, of the cut and the round, a length
below zero. Polars refuses some of them only for the dtype of what it is given to, so the moment is
of the dtype that the value before the method has: a `Date` cannot be cut by a length of days and
hours together, nor by a length of nothing, where a `Datetime` can, and a length of a nanosecond is
none to a moment counted in microseconds, which Polars' rounding divides by and panics, in the words it
panics with. The build works the dtype out from the schema, unit and zone included, where it traces the
dtypes, and asks on a `Datetime` counted in nanoseconds otherwise; the walk follows the unit of a moment
along its chain, from the column or the literal to each call that keeps it or gives it, so that what
the tables, which were measured on microseconds, name `Datetime(us)` is named by the unit the chain has
after a method of the `dt` namespace that keeps it. A unit that no such method gives is not followed
past the method that does not keep it, and a method outside the namespace is named as the tables name
it. An amount whose units add up to more than an `i64` of
nanoseconds is refused before Polars is asked, in every build, as [a typed duration is](#how-a-typed-text-is-read).

- A duration typed in that Polars refuses is refused where the step is built, at the parameter that
  holds it, and where the walk asks, by the named rule `duration-is-valid`, whatever follows it.
- A duration that is a column is Polars' to read, row by row, when the rows run, and the tables
  measured the text of the sample's rows, which is `1d` and `2d`. The target gives a duration that is
  a column only that column, for the text of the others is no duration, which is the person's to
  give and not a mistake of the tables.
- The parameter of the move is `by`; that of the cut and the round is `every`.

### How A Typed Text Is Read

The view sends the text it was given with what it is typed into, which is the
kind of the parameter as the catalogue writes it (an expression, a literal of a
type, a duration, a pattern or a path) and, for an expression, the slot it stands in. It
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
  tables allow there and Polars reads when it runs rows, and which the step holds to a
  duration Polars reads, [as below](#the-duration-string); only a parameter that
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
- **A pattern** is read as typed, with no dtype, whatever it holds: whether it is a regular expression
  depends on a flag the slot does not know, and a pattern half typed is the person's to finish. The step says,
  with Polars' reason and at the pattern, where one is none.
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

The engine also makes every edit of a document. An edit takes a document and
gives the edited document or an `EditError` that says why not, so a host or a view
sends what the user did and does not build or change the JSON itself. Each edit is
made on the model: the calls, expressions and steps on the way to what is changed
are made again with their constructors, so the model checks an edit as it is made
and the document an edit gives reads back as the same document. An edit names where
it is made as offers and reports name places, a step by its number and its
identifier, then the way through the step's arguments. The check of the identifier
refuses a place whose step has changed, but not one whose identifier a later step reused
or one made with none, so a host also keeps a revision of the document with each edit
and refuses an edit made against an older one. One action of a view can be several
edits one after another, such as a join's table and then its right keys. What the engine decides a new part starts
with, a step, a link of an expression, an option or a test of a condition, comes
from the catalogue's starts and the tables of allowed combinations, so no host or
view repeats it. An edit may leave a step incomplete, with a hole, or refused, with
an argument left out that Polars needs; running the document says so, as it does for
a document written by hand. The edits are listed in the documentation of
`Document` in the engine.

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
and keeps what it found as four text artifacts in
[api/catalogue/](../api/catalogue/dtypes.txt), one combination to a line,
sorted, so that a Polars upgrade shows as a readable diff. A combination counts
as allowed only when Polars resolved its schema, ran it on three rows that
include a null, and ran it on a table with no rows. Where Polars refuses, the
line records where it did and its own message, cut at the first blank line, with
no plan, address or timing in it. The stage is `schema`, `rows` (the schema
resolved and the rows were refused: a mistake only running shows) or `empty`.
An allowed line records the dtype of the result and whether it is `one` value
for the three rows, `many` or `counted`.

| Table | One line for |
| --- | --- |
| `counts.txt` | A chain that ends in each context and gives one value, as many values as the table has rows, or another number of them, the first two of the three rows: [a count of values](#a-count-of-values). One line for each context and each of the three. |
| `dtypes.txt` | A method of an expression on a receiver of each of the representative dtypes, and for each of its expression parameters, with an argument of each of them. Parameters are varied one at a time, the others at a fitting default. A function that starts an expression and takes expressions has no receiver: its lines write a dash for it, one for each argument of each of its expression parameters. |
| `contexts.txt` | An entry in each context: a position that holds an expression, or such a position inside an expression that stands in one. Two lines for each, one for the chain going on from the entry and one for the chain ending at it. |
| `options.txt` | A method that has options, on a receiver of each of the representative dtypes, or a step, or a function that starts an expression, on each dtype of its first expression, with each pair of values of its options, the flags, the words of a choice and the dtype a call gives; with one value of its option when it has only one. A call that needs a dtype, as a cast needs the one it gives, has a line for each dtype alone and with each value of another option that is not its default. |

The representative dtypes are every integer and float width, `String`,
`Boolean`, `Date`, `Time`, `Datetime` with and without a zone, `Duration`,
`Null`, `Decimal` and `Struct`, and a `List` and an `Array` of each of those that is not a
`Struct`: 22 scalar dtypes, so 67 classes. A list or an array of a list, an array or a
struct is no class, and the walk says nothing of what follows one, as it says nothing of a dtype
the tables do not cover. A dtype of a class is any member of it: an array of any width
is its class, and the tables measure arrays at width 2, which the walk carries as it carries the
unit and the zone of a moment. [Lists and arrays](#lists-and-arrays) says which of the
classes each method is measured on.

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

A method that takes an option in place of a parameter, as `fill_null` takes a
`strategy` in place of a `value`, has a line of the context table for each word of it as
well as for the method as it is, under its name with the form after it, such as
`Expr.fill_null[strategy=forward]`: Polars calls a fill by `zero` or by `one` elementwise, as a
join's key must be, and a fill by any other word not, so one line would say a wrong thing of
one of them. `is_in` has a form of its own, `Expr.is_in[other=list]`, measured with a list column as the values it tests
membership in: Polars meets a list row by row, so a join may key on it, where the same call given a column of its own dtype,
which is a set met whole, is no key. `drop_nulls` and `drop_nans` are measured on a column that holds no null: how many
values a column has once the nulls are dropped is for the rows to say, and a column that holds
one has fewer than the table has rows, which Polars refuses to meet them.

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
| `element-dtype-is-measured` | A method whose answer depends on the element dtype of a list or an array, which `SENSITIVE` names, is given one the tables measure it on: whole numbers, floats, text, booleans, dates and datetimes. For another list or array Polars is not asked, since it panics on some, as on `diff` of an array of `Null` and on arithmetic of a list of `Null` with a null. A document that holds such a call is refused before it runs. |
| `element-only-in-pivot-aggregate` | `polars.element` stands only in a pivot's aggregate. Polars refuses it elsewhere, but as a panic when the table is sorted. |
| `join-key-names-are-not-repeated` | A join pairs no two output column names twice. Polars names a key by the column its chain starts from, whatever methods follow it, `literal` for a literal and by its first value for a condition, and refuses a join in which the key of one side and the key it is paired with repeat the names of an earlier pair: `left_on=[k, k.sum()]` with `right_on=[rk, rk.sum()]` is refused, and with `right_on=[rk, ra]` is not, for the pairs differ. A name repeated on one side only is allowed. `on` pairs each key with itself. An alias is refused in a join's key by Polars, and the catalogue holds none. The rule is asked by the walk, which finds the names of the whole join and places the refusal at the later left key that makes the pair again. Polars pairs the keys after expanding each side, so a key that may expand to several columns (a `col` of several names or a pattern), a key that starts from an input or is not picked yet (a hole), and an item that is no expression shift every pair after them. The rule judges only the pairs before the first such key on either side, and none after it. |
| `key-is-elementwise` | A join's own key holds no aggregate beside a column, no method that takes some of the values, as `head` and `filter` do, unless it goes on to an aggregate, which makes the key one value; such a method is not elementwise even of a literal, so a key that holds one of a literal is refused, and no column compared with a null literal, `is_between` being two comparisons of its value with each bound, so a bound that is a null literal with no dtype is one and a null it follows, or a bound of a dtype, is not. Polars refuses such a key as not elementwise: it folds a comparison with a null literal into a null, at any depth, in an operand, before a method or in a condition's predicate, and a key whose column has gone is not elementwise. A key holds no replacement whose old or new values read a column, which Polars calls elementwise only where they are literals, except where a fold drops it as it drops an aggregate: `a.replace_strict(old=b, new=1).sum() < null` in a condition's predicate is taken, for Polars never evaluates the replacement, and the same replacement given a number to compare with is refused. A subtraction with a null does not drop an aggregate it follows or is given: `k != f.len() - null` is refused. Nor does a fold drop a side that is more than one value or literals alone: `(k - a.sum()) > (b.first() == null)` is refused, for Polars evaluates the side that holds a column beside the aggregate. It allows a key that is wholly an aggregate, a null alone, a comparison of literals, a null beside a column that is not compared, an aggregate compared with a null, a column compared with a null inside the one value that a comparison of a null is given (`null != (f != null).min()` is a null whatever it is given, and `null != f` leaves a null where a column was), a column compared with a null that a later comparison with a null drops with the one value it follows (`(null > h).last() >= null` drops `null > h`, which `(null > h) >= null` does not), and in the key of a window in a select, a filter or a sort an aggregate beside a column. `and_` and `or_` given no operand are the expression they follow, so a null followed by one is still the null a comparison is given, and a comparison that follows it is the first Polars meets. A comparison of missing values, `eq_missing` or `ne_missing`, with a whole number that some dtype cannot hold, below zero or above 127, is one Polars decides at once where the dtype of what it compares cannot hold the number, as an unsigned count cannot hold one below zero, so what it follows is a literal and an aggregate in it is not met: `len() ne_missing -1` less a column is taken, and a number from zero to 127, or any other comparison, is not decided, so the key is refused; which dtype it is the document does not decide, so a limit covers it. A column of the dtype Null holds a null in every row, so comparing it with a null or a column leaves no value of it to replace, though it is a column to an aggregate beside it. The rule holds what the evidence settles. What Polars does with a comparison with what an earlier one folded depends on dtypes that the document does not decide, so a limit covers it, and so does an interval with a null bound of a dtype or a whole number below zero as a bound. The context table measured an entry as the whole key, and what a key holds is a matter of the whole expression, so the caller that walks it asks `rule_forbids_a_key_known_not_elementwise`. |
| `folded-function-is-no-join-key` | A moment or a length of time made of numbers typed in is no part of a join's key. Polars makes a literal of a `polars.datetime` of no zone, or a `polars.duration`, whose every part is a number typed in or not given, and names it by an alias, which a join's key may not hold anywhere, in a condition of it as well: it refuses the key with `'alias' is not allowed in a join key`. A column among the parts, or a zone, leaves the call a call. The values may stop the folding, as a month of 13 does, and Polars then refuses the call when the rows run, which both refuse. The walk asks the rule of each such start in a key; `polars.date` and `polars.time` are always named by an alias, so the tables refuse them in a key. |
| `pattern-is-valid` | A pattern is a regular expression Polars can compile, unless a flag such as `literal` makes it plain text. The rule is not a combination of dtypes the tables measure but a value, so it asks Polars: the walk gives `offer` the patterns a call is given, and `offer` refuses one that `check_pattern` refuses, in the rule's words followed by Polars', before the tables are read and whatever the receiver is. Polars refuses the same pattern with the words `regex error` where a method that takes it runs, which the target holds the rule to. |
| `replacement-values-match-in-number` | A replacement takes as many new values as old ones, or one, and a strict one a default of as many values as what it follows, or one. One value given an operand of many values, or put over a window that does not explode, is as many values, for the window gives each row of its group what it gives the group, while a window that explodes gives one row for each group; what a replacement lists is a table it looks values up in, so it gives as many values as what it follows. A condition whose predicates and values are each one value gives one value, whether or not Polars drops its predicates, so what follows it is counted; where only its values are one, the number depends on what Polars drops, and is not counted. Polars refuses `new` of many values beside an `old` of one, and a `default` of many beside a value of one, when the rows run. The walk finds how many values an expression gives from its shape, and does not judge one that is not walked to its end or whose count Polars's plan decides, as a condition with one expression for all its values. |
| `parameter-takes-one-value` | A parameter that Polars reads as one value is given one. `shift`'s count of rows and fill and `diff`'s count refuse the rows, a count of values and the one value that a method took of one value, which Polars cannot tell is one before the rows run, with "must be a scalar value", in a select and in a window; a percentage change, a quantile and `top_k` and `bottom_k` refuse the rows and a count, and take that value. A number typed in and an aggregate are one value for all of them. |
| `values-meet-in-count` | The values a call meets are one value, which is met by each of the others, or as many as one another: the operands of a method, the predicates and values of a condition, and the items of a select. The rows are no count of values that the data or the arguments decide, and two such counts that are fixed and differ are not the same; a step that adds columns meets its items with the rows. Polars refuses them with "zip node received non-equal length inputs" when the rows run, except where the count is by chance the rows. |
| `element-dtype-is-implemented` | A method of the `list` or the `arr` namespace is run on an element dtype that Polars implements it for. Polars panics where the rows run, on a thread of its own that its streaming executor may not recover from, on `arr.min` and `arr.max` of an array of anything but whole numbers and floats, on `reverse` of one, and on `list.mean` of a list of `Float16`. The tables never run such a combination, a call is refused with what the element lacks before the tables are read, and a document that holds one is refused before Polars is asked, by the dtype that the value before the call has, and by the walk wherever the builder cannot see it, as in a pivot's aggregate. A test runs each combination Polars panics on in a process of its own, and holds the rule to those and no other. The `set_*` methods panic on a list of `Null`, and the rule will name them where they are catalogued. |
| `window-input-is-not-counted` | A window takes a value as long as its group, which a count of values that the data or the arguments decide is not, nor is the one value that a method took of one value, as `head(2)` of a sum is. Polars refuses it with "expressions must have matching group lengths", unless the window joins the values of a group into a list or explodes them. |

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
different results the result is not known. A `Struct` is two fields and a `Decimal` of ten digits: the tables say nothing of other sizes
of them, and an array is of two, though the walk writes the width of the one it follows.

Where a method is given an argument, the line of the call that has it says what Polars does of the call, and
that is the refusal told when the argument is refused, before the line of the method with none of its
arguments, which is told when none is given. The two can refuse for different reasons: `dt.replace`
converts a part it is given before it meets the value it follows, so text given text as the month is
refused for the failed conversion, and the call with no part is refused for the text being no moment.

### Lists And Arrays

A list and an array are classes of their own for each scalar element dtype, so a step can follow
a column of lists of dates as it follows a column of dates, and every call carries the element
dtype through: what `first` gives of a list of `Date` is a `Date`. The tables do not hold a line
for each of the 67 classes of every method, which would be 67 receivers by 67 arguments for a
method that takes an operand. They hold lines for the 26 classes of
`Representative::MEASURED`, which are the scalar dtypes, `Struct`, the list of `Int64`, the list of
`String` and the array of `Int64`, and read the others by the class that stands for them:

| A call follows or is given | The tables read | Because |
| --- | --- | --- |
| a list of text | the list of `String` | Polars judges every method that is not named below alike for two element dtypes. |
| any other list | the list of `Int64` | The same. |
| any array | the array of `Int64` | The same. |
| a list or an array, of a method that depends on the element dtype | the line of its own, for the classes it is measured on | Polars finds the supertype of the elements and the operand, compares elements that may have none, or fills, searches, orders, replaces and converts by them: `add`, `sub`, `mul`, `truediv`, `floordiv`, `mod`, `eq`, `ne`, `eq_missing`, `ne_missing`, `and_`, `or_`, `xor`, `fill_null`, `fill_nan`, `is_close`, `is_in`, `diff`, `replace`, `replace_strict`, `search_sorted`, `cast` and `reverse`, which `SENSITIVE` names. |
| any other list or array, of those | none | The call is refused by the rule `element-dtype-is-measured` and a document that holds it is not run: Polars is not asked of what the tables do not measure, and panics on some of it. |

A method that is not named is held to the claim by a test that runs every combination of every
class that is read as another, in Polars, on one thread, in the dtype table and in the option table: it finds the methods Polars judges
differently for the element dtype than for the class that stands for it, and fails unless they
are the ones `SENSITIVE` names, which are measured on the lists and the arrays of a whole number,
a float, text, a `Boolean`, a `Date` and a `Datetime` as well. A result that is the class that was
read is written for the class that was asked: the first of a list of `Float64` is a list of
`Float64` where the table says it is a list of `Int64`.

A method of the `list` or the `arr` namespace is measured on every class, for what it gives is
the element's: the sum of a list of `Int8` is an `Int64`, of a list of `Boolean` a `UInt32`, the
mean of a list of `Date` a `Datetime`, and a list of text has no sum. The catalogue holds the 37 of them
that take no expression argument, as Python Polars 2.1.0 builds them, with the groups `List` and
`Array` for what they accept: `len`, `sum`, `mean`, `median`, `std`, `var`, `min`, `max`, `first`,
`last`, `n_unique`, `arg_max`, `arg_min`, `any`, `all`, `sort` and `unique` of each, `list.reverse`, and
`list.drop_nulls` and `arr.to_list`. `arr.reverse` is left out: Polars panics where an aggregate, a cut or a
window follows it. `first` and `last` are the value at the first and the last place,
which is a null where a list has none, and `n_unique`, `any`, `all`, `reverse` and `unique` are built
as Python builds them, as an evaluation over the elements of one list. A call that the walk refuses for what the tables recorded of
a method of lists or arrays or of a method that follows one, that its schema and its rows differ in dtype or that
Polars panicked, is not run either, for an aggregate or a cut that follows it trusts the schema and panics. A
method gives a value for each
list, so a column of lists stays as many values as it had rows, and none of them is an aggregate for the
rules that hold a join's key. An array that a call gives is written at the width the array it follows
has: `sort` of an array of three is an array of three, which the tables, measured at two, write only as
the class.

A window that joins the values of a group into a list gives a list of what it follows for every scalar
element dtype, and a method that follows a list of one dtype gives lists of that dtype, so what comes of
a column of lists is carried through each call, in each place that holds an expression.

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
each call, the whole numbers that a rounding keeps, and one bound or both for a `clip`; or one of the
[steps that cut and number a table](#the-steps-that-cut-and-number-a-table), which holds no chain and is
drawn with the arguments Polars takes of it (counts and offsets, names of the columns of the sample table, a
name the table lacks only where `drop` is not strict, the value `fill_nan` fills with), whatever the dtypes of
`a` and `b` are. For each it takes the engine's
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
document's own text, so a kept case replays exactly. A campaign draws its methods
evenly, or focused on chosen methods so that every case holds one of them, which keeps
a batch of new methods checked against the rest as deeply however many the catalogue
holds. The [how-to](how-to.md#check-the-tables-against-polars) says how to run it for
longer, to focus it and to replay a case.

Each case of a campaign is timed, in the campaign's own process, because a case that Polars
never returns from cannot be ended from inside: nothing unwinds a thread that waits for
ever. A panic of one of Polars' own tasks inside a streaming query leaves its executor
broken for every query after it, which is how the cases after the panic stall (#102), and
the `catch_unwind` of the target does not see a panic on a thread of Polars', so a hook notes
the case in which any thread panicked. A timer thread looks at the case in progress and, when
it is late, keeps it, keeps the last case that a panic was seen in as its likely cause, says
what the campaign found, and ends the process with a status of its own, which no case ends
with. The campaign then goes on from the case after it in a new process: the cases are those
of the seed, so a run that starts there makes the ones an uninterrupted run would, and the
process the panic broke is not the one they run in. A campaign that is run in a loop
therefore always ends: each case has its deadline, and a campaign can end early at most once
for each case.

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
  and the tables both gave, or that Polars gave an argument where the tables made no claim of
  it, as of a condition with no `otherwise`, so that nothing an argument holds, as a comparison the tables
  allow without knowing the dtype of a number, is refused before the call is. Where an argument
  has no dtype that Polars gave, as a part Polars refuses first, no column can stand for it, and
  the call is asked a last time with each such argument left out, at its default, where it has
  one. A pivot's aggregate
  reads no column to stand in, but its `element` holds the values of the column `a` that the
  pivot takes the values of, so the call stands on `element` in the same pivot where the dtype
  the tables say reaches it is the dtype of `a`; Polars is not asked of the dtype there, for what
  it says of a prefix is said in a select, where `element` stands nowhere, and a literal in the
  call stays as it is;
- for a refusal that a join's key is not elementwise, the part that holds the refused
  call as the whole key of the same join, the only one where the join has several, for the refusal is of the position and
  Polars skips it where the key is folded away; and, where that part is one value, as
  an aggregate and a running value of it are, which Polars allows as a key whatever it
  holds, the refused call on a column as the key;
- for a refusal of a function that starts a chain and takes expressions, such as one that
  makes a moment, the start again with each literal it is given a column of its dtype, which
  Polars folds where the call is asked alone, and then with every argument a column of the dtype
  that reaches it, which a condition that no row takes, or a part that a length of the call hides,
  is not. It counts when Polars refuses the call so for the tables' reason, at the call. A
  refusal by a rule, one of a link, and one in a pivot's aggregate, which reads no column to stand in,
  are settled by no such probe; and the rule that moments be in one time zone has its own probe,
  the chain through the call alone, which counts when Polars refuses it for the dtype that two zones
  have none of;
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
- for a refusal by the rule that a condition's predicate be a `Boolean`, where no column of the
  dtype can be asked, as the dtype is not one Polars gives (it refuses the predicate on its own
  for another call it holds first) or the predicate is in a pivot's aggregate, which reads no
  column: Polars does not evaluate a predicate when a call after the condition makes it one value
  that Polars drops, as a sort key of one value, or folds, as `or_(true)` does what it follows.
  The condition that holds the predicate, standing alone in the place of the expression that
  holds it (the aggregate of the same pivot, a `select` otherwise), with what follows it removed
  and the last value of its `otherwise` made a fill forward of what it was so that Polars does not
  drop its predicates either. It counts when Polars refuses it in the predicate, or in the rows
  that evaluate it, and allows the same condition with the predicate made `false` every time it
  runs, which drops the branch the predicate guards and leaves the other predicates evaluated, as a
  `true` would not where Polars takes its branch and never evaluates those after it. The same
  condition with it made `true`, which keeps the value the branch guards, is allowed too, so the
  value is no cause, and it shows that it is the predicate Polars refuses once nothing hides it.
  The refusal may be in the rule's words or in other words, for what the predicate holds is met
  before its dtype is judged, and the outcome says which; both are a refusal of the predicate where
  it is evaluated, which the editor keeps refusing where Polars does not evaluate it, as it holds
  every part to its types;
- for a refusal of Polars' own words at a call inside a predicate of a condition that has an
  `otherwise`, which Polars allows: it takes a condition whose values are one expression for
  that expression and drops its predicates, so it never evaluates the part, and the part
  alone may be refused first for an argument it holds. The case itself, with the last value
  of each condition made a fill forward of what it was so that the values differ and the
  predicates are evaluated. It counts when Polars refuses that case for the tables' reason,
  which shows the dropping is why Polars allowed the original; a case with no `otherwise`
  has no value to make differ, and Polars allowing the control or refusing it for another
  reason settles nothing;
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
predicate be a `Boolean` by Polars' words for it, and for the rule that a replacement take as many
values as it is given by its words about the lengths. A call that the tables judge by each of its
values beside a companion, a replacement, an interval or a test of closeness, counts when Polars
refuses the part at the call or in what it is given, whatever it names: the value the tables
refused first is not always the one Polars meets. A refusal that Polars places must
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
settles it. The one exception is a part Polars folds away, which a control run in Polars
settles, as the table says of a part Polars folds to a null. The table below lists the
limits. Each is named in the target, counted when it excuses a
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
wherever it refuses, which is about half of the pivot cases (61 of the 144 of
the fixed seed, and the same share in longer campaigns), so those pivot aggregates are checked
little; the tally counts the pivot cases and those the rule refuses apart.

A refusal of a [count of values](#a-count-of-values) that Polars allows is of two kinds, and neither is
a disagreement. Where Polars drops the part, as a sort key that is one value, the probe asks about the part
alone: the chain through the refused call, or, for a count that may not end a value or a predicate of a
condition, the chain of the call that takes it, is refused by Polars for lengths that do not match, in the
words of either engine, where it stands in a `select`, and the part before it is not. Where Polars evaluates
the part and allows it because its count is the table's rows, or one value, by chance, as `head(10)` of three
rows is, the tables are stricter by design: a count is no number the editor can know. What shows it is the
case run again with the count made another and nothing else changed, which Polars refuses for lengths: on
twelve rows, which are the sample's three four times over; on four rows that tie, the second and the third
twice, where the values that occur most and the values that differ are two, as the sample leaves them one or
all; and with the number that decides each first, last, nth, placed or repeated count made 2 (`head`, `tail`,
`limit`, `top_k`, `bottom_k`, `gather_every`, `slice` and `repeat`), which is neither one value, which Polars
broadcasts, nor the three rows. A part that Polars drops and whose count is by chance is asked
apart from the case, where it is allowed on the sample. A refusal that stays when the count differs is
not this: a count that is the rows whatever they are, as the values that occur most of the places that sort a
column are, which Polars allows with any rows, stays a finding, as does a count met by a part Polars folds
to a null. Only the rules for counts and the table of counts are asked. A control changes every count the case
holds, so it is asked of the item of the step that holds the refused place, and of nothing where that item
holds a count in more than one place, a count in a chain and one in an expression a call of it is given, as
another count's refusal for lengths says nothing of the first; such a case stays a finding. Such a case is kept as `regressions/by-design-<what it shows>.case`, counted as refused
where a count is the rows by chance, and is an expected rejection.

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
part, that a rule stricter by design refused, that were refused where a count is the rows by chance, and that disagreed. The fixed seed's
1000 cases give none disagreeing; 9 of those that agree are refused by the rule against a
repeated pair of key names, and the tables could not be asked past a point in 29 more. These
counts are the same at one thread, at two and at the machine's own, and so are the 42 cases
refused by the rule that is stricter by design, of 144 pivot cases, the 842 that agree, the 87 refused by both for
another reason and the 9 refused where Polars does not evaluate the part; the cases that a limit excused are 6, and 5 at one thread. Another seed's counts may differ between thread counts: Polars' thread count changes which reason it
gives where the tables and Polars both refuse, and a case in which Polars panics at some counts and
not at others moves a case between them. The test
asserts none of those.

The methods of the `list` and the `arr` namespace are drawn only where `DAGEIST_FUZZ_FOCUS` names one of them,
and the column under test is then a list or an array of any element dtype.
The methods that give [a count of values](#a-count-of-values) and the functions that start a chain with one are
drawn only where `DAGEIST_FUZZ_FOCUS` names them, so the fixed seed does not draw them. The gate stays, though the
two kinds that a focused campaign found are now settled: a count that is the table's rows, or one value, by chance
is stricter by design, and a count that Polars never evaluates is asked of the part apart from the case, both as
above, and a `gather` that takes a place beyond the rows is a limit. Drawn in broad campaigns, the methods and the
functions found fifty-four disagreements in two campaigns of 20000 cases, at the machine's own thread count and at
two, which no control settles, so broad campaigns would not pass. They are of these kinds:

- a count that is the rows, or one value, whatever the rows are, and so is no count by chance: the values that
  occur most, the values that differ and their first places, of a column whose values are all different or all
  alike, as the places that sort a column, a running count, a shift or a test for a null leave them;
- counts in more than one place of one expression, or of one condition, which a control changes together, so that
  Polars refusing it for lengths does not say which count was the refused one's (a first value beside a gathered
  quantile, a `top_k` beside a `bottom_k`);
- a count in a pivot's aggregate, whose groups hold one or two values, which no number of rows changes;
- a count met by a part that Polars folds to a null, which the controls for a count do not remove;
- a count that is a value that Polars broadcasts, as the predicate of a condition is, beside values of a count;
- a range that Polars refuses by the values its bounds hold when the rows run, or when the table has none:
  a time or a start out of range, a number of samples that is no whole number, a null given to `int_range`,
  a step of the wrong sign, and a number of repeats typed as text, which Polars converts and the tables refuse.

These are what remains of the fuzz items of #129. The gate is lifted when a campaign without a focus agrees.

| Limit | Excuses |
| --- | --- |
| An `explode` changes the lengths | `Expr.over`'s `mapping_strategy` as `explode` gives other lengths, which meet those of what follows it. The tables do not measure what meets them. Polars refusing the rows, with a reason about lengths, in a case with an `explode` that is why: the case run again with each `explode` undone is not refused for lengths (no place: the reason gates it, and that run shows the cause); and the tables refusing, with a reason about lengths, at the window or at a call after it, or in the arguments of a link after it, in the same chain, which Polars allows; not before the window, not in its own arguments, and not an `explode` in another operand or item. |
| A literal | An argument or a start is measured as a column of the dtype, except the values of `shift` and the arguments of a pivot's group. A literal, or a condition of literals, is one value, which Polars casts and broadcasts as it does not a column. The tables refusing at a call that follows a literal or is given one, which Polars allows (a literal inside what the call is given counts at any depth, as the null of `b / null` in a comparison, which Polars folds into a null), and what a call after a literal is given, at any depth, is met by one value: the tables refusing it when they run rows, which Polars allows. A literal at the place is why only where Polars refuses the case for the tables' reason once each such literal is a column of its dtype (the class Polars gave it, the same case in all else): the case run again. Where the call is a replacement, an interval or a test of closeness, which the tables judge by each of its values beside a companion, Polars names whichever it meets, and a comparison of two series it cannot compare in the words of either of its engines, so the run need only refuse the same call, or what the call is given, which a literal that the call follows makes Polars skip with the call. A literal beside a part Polars skips for another cause is no cause, for the column is allowed as well, and the limit that is the cause, or the probe of the call on columns, excuses the case. A key that the tables refused for a call and not as a position is run again as the only item of a `select`, for a column beside an aggregate is not elementwise and a literal is. None in a pivot's aggregate, which reads no column. |
| A pivot's aggregate | It is run on the values of each group, with kernels of its own, and on no groups when the table is empty. Polars refusing the empty table, with a reason that an operation is not supported or that a dtype is null, in either quoting (no place); and the tables refusing, with a reason about a dtype an operation does not support, a comparison, an operation, an evaluation, a cast, a struct field, a function that makes a length of time expecting numbers, lengths of a window over a group, a replacement pattern per row, duplicates in what a replacement replaces, a fill by a strategy for a dtype, a dtype that two others have none in common with or the plan of the call saying another dtype than its rows give, at a call of the aggregate, which Polars allows; and a failed conversion of the values a replacement lists, whose text reads as the number where the sample's text does not, and of a cast of a literal, or of a value that a function made of literals alone, as a count since 1970 of a null, which a select converts without a failure, or of a comparison or an arithmetic with a null, or of a bitwise call of whole numbers given a null, which converts nothing the sample's rows hold, where a `skew` or a `kurtosis`, which convert what they follow to a float, stands in the place of the cast, and of text that a count or a position becomes (`count`, `len`, `n_unique`, `null_count`, `approx_n_unique`, `arg_sort`, `arg_max`, `arg_min`, `rle_id`, `cum_count`) through a fill with text, or of the numbers of the group cast or filled to text by the first call, which calls that keep each value (a fill, a reverse, a sort, one value picked, a running maximum or minimum) may follow, which read as the numbers, or of the values of a group written by the first call with a format of digits alone, as the year, which read as numbers too, or of a comparison or an arithmetic of a column of the dtype Null, which a cast, a first, a last, a minimum, a maximum, a reverse or a sort may follow, whose values stay null; and Polars refusing the rows, with no place, of an aggregate that holds a quantile, with a reason that an expression of a numeric dtype is expected for it, which is a share written as text that a select reads as the number it writes |
| A single value in a key | A key that is wholly a single value, an aggregate or a null, is not one that has to be elementwise in what it holds: Polars allows a running count under a `last` there. The context table measured the entry as the whole key. The tables refusing, with a reason about a key not being elementwise, inside a key that is wholly single, which Polars allows. A key that holds an aggregate beside a column, or a column compared with a null, is refused by the rule above, and is not this. |
| A null comparison in a key | Polars folds a comparison with a null literal into a null before it asks whether a join's key is elementwise, and what it leaves depends on the dtypes and on what the comparison follows, such as an aggregate. A column compared with a null literal is refused by the rule, and the rest, such as a comparison with what one folded, is found by running. Polars refusing the join as not elementwise, with no place, in a case where a join key, or the key of a window, holds a comparison with a null literal, or an interval with a null literal or a whole number below zero as a bound, or an `eq_missing` or an `ne_missing` with a whole number below zero or above 127, at any depth, and no other key of the join is a cause: a key that is not wholly one value and holds a running value, an aggregate or a window. A key that is wholly one value is allowed beside another key, so it is not. |
| A shift that leaves nulls | The tables ran every call on columns that hold values, so what Polars does with a call that has only nulls to meet was not measured: it does not meet the dtype that the tables refuse when they run rows. A shift fills with a null unless it is given a `fill_value`, and the sample's three rows decide what is left, so the target follows them through the chain. The start column holds a value in each row, and `a` and `b` a null in the middle one; a start that is not one column of the sample holds nothing known, except the null literal that a fill of NaN follows, which keeps its null whatever it is given. A shift moves the rows down for a positive number and up for a negative one, and the rows it empties hold the `fill_value` (a null where there is none). A running sum or maximum and the parts of a date keep each row where it is and null where it was; `is_null`, `is_not_null` and `cum_count` give a value in every row; a comparison and arithmetic give a null in each row where the rows they follow hold one, or an operand does (a column with a null in the middle row, or an aggregate that is null, meets every row), and leave the others unknown; `and` and `or` carry none, for a null with a true is a true; `xor` does carry one, and is left unknown, which excuses less; `offset_by`, a condition and a window that is not an `explode` keep the rows in place and leave what they hold unknown, and give the one value that an aggregate leaves to each of the sample's three rows; any other call leaves nothing known. The first and the last take a row, or the first or last that is not null where they skip nulls; the minimum and the maximum are null of rows that are all null and a value of any that holds one; a sum of rows that are all null is not known, since it is zero of numbers and null of a column of `Null`. The tables refusing, when they run rows, at a call that follows a chain whose rows are all null, or is given one. Polars refuses a comparison of a list with a `Boolean` after none, one or two shifts, and allows it after three. Nulls that Polars folds whatever the rows are, as those of a comparison with a null, are not the sample's: a probe settles them. A literal that holds a value is one value, where the sample's columns hold three, so a shift that moves it away leaves a null, as the one value a moment is counted from does. |
| A join key's dtype | The other table's key is made of the dtype the chain gives on its own, and the join may resolve the chain to another, as the precision of a decimal a null is taken from. Polars refusing the join, with a reason that names a decimal (no place). |
| The lengths of the rows | Whether lists, or other values, of different lengths combine depends on the rows. Polars refusing the rows with a reason about lengths, in a case with no `explode`, or one that is refused for lengths still when each `explode` is undone, as a window that explodes beside a literal over a window that Polars refuses for it, and with no drop of nulls or NaNs, or one that is refused for lengths still when each is made a fill (no place). |
| An overflow | A debug build of Polars panics where a release build wraps. Polars panicking so (no place). |
| A value beyond a decimal | Polars compares a `UInt128` with a `Decimal` by casting the `UInt128` to `decimal[38,0]`, which holds 38 digits, and a `UInt128` holds more. The sample's values are within the decimal, so the tables measured what a value within it meets, and a comparison of a `UInt128` with a `Decimal` is allowed by the dtypes alone; a value beyond it, which a subtraction wraps to below zero, fails the cast. Polars refusing the rows with a failed conversion of `u128` to a decimal, in a case that holds a subtraction (no place: the reason and the subtraction gate it). The table with no rows holds no value to fail, and another conversion is not this. |
| A panic | One is an internal check that failed on the plan Polars made in `select`; another place, or a combination of what was measured singly, may not reach it, or may reach another. Polars panicking where the tables allow (no place), and the tables recording a panic where Polars allows. |
| A window over a window | A window whose receiver holds another window, in its chain before it, or in the arguments of those calls, is run in the aggregation context of the outer one, where a key that is not elementwise is refused; the tables measured one window. Polars refusing the rows with that reason (no place). A window in a key, or two windows apart, is not this. |
| A hidden schema | The tables hold the schema of a method and what its rows give as one thing, and refuse a method whose plan says another dtype than its rows give. That is the target's own comparison, not a refusal of Polars, and it shows only where the method is the output of the step: inside an expression that goes on from it or compares it, Polars evaluates the method and the step's rows do not show the difference. The tables refusing, with the reason that the schema says one dtype and the rows give another, at a call that is not the last of the step's own expression, which Polars allows. A sort key and a join's key are no output of the step either, so the same reason at the last call of one is excused, unless a probe of the call on a column in the key settles it, which says more of it and is asked first. |
| A negative exponent | Polars raises an integer to a power by taking the exponent as a `UInt32`, so it refuses a negative whole number as the exponent of an integer when the rows run. The exponent is a value, which the tables measured as the dtype and the number 1. Polars refusing the rows with a failed conversion to `u32`, in a case with a `pow` whose exponent is a whole number typed below zero or holds a subtraction, a negation, a not or a difference, which can take the values of a column below zero (no place). The sample's rows are 1, a null and 3, so a sum or a product of them, a power of a positive number or of a column is not this. |
| A null in a bitwise call | A value of the dtype Null, a column or a literal (typed as it or not) or what a call gives that dtype, is an operand of `and_`, `or_` and `xor` that the tables allow, giving a `Boolean`, as `select` shows, and Polars meets it where `select` hides it. It negates what a bitwise call combines operand by operand, so a `not_` meets each operand and refuses the dtype Null: Polars refusing the rows with the dtype Null not supported in `not`, in a case with a `not_` after a run of `and_` and `or_`, any call of which meets a null, as the receiver or an operand, by the dtypes the tables give and as a null goes on through the calls after it, which the negation is pushed through (no place); an `xor` or any other call ends the run. It gives a filter's predicate the dtype Null: Polars refusing the rows with a predicate that must be a `Boolean`, got `Null`, in a filter with a predicate that ends in bitwise calls the first of which meets a null (no place). An `and_` with a null on either side gives Null, so does an `or_` of nulls, and the dtype goes on through an `and_`, and an `or_` of another null, that follows; an `or_` or an `xor` of a `Boolean` and a null is a `Boolean` and is not this, nor is a predicate that a call other than a bitwise one ends. |
| A division by zero | Polars refuses to divide a `Decimal` by zero, where a whole number divided by zero is null and a float is infinite. The divisor is a value, which the tables measured as the dtype and the sample's values, none of them zero; a remainder or a difference of them can be, and the sum of no rows is. Polars refusing the rows or the empty table with the reason that it divides a `Decimal` by zero, in a case with a `Decimal` as a column that holds a division, a floor division or a remainder (no place). |
| A bound the dtype cannot hold | Polars casts a bound of `clip` to the dtype it clips as strictly as it can, and refuses the rows when the bound holds a value that dtype cannot hold: a number typed without a dtype that is out of its range or below zero beside an unsigned dtype, or a value of a float that is no whole number it can hold, such as the infinity and the not-a-number that a logarithm to the base 1 or a root of a negative number gives beside whole numbers. The tables measured a bound as a column of the sample's rows, whose numbers every number dtype holds. Polars refusing the rows, with a failed conversion that has no place, in a case whose bound is why: the case run again with the bounds of every `clip` undone is not refused for a failed conversion, which is also why a failed conversion that another call causes, and a clip with no bound, are not excused. The table with no rows holds no value to convert, except for a bound that holds an aggregate, whose value of nothing, as the not-a-number of an entropy, is there. The values a `replace` or a `replace_strict` lists, what it replaces and what replaces it, are cast to the dtype it follows in the same way, and are bounds here: the run again makes each of them a null. |
| A bound with values of its own | Polars casts the values of a bound of `clip` to the dtype it clips, and the count of characters that a method of text takes (`str.head`, `str.tail`, `str.slice`, `str.pad_start`, `str.pad_end` and `str.zfill`) to a whole number, and a bound or a count whose values that dtype holds is cast whatever its dtype is: text that reads as the number, or a list or another dtype that holds nothing but nulls. The tables measured a bound or a count as a column of the sample's rows, whose text is a duration no number reads and whose lists no dtype holds, so they refuse the cast of a dtype that the values of another bound pass. The tables refusing, when they run rows, with a failed conversion or a failed cast of a dtype, at a `clip` that has a bound, or a method of text that has a count, that is not a bare column, which Polars allows, and where the tables, asked again with the computed bounds of every clip undone and the computed counts the whole number 1, no longer refuse the call: the computed bound or count is then why. A bound or a count that is a bare column holds the sample's rows, as the tables measured it, so a refusal that is its own is not this, whatever the other is, and neither is a clip with no bound, a method with no count or a refusal at another call. |
| A window over a literal | A window whose partition holds no column, a literal or a running sum of one, gives Polars' streaming engine no rows, so what follows it in a chain that starts from a literal is never evaluated, and the rows do not show a plan that they contradict. The tables hold the schema of a method and what its rows give as one thing, and refuse a root of a date, whose schema names the date and whose rows are a float, which is the target's own comparison and shows only where rows are given. The tables refusing, when they run rows, with the reason that the schema says one dtype and the rows give another, at a call after such a window in a chain that starts from a literal, which Polars allows; the window is why only where Polars, asked again with each literal partition a column of the table, refuses the case for that reason, which a column start, a window over a column and no window do. None in a pivot's aggregate, which has no column to stand in. The same holds for a window of a value that reads no column whose keys, in its partition or its order, are literals that read no column or one value each, as an aggregate is, such as `lit(None).over(order_by=a.last())`: it gives no rows, so what a replacement is given as its new values has none beside old values of the table's rows, which Polars refuses as the new values being neither as many as the old nor one, with no place, in a case with such a window and the words of that refusal: the case run again with the partition of each such window a column of the table is not refused for them. |
| A drop leaves no rows | `drop_nulls` can leave no value, as it does of a null literal or of a column of the dtype Null, so the rows are none and do not show a plan that they contradict. The tables hold the schema of a method and what its rows give as one thing, and refuse a method whose schema says the dtype Null and whose rows give another, as the cube root of a null does, which is the target's own comparison and shows only where rows are given. The tables refusing, when they run rows, with the reason that the schema says one dtype and the rows give another, at a call after a `drop_nulls` or a `drop_nans` in its chain, or in what the chain's start is given, which Polars allows; the drop is why only where Polars, asked again with each drop a fill of nulls forward, refuses the case for that reason. |
| A cast of a value the dtype cannot hold | A strict `cast`, a strict `replace_strict` asked for a dtype and given no default (given one it converts as a cast that gives a null does), the test of closeness and the fill by a value or of NaN, which convert what they meet to the one dtype that holds both as a strict cast does, the difference, which converts unsigned numbers to the signed dtype of its result (a subtraction of unsigned numbers wraps to a number beyond it), and the count of characters that `str.head`, `str.tail`, `str.slice`, `str.pad_start`, `str.pad_end` and `str.zfill` are given, which they convert to a whole number as one does, signed for where to start and how many to keep and unsigned for how long, refuse a value the dtype cannot hold: a number out of its range, below zero beside an unsigned dtype, the infinity or the not-a-number of a float beside a whole number, text that does not read as the number, or a `UInt128` above the largest `Int128` beside one. The tables measured them on the sample's rows, whose values every number dtype holds, and a table with no rows has the value that an aggregate of nothing is. Polars refusing the rows or the empty table with a failed conversion that has no place, in a case with a strict cast, a test of closeness, a fill by a value or of NaN, a difference (where the failed conversion is of an unsigned number to a signed one), a count of characters, or a strict replacement given a dtype and no default: the case run again with each cast not strict, each test of closeness a comparison of the same two, each fill and each difference a fill forward, each count the whole number 1 and each replacement not asked for a dtype is not refused for a failed conversion. A literal is one value that Polars converts when it works out the schema, so the refusal may be the schema's, which names the scalar the literal is, where a column's is the rows'. |
| A drop changes the lengths | `drop_nulls` and `drop_nans` leave the values that are not null or not NaN, so a column that holds one has fewer values than the table has rows, which Polars refuses to meet the table's rows or another expression's values. The tables measure the two in a context on a column that holds no null, where they drop nothing. Polars refusing the rows with a reason about lengths, or about the shape of the exponent of a power that does not match its base, or about what a replacement replaces by being fewer than what it replaces, or about its default having other lengths than its input, that has no place, in a case with either: the case run again with each made a fill of nulls forward, which keeps every value, is not refused for lengths. |
| A replacement of columns in a window | A window evaluates what it follows once for each group, and what a replacement lists as the values of a column is then a list for each group, which Polars refuses as a replacement pattern per row where what it replaces is one value for each group, as an aggregate is, and as an output of the length of the column where what it replaces is one value, as a literal is. The tables measure a replacement in a context with the receiver's own rows. Polars refusing the rows with a reason that names a pattern per row or an output of another length than its input, in a case that has a window and a replacement that lists the values of a column: the case run again with what each replacement lists made null literals is not refused for it. |
| A drop of NaNs in a window | `drop_nans` is a filter by the test for NaN, which Polars leaves out of a plan for a dtype that is no float, where it has nothing to drop, and keeps where an aggregate of what it drops is evaluated for each group of a window, and refuses there for the dtype it cannot test. The tables measure the call where it is left out. Polars refusing the rows with the refusal that the test for NaN is not supported for a dtype, which has no place, in a case with a `drop_nans` that a window follows in its chain: the case run again with each drop made a fill of nulls forward, and again with each window made one, is not refused for it. |
| A null struct to fill | A fill by a strategy refuses a `Struct` only where it meets a null to fill, and the sample's `Struct` holds none. A shift leaves nulls, and the first, the last, the minimum and the maximum of a table with no rows, or of a group of a pivot that holds nulls alone, are one null, where those of the sample's rows are a value. Polars refusing the rows or the empty table with the refusal that a strategy of a fill is not supported for a struct, which has no place, in a case with a fill by a strategy that follows one of the five: the case run again with each of the five made a fill of nulls forward, which leaves the nulls of the column and no more, is not refused for it. Inside a condition Polars refuses the strategy for a struct where it fills nothing, which it runs in a plain select: the case with a condition and a fill by a strategy, run again with each fill a fill forward, is not refused for it. |
| A strict replacement given no default | A strict replacement that is given no default is held to more than one that is: it refuses a value that it does not list, which depends on the rows, and values of a dtype that cannot become the dtype of the column it follows, which a default lets pass. The tables measured it with a default and with what it replaces the column it follows. Polars refusing the rows with the refusal of an incomplete mapping, of nothing listed to replace, or of values of a type that cannot replace the column's, which has no place, or, of the empty table, in the words of what follows it, for Polars gives back what the replacement follows where it lists nothing, in a case with a `replace_strict` given no default: the case run again with each given a null default is not refused for it. |
| A replacement of every match given a null value | `str.replace_all` refuses a value that is null where Polars reads the value as one value, which it does of a literal and of a piece of one row. Polars' streaming engine runs the rows in pieces, whose length depends on how the rows are shared among the threads, so a column that holds a null is refused where a piece is one row and not where it is longer, though Polars' in-memory engine and the tables allow it. A literal null is refused at every count. Polars refusing the rows or the empty table with the refusal that the value cannot be null, which has no place, in a case with a `str.replace_all`, or a `str.replace` given a count below zero, which replaces every match: the case run again with the value of each made a text literal is not refused for it. |
| A replacement of more than the first match | Polars replaces more than the first match only of plain text and of one value, and refuses a pattern that has an ASCII punctuation mark, which it takes for a regular expression, or a value that is many, with a count above one. The tables measured the first match, which is `n`'s default. Polars refusing the rows or the empty table with the refusal that a replacement of a pattern or of many values with a count above one is not yet supported, which has no place, in a case with a `str.replace` given a count above one: the case run again with each count left out is not refused for it. |
| A literal text beside many replacement values | A literal text is one value, which Polars' streaming engine broadcasts over the rows of a table only where each piece it runs them in is no longer than that one value, and never over a table with no rows, so a replacement of it given many values is refused where the column of text the tables measured it with is not. How long a piece is depends on how the rows are shared among the threads: the rows of the target's three-row tables are one piece of three at one thread, pieces of two and one at two, and pieces of one at three or more, so the rows are refused at the first two and pass at the others, and the empty table is refused at every count. Polars refusing the rows or the empty table with the refusal that the length of the value does not match that of the text, which has no place, in a case with a replacement in a chain that starts from a literal: the case run again with each such literal a column of text is not refused for it. |
| A struct with no fields | A struct with no fields, which `str.extract_groups` gives of a pattern that has no group, cannot be grouped by: Polars refuses a test for a value that is unique or duplicated of it, with the refusal that at least one key is required. The tables measured a struct on two fields. Polars refusing the rows or the empty table with that refusal, which has no place, in a case with an `str.extract_groups` of a pattern with no group: the case run again with each such pattern given a group is not refused for it. |
| A format the dtype cannot write | A format writes the fields of the calendar and of the clock that a dtype has, and Polars refuses one it has no field for: a year for a time of day, an hour for a date, a field that does not exist, and any but `iso` and `polars` for a duration. The tables measured the format `iso`, which every dtype takes. Polars refusing the rows or the empty table with the refusal that it cannot format the dtype with the format, that a duration has no such format or that the format is not a valid one, which has no place, in a case with a method that writes a moment as text, given a format: the case run again with each format made `iso` is not refused for it. |
| A part of a date that depends on the rows | `dt.replace`, and the functions that make a date, a moment, a time or a length of time of parts, `polars.date`, `datetime`, `time` and `duration`, convert each part they are given to a whole number of the width of the part and refuse a value outside the calendar's range, and one that does not convert, when the rows run, so what they do with a part that is no number written in depends on the values of the rows. The tables measured the column that stands for an argument, whose values are 1 and 3, or true and false, of which false is a day of 0 that no date has; for the month and the day of the functions they measure a Boolean column of true, which Polars makes a date of, so that a row of false is for the rows to say. Polars refusing the rows or the empty table with the refusal that the components of a date or a time are invalid, or that a value does not convert, which has no place, in a case with a call given a part: the case run again with each part made the number 1 is not refused for it. The tables refusing the rows for invalid components that Polars allows, in such a case: the tables then no longer refuse it for them with each part made the number 1, and Polars refuses it for them with each part made `false`, which shows that it evaluates the part and does not skip it, as it skips one in a sort key of one value. The tables refusing the rows with a failed conversion of text to the whole number of a part, as of the text of a column of durations, that Polars allows, in such a case given only literals for its parts, one of them text: the tables then no longer refuse it with each part made the number 1, and Polars refuses it with each text part made the literal `x`, which shows that it converts a literal where it makes the plan, so that a literal is converted whether or not the rows evaluate it; a part that is a column is not excused, for Polars converts it only where it evaluates it. |
| A moment beyond the range of a unit | `dt.cast_time_unit` to nanoseconds multiplies the count of a moment, and Polars refuses one outside the years that nanoseconds hold, such as the first of January of the year 1, when the rows run. The tables measured moments of 1970 and after. Polars refusing the rows or the empty table with the refusal that a conversion to a moment counted in nanoseconds failed, which has no place, in a case with a `dt.cast_time_unit` given nanoseconds: the case run again with each unit made milliseconds, which a moment cannot overflow, is not refused for it. |
| A replacement of an aggregate in a window key | Polars' streaming engine does not meet a text of one value, a literal or an aggregate, with each of the many values of a replacement in the order key of a window, or in a key inside it, which it does in a select, where the tables measured the replacement. It refuses the lengths at every thread count. Polars refusing the rows or the empty table with the refusal that the length of the value does not match that of the text, which has no place, in a case with a window and a replacement: the case run again with the value of each replacement made a text literal is not refused for it, and nor is the case run again with each window made a fill. |
| A replacement of a literal aggregate in a branch | Polars' streaming engine refuses a replacement of the aggregate of a literal text, which is one value, where it stands in a branch of a condition, and meets it where it stands in a select, in a sum, in a window or in memory. It reads the pattern of the replacement as many values, and what it says depends on how it splits the rows of the table: that the length of the pattern is dynamic where a piece of them is longer than one row, and that the pattern is null where it is not. The tables measured the replacement in select, on columns. Polars refusing the rows or the empty table with either refusal, which has no place, in a case with a branch of a condition that holds a chain that starts from a literal and replaces after an aggregate or a running value (the sum, the product, the minimum, the maximum or the count so far, which it reads of a literal as it does an aggregate): the case run again with each such literal a column of text is not refused for it. |
| An approximate quantile of a literal in a condition | Polars' streaming engine refuses the approximate quantile of a literal, of a value that literals alone make (a date, a time, a moment or a length of time that reads no column), or of a value it folds into a null literal (a comparison or an arithmetic given the null literal, such as `mean() >= null`), which is one value, where it stands in a value of a branch of a condition, or in the predicate of a `when` after the first, and meets it in a select, a filter, a sort key, a join key, a window, the predicate of the first `when`, and on a column in the condition: it reads the quantile as null. The tables measured the call in a condition on columns. Polars refusing the rows or the empty table with the refusal that the quantile must not contain null values, which has no place, in a case with a condition that holds in a value or in a later predicate a chain that starts from a literal, or from such a function, or has such a fold before it, and takes an approximate quantile: the case run again with each such start a column is allowed, or, for a fold, with every such fold left out, or, where that is refused for what follows the quantile, cut after the quantile or with a column of the dtype of its own literal, chosen for each quantile (a `Boolean` literal's `f`, a text one's `g`, any other's `k`, cast to the dtype the literal is given where no column has it, such as a time of the day, and in a join's key the column `u` or `key`, whichever no other key of the join is named for), it is allowed (a refusal of a call after the quantile, which Polars would give once the quantile is not refused, is then not seen), so a share that is a null, which a column is refused for as well, is excused by nothing. |
| A moment of one value filled in another zone | A shift of a moment that is one value, which a function of literals alone or an aggregate of a column makes, takes the number it is filled with whatever the zone of the fill is. Polars' schema names the zone of the receiver, and so do the rows in memory; the streaming engine gives rows in the zone of the fill for an aggregate and for a moment of literals in `with_columns`, and in the zone of the receiver for a moment of literals in a `select`. The streaming engine panics when the same shift follows a column of moments (in memory it shifts them), which the named rule that moments share a zone refuses for every receiver. The rule is the tables' own, so the disagreement is of a rule that refuses what Polars allows: the rule `moments-share-a-zone` refusing a shift that is filled, whose receiver gives one value (a literal moment, or an aggregate of a column such as its `max` or `first`), is excused, and a shift of a column or of what keeps a column's many values is not. |
| A part Polars folds to a null | Polars folds a call that carries nulls, given the null literal as an operand, or as the receiver of the first call of a chain, into a null before it runs what the call followed, so `h + null` is a null whatever `h` holds and what follows it is run on a null. As observed, it folds the comparisons and the `add` of text; for text it refuses `sub`, `mul`, `floordiv`, `mod` and `truediv` before it folds, so a case with one of them is excused only where the control, which runs Polars, shows the fold is why. The tables judge a call for the dtype of what it is given, as written. Polars allowing the step, where the call the tables refused is given an expression that holds such a call, at any depth, with another call after it (a fold that ends a chain leaves a null literal, which is a literal's, and a call after a fold in its own chain is asked of a value of its dtype), and Polars refusing the case run again with every such call left out for the reason the tables gave, at no place or inside the expression that holds the refused call: the fold is then why Polars allows it. The control puts the other operand in the place of a null receiver. A panic is never excused here: it stays with the limit for a panic. A case that is the same without the folds, or that Polars still allows or refuses for another reason, is excused by nothing. |
| A gather beyond the rows | A gather takes places, and Polars refuses one beyond what it follows when the rows run, and every place when the table has none. The tables measured a gather that gives a null for such a place. Polars refusing the rows or the empty table with the words for a place out of bounds, with no place, in a case with a gather that is not told to give a null, where the case run again with each gather told to give one is no longer refused for them: the places the rows leave it are why. A case with no gather, another reason, or a gather that already gives a null is excused by nothing. |
| A bitwise call in a join's key | Polars refuses some dtypes of `and_`, `or_` and `xor` when it works out the schema of a select, a filter or a sort key, and accepts them in a join's key, on either side: it checks the dtypes of a bitwise call there less than anywhere else (the pairs it accepts are listed below the table). The tables measured a bitwise call in select, and refuse it for the dtypes it is given; for an argument whose dtype they do not know, for one it might have. The tables refusing, with a reason that names the two dtypes of a bitwise call (`^ not allowed on date and i8`, `\| on Boolean and Int64 is not supported`), at an `and_`, `or_` or `xor` in one of a join's own keys, which Polars allows; the key is why only where Polars, asked to select the refused call on a column of each of the two dtypes the reason names, refuses it at that call for the tables' reason, which a call whose dtypes select accepts, a refusal about another call, a step that is no join and the key of a window do not meet. |


What Polars accepts of a bitwise call in a join's key, measured with `&`, `\|` and `^` on columns of each pair of `Boolean`, `Int8`, `Int64`, `UInt8`, `Float64`, `String`, `Date`, `Datetime`, `Duration` and `Time` in Polars 2.1, a join of either side, and compared with `select`. Every pair that select refuses and that has no temporal dtype (`Boolean`, a whole number, a float or text with another of them) is accepted. Of the pairs with a temporal dtype, accepted are a temporal dtype with `Int64` or `Float64`, in either order, and `String` with `Duration` in either order, and `str & time` but not `time & str`. A `Date` with `^` is also accepted with an `Int32`, `UInt32`, `UInt64` and `Float32`, and refused with an `Int16` and `UInt16`, as with an `Int8` and `UInt8`. Refused, as select refuses them, are a temporal dtype with `Boolean`, `Int8`, `UInt8`, a number given no dtype after it or another temporal dtype, including its own (`date ^ date`), a `Date` or `Datetime` with `String` in either order, and `time & str`. The pairs that select accepts are accepted. The list was taken with a left key of whole numbers, and `String` with `Duration` passes the bitwise check only: Polars then refuses the join for the key dtypes not matching or for a cast. The limit's control has one weakness: it selects the call on the dtypes the tables' objection names, not on the argument's real dtype in the join, which for an argument whose dtype the tables do not know may be another one that a join key accepts.

A case that the tables cannot be asked about, because the dtype of an argument or
before a method is not known, is not predicted past that point and is counted. An
expression that holds such a call has the dtype of its own calls, and Polars refusing it
at that call, when it works out the schema, is no dtype the tables got wrong. A join that
pairs one column of the other table with two keys that Polars gives different dtypes, as
an array of whole numbers and the array of floats that a division of it gives, which the
tables take for one class, and that Polars refuses for the dtypes of the keys not matching,
is no case at all: no dtype of the column fits both, and the target counts it apart
(`join keys of different dtypes share a column`) and does not compare it. Polars joins whole
numbers of different widths with one column, so a join it accepts is compared. The
generator keeps to what the tables measured where a value, not a dtype, decides: a
`shift` by the number 1 of a dtype, or the number -1 of a signed integer dtype (the
tables measured the dtype, which the sign does not change), a number of significant figures
of one or more, durations that are text of `b`, two values of a condition of one dtype, a join
without `validate`, a tolerance of `is_close` from zero, a fill of a value or of a strategy and
never both, a limit only for a fill that goes `forward` or `backward`, what a replacement
replaces a column of values that are all different or a literal, and a strict replacement mostly
given a default, for a value it does not list is refused when the rows run, and sometimes given
none, which a limit holds. A pattern is drawn from a small fixed set of fourteen that Polars compiles,
with groups, named groups in both spellings, classes, anchors, alternation and flags, and ten that it
cannot, an unclosed group or class, a repetition that ends before it begins, a group whose name is
none or is named twice, a flag or a property that does not exist, an escape with nothing after it, a repetition of nothing and
an unopened group, so that the refusal of a pattern is checked as the pattern is, with the rule that
asks Polars, a replacement's value is often text with a reference to a group, and a split's separator is often text drawn from the same two sets, and the groups of a pattern are often tested for unique values. `n` and a group
number are drawn from a few values, and a limit holds the count that Polars refuses. A duration that
a method reads as a duration string is text typed in, drawn from twelve that Polars reads, which include
a sign and two units, and nine that it does not, with no amount, no unit, a fraction, a unit that does not
exist, one in the wrong case, a second sign, a space and an amount too large to hold, and otherwise
the column of the table that stands for an argument, whose text is the durations the tables were measured
with. A time zone is drawn from eight names and offsets that Polars reads, four texts that it does not,
and, for a method that takes a zone off, none, and a method that must be given a time zone or a unit
is always given one.

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

The application opens its window on the flow canvas, which fills the window
behind everything else. A side panel at the left, which slides away, lists
the workspaces, and the one chosen is drawn over the canvas.

The canvas draws the engine's claim-timing sample node and what supplies its
inputs, as the host reports them: a box for each supplier with the fields of
its table or its values, a box for the node with a pin for each input, and a
wire from each output to the input it feeds. The wires run at right angles,
each on a lane of its own, around the boxes, and are routed again once a box
is dropped or a resize ends. Boxes can be moved, and the node's box resized.
What the host does not report yet, such as a supplier's kind, the node's name
and its steps, is drawn as stand-ins of the size it will take. The places of
the boxes are the view's own and are not kept, and the buttons for adding a
trigger or a following node do nothing yet.

The steps of a transform are edited on the same sample node, offering the
operations of the engine's catalogue; its two windows keep the steps the same
by telling each other of every change, where the core will later own them.
The step editor runs its steps on the engine through the host and shows the
engine's columns, dtypes, refusals and rows. The table viewer shows a
generated sample through the engine. The graph model, node types and pipeline
execution are not implemented yet.

The side panel also lists the Pieces page. It is a design prototype, not a
feature: a static page in plain HTML, CSS and JavaScript where the way the step
editor is drawn is tried out in several versions before one is built. It
works on made-up samples of its own, shares no code with the view's
packages and calls no host command. The workspace that is open is kept in the
page's address, so reloading the page returns to it.

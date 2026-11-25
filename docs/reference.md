# Reference

## Packages And Workspace

The root [Cargo.toml](../Cargo.toml) declares `dageist` 0.1.0, Rust edition
2024, MSRV 1.99.1 and the MIT license. It forbids `unsafe` code for every
workspace member through `[workspace.lints.rust]`. The workspace uses
`default-members = ["."]`, excludes `vendor/`, and has one committed root
[Cargo.lock](../Cargo.lock). Its `[patch.crates-io]` applies the crate patches
of the Polars release in `vendor/polars`: Polars' forks of `hashbrown` and
`tikv-jemalloc-sys`. [rust-toolchain.toml](../rust-toolchain.toml) selects
`nightly-2026-09-01`, the toolchain that release is built with.

| Package | Path | Role |
| --- | --- | --- |
| `dageist` | [src/](../src/lib.rs) | Host-independent core library. |
| `dageist-engine` | [crates/dageist-engine/](../crates/dageist-engine/Cargo.toml) | Dataframe engine on Polars, built from `vendor/polars` as the release there is built, with Polars' allocator as the global allocator and Polars [set up as the release sets it up](explanation.md#polars-is-set-up-as-the-release-sets-it-up) by every entry that runs it and by `PolarsSession`: sample data, working windows, stored results, Arrow output, the [catalogue of operations](explanation.md#the-catalogue-of-operations) with its English language file, the [step document](explanation.md#the-step-document), its edits and the running of its steps, the claim-timing sample node the step editor opens with, and the [tables of allowed combinations](explanation.md#which-combinations-polars-allows) with the offers made from them and the [reading of a typed text](explanation.md#how-a-typed-text-is-read). Not published. |
| `dageist-fuzz` | [crates/dageist-fuzz/](../crates/dageist-fuzz/Cargo.toml) | Fuzz framework, ported from Surgeist's `surgeist-fuzz`: bounded generation, same-failure reduction, replayable failure evidence and process-isolated campaigns. A workspace member outside `default-members`; no production crate depends on it. Its controlled author fixture is a standalone workspace with its own lock file, outside the product workspace. Not published. See its [README](../crates/dageist-fuzz/README.md). |
| `dageist-tauri` | [crates/dageist-tauri/](../crates/dageist-tauri/Cargo.toml) | Tauri 2 host; builds the `dageist` binary. Not published. |
| `dageist-ui` | [ui/](../ui/package.json) | Private npm workspace for the React view; see [View Packages](#view-packages). |
| `dageist-api-generator` | [api/generator/](../api/generator/Cargo.toml) | Writes and checks the [public API audits](#public-api-audits). A workspace of its own with its own lock file, outside the product workspace. Not published. |

## Vendored Sources

| Path | Source | Pinned to |
| --- | --- | --- |
| `vendor/polars` | [pola-rs/polars](https://github.com/pola-rs/polars), as a shallow Git submodule declared in [.gitmodules](../.gitmodules) | Python Polars `py-2.2.0`, commit `08b2814ce6386c0efe0fea512912353f42a8b7b6`. |

The engine builds Polars from the submodule, with the features, crate
patches, dependency versions and toolchain of that release's
`polars-runtime-32`. The
[release check](how-to.md#check-the-build-against-the-polars-release) compares
the two builds.

## Public API Audits

Each audit lists one crate's public API, one line per item, as rustdoc JSON
describes it. The [audit procedures](how-to.md#check-the-public-api-audits)
check and refresh them.

| Audit | Crate |
| --- | --- |
| [api/public-api.txt](../api/public-api.txt) | The core, `dageist`. |
| [api/crates/](../api/crates/dageist-engine.txt) | Each `crates/dageist-*` crate that has a library. The host has only a binary and is not audited. |
| [api/polars/](../api/polars/polars-plan.txt) | `polars`, `polars-lazy` and `polars-plan`, with the version, the `vendor/polars` commit they are built from and the features a build of `dageist-tauri` resolves for each. `polars-plan` holds expressions and their functions, `polars-lazy` the lazy frame, and `polars` the preludes that re-export them. |
| [api/polars/python.txt](../api/polars/python.txt) | The Python API of the same release, each public method, property, operator and function traced through the binding to the Rust items it reaches, with a count of how many keep the Rust name, use another, are written in Python, or are not resolved. |

## Core API

| Item | Returns |
| --- | --- |
| `dageist::crate_name()` | The core's package name, `"dageist"`. |
| `dageist::crate_version()` | The core's package version. |

## IPC Commands

The host registers these commands in
[crates/dageist-tauri/src/main.rs](../crates/dageist-tauri/src/main.rs) and
defines the table commands in
[sample_table.rs](../crates/dageist-tauri/src/sample_table.rs). The view calls
them through the host package in
[ui/packages/host/src/](../ui/packages/host/src/tauri.ts).

| Command | Arguments | Result |
| --- | --- | --- |
| `core_identity` | None | `{ "name": string, "version": string }` from the core. |
| `operation_catalogue` | None | The engine's catalogue of operations with its English words: `{ "families", "dtypes", "listedDtypes", "comparisons", "entries" }`, `comparisons` the comparisons a test makes, in the order they are offered, each the `entry` that makes it and its `sign` from the language file, each entry with its identifier, family, shown name, hint, step head, accepted dtypes, laziness and parameters, each parameter with the position that holds the expression it takes when a named position does, its Polars `default` read as a value, the same as a setting (`defaultSetting`, or null where Polars decides or the parameter must be given) and `listedDtypes` the dtypes a document names by one word. The shapes are `CatalogueView` and the types beside it in [catalogue.rs](../crates/dageist-tauri/src/catalogue.rs) and [contract.ts](../ui/packages/host/src/contract.ts). |
| `node_inputs` | None | What the sample node is connected to: `{ "source": { "name": string, "from": Origin, "columns": Field[] }, "inputs": Input[] }`, where a `Field` is `{ "name": string, "dtype": string }` with the engine's short dtype name, such as `Datetime(us)`, and an `Origin` says what supplies it to the node: `{ "kind": "node", "name" }` for the table a node gives, `{ "kind": "column", "node", "column" }` for one column of it, or `{ "kind": "parameter" }` for a value set on the graph itself. An `Input` is a `table` with its columns, a `value` with its dtype and `value`, or a `list` with its dtype and `values`, each with its `from` and by the name a step document uses for it, in the order of their names. A value is written as Polars writes it and is `null` where it is null; `values` is `{ "shown": [string or null], "more": number }`, the first four values and how many follow them. |
| `node_document` | None | What the host holds of the sample node's steps: `{ "revision": number, "document", "run" }`. `revision` is a whole number that goes up by one with each edit. A host starts at the milliseconds since the Unix epoch when it started, not at 1, so that a page that outlives its host does not hold a revision the next host is at, as long as edits are slower than a millisecond and the clock does not step back. `document` is `{ "steps": [{ "id", "call" }] }`, each `call` being `{ "entry", "arguments": [{ "parameter", "value" }] }` with the arguments in the order they were given, and each `value` an `ArgumentView` tagged by `kind` (`expression` with its `start` and `links`, `columns`, `integer` and `float` with the words the engine shows the number as, `string`, `flag`, `choice`, `dtype`, `duration`, `pattern`, `timeZone`, `table`, `path`, `pairs` or `many`); no number is sent as a number. `run` is the engine's report of running the document on the sample node, as `{ "source", "steps" }`, where `source` is the columns of the table the steps start from. One report for each step: its `id`, what it works on in a few words as `summary`, whether it is `lazy`, its `outcome` (`valid` with its columns and `changes`, the columns it `added` or `replaced`, `refused` with its place and cause, `incomplete` with the place of what is still to be picked, or `notRun` with the step that blocked it), its `count` (`counted` with the rows it `received` and `kept` on the sample's seven rows and, for each column it changed, the first values written as text with how many `more` follow, `notCounted` where no rows were run, `notReached` with the step that `failed` where Polars accepts the step and an earlier step failed on the rows, or `failed` with Polars' reason where it could not run the step on the rows), the dtype of each expression at its start and after each link, what the tables of allowed combinations say of each call, and `atDefault`, the places of its arguments, its own and those of the calls inside its expressions, that are what their parameter holds when it is not given. A place is a step and the way down from it, a list of segments (`parameter`, `item`, `key`, `value`, `start`, `link`), the same shape in the document, the report and an edit. The shapes are `DocumentStateView` in [document.rs](../crates/dageist-tauri/src/document.rs), `RunView` in [report.rs](../crates/dageist-tauri/src/report.rs) and the types beside them in [contract.ts](../ui/packages/host/src/contract.ts). |
| `edit_document` | `revision`: the revision the edit was made against; `edit`: one of the nineteen edits of the engine's editing API, tagged by `kind`: `addStep`, `swapStep`, `removeStep`, `moveStep`, `setArgument`, `clearArgument`, `addOption`, `setForItem`, `insertItem`, `removeItem`, `insertTest`, `insertEntry`, `removeEntry`, `setStart`, `startWith`, `insertLink`, `swapLink`, `removeLink` and `moveLink`, each naming the place it works on and what it needs; a value is a `ValueRequest` tagged by the kind of value, as the catalogue gives the parameter's | `{ "status": "edited", "revision", "document", "run" }`, the document as it is after the edit at its new revision with the report of running it, or `{ "status": "refused", "revision", "place", "problem", "message" }`, where the engine refused the edit: the place it names, a `problem` tagged by `kind` from a closed list (`noStep`, `stepChanged`, `nothing`, `doesNotFit`, `wrongPlace`, `notPerItem`, `noDefault`, `outOfRange`, `unknownEntry`, `noStart`, `cannotAdd`, `needsValue`, `refused`), the engine's words, and the revision, which is unchanged. An edit made against another revision is not an answer but a failure of kind `stale`; nothing changes. A place in an edit names its step by both its `stepIndex` and its `stepId`, and a request whose place lacks either is not an edit: Tauri refuses it and the bridge answers a `400`. A request that names an entry or a dtype the catalogue lacks is a failure too. The edited document is held, and its revision moved on, only once the report of running it has been made, and the check of the revision and the edit are made together, so of several edits made against one revision exactly one is made and the others are `stale`. |
| `offers_at` | `positions`: the positions that hold the expression, outermost first, each `{ "kind": "outer" or "nested", "name" }` by the tables' words; `after`: what it has so far, as the walk reports it | The engine's offers at that place: `{ "place", "entries" }`, with one entry for each call that stands in an expression, ordered by identifier, with its `verdict` (allowed with its result, shape and whether a chain may end, or refused with who refused and why) and the `argument` a method that may stand starts with, the first parameter it starts without (`leftOut`) and the `slots` it starts with to be filled. Shapes are `OffersView` and the types beside it in [offers.rs](../crates/dageist-tauri/src/offers.rs) and [contract.ts](../ui/packages/host/src/contract.ts). |
| `candidate_fits` | `start`: the positions that hold the expression a candidate would start, or null; `argument`: `{ entry, parameter, place }` for the argument of a method, or null; `candidates`: fields and connected inputs by their short dtype name | `{ "fits": [...] }`, one for each candidate in order: `judged` with the dtype the engine gave it and its verdict, or `unjudged` with why the tables say nothing of it. |
| `read_typed` | `text`: what was typed; `typed`: what it is typed into, `{ "kind": "expression" }`, `{ "kind": "value", "type": "integer", "float", "string", "date" or "any" }`, `{ "kind": "duration" }`, `{ "kind": "pattern" }` or `{ "kind": "path" }`; `start` and `argument`: the slot, as for `candidate_fits`, or null for both | `{ "readings": [...] }`, each way the engine reads the text in the order the slot offers them: `read` with the `literal` the document would hold (`integer`, `float` or `text`, with its `value` and the `shown` words the engine writes it with), its `dtype` or null, and its `fit` in the slot (as one of `candidate_fits`) or null, or `unread` with the `reason` the text is not what it was typed into. Nothing typed has no readings. Rules are in [How a typed text is read](explanation.md#how-a-typed-text-is-read). |
| `option_offers` | `entry`: an identifier; `chosen`: options already set, `{ parameter, word }` | `{ "options": [...] }`: each flag and choice with its default word, the `starter` word it starts with when it is added (the first that is not the default and that Polars allows beside the options chosen, or null), the `stuck` refusal that says why it cannot be added where it has no starter, and every word it may be set to, with Polars' refusal beside the options chosen, or null. |
| `step_starts` | `columns`: the names of the columns of the table a step is added to, in order; `tables`: the names of the connected tables | `{ "starts": [{ "entry" }] }`: each step of the list that can be added to such a table, ordered by identifier. What a new step holds is the engine's to say when it is added, as an `addStep` edit. The shapes are `StepStartsView` in [offers.rs](../crates/dageist-tauri/src/offers.rs) and [contract.ts](../ui/packages/host/src/contract.ts). |
| `rows_after_step` | `revision`: the revision of the document; `step`: a step's identifier; `limit`: how many rows, at most 1,000 | `{ "columns": Field[], "rows": [[string or null]] }`: the rows after that step of the document at that revision, run on the sample node's seven rows, each cell written as text by the engine and a null as `null`. A revision that is not the current one is a `stale` failure. |
| `open_sample_window` | `keys`: how many whole keys to hold in memory, at most 50,000 | `{ "rows": number, "columns": [{ "name": string, "dataType": string, "numeric": boolean }], "figures": { "rows", "columns", "opened" } }` for the working window: `numeric` says whether the column holds numbers, and `figures` has the row count (`2,552 rows`), the column count (`300 columns`) and the time the host took to open the table (`12.3 ms`) written as text. |
| `open_sample_result` | None | The same shape for the whole sample, opened where it is stored without loading it. |
| `table_block` | `offset`, `length`: the rows, at most 10,000; `columns`: the column names, each once, or an empty list for all | That block of whichever table is open, as the raw bytes of a [frame](#the-block-frame), not JSON. Rows past the end are left out. |
| `open_view_window` | `name`: which window, `"steps"` | Nothing. Opens that window, or brings it forward when it is already open. Defined in [windows.rs](../crates/dageist-tauri/src/windows.rs). |

The windows also talk to each other without the core. The host package sends a
message as a Tauri event named `dageist:<topic>`, and every other window that
listens to the topic receives it. The host holds the steps. A window that has
made an edit says on the `document` topic which revision it is at, and the other
windows ask for the document again; a `selection` topic carries which step is
chosen.

The node commands are defined in [node.rs](../crates/dageist-tauri/src/node.rs)
over plain functions that answer without Tauri, so another host can serve the
same views. The node is the engine's claim-timing sample until the graph
supplies real ones. The host holds that node's document and its revision, in
[document.rs](../crates/dageist-tauri/src/document.rs), and the bridge holds it
for as long as it runs; no other command keeps anything between calls. An edit
and the rows after a step name the revision they are for. The engine formats every cell and names every dtype, so
the view formats no value, and the host has no Polars of its own.

A command that fails rejects with `{ "kind": string, "message": string }`. The
kind is one of a closed list the view decides on, and the message is for a
person: `noSuchStep` (the document has no such step), `notRun` (a step before
the one asked for was refused), `refused` (the step asked for was refused),
`limit` (more was asked for than the command's window holds), `stale` (the
document is at another revision than the one the request was made for),
`unknownName` (an entry or a parameter the catalogue lacks,
or a way down to an expression that starts at no position of a step; a position
or a dtype the engine has no word for is no argument, which Tauri refuses with
its own text and the bridge with a `400`) and `failed` (anything else). The bridge answers a
failed command with the same object. The view's hosts reject with a `HostError`
that carries the kind; arguments of the wrong type are rejected by Tauri with
its own text, which the view reads as `failed`. The first call to
either open command generates the sample table, about two million rows of 300
columns and 2 GB of Parquet, into a `claims-…` folder in the application cache
(`~/Library/Caches/io.github.bj-data-eng.dageist/` on macOS). Later calls reuse
it; delete the folder to reclaim the space. The sample is written beside that
folder and moved into place when it is complete, and within one running
application only one command generates it at a time. Two copies of the
application generating at once are not guarded against.

## The Block Frame

`table_block` answers a block as a frame, so that the view shows the number of
each row and a report of the block without writing either. It is defined in
[block.rs](../crates/dageist-tauri/src/block.rs) and read in
[decode-block.ts](../ui/packages/host/src/decode-block.ts).

| Part | Contents |
| --- | --- |
| Length | Four bytes: the length of the header, as a big-endian number. |
| Header | UTF-8 JSON, an object of exactly `numbers` and `report`. `numbers` is the number of each row of the block, counting from 1, as text with a comma between groups of three digits, such as `1,234`; it holds a number for each row the block has, and none for rows past the end of the table. `report` is the block in a line: `140 rows × 17 columns · 38.2 KB · built in 6.2 ms`, with its rows, its columns, the size of its Arrow bytes and the time the engine took to write them. |
| Arrow | The block as an Arrow IPC stream to the end of the frame. It has the columns asked for, in the order asked, or every column when none were asked for, and every one is text: a cell is written by Polars as the step editor's previews write it, so `2026-03-02`, `1d 7h 25m` or `[1, 2]`, and a null is a missing cell. |

The figures are written in [figures.rs](../crates/dageist-tauri/src/figures.rs):
a count with commas, a size in bytes, kilobytes or megabytes (1,024 to the
unit, one decimal), and a time in microseconds, milliseconds or seconds (one
decimal). They are English: commas between groups of three digits, `row` and
`rows`, `KB` and `MB` for multiples of 1,024 bytes, and `µs`. Another language
or number convention is not written yet. The view refuses a frame whose header is not that object, whose
Arrow stream cannot be read, whose columns are not the ones asked for or are
not text, or whose rows are not as many as the header has numbers.

## The Bridge

`dageist bridge [--port N]` serves the commands above, offers included, except
`open_view_window`, to a browser over HTTP, for the development server's
browser preview. It is defined in
[bridge.rs](../crates/dageist-tauri/src/bridge.rs) over the same functions the
Tauri commands call, and its command line is read in
[cli.rs](../crates/dageist-tauri/src/cli.rs). `dageist` with no arguments opens
the application; any other command is refused with the usage. The bridge opens
no window and starts no Tauri, listens on `127.0.0.1` only, and prints
`dageist bridge listening on http://127.0.0.1:<port>` when it is ready. The port
is 7878 unless `--port` gives one; `--port 0` lets the system choose.

| Part | Behaviour |
| --- | --- |
| Request | `POST /bridge/<command>`, with `Content-Type: application/json` and the command's arguments as a JSON object by their Tauri names, `{}` for a command that takes none. The arguments are `deny_unknown_fields`. Only `Content-Length` bodies up to 8 MiB are read; a request is one per connection. |
| `200` | The command's JSON with `Content-Type: application/json`; for `table_block`, the bytes of the block's [frame](#the-block-frame) with `Content-Type: application/vnd.dageist.block`. |
| `422` | The command ran and failed: `{ "message": string }`, the same view the Tauri command rejects with. |
| `400` | The request or its body is not a request or the command's arguments: `{ "message": string }`. |
| `403` | The `Host` header is missing, or is not `127.0.0.1` or `localhost` with the bridge's own port. |
| `404` | The path is not `/bridge/<command>` of a served command, including `open_view_window`. |
| `405` | The method is not `POST`; the answer carries `Allow: POST`. This is also what a preflight (`OPTIONS`) gets. |
| `413`, `408`, `501` | The head or the body is too long; the whole request took the client more than 30 seconds, or 10 seconds passed between two of its reads; or the body has a transfer encoding, which the bridge does not take. A client that takes the answer for more than 60 seconds is closed on. |
| `415` | The content type is not `application/json`, with or without parameters such as `charset`. |
| Every answer | `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `Connection: close`. No `Access-Control-*` header is ever sent. |

A connection whose peer address is not loopback is closed without an answer.
The checks run in the order of the table's `403`, `404`, `405`, `415`, `400`,
and no command runs before all pass. Each answer is a view that Tauri's IPC
would give, so the snapshots in
[ui/packages/host/fixtures/](../ui/packages/host/fixtures/step-report.json) hold
for both.

The bridge keeps the sample gate and the open table for as long as it runs,
as the application keeps them as managed state, and reads and writes the sample
in the application's cache folder. It finds that folder from the platform's
cache folder and the identifier in `tauri.conf.json`, as Tauri does; a test
holds the two equal. Release builds set `windows_subsystem = "windows"`, so on
Windows a release binary has no console to print to; the bridge is run from
`cargo run`, which is a debug build.

In the view, [bridge.ts](../ui/packages/host/src/bridge.ts) is the bridge host:
it posts to `/bridge/<command>` on the page's own origin, and
[vite.config.ts](../ui/apps/desktop/vite.config.ts) forwards `/bridge` to
`127.0.0.1:7878`, or to the port in `DAGEIST_BRIDGE_PORT`, with the target's
`Host`, so that the forwarded request passes the checks above. That rewrite
means the bridge's `Host` check does not protect the forwarded path. Through
the development server, and through `vite preview`, which forwards the same
path, safety rests on Vite's own `Host` check, limited by `server.allowedHosts`
and `preview.allowedHosts` to `localhost` and `127.0.0.1` (Vite also always accepts an
IP address and a name under `.localhost`), and on the bridge's
content-type rule with `cors: false`.

A manual check: with the bridge and the server running, this request is answered
`403` by Vite, not by the bridge, and the same command without the `Host` option
is answered `200`:

```sh
curl -i -X POST -H 'Content-Type: application/json' -H 'Host: other.example:5173' -d '{}' http://localhost:5173/bridge/core_identity
```

## Host Configuration

[crates/dageist-tauri/tauri.conf.json](../crates/dageist-tauri/tauri.conf.json)
sets the following:

| Setting | Value |
| --- | --- |
| Bundle identifier | `io.github.bj-data-eng.dageist` |
| View | `../../ui/dist`, embedded at compile time; build it with `npm --prefix ui run build` first |
| Global API | `app.withGlobalTauri` is `true`, which injects `window.__TAURI__` |
| Main window | Label `main`, title `dageist`, 1440×900, minimum 960×600, dark theme |
| Steps window | Label `steps`, title `dageist · steps`, 1180×820, minimum 820×520, dark theme. It loads the view's page as `index.html?window=steps` and is opened by `open_view_window`, not at start. It closes when the main window does. |
| Minimum macOS | 13.3, the first version whose system WebKit supports the CSS that Tailwind CSS 4 emits |
| Content security policy | Same-origin scripts, styles and images, plus `data:` images and the IPC channel |

[crates/dageist-tauri/capabilities/default.json](../crates/dageist-tauri/capabilities/default.json)
grants the `main` and `steps` windows the `core:default` permission set, which
includes sending and listening to events.

## View Packages

The view is an npm workspace rooted at [ui/package.json](../ui/package.json).
Every package is private and is consumed as TypeScript source.

| Package | Path | Role | May depend on |
| --- | --- | --- | --- |
| `@dageist/desktop` | [ui/apps/desktop/](../ui/apps/desktop/src/App.tsx) | The shell: picks the host, composes the features, owns the page, the bundler configuration and the design tokens. | Every package below and React |
| `@dageist/host` | [ui/packages/host/](../ui/packages/host/src/contract.ts) | The contract of every IPC command, the decoders that read what the host sends, a block's Arrow stream included, and the two hosts: over Tauri, and over the [bridge](#the-bridge) in a plain browser. | flechette |
| `@dageist/design` | [ui/packages/design/](../ui/packages/design/src/index.ts) | Primitive components: `Block` (a word that can be pressed), `Piece`, `Slot`, `ManySlot`, `OfferList`, `TypedBlock`, `Popover`, `Hint`, `Peek`, `ToggleButton`, `Panel`, and `blocks.css`, the styles of the words a step is written in. `Popover` is placed by Floating UI. | Floating UI, React, React DOM |
| `@dageist/steps` | [ui/packages/steps/](../ui/packages/steps/src/StepsWorkspace.tsx) | The steps of a transform: the editor, which draws the document the host holds and sends each edit with the revision it was made against, the reading of the document's shapes, the preparation of the engine's report for display, the scheduling of the lists' requests, the preparation of the engine's offers, the functions that decide what the editor can do with a piece and which control a parameter gets, and the declared gaps with the grid test that checks them, the main window's table with its list of steps, and the window the steps are edited in. | `@dageist/design`, `@dageist/host`, dnd kit, React, React DOM |
| `@dageist/table` | [ui/packages/table/](../ui/packages/table/src/TableViewer.tsx) | The table viewer: fetches blocks of text and draws only those, with the lines it writes under its heading. | `@dageist/design`, `@dageist/host`, React |

| File | Contents |
| --- | --- |
| [ui/apps/desktop/index.html](../ui/apps/desktop/index.html) | The page Vite builds from. |
| [ui/apps/desktop/vite.config.ts](../ui/apps/desktop/vite.config.ts) | Bundler settings; the bundle is written to `ui/dist/`. |
| [ui/apps/desktop/src/styles.css](../ui/apps/desktop/src/styles.css) | Tailwind entry, the colour and font tokens, and the declarations of the two typefaces. |
| [ui/apps/desktop/public/fonts/](../ui/apps/desktop/public/fonts/) | The typefaces the view is set in, each with the licence it comes under, copied into the bundle as they are: Inter for everything that is not code or data, and JetBrains Mono for code and data. |
| [ui/packages/host/src/decode.ts](../ui/packages/host/src/decode.ts) | The readers every decoder is made of. A decoder reads what the host sent, which is `unknown`, and returns it typed or refuses it with a `DecodeError` that names the place, such as `$.steps[3].outcome.status`, and the reason. Objects are closed: a missing field, a field the contract lacks and a tag it does not name are refused. [decode-catalogue.ts](../ui/packages/host/src/decode-catalogue.ts), [decode-node.ts](../ui/packages/host/src/decode-node.ts) and [decode-run.ts](../ui/packages/host/src/decode-run.ts) read the catalogue, a node's inputs with the rows after a step, and the report of a run, and [decode-sample.ts](../ui/packages/host/src/decode-sample.ts) the core's identity and the shape of the sample table, and [decode-block.ts](../ui/packages/host/src/decode-block.ts) a block's frame, refusing a header, an Arrow stream, columns or rows that are not what was asked for. The closed lists of words they read are constants in [contract.ts](../ui/packages/host/src/contract.ts). |
| [ui/packages/host/test-support/](../ui/packages/host/test-support/tamper.ts) | Test support that is not part of the package's source: copies of a fixture with one thing changed, to see what a decoder says of a value the host does not send, the reader of a block fixture and the reader of the corpus's files. The compiler settings include it. |
| [ui/packages/host/fixtures/](../ui/packages/host/fixtures/catalogue.json) | Snapshots of what the host's commands return, which the decoders' tests read, and which the bridge's tests compare the bridge's answers with: `catalogue.json`, `node-inputs.json`, `node-document.json` (what `node_document` gives for the claim-timing sample), `edit-edited.json` and `edit-refused.json` (what `edit_document` answers for an edit made and for one the engine refuses), `step-report.json` (the claim-timing steps), `step-report-refused.json` (a step the tables refuse, with the words recorded of Polars, and the step after it), `step-report-incomplete.json` (a step with something still to be picked and the step after it), `step-rows.json`, `offers-start.json`, `offers-value.json` and `offers-condition.json` (the offers at a select's start, after a timestamp and after `then`), `fits-start.json` and `fits-argument.json` (the fits of fields and inputs for a predicate's start and for `dt.offset_by`'s argument), `typed-expression.json`, `typed-start-and-argument.json`, `typed-whole.json`, `typed-date.json`, `typed-unread.json` and `typed-sample-values.json` (how `1.5` typed into `dt.offset_by`'s argument, and into `shift`'s argument where it also starts a filter's predicate, `12` into a whole number, `10-03-2026` into a date and `1x` into a duration read, and how the numbers and text of the editor's starting steps read, which those steps are tested to hold), `options.json` (the options of a join beside a cross join), `step-starts.json` (which steps can be added on the sample node's columns and tables), `offers-grid.json` (every call at every place of every kind, one bit for each entry, in words of 32 bits), `step-report-variants.json` (a report the host builds by hand to hold the variants no real document gives together), the five `corpus-*.json` files (step documents the engine made by its own edits, each with the report the host gives it) and `table-window.json` and `table-result.json` (the shape of an opened working window and stored result, with its figures at a fixed open time), `table-block-window.json` and `table-block-result.json` (a request for a block and the frame the host answered, in hexadecimal, at a fixed build time) and `vocabulary.json` (the words of every closed list, the failure kinds included, from the engine's own lists of variants, which the decoders' tests compare their word lists with). Generated by host tests; [refresh the catalogue snapshot](how-to.md#refresh-the-catalogue-snapshot) and [the step report snapshots](how-to.md#refresh-the-step-report-snapshots), never edit them. |
| [ui/vitest.config.ts](../ui/vitest.config.ts) | Vitest settings: it runs the `*.test.ts` files under `apps/*/src` and `packages/*/src` in Node. |
| [ui/apps/desktop/public/pieces/](../ui/apps/desktop/public/pieces/index.html) | The Pieces page: a prototype of how the step editor is drawn, in plain HTML, CSS and JavaScript, copied into the bundle as it is and shown in the Pieces tab. |
| [ui/tsconfig.json](../ui/tsconfig.json) | Strict compiler settings for every package's `src/` and `test-support/`; emits nothing. |
| [ui/tsconfig.node.json](../ui/tsconfig.node.json) | Strict type checking of the Vite and Vitest configurations; emits nothing. |
| [ui/.oxlintrc.json](../ui/.oxlintrc.json) | Lint rules: the `correctness` and `suspicious` categories plus the listed rules, all as errors, with type-aware, React and accessibility rules enabled. |
| [ui/.oxfmtrc.json](../ui/.oxfmtrc.json) | Formatter settings: 100-column lines, single quotes, semicolons. |

| npm script | Action |
| --- | --- |
| `dev` | Serves the view at `http://localhost:5173` and updates the page as files change. |
| `check` | Type-checks every package and the Vite configuration without emitting files. |
| `lint` | Lints `ui/` with oxlint, denying warnings and reporting unused suppressions. |
| `format` | Rewrites the files in `ui/` with oxfmt. |
| `format:check` | Reports files that oxfmt would change, without writing. |
| `build` | Bundles the view into `ui/dist/` with Vite. |
| `test` | Runs the view's tests once with Vitest. They cover only the data layers, the decoders in `@dageist/host` and the functions that prepare data for display, against the fixtures written from Rust, with expectations written by hand; React components are not tested. |

Runtime dependencies, bundled into `ui/dist/` and pinned to exact versions in
the package that uses them: React 19.3.1 (`react`, `react-dom`),
[dnd kit](https://dndkit.com) 0.5.3 (`@dnd-kit/react`, `@dnd-kit/dom`) for drag
and drop, and [flechette](https://github.com/uwdata/flechette) 2.7.1, used by
`@dageist/host`, for reading the Arrow stream of a block. Development dependencies, pinned in
[ui/package.json](../ui/package.json), are Vite with its React plugin, Tailwind
CSS with its Vite plugin, React's type declarations, TypeScript, the oxlint linter
with its type-aware companion `oxlint-tsgolint`, the oxfmt formatter,
Vitest 4.1.12 for the tests of the data layers, and Node's type declarations
(`@types/node`) for the Vite configuration.

The Vite and Vitest configurations are TypeScript that runs in Node. They are
type-checked against `@types/node` through
[ui/tsconfig.node.json](../ui/tsconfig.node.json) and linted with the Node rules
in [ui/.oxlintrc.json](../ui/.oxlintrc.json).

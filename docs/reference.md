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
| `dageist-engine` | [crates/dageist-engine/](../crates/dageist-engine/Cargo.toml) | Dataframe engine on Polars, built from `vendor/polars` as the release there is built, with Polars' allocator as the global allocator and Polars [set up as the release sets it up](explanation.md#polars-is-set-up-as-the-release-sets-it-up) by every entry that runs it and by `PolarsSession`: sample data, working windows, stored results, Arrow output, the [catalogue of operations](explanation.md#the-catalogue-of-operations) with its English language file, the [step document](explanation.md#the-step-document) and the running of its steps, the claim-timing sample node the step editor opens with, and the [tables of allowed combinations](explanation.md#which-combinations-polars-allows) with the offers made from them and the [reading of a typed text](explanation.md#how-a-typed-text-is-read). Not published. |
| `dageist-fuzz` | [crates/dageist-fuzz/](../crates/dageist-fuzz/Cargo.toml) | Fuzz framework, ported from Surgeist's `surgeist-fuzz`: bounded generation, same-failure reduction, replayable failure evidence and process-isolated campaigns. A workspace member outside `default-members`; no production crate depends on it. Its controlled author fixture is a standalone workspace with its own lock file, outside the product workspace. Not published. See its [README](../crates/dageist-fuzz/README.md). |
| `dageist-tauri` | [crates/dageist-tauri/](../crates/dageist-tauri/Cargo.toml) | Tauri 2 host; builds the `dageist` binary. Not published. |
| `dageist-ui` | [ui/](../ui/package.json) | Private npm workspace for the React view; see [View Packages](#view-packages). |
| `dageist-api-generator` | [api/generator/](../api/generator/Cargo.toml) | Writes and checks the [public API audits](#public-api-audits). A workspace of its own with its own lock file, outside the product workspace. Not published. |

## Vendored Sources

| Path | Source | Pinned to |
| --- | --- | --- |
| `vendor/polars` | [pola-rs/polars](https://github.com/pola-rs/polars), as a shallow Git submodule declared in [.gitmodules](../.gitmodules) | Python Polars `py-2.1.0`, commit `7f902bd3ffd27ff82d14278a09a295ff2f7aed92`. |

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
| `operation_catalogue` | None | The engine's catalogue of operations with its English words: `{ "families", "dtypes", "listedDtypes", "entries" }`, each entry with its identifier, family, shown name, hint, step head, accepted dtypes, laziness and parameters, each parameter with the position that holds the expression it takes when a named position does, and `listedDtypes` the dtypes a document names by one word. The shapes are `CatalogueView` and the types beside it in [catalogue.rs](../crates/dageist-tauri/src/catalogue.rs) and [contract.ts](../ui/packages/host/src/contract.ts). |
| `node_inputs` | None | What the sample node is connected to: `{ "source": { "name": string, "from": Origin, "columns": Field[] }, "inputs": Input[] }`, where a `Field` is `{ "name": string, "dtype": string }` with the engine's short dtype name, such as `Datetime(us)`, and an `Origin` says what supplies it to the node: `{ "kind": "node", "name" }` for the table a node gives, `{ "kind": "column", "node", "column" }` for one column of it, or `{ "kind": "parameter" }` for a value set on the graph itself. An `Input` is a `table` with its columns, a `value` with its dtype and `value`, or a `list` with its dtype and `values`, each with its `from` and by the name a step document uses for it, in the order of their names. A value is written as Polars writes it and is `null` where it is null; `values` is `{ "shown": [string or null], "more": number }`, the first four values and how many follow them. |
| `run_document` | `document`: a step document's JSON text | The engine's report of running the document on the sample node, as `{ "status": "run", "source", "steps" }`, with one report for each step: its `id`, whether it is `lazy`, its `outcome` (`valid` with its columns and `changes`, the columns it `added` or `replaced`, `refused` with its place and cause, or `notRun` with the step that blocked it), its `count` (`counted` with the rows it `received` and `kept` on the sample's seven rows and, for each column it changed, the first values written as text with how many `more` follow, `notCounted` where no rows were run, or `failed` with Polars' reason where it could not run the step on the rows), the dtype of each expression at its start and after each link, and what the tables of allowed combinations say of each call. Text that is not a document is not a failed command; it is `{ "status": "unreadable", "problem" }`, with the line and column of a syntax error, or the place of a refusal. The shapes are `RunView` and the types beside it in [report.rs](../crates/dageist-tauri/src/report.rs) and [contract.ts](../ui/packages/host/src/contract.ts). |
| `offers_at` | `positions`: the positions that hold the expression, outermost first, each `{ "kind": "outer" or "nested", "name" }` by the tables' words; `after`: what it has so far, as the walk reports it | The engine's offers at that place: `{ "place", "entries" }`, with one entry for each call that stands in an expression, ordered by identifier, with its `verdict` (allowed with its result, shape and whether a chain may end, or refused with who refused and why) and the `argument` a method that may stand starts with. Shapes are `OffersView` and the types beside it in [offers.rs](../crates/dageist-tauri/src/offers.rs) and [contract.ts](../ui/packages/host/src/contract.ts). |
| `candidate_fits` | `start`: the positions that hold the expression a candidate would start, or null; `argument`: `{ entry, parameter, place }` for the argument of a method, or null; `candidates`: fields and connected inputs by their short dtype name | `{ "fits": [...] }`, one for each candidate in order: `judged` with the dtype the engine gave it and its verdict, or `unjudged` with why the tables say nothing of it. |
| `read_typed` | `text`: what was typed; `typed`: what it is typed into, `{ "kind": "expression" }`, `{ "kind": "value", "type": "integer", "float", "string", "date" or "any" }`, `{ "kind": "duration" }` or `{ "kind": "path" }`; `start` and `argument`: the slot, as for `candidate_fits`, or null for both | `{ "readings": [...] }`, each way the engine reads the text in the order the slot offers them: `read` with the `literal` the document would hold (`integer`, `float` or `text`), its `dtype` or null, and its `fit` in the slot (as one of `candidate_fits`) or null, or `unread` with the `reason` the text is not what it was typed into. Nothing typed has no readings. Rules are in [How a typed text is read](explanation.md#how-a-typed-text-is-read). |
| `option_offers` | `entry`: an identifier; `chosen`: options already set, `{ parameter, word }` | `{ "options": [...] }`: each flag and choice with its default word and every word it may be set to, with Polars' refusal beside the options chosen, or null. |
| `rows_after_step` | `document`; `step`: a step's identifier; `limit`: how many rows, at most 1,000 | `{ "columns": Field[], "rows": [[string or null]] }`: the rows after that step, run on the sample node's seven rows, each cell written as text by the engine and a null as `null`. |
| `open_sample_window` | `keys`: how many whole keys to hold in memory, at most 50,000 | `{ "rows": number, "columns": [{ "name": string, "dataType": string }] }` for the working window. |
| `open_sample_result` | None | The same shape for the whole sample, opened where it is stored without loading it. |
| `table_block` | `offset`, `length`: the rows, at most 10,000; `columns`: the column names, each once, or an empty list for all | That block of whichever table is open, as the raw bytes of an Arrow IPC stream, not JSON. Rows past the end are left out. |
| `open_view_window` | `name`: which window, `"steps"` | Nothing. Opens that window, or brings it forward when it is already open. Defined in [windows.rs](../crates/dageist-tauri/src/windows.rs). |

The windows also talk to each other without the core. The host package sends a
message as a Tauri event named `dageist:<topic>`, and every other window that
listens to the topic receives it. The steps window and the main window keep
the steps the same this way while the steps are held in the view.

The node commands are defined in [node.rs](../crates/dageist-tauri/src/node.rs)
over plain functions that answer without Tauri, so another host can serve the
same views. The node is the engine's claim-timing sample until the graph
supplies real ones, and no command keeps anything between calls: each reads the
document it is given. The engine formats every cell and names every dtype, so
the view formats no value, and the host has no Polars of its own.

A command that fails rejects with `{ "kind": string, "message": string }`. The
kind is one of a closed list the view decides on, and the message is for a
person: `noSuchStep` (the document has no such step), `notRun` (a step before
the one asked for was refused), `refused` (the step asked for was refused),
`limit` (more was asked for than the command's window holds), `unreadable` (the
text sent is not a document), `unknownName` (an entry or a parameter the catalogue lacks,
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
| `200` | The command's JSON with `Content-Type: application/json`; for `table_block`, the bytes of the Arrow IPC stream with `Content-Type: application/vnd.apache.arrow.stream`. |
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
| `@dageist/host` | [ui/packages/host/](../ui/packages/host/src/contract.ts) | The contract of every IPC command, the decoders that read what the host sends, and the two hosts: over Tauri, and over the [bridge](#the-bridge) in a plain browser. | Nothing |
| `@dageist/design` | [ui/packages/design/](../ui/packages/design/src/index.ts) | Primitive components: `Block` (a word that can be pressed), `Piece`, `Slot`, `ManySlot`, `OfferList`, `TypedBlock`, `Popover`, `Hint`, `Peek`, `ToggleButton`, `Panel`, and `blocks.css`, the styles of the words a step is written in. `Popover` is placed by Floating UI. | Floating UI, React, React DOM |
| `@dageist/steps` | [ui/packages/steps/](../ui/packages/steps/src/StepsWorkspace.tsx) | The steps of a transform: their model and its translation into a step document, the preparation of the engine's report for display, the scheduling of runs and of the lists' requests, the preparation of the engine's offers and the check of what it cannot offer, the main window's table with its list of steps, and the window the steps are edited in. | `@dageist/design`, `@dageist/host`, dnd kit, React, React DOM |
| `@dageist/table` | [ui/packages/table/](../ui/packages/table/src/TableViewer.tsx) | The table viewer: fetches blocks as Arrow bytes and draws only those. | `@dageist/design`, `@dageist/host`, flechette, React |

| File | Contents |
| --- | --- |
| [ui/apps/desktop/index.html](../ui/apps/desktop/index.html) | The page Vite builds from. |
| [ui/apps/desktop/vite.config.ts](../ui/apps/desktop/vite.config.ts) | Bundler settings; the bundle is written to `ui/dist/`. |
| [ui/apps/desktop/src/styles.css](../ui/apps/desktop/src/styles.css) | Tailwind entry, the colour and font tokens, and the declarations of the two typefaces. |
| [ui/apps/desktop/public/fonts/](../ui/apps/desktop/public/fonts/) | The typefaces the view is set in, each with the licence it comes under, copied into the bundle as they are: Inter for everything that is not code or data, and JetBrains Mono for code and data. |
| [ui/packages/host/src/decode.ts](../ui/packages/host/src/decode.ts) | The readers every decoder is made of. A decoder reads what the host sent, which is `unknown`, and returns it typed or refuses it with a `DecodeError` that names the place, such as `$.steps[3].outcome.status`, and the reason. Objects are closed: a missing field, a field the contract lacks and a tag it does not name are refused. [decode-catalogue.ts](../ui/packages/host/src/decode-catalogue.ts), [decode-node.ts](../ui/packages/host/src/decode-node.ts) and [decode-run.ts](../ui/packages/host/src/decode-run.ts) read the catalogue, a node's inputs with the rows after a step, and the report of a run, and [decode-sample.ts](../ui/packages/host/src/decode-sample.ts) the core's identity, the shape of the sample table and the bytes of a block. The closed lists of words they read are constants in [contract.ts](../ui/packages/host/src/contract.ts). |
| [ui/packages/host/test-support/](../ui/packages/host/test-support/tamper.ts) | Test support that is not part of the package's source: copies of a fixture with one thing changed, to see what a decoder says of a value the host does not send. The compiler settings include it. |
| [ui/packages/host/fixtures/](../ui/packages/host/fixtures/catalogue.json) | Snapshots of what the host's commands return, which the decoders' tests read, and which the bridge's tests compare the bridge's answers with: `catalogue.json`, `node-inputs.json`, `step-document.json` (the claim-timing sample's steps as the engine's document, which the editor's starting steps are tested to translate to), `step-report.json` (the claim-timing steps), `step-report-refused.json` (a step Polars refuses and the step after it), `step-report-unreadable.json` (text that is not a document), `step-rows.json`, `offers-start.json`, `offers-value.json` and `offers-condition.json` (the offers at a select's start, after a timestamp and after `then`), `fits-start.json` and `fits-argument.json` (the fits of fields and inputs for a predicate's start and for `dt.offset_by`'s argument), `typed-expression.json`, `typed-start-and-argument.json`, `typed-whole.json`, `typed-date.json` and `typed-unread.json` (how `1.5` typed into `dt.offset_by`'s argument, and into `shift`'s argument where it also starts a filter's predicate, `12` into a whole number, `10-03-2026` into a date and `1x` into a duration read), `options.json` (the options of a join beside a cross join), `offers-grid.json` (every call at every place of every kind, one bit for each entry) and `step-report-variants.json` (a report the host builds by hand to hold the variants no real document gives together) and `vocabulary.json` (the words of every closed list, the failure kinds included, from the engine's own lists of variants, which the decoders' tests compare their word lists with). Generated by host tests; [refresh the catalogue snapshot](how-to.md#refresh-the-catalogue-snapshot) and [the step report snapshots](how-to.md#refresh-the-step-report-snapshots), never edit them. |
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
and drop, and [flechette](https://github.com/uwdata/flechette) 2.7.1 for
reading Arrow data. Development dependencies, pinned in
[ui/package.json](../ui/package.json), are Vite with its React plugin, Tailwind
CSS with its Vite plugin, React's type declarations, TypeScript, the oxlint linter
with its type-aware companion `oxlint-tsgolint`, the oxfmt formatter,
Vitest 4.1.12 for the tests of the data layers, and Node's type declarations
(`@types/node`) for the Vite configuration.

The Vite and Vitest configurations are TypeScript that runs in Node. They are
type-checked against `@types/node` through
[ui/tsconfig.node.json](../ui/tsconfig.node.json) and linted with the Node rules
in [ui/.oxlintrc.json](../ui/.oxlintrc.json).

# Reference

## Packages And Workspace

The root [Cargo.toml](../Cargo.toml) declares `dageist` 0.1.0, Rust edition
2024, MSRV 1.99.1 and the MIT license. It forbids `unsafe` code for every
workspace member through `[workspace.lints.rust]`. The workspace uses
`default-members = ["."]` and one committed root [Cargo.lock](../Cargo.lock).

| Package | Path | Role |
| --- | --- | --- |
| `dageist` | [src/](../src/lib.rs) | Host-independent core library. |
| `dageist-engine` | [crates/dageist-engine/](../crates/dageist-engine/Cargo.toml) | Dataframe engine on Polars: sample data, working windows, stored results and Arrow output. Not published. |
| `dageist-tauri` | [crates/dageist-tauri/](../crates/dageist-tauri/Cargo.toml) | Tauri 2 host; builds the `dageist` binary. Not published. |
| `dageist-ui` | [ui/](../ui/package.json) | Private npm workspace for the React view; see [View Packages](#view-packages). |

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
them through the host bridge in
[ui/packages/host/src/](../ui/packages/host/src/tauri.ts).

| Command | Arguments | Result |
| --- | --- | --- |
| `core_identity` | None | `{ "name": string, "version": string }` from the core. |
| `open_sample_window` | `keys`: how many whole keys to hold in memory, at most 50,000 | `{ "rows": number, "columns": [{ "name": string, "dataType": string }] }` for the working window. |
| `open_sample_result` | None | The same shape for the whole sample, opened where it is stored without loading it. |
| `table_block` | `offset`, `length`: the rows, at most 10,000; `columns`: the column names, each once, or an empty list for all | That block of whichever table is open, as the raw bytes of an Arrow IPC stream, not JSON. Rows past the end are left out. |
| `open_view_window` | `name`: which window, `"steps"` | Nothing. Opens that window, or brings it forward when it is already open. Defined in [windows.rs](../crates/dageist-tauri/src/windows.rs). |

The windows also talk to each other without the core. The host bridge sends a
message as a Tauri event named `dageist:<topic>`, and every other window that
listens to the topic receives it. The steps window and the main window keep
the steps the same this way while the steps are a storyboard in the view.

A table command that fails rejects with `{ "message": string }`; arguments of
the wrong type are rejected by Tauri with its own text. The first call to
either open command generates the sample table, about two million rows of 300
columns and 2 GB of Parquet, into a `claims-…` folder in the application cache
(`~/Library/Caches/io.github.bj-data-eng.dageist/` on macOS). Later calls reuse
it; delete the folder to reclaim the space. The sample is written beside that
folder and moved into place when it is complete, and within one running
application only one command generates it at a time. Two copies of the
application generating at once are not guarded against.

## Host Configuration

[crates/dageist-tauri/tauri.conf.json](../crates/dageist-tauri/tauri.conf.json)
sets the following:

| Setting | Value |
| --- | --- |
| Bundle identifier | `io.github.bj-data-eng.dageist` |
| View | `../../ui/dist`, embedded at compile time; build it with `npm --prefix ui run build` first |
| Global API | `app.withGlobalTauri` is `true`, which injects `window.__TAURI__` |
| Main window | Label `main`, title `dageist`, 1440×900, minimum 960×600, dark theme |
| Steps window | Label `steps`, title `dageist · steps`, 1180×820, minimum 820×520, dark theme. It loads the view's page as `index.html?window=steps` and is opened by `open_view_window`, not at start. |
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
| `@dageist/desktop` | [ui/apps/desktop/](../ui/apps/desktop/src/App.tsx) | The shell: picks the host, composes the features, owns the page, the bundler configuration and the design tokens. | Every package below, React, and flechette for the stand-in host |
| `@dageist/host` | [ui/packages/host/](../ui/packages/host/src/contract.ts) | The contract of every IPC command and its implementation over Tauri. | Nothing |
| `@dageist/design` | [ui/packages/design/](../ui/packages/design/src/index.ts) | Primitive components: `Block`, `Piece`, `Slot`, `ManySlot`, `OfferList`, `TypedBlock`, `Popover`, `Hint`, `ToggleButton`, `Panel`, and the blocks' stylesheet `blocks.css`. `Popover` is placed by Floating UI. | Floating UI, React, React DOM |
| `@dageist/steps` | [ui/packages/steps/](../ui/packages/steps/src/StepsWorkspace.tsx) | The steps of a transform and their storyboard model: the main window's table with its list of steps, and the window the steps are edited in. | `@dageist/design`, `@dageist/host`, dnd kit, React, React DOM |
| `@dageist/table` | [ui/packages/table/](../ui/packages/table/src/TableViewer.tsx) | The table viewer: fetches blocks as Arrow bytes and draws only those. | `@dageist/design`, `@dageist/host`, flechette, React |

| File | Contents |
| --- | --- |
| [ui/apps/desktop/index.html](../ui/apps/desktop/index.html) | The page Vite builds from. |
| [ui/apps/desktop/vite.config.ts](../ui/apps/desktop/vite.config.ts) | Bundler settings; the bundle is written to `ui/dist/`. |
| [ui/apps/desktop/src/styles.css](../ui/apps/desktop/src/styles.css) | Tailwind entry and the colour and font tokens. |
| [ui/apps/desktop/src/demo-host.ts](../ui/apps/desktop/src/demo-host.ts) | A stand-in host with made-up rows, loaded only when the page is open outside Tauri. |
| [ui/apps/desktop/public/pieces/](../ui/apps/desktop/public/pieces/index.html) | The Pieces page: a prototype of the step editor's blocks in plain HTML, CSS and JavaScript, copied into the bundle as it is and shown in the Pieces tab. |
| [ui/tsconfig.json](../ui/tsconfig.json) | Strict compiler settings for every package's `src/`; emits nothing. |
| [ui/tsconfig.node.json](../ui/tsconfig.node.json) | Strict type checking of the Vite configuration; emits nothing. |
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

Runtime dependencies, bundled into `ui/dist/` and pinned to exact versions in
the package that uses them: React 19.3.1 (`react`, `react-dom`),
[dnd kit](https://dndkit.com) 0.5.3 (`@dnd-kit/react`, `@dnd-kit/dom`) for drag
and drop, and [flechette](https://github.com/uwdata/flechette) 2.7.1 for
reading Arrow data. Development dependencies, pinned in
[ui/package.json](../ui/package.json), are Vite with its React plugin, Tailwind
CSS with its Vite plugin, React's type declarations, TypeScript, the oxlint linter
with its type-aware companion `oxlint-tsgolint`, the oxfmt formatter, and
Node's type declarations (`@types/node`) for the Vite configuration.

The Vite configuration is TypeScript that runs in Node. It is type-checked
against `@types/node` through [ui/tsconfig.node.json](../ui/tsconfig.node.json)
and linted with the Node rules in [ui/.oxlintrc.json](../ui/.oxlintrc.json).

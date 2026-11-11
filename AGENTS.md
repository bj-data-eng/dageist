# Dageist Repository Guide

Use `$pisct:coordination` for delivery coordination and the task-appropriate
focused PISCT skill. Follow higher-priority instructions.

## Authority Split

This file owns repository discovery, product boundaries and command inventory.
Crate-local `AGENTS.md` files, when present, supplement it with domain facts and
focused checks; they do not establish separate ownership or delivery workflows.
Keep guides as concise discovery maps. Detailed contracts belong with their
owning documentation/source; load only material relevant to the current task.

Use [dageist-admin](.agents/skills/dageist-admin/SKILL.md) for issues and labels,
pull requests, local working material and pause/resume handoffs. PISCT owns
engineering methods. Apply `$pisct:invariants` to engineering and
`$pisct:rust-modeling` to Rust models and APIs. Dageist-owned Rust prohibits
executable `unsafe` without exception. TypeScript follows the Pragmatic
TypeScript Guidelines (`pragmatic-typescript`); its type check, lint and
format check are required gates for every change under `ui/`.

The coordinator owns issue and label updates, staging, commits, integration and
publication within the authorized scope. Use `$pisct:holistic-review` for final
independent integration review.

## Repository Identity And Ownership

This repository owns the host-independent `dageist` core in `src/`, the host
adapters under `crates/`, the TypeScript view in `ui/`, their contracts,
manifests, tests, documentation and shared tooling. Root owns workspace wiring
and the core. The Tauri host owns its window, webview configuration, IPC
commands and the view it serves.

## Discover The Current Structure

Read the relevant sources below; manifests establish package/dependency facts
and current source establishes behavior.

| Fact | Authoritative source |
| --- | --- |
| Root identity, MSRV, lints, workspace/default members | [Cargo.toml](Cargo.toml) |
| Rust dependency resolution | [Cargo.lock](Cargo.lock) |
| Host identity and dependencies | [crates/dageist-tauri/Cargo.toml](crates/dageist-tauri/Cargo.toml) |
| Window, bundle, view location and content security policy | [crates/dageist-tauri/tauri.conf.json](crates/dageist-tauri/tauri.conf.json) |
| Webview permissions | [crates/dageist-tauri/capabilities/](crates/dageist-tauri/capabilities/) |
| IPC commands and their serialized views | [crates/dageist-tauri/src/main.rs](crates/dageist-tauri/src/main.rs) |
| View toolchain, scripts and compiler settings | [ui/package.json](ui/package.json), [ui/package-lock.json](ui/package-lock.json), [ui/tsconfig.json](ui/tsconfig.json) |
| View lint rules and formatter settings | [ui/.oxlintrc.json](ui/.oxlintrc.json), [ui/.oxfmtrc.json](ui/.oxfmtrc.json) |
| Architecture and integration boundaries | [docs/explanation.md](docs/explanation.md) |
| Documentation navigation and source locations | [README.md](README.md), [docs/reference.md](docs/reference.md) |
| Licensing and dependency attribution | [LICENSE](LICENSE), [NOTICE.md](NOTICE.md) |

When sources disagree, report exact paths and revisions. Do not guess, silently
rewrite another authority, or widen the task to reconcile them.

## Product Boundary

Dageist is a Rust-first visual studio for dataframe pipelines. Users compose
sources, joins, transforms, aggregations and outputs such as tables, charts and
file sinks as a directed acyclic graph on a canvas. Follow the
[architecture explanation](docs/explanation.md).

Rust owns application behavior: the graph model, validation, execution, state
and every value the view displays. Put that behavior in the core or in crates the
core depends on. The core never depends on a host. The Tauri host translates
core contracts into IPC commands and serializable views. The TypeScript view
owns presentation and input only: it renders what the host reports, forwards
user intent through IPC commands, holds no authoritative application state and
performs no data processing.

Tauri is the interim host. [Surgeist](https://github.com/bj-data-eng/surgeist)
is the intended eventual host, so keep host-specific code inside the host
adapter where a later Surgeist adapter can replace it without core changes.
A capability the view needs that Surgeist lacks is Surgeist's work; report it
to that repository rather than implementing it here.

## Backward Compatibility

Before the root `dageist` public API reaches v1.x.x, do not add or retain
compatibility/migration shims or machinery in any crate, the IPC protocol or
the view. Reaching v1 permits consideration, not automatic authorization.
Apply `$pisct:invariants` for semantic API naming.

## Subagent Models

Use `gpt-6.1-sol` for every subagent and the coordinator, with no automatic
alternatives. Follow `$pisct:coordination` for explicit launch settings,
assignment-specific reasoning effort and review independence.

## Generated Artifacts

Source remains authoritative; never hand-edit generated artifacts.

| Artifact | Owner and procedures |
| --- | --- |
| Raster icons in `crates/dageist-tauri/icons/` | [icon procedure](docs/how-to.md#regenerate-the-application-icons) from `icon.svg` |
| `ui/package-lock.json` | npm, from [ui/package.json](ui/package.json) |
| `ui/dist/` (ignored) | `npm --prefix ui run build` |
| `crates/dageist-tauri/gen/` (ignored) | `tauri-build`, during any build of `dageist-tauri` |

## Command Inventory

`default-members = ["."]` limits unqualified Cargo commands to the core.
Product packages share the root lockfile.

Select focused checks through `$pisct:testing`; execute through `$pisct:process`.
No blanket workspace tests. Targeted Cargo commands may run in parallel, with
one build job per command. Monitor `pisct host status` and follow coordination's
pressure guidance. Use separate targets for concurrent worktrees changing the
same packages; reuse an existing target when source/context permit. Keep default
test parallelism unless host status recommends a limit.

Core checks from the repository root:

```sh
cargo check --offline --locked -j 1 -p dageist
cargo test --offline --locked -j 1 -p dageist --lib
cargo clippy --offline --locked -j 1 -p dageist --all-targets --no-deps -- -F unsafe-code -D warnings
cargo fmt --all -- --check
```

View checks from the repository root. `npm --prefix ui ci --offline` restores
`ui/node_modules/` from the local npm cache:

```sh
npm --prefix ui ci --offline
npm --prefix ui run check
npm --prefix ui run lint
npm --prefix ui run format:check
npm --prefix ui run build
```

The lint denies warnings and reports unused suppressions.
`npm --prefix ui run format` rewrites files; `format:check` only reports.

Host checks embed `ui/dist/`, so build the view first:

```sh
cargo test --offline --locked -j 1 -p dageist-tauri
cargo clippy --offline --locked -j 1 -p dageist-tauri --all-targets --no-deps -- -F unsafe-code -D warnings
```

`cargo run --offline --locked -p dageist-tauri` opens the native application
window and runs until it is closed. It is not a finite check; do not supervise
it as one.

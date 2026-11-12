# Dageist

Dageist is a Rust-first desktop studio for building dataframe pipelines as a
visual graph. Sources, joins, transforms, aggregations and outputs such as
tables, charts and file sinks become nodes on a canvas, connected in a directed
acyclic graph.

Rust owns the application's behavior. A host-independent core holds it, a Tauri 2
host provides the native window and IPC commands, and a React view
renders what the host reports. The current release opens the application window
on a step editor storyboard and a table viewer over a generated sample; the
graph model, nodes and pipeline execution are not implemented yet.

## Start

With Rust, Node.js and the locked dependencies already available locally, build
the view and open the application from the repository root:

```sh
npm --prefix ui run build
cargo run --offline --locked -p dageist-tauri
```

Expect a dark window titled `dageist` whose status badge reads `dageist 0.1.0`,
reported by the Rust core over IPC. Close the window to stop the application.
See [getting started](docs/getting-started.md) for setup and the next step.

## Documentation

| Guide | Purpose |
| --- | --- |
| [Getting started](docs/getting-started.md) | Prepare a checkout and open the application. |
| [How-to](docs/how-to.md) | Run focused checks and regenerate the application icons. |
| [Reference](docs/reference.md) | Find packages, IPC commands, view files and source paths. |
| [Explanation](docs/explanation.md) | Understand the core, host and view boundaries. |

## License And Attribution

Dageist is licensed under the [MIT License](LICENSE).
[Dependency attribution](NOTICE.md) links the accompanying upstream license
material for its direct dependencies.

# Dageist

[Dageist](https://github.com/bj-data-eng/dageist) is licensed under its own
[MIT License](LICENSE). This notice records attribution for the direct Rust
dependencies declared in
[crates/dageist-tauri/Cargo.toml](crates/dageist-tauri/Cargo.toml) and
[crates/dageist-engine/Cargo.toml](crates/dageist-engine/Cargo.toml) at the
versions resolved by [Cargo.lock](Cargo.lock). It does not enumerate their
transitive, optional, platform, or compiled dependencies, and it is not a
complete inventory for a distributed application bundle.

TypeScript, oxlint, oxlint-tsgolint, oxfmt and the Node type declarations,
declared in [ui/package.json](ui/package.json), are development-only tools.
The view in `ui/dist/` contains only this repository's compiled source, so
none of them is redistributed.

## Dependencies

This product depends on polars 0.55.2, distributed by Ritchie Vink, with some
portions by NVIDIA Corporation and affiliates:

* License: [MIT License](licenses/polars/LICENSE)
* Homepage: [Polars](https://www.pola.rs/)

This product depends on serde 1.0.229, distributed by Erick Tryzelaar and
David Tolnay, under your choice of either license:

* License: [Apache License 2.0](licenses/serde/LICENSE-APACHE)
* License: [MIT License](licenses/serde/LICENSE-MIT)
* Homepage: [Serde](https://serde.rs)

This product's tests depend on serde_json 1.0.154, from the same upstream
project, under your choice of either license:

* License: [Apache License 2.0](licenses/serde/LICENSE-APACHE)
* License: [MIT License](licenses/serde/LICENSE-MIT)
* Homepage: [serde_json](https://github.com/serde-rs/json)

This product depends on tauri 2.12.5 and its build-time companion tauri-build
2.7.4, distributed by the Tauri Programme within The Commons Conservancy, under
your choice of either license:

* License: [Apache License 2.0](licenses/tauri/LICENSE-APACHE-2.0)
* License: [MIT License](licenses/tauri/LICENSE-MIT)
* Homepage: [Tauri](https://tauri.app/)

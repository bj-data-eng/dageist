# dageist-fuzz Crate Guide

Read the [repository guide](../../AGENTS.md) for shared policy and workflow.
[Cargo.toml](Cargo.toml), [src/lib.rs](src/lib.rs) and [README.md](README.md)
own package facts, the public front door and capability/verification guidance.

This crate owns shared fuzz contracts, bounded generation mechanics, safe process execution, immutable
evidence, same-failure reduction and local campaign control. Semantic cases,
expectations, codecs, domain reductions and requirement references stay with
the consumer: fuzz-campaign cases in its `fuzz/` package, and property cases
built on the `property` feature in its ordinary tests through a
dev-dependency. Generic forests here are fixture topology, not a pipeline graph
or a table model. Benchmarking is outside the current boundary.

Production crates do not depend on this crate. Its optional property and
coverage features provide authoring adapters; ordinary builds do not link
libFuzzer. The controlled author fixture under `tests/fixtures/subject/` is a
standalone test workspace, with its own lockfile and no production membership.
It verifies the implementor workflow without adding product requirements.

## Command Inventory

Follow the repository's process discipline and Cargo job guidance. Framework
checks are:

```sh
cargo test --offline --locked -p dageist-fuzz
cargo test --offline --locked -p dageist-fuzz --all-features
cargo clippy --offline --locked -p dageist-fuzz --all-targets --all-features --no-deps -- -F unsafe-code -D warnings
```

Select campaign integration and real sanitizer coverage separately as described
in the [contributor guide](README.md). They compile isolated source bases and
control descendants; never point their output at another worker's checkout or
target directory. Use a clean source revision and explicit finite budgets.
Continuous coverage is an explicit single-worker mode composed of finite engine
rounds; tests must arrange bounded cancellation. Mutable session retention is
separate from immutable failure evidence.

Failure identity is the declared property, class and stable discriminator.
Unqualified crashes/panics cannot establish same-bug minimization. Retain exact
canonical bytes before evaluation; never call a seed or a libFuzzer entropy
input a concrete structured case. Missing evidence remains explicit.

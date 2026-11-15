//! Checks that the workspace builds an upstream project's crates exactly as
//! that project's own release build does: each crate with the same features,
//! and every package those crates use at the same version and from the same
//! source. What the workspace deliberately leaves out, or cannot avoid, is
//! listed in its [`Upstream`] description with the reason, and a listed
//! exception that no longer applies is reported too.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::RUSTDOC_TOOLCHAIN;

/// A feature of an upstream crate that the release turns on and the workspace leaves out.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeftOutFeature<'a> {
    pub krate: &'a str,
    pub feature: &'a str,
    pub reason: &'a str,
}

/// A package, named once with why it is listed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Listed<'a> {
    pub name: &'a str,
    pub reason: &'a str,
}

/// A package that upstream crates use, compiled with features that only one
/// of the builds turns on, and why.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpectedFeatures<'a> {
    pub package: &'a str,
    /// Features only the workspace's build turns on.
    pub only_here: &'a [&'a str],
    /// Features only the release build turns on.
    pub only_in_release: &'a [&'a str],
    pub reason: &'a str,
}

/// The upstream project whose release build the workspace builds like.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Upstream<'a> {
    /// The upstream workspace's directory, relative to the workspace root.
    pub root: &'a str,
    /// The package the upstream release builds.
    pub package: &'a str,
    /// Features the release turns on in upstream crates that the workspace leaves out.
    pub left_out_features: &'a [LeftOutFeature<'a>],
    /// Upstream crates the release builds and the workspace does not.
    pub left_out_crates: &'a [Listed<'a>],
    /// Packages the workspace's upstream crates use that the release build does not.
    pub extra_packages: &'a [Listed<'a>],
    /// Packages upstream crates use that one build compiles with features the other does not.
    pub expected_features: &'a [ExpectedFeatures<'a>],
}

/// Where a package comes from, as `cargo tree` names it.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Origin {
    /// The default registry.
    Registry,
    /// A path on disk.
    Path(PathBuf),
    /// Anything else, such as a git repository at a revision, as Cargo writes it.
    Other(String),
}

/// One package of a build.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub origin: Origin,
}

impl std::fmt::Display for Package {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} {}", self.name, self.version)?;
        match &self.origin {
            Origin::Registry => Ok(()),
            Origin::Path(path) => write!(formatter, " ({})", path.display()),
            Origin::Other(source) => write!(formatter, " ({source})"),
        }
    }
}

/// One package as one build compiles it: with a set of features. A package
/// compiled both for the program and for the build's own macros and scripts,
/// with different features, is two units.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Unit {
    pub package: Package,
    pub features: BTreeSet<String>,
}

/// A build's units and what each uses.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Graph {
    pub uses: BTreeMap<Unit, BTreeSet<Unit>>,
}

/// Reads one line of `cargo tree --prefix depth --format "{p}|{f}"`: a
/// depth, then `name vVERSION [(source)] [(proc-macro)]|features`, with
/// ` (*)` after a package whose dependencies were printed earlier.
pub fn parse_tree_line(line: &str) -> Option<(usize, Package, BTreeSet<String>)> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let depth = line[..digits].parse().ok()?;
    let line = line[digits..].trim_end().trim_end_matches(" (*)");
    let (package, features) = line.split_once('|')?;
    let package = package.trim_end().trim_end_matches(" (proc-macro)");
    let mut words = package.splitn(3, ' ');
    let name = words.next()?.to_owned();
    let version = words.next()?.strip_prefix('v')?.to_owned();
    let source = match words.next() {
        None => Origin::Registry,
        Some(rest) => {
            let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
            if inner.starts_with('/') {
                Origin::Path(PathBuf::from(inner))
            } else {
                Origin::Other(inner.to_owned())
            }
        }
    };
    let features = features
        .split(',')
        .map(str::trim)
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
        .collect();
    Some((
        depth,
        Package {
            name,
            version,
            origin: source,
        },
        features,
    ))
}

/// Rebuilds a build's graph from its `cargo tree` output. A unit's
/// dependencies are printed once, so every edge is still there.
pub fn parse_graph(tree: &str) -> Result<Graph, String> {
    let mut graph = Graph::default();
    let mut parents: Vec<Unit> = Vec::new();
    for line in tree.lines().filter(|line| !line.trim().is_empty()) {
        let (depth, package, features) =
            parse_tree_line(line).ok_or_else(|| format!("unreadable cargo tree line: {line}"))?;
        if depth > parents.len() {
            return Err(format!("cargo tree line deeper than its parent: {line}"));
        }
        parents.truncate(depth);
        let unit = Unit { package, features };
        if let Some(parent) = parents.last() {
            graph
                .uses
                .entry(parent.clone())
                .or_default()
                .insert(unit.clone());
        }
        graph.uses.entry(unit.clone()).or_default();
        parents.push(unit);
    }
    Ok(graph)
}

/// Returns whether `package` is one of the upstream project's own crates.
fn is_upstream(package: &Package, upstream_dir: &Path) -> bool {
    matches!(&package.origin, Origin::Path(path) if path.starts_with(upstream_dir))
}

/// Returns every package that is not an upstream crate and that an upstream
/// crate uses, directly or not.
fn used_by_upstream(graph: &Graph, upstream_dir: &Path) -> BTreeSet<Package> {
    reached_from_upstream(graph, upstream_dir, &|_| false)
        .into_iter()
        .map(|unit| unit.package.clone())
        .collect()
}

/// Returns every unit that is not an upstream crate and that an upstream
/// crate uses, directly or not, counting only the upstream crates that
/// `skipped` does not name.
fn reached_from_upstream<'g>(
    graph: &'g Graph,
    upstream_dir: &Path,
    skipped: &dyn Fn(&str) -> bool,
) -> BTreeSet<&'g Unit> {
    let mut seen: BTreeSet<&Unit> = BTreeSet::new();
    let mut waiting: Vec<&Unit> = graph
        .uses
        .keys()
        .filter(|unit| is_upstream(&unit.package, upstream_dir) && !skipped(&unit.package.name))
        .collect();
    while let Some(unit) = waiting.pop() {
        for used in graph.uses.get(unit).into_iter().flatten() {
            if seen.insert(used) {
                waiting.push(used);
            }
        }
    }
    seen.retain(|unit| !is_upstream(&unit.package, upstream_dir));
    seen
}

/// Returns the features each package is compiled with in `units`, counting
/// every copy of it together.
fn features_by_package<'g>(
    units: impl IntoIterator<Item = &'g Unit>,
) -> BTreeMap<&'g Package, BTreeSet<&'g str>> {
    let mut features: BTreeMap<&Package, BTreeSet<&str>> = BTreeMap::new();
    for unit in units {
        features
            .entry(&unit.package)
            .or_default()
            .extend(unit.features.iter().map(String::as_str));
    }
    features
}

/// Returns the features in `found` that `listed` does not expect.
fn unexpected<'f>(found: &BTreeSet<&'f str>, listed: Option<&[&str]>) -> Vec<&'f str> {
    found
        .iter()
        .filter(|feature| !listed.is_some_and(|listed| listed.contains(feature)))
        .copied()
        .collect()
}

/// Compares the features of the packages upstream crates use in both builds,
/// adding a problem for each difference that `upstream` does not expect and
/// for each expected one that no longer occurs. The release's own package
/// and the crates the workspace leaves out are not counted as users.
fn compare_shared_features(
    ours: &Graph,
    release: &Graph,
    upstream: &Upstream<'_>,
    upstream_dir: &Path,
    problems: &mut Vec<String>,
) {
    let skipped = |name: &str| {
        name == upstream.package
            || upstream
                .left_out_crates
                .iter()
                .any(|listed| listed.name == name)
    };
    let our_features = features_by_package(reached_from_upstream(ours, upstream_dir, &skipped));
    let release_reached =
        features_by_package(reached_from_upstream(release, upstream_dir, &skipped));
    let mut differences: BTreeMap<&str, (BTreeSet<&str>, BTreeSet<&str>)> = BTreeMap::new();
    for (package, ours) in &our_features {
        // A package the release's shared crates do not use is reported with the versions.
        let Some(theirs) = release_reached.get(package) else {
            continue;
        };
        let only_here = ours.difference(theirs).copied().collect::<BTreeSet<_>>();
        let only_in_release = theirs.difference(ours).copied().collect::<BTreeSet<_>>();
        let expected = upstream
            .expected_features
            .iter()
            .find(|expected| expected.package == package.name);
        let here = unexpected(&only_here, expected.map(|expected| expected.only_here));
        let there = unexpected(
            &only_in_release,
            expected.map(|expected| expected.only_in_release),
        );
        if !here.is_empty() {
            problems.push(format!(
                "{package} is compiled here with features the release does not turn on: {}",
                here.join(", ")
            ));
        }
        if !there.is_empty() {
            problems.push(format!(
                "{package} is compiled without features the release turns on: {}",
                there.join(", ")
            ));
        }
        differences.insert(package.name.as_str(), (only_here, only_in_release));
    }
    for expected in upstream.expected_features {
        let (only_here, only_in_release) = differences
            .get(expected.package)
            .cloned()
            .unwrap_or_default();
        for (listed, found, side) in [
            (expected.only_here, &only_here, "here"),
            (expected.only_in_release, &only_in_release, "in the release"),
        ] {
            for feature in listed {
                if !found.contains(feature) {
                    problems.push(format!(
                        "the expected feature {feature} of {} {side} no longer differs; remove it from the list",
                        expected.package
                    ));
                }
            }
        }
    }
}

/// The upstream crates of a build by name, with every set of features each
/// is compiled with.
type UpstreamCrates<'g> = BTreeMap<&'g str, Vec<&'g BTreeSet<String>>>;

fn upstream_crates<'g>(graph: &'g Graph, upstream_dir: &Path) -> UpstreamCrates<'g> {
    let mut crates = UpstreamCrates::new();
    for unit in graph
        .uses
        .keys()
        .filter(|unit| is_upstream(&unit.package, upstream_dir))
    {
        crates
            .entry(unit.package.name.as_str())
            .or_default()
            .push(&unit.features);
    }
    crates
}

/// Returns whether any compiled copy of `krate` has `feature`.
fn has_feature(crates: &UpstreamCrates<'_>, krate: &str, feature: &str) -> bool {
    crates
        .get(krate)
        .is_some_and(|sets| sets.iter().any(|features| features.contains(feature)))
}

/// Compares the features of one upstream crate in both builds, adding a
/// problem for each difference that is not listed in `upstream`.
fn compare_features(
    name: &str,
    release_sets: &[&BTreeSet<String>],
    our_sets: &[&BTreeSet<String>],
    upstream: &Upstream<'_>,
    problems: &mut Vec<String>,
) {
    let ([release_features], [our_features]) = (release_sets, our_sets) else {
        problems.push(format!(
            "{name} is compiled {} times by the release and {} times here",
            release_sets.len(),
            our_sets.len()
        ));
        return;
    };
    let left_out = |feature: &str| {
        upstream
            .left_out_features
            .iter()
            .any(|listed| listed.krate == name && listed.feature == feature)
    };
    let missing = release_features
        .iter()
        .filter(|feature| !our_features.contains(*feature) && !left_out(feature))
        .cloned()
        .collect::<Vec<_>>();
    let extra = our_features
        .iter()
        .filter(|feature| !release_features.contains(*feature))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        problems.push(format!(
            "{name} is built without features the release turns on: {}",
            missing.join(", ")
        ));
    }
    if !extra.is_empty() {
        problems.push(format!(
            "{name} is built with features the release does not turn on: {}",
            extra.join(", ")
        ));
    }
}

/// Compares the workspace's build with the release build and returns every
/// difference that is not listed in `upstream`, and every listed exception
/// that no longer applies. Both graphs name upstream crates by their paths
/// under `upstream_dir`.
pub fn compare(
    ours: &Graph,
    release: &Graph,
    upstream: &Upstream<'_>,
    upstream_dir: &Path,
) -> Vec<String> {
    let mut problems = Vec::new();
    let our_crates = upstream_crates(ours, upstream_dir);
    let release_crates = upstream_crates(release, upstream_dir);

    for (name, release_sets) in &release_crates {
        if *name == upstream.package {
            continue;
        }
        match our_crates.get(name) {
            Some(our_sets) => {
                compare_features(name, release_sets, our_sets, upstream, &mut problems);
            }
            None => {
                if !upstream
                    .left_out_crates
                    .iter()
                    .any(|listed| listed.name == *name)
                {
                    problems.push(format!("{name} is built by the release and not here"));
                }
            }
        }
    }
    for name in our_crates.keys() {
        if !release_crates.contains_key(name) {
            problems.push(format!("{name} is built here and not by the release"));
        }
    }

    let release_packages = release
        .uses
        .keys()
        .map(|unit| &unit.package)
        .collect::<BTreeSet<_>>();
    let ours_used = used_by_upstream(ours, upstream_dir);
    for package in &ours_used {
        if release_packages.contains(package)
            || upstream
                .extra_packages
                .iter()
                .any(|listed| listed.name == package.name)
        {
            continue;
        }
        let release_has = release_packages
            .iter()
            .filter(|candidate| candidate.name == package.name)
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        problems.push(if release_has.is_empty() {
            format!("{package} is used here and not by the release")
        } else {
            format!(
                "{package} is used here; the release uses {}",
                release_has.join(", ")
            )
        });
    }

    compare_shared_features(ours, release, upstream, upstream_dir, &mut problems);

    for listed in upstream.left_out_features {
        if !has_feature(&release_crates, listed.krate, listed.feature)
            || has_feature(&our_crates, listed.krate, listed.feature)
        {
            problems.push(format!(
                "the left-out feature {} of {} no longer applies; remove it from the list",
                listed.feature, listed.krate
            ));
        }
    }
    for listed in upstream.left_out_crates {
        if !release_crates.contains_key(listed.name) || our_crates.contains_key(listed.name) {
            problems.push(format!(
                "the left-out crate {} no longer applies; remove it from the list",
                listed.name
            ));
        }
    }
    for listed in upstream.extra_packages {
        let applies = ours_used
            .iter()
            .any(|package| package.name == listed.name && !release_packages.contains(package));
        if !applies {
            problems.push(format!(
                "the extra package {} no longer applies; remove it from the list",
                listed.name
            ));
        }
    }
    problems
}

/// Returns `package` with a path resolved through any links, so that a
/// crate is recognised under the upstream directory however it is reached.
fn resolved(package: &Package) -> Package {
    match &package.origin {
        Origin::Path(path) => Package {
            origin: Origin::Path(path.canonicalize().unwrap_or_else(|_| path.clone())),
            ..package.clone()
        },
        _ => package.clone(),
    }
}

/// Returns `graph` with every path resolved through any links.
fn with_resolved_paths(graph: &Graph) -> Graph {
    let unit = |unit: &Unit| Unit {
        package: resolved(&unit.package),
        features: unit.features.clone(),
    };
    Graph {
        uses: graph
            .uses
            .iter()
            .map(|(user, used)| (unit(user), used.iter().map(unit).collect()))
            .collect(),
    }
}

/// Returns the graph of a build of `package` in the workspace whose manifest is `manifest`.
fn build_graph(manifest: &Path, package: &str) -> Result<Graph, String> {
    let output = Command::new("rustup")
        .args([
            "run",
            RUSTDOC_TOOLCHAIN,
            "cargo",
            "tree",
            "--locked",
            "--manifest-path",
        ])
        .arg(manifest)
        .args([
            "--package",
            package,
            "--edges",
            "normal,build",
            "--prefix",
            "depth",
        ])
        .args(["--format", "{p}|{f}"])
        .output()
        .map_err(|error| format!("run cargo tree for {package}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo tree for {package} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_graph(&String::from_utf8_lossy(&output.stdout)).map(|graph| with_resolved_paths(&graph))
}

/// Returns the toolchain a `rust-toolchain.toml` names in its `channel`, if any.
pub fn toolchain_channel(text: &str) -> Option<&str> {
    text.lines().find_map(|line| {
        let value = line
            .trim()
            .strip_prefix("channel")?
            .trim_start()
            .strip_prefix('=')?;
        value.trim().strip_prefix('"')?.strip_suffix('"')
    })
}

/// Names the problems with the toolchains: the workspace's must be the
/// upstream release's when the release pins one, and must be the one this
/// generator runs Cargo with.
pub fn toolchain_problems(
    ours: Option<&str>,
    release: Option<&str>,
    generator: &str,
) -> Vec<String> {
    let mut problems = Vec::new();
    if let Some(release) = release {
        match ours {
            Some(ours) if ours == release => {}
            Some(ours) => problems.push(format!(
                "the workspace builds with {ours} and the release with {release}"
            )),
            None => problems.push(format!(
                "the workspace pins no toolchain and the release builds with {release}"
            )),
        }
    }
    if let Some(ours) = ours
        && ours != generator
    {
        problems.push(format!(
            "the workspace builds with {ours} and the generator runs Cargo with {generator}"
        ));
    }
    problems
}

/// Reads the channel of the `rust-toolchain.toml` in `dir`, or `None` when there is none.
fn read_channel(dir: &Path) -> Result<Option<String>, String> {
    let path = dir.join("rust-toolchain.toml");
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    toolchain_channel(&text)
        .map(|channel| Some(channel.to_owned()))
        .ok_or_else(|| format!("{} names no channel", path.display()))
}

/// Checks the build of `release_package` in the workspace at `root` against
/// the upstream release build: the toolchain, and the crates as [`compare`]
/// compares them.
pub fn check_release(
    root: &Path,
    release_package: &str,
    upstream: &Upstream<'_>,
) -> Result<(), String> {
    let upstream_dir = root.join(upstream.root);
    let upstream_manifest = upstream_dir.join("Cargo.toml");
    if !upstream_manifest.is_file() {
        return Err(format!(
            "{} is missing; fetch the upstream source first",
            upstream_manifest.display()
        ));
    }
    let upstream_dir = upstream_dir
        .canonicalize()
        .map_err(|error| format!("resolve {}: {error}", upstream_dir.display()))?;
    let ours = build_graph(&root.join("Cargo.toml"), release_package)?;
    let release = build_graph(&upstream_manifest, upstream.package)?;
    let mut problems = toolchain_problems(
        read_channel(root)?.as_deref(),
        read_channel(&upstream_dir)?.as_deref(),
        RUSTDOC_TOOLCHAIN,
    );
    problems.extend(compare(&ours, &release, upstream, &upstream_dir));
    if problems.is_empty() {
        println!(
            "{release_package} builds the crates of {} as {} does",
            upstream.root, upstream.package
        );
        return Ok(());
    }
    for problem in &problems {
        println!("{problem}");
    }
    Err(format!(
        "{release_package} does not build the crates of {} as {} does: {} differences",
        upstream.root,
        upstream.package,
        problems.len()
    ))
}

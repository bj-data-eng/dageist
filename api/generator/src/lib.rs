//! Generates and checks the public API audits of a workspace: its root crate,
//! each of its library crates, and the dependencies it builds on.
//!
//! An audit is the crate's public API as rustdoc JSON from a pinned nightly
//! toolchain describes it, one sorted line per item, as the `public-api`
//! crate writes it. A dependency is documented with the features a build of
//! the workspace's release package resolves for it, and its audit records the
//! dependency's version and those features, so an upgrade or a change of
//! features shows as a diff.

use std::path::{Path, PathBuf};
use std::process::Command;

mod release;

pub use release::{
    Graph, LeftOutFeature, Listed, Origin, Package, Unit, Upstream, check_release, compare,
    parse_graph, parse_tree_line,
};

/// The toolchain whose rustdoc writes the JSON, and whose Cargo reads the
/// builds. It is the toolchain the workspace and the upstream release build with.
const RUSTDOC_TOOLCHAIN: &str = "nightly-2026-09-01";

/// What the generator audits in a workspace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Workspace<'a> {
    /// The package at the workspace root, audited to `api/public-api.txt`.
    pub root_package: &'a str,
    /// The prefix of the directories under `crates/` that hold its crates.
    pub crate_prefix: &'a str,
    /// The package a release builds, whose resolved features a dependency is documented with.
    pub release_package: &'a str,
    /// The directory under `api/` that holds the dependencies' audits.
    pub dependency_directory: &'a str,
    /// The dependencies that are audited, by package name.
    pub dependencies: &'a [&'a str],
    /// The upstream project whose release build the workspace builds its dependencies like.
    pub upstream: Upstream<'a>,
}

/// Why the Python binding's own features are left out: they connect Polars to Python.
const PYTHON_GLUE: &str = "connects Polars to Python for the Python binding";

/// What dageist audits: its own crates, and the Polars crates whose API its steps are built from.
pub const DAGEIST: Workspace<'static> = Workspace {
    root_package: "dageist",
    crate_prefix: "dageist-",
    release_package: "dageist-tauri",
    dependency_directory: "polars",
    dependencies: &["polars", "polars-lazy", "polars-plan"],
    upstream: Upstream {
        root: "vendor/polars",
        package: "polars-runtime-32",
        left_out_features: &[
            LeftOutFeature {
                krate: "polars-core",
                feature: "python",
                reason: PYTHON_GLUE,
            },
            LeftOutFeature {
                krate: "polars-error",
                feature: "python",
                reason: PYTHON_GLUE,
            },
            LeftOutFeature {
                krate: "polars-io",
                feature: "pyo3",
                reason: PYTHON_GLUE,
            },
            LeftOutFeature {
                krate: "polars-io",
                feature: "python",
                reason: PYTHON_GLUE,
            },
            LeftOutFeature {
                krate: "polars-lazy",
                feature: "pyo3",
                reason: PYTHON_GLUE,
            },
            LeftOutFeature {
                krate: "polars-lazy",
                feature: "python",
                reason: PYTHON_GLUE,
            },
            LeftOutFeature {
                krate: "polars-mem-engine",
                feature: "pyo3",
                reason: PYTHON_GLUE,
            },
            LeftOutFeature {
                krate: "polars-mem-engine",
                feature: "python",
                reason: PYTHON_GLUE,
            },
            LeftOutFeature {
                krate: "polars-plan",
                feature: "python",
                reason: PYTHON_GLUE,
            },
            LeftOutFeature {
                krate: "polars-stream",
                feature: "pyo3",
                reason: PYTHON_GLUE,
            },
            LeftOutFeature {
                krate: "polars-stream",
                feature: "python",
                reason: PYTHON_GLUE,
            },
            LeftOutFeature {
                krate: "polars-utils",
                feature: "python",
                reason: PYTHON_GLUE,
            },
        ],
        left_out_crates: &[
            Listed {
                name: "polars-python",
                reason: "the Python binding itself",
            },
            Listed {
                name: "polars-io-ext-ffi",
                reason: "used only by the Python binding, to hand Iceberg reads to Python",
            },
        ],
        extra_packages: &[Listed {
            name: "objc2-exception-helper",
            reason: "Tauri turns on objc2's exception feature, and Polars' sysinfo uses objc2",
        }],
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArtifactCheck {
    Current,
    Different,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    Generate,
    Check,
    List,
    /// Checks the release package's build against the upstream release build.
    CheckRelease,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetSelection {
    All,
    Root,
    Dependencies,
    Crate(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cli {
    pub action: Action,
    pub selection: TargetSelection,
}

impl Cli {
    pub fn parse<I, S>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut args = args.into_iter().map(Into::into);
        let _program = args.next();

        let mut action = None;
        let mut selection = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--all" => set_selection(&mut selection, TargetSelection::All)?,
                "--root" => set_selection(&mut selection, TargetSelection::Root)?,
                "--polars" => set_selection(&mut selection, TargetSelection::Dependencies)?,
                "--crate" => {
                    let Some(name) = args.next() else {
                        return Err("--crate requires a crate name".to_owned());
                    };
                    if name.starts_with("--") {
                        return Err("--crate requires a crate name".to_owned());
                    }
                    set_selection(&mut selection, TargetSelection::Crate(name))?;
                }
                "--list" => set_action(&mut action, Action::List)?,
                "--check" => set_action(&mut action, Action::Check)?,
                "--check-release" => set_action(&mut action, Action::CheckRelease)?,
                unknown if unknown.starts_with("--") => {
                    return Err(format!("unknown flag {unknown}"));
                }
                unknown => {
                    return Err(format!("unexpected argument {unknown}"));
                }
            }
        }

        if action == Some(Action::CheckRelease) && selection.is_some() {
            return Err(
                "--check-release checks the whole build and takes no target selector".to_owned(),
            );
        }
        Ok(Self {
            action: action.unwrap_or(Action::Generate),
            selection: selection.unwrap_or(TargetSelection::All),
        })
    }
}

/// Where an audited package comes from, which decides the features it is documented with.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Source {
    /// A package of the workspace, documented with its default features.
    Workspace,
    /// A dependency, documented with the features a build of the release package resolves for it.
    Dependency,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiTarget {
    name: String,
    /// The package's directory, or the workspace root for a dependency.
    root: PathBuf,
    artifact: PathBuf,
    source: Source,
}

impl ApiTarget {
    pub fn new(
        name: impl Into<String>,
        root: impl Into<PathBuf>,
        artifact: impl Into<PathBuf>,
    ) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
            artifact: artifact.into(),
            source: Source::Workspace,
        }
    }

    /// An audit of the dependency `name` of the workspace at `root`.
    pub fn dependency(
        name: impl Into<String>,
        root: impl Into<PathBuf>,
        artifact: impl Into<PathBuf>,
    ) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
            artifact: artifact.into(),
            source: Source::Dependency,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("Cargo.toml")
    }

    pub fn artifact_path(&self) -> PathBuf {
        self.artifact.clone()
    }

    pub fn is_dependency(&self) -> bool {
        self.source == Source::Dependency
    }
}

/// Lists the audits of the workspace at `root`: its root package, then each
/// crate under `crates/` that has a library, by name, then its dependencies
/// in the order `workspace` gives them. A crate with only a binary has no
/// public API to audit.
pub fn discover_targets(root: &Path, workspace: &Workspace<'_>) -> Result<Vec<ApiTarget>, String> {
    let root_manifest = root.join("Cargo.toml");
    if !root_manifest.is_file() {
        return Err(format!("missing root manifest {}", root_manifest.display()));
    }

    let mut targets = vec![ApiTarget::new(
        workspace.root_package,
        root,
        root.join("api").join("public-api.txt"),
    )];
    let crates_dir = root.join("crates");

    if crates_dir.is_dir() {
        let mut crate_targets = Vec::new();
        for entry in std::fs::read_dir(&crates_dir).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let Some(name) = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            if !name.starts_with(workspace.crate_prefix) {
                continue;
            }
            if !path.is_dir() {
                continue;
            }
            let manifest = path.join("Cargo.toml");
            if !manifest.is_file() {
                return Err(format!(
                    "API target {name} is present under crates/ but missing {}; restore the crate manifest in the source workspace",
                    manifest.display()
                ));
            }
            if !path.join("src").join("lib.rs").is_file() {
                continue;
            }
            let artifact = root.join("api").join("crates").join(format!("{name}.txt"));
            crate_targets.push(ApiTarget::new(name, path, artifact));
        }
        crate_targets.sort_by(|left, right| left.name().cmp(right.name()));
        targets.extend(crate_targets);
    }

    for name in workspace.dependencies {
        targets.push(ApiTarget::dependency(
            *name,
            root,
            root.join("api")
                .join(workspace.dependency_directory)
                .join(format!("{name}.txt")),
        ));
    }

    Ok(targets)
}

pub fn select_targets(
    root: &Path,
    workspace: &Workspace<'_>,
    selection: TargetSelection,
) -> Result<Vec<ApiTarget>, String> {
    let targets = discover_targets(root, workspace)?;
    match selection {
        TargetSelection::All => Ok(targets),
        TargetSelection::Root => Ok(targets
            .into_iter()
            .filter(|target| !target.is_dependency() && target.name() == workspace.root_package)
            .collect()),
        TargetSelection::Dependencies => Ok(targets
            .into_iter()
            .filter(ApiTarget::is_dependency)
            .collect()),
        TargetSelection::Crate(name) => {
            let matches = targets
                .iter()
                .filter(|target| target.name() == name)
                .cloned()
                .collect::<Vec<_>>();
            if !matches.is_empty() {
                return Ok(matches);
            }
            let mut available = targets.iter().map(ApiTarget::name).collect::<Vec<_>>();
            available.dedup();
            Err(format!(
                "unknown API target {name}; available targets: {}",
                available.join(", "),
            ))
        }
    }
}

pub fn run<I, S>(root: &Path, workspace: &Workspace<'_>, args: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let cli = Cli::parse(args)?;

    match cli.action {
        Action::CheckRelease => check_release(root, workspace.release_package, &workspace.upstream),
        Action::List => {
            for target in select_targets(root, workspace, cli.selection)? {
                println!("{}", render_list_line(root, &target));
            }
            Ok(())
        }
        Action::Generate => {
            for target in select_targets(root, workspace, cli.selection)? {
                let generated = generate_target_artifact(&target, workspace)?;
                write_artifact(&target, &generated)?;
                println!("wrote {}", target.artifact_path().display());
            }
            Ok(())
        }
        Action::Check => {
            let mut stale = Vec::new();
            for target in select_targets(root, workspace, cli.selection)? {
                let generated = generate_target_artifact(&target, workspace)?;
                match compare_artifact(&target, &generated)? {
                    ArtifactCheck::Current => {
                        println!("current {}", target.artifact_path().display());
                    }
                    ArtifactCheck::Different | ArtifactCheck::Missing => {
                        stale.push(target.name().to_owned());
                    }
                }
            }

            if stale.is_empty() {
                Ok(())
            } else {
                Err(format!("stale API artifacts: {}", stale.join(", ")))
            }
        }
    }
}

pub fn render_list_line(root: &Path, target: &ApiTarget) -> String {
    if target.is_dependency() {
        return format!("{} dependency", target.name());
    }
    let relative = target
        .root()
        .strip_prefix(root)
        .ok()
        .filter(|path| !path.as_os_str().is_empty());
    let path = relative
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_owned());
    format!("{} {}", target.name(), path)
}

/// A package as Cargo resolves it in one build: its version and enabled features.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Resolved {
    pub version: String,
    /// Where it comes from, as Cargo writes it between brackets: a path or a git
    /// revision. `None` for the default registry.
    pub source: Option<String>,
    /// The enabled features, sorted.
    pub features: Vec<String>,
}

/// Finds the package `name` in the output of `cargo tree --prefix none
/// --format "{p}|{f}"` and returns its version and features, or `None` when
/// the tree does not hold it. A line names one package, as
/// `name vVERSION [(source)]|feature,feature`. The first line for `name` is
/// taken: should a tree hold `name` at two versions, the build of `name` on
/// its own is ambiguous and fails, so no audit is written from the wrong one.
pub fn find_resolved(tree: &str, name: &str) -> Option<Resolved> {
    tree.lines().find_map(|line| {
        let line = line.trim_end().trim_end_matches(" (*)");
        let (package, features) = line.split_once('|')?;
        let package = package.trim_end().trim_end_matches(" (proc-macro)");
        let mut words = package.splitn(3, ' ');
        if words.next()? != name {
            return None;
        }
        let version = words.next()?.strip_prefix('v')?.to_owned();
        let source = match words.next() {
            None => None,
            Some(rest) => Some(rest.strip_prefix('(')?.strip_suffix(')')?.to_owned()),
        };
        let mut features = features
            .split(',')
            .map(str::trim)
            .filter(|feature| !feature.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        features.sort();
        Some(Resolved {
            version,
            source,
            features,
        })
    })
}

/// Returns how the package `name` is resolved in a build of `spec` in the
/// workspace whose manifest is `manifest`. Only normal dependencies on the
/// host are followed, as `cargo rustdoc` builds the library for the host;
/// both sides of the release check are read the same way.
fn resolve_in_build(manifest: &Path, spec: &str, name: &str) -> Result<Resolved, String> {
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
        .args(["--package", spec, "--edges", "normal", "--prefix", "none"])
        .args(["--format", "{p}|{f}"])
        .output()
        .map_err(|error| format!("run cargo tree for {spec}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo tree for {spec} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let tree = String::from_utf8_lossy(&output.stdout);
    find_resolved(&tree, name)
        .ok_or_else(|| format!("a build of {spec} does not include the package {name}"))
}

/// Succeeds when the dependency `name` would be documented as a release
/// builds it, and otherwise says what differs.
pub fn require_release_features(
    name: &str,
    release_package: &str,
    released: &Resolved,
    documented: &Resolved,
) -> Result<(), String> {
    if released == documented {
        return Ok(());
    }
    let only = |from: &Resolved, other: &Resolved| {
        from.features
            .iter()
            .filter(|feature| !other.features.contains(feature))
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };
    Err(format!(
        "{name} would not be documented as a release builds it: the audit would document \
         {} and a build of {release_package} resolves {}; features only in the audit: [{}]; \
         features only in the release: [{}]",
        documented.version,
        released.version,
        only(documented, released),
        only(released, documented),
    ))
}

pub fn generate_target_artifact(
    target: &ApiTarget,
    workspace: &Workspace<'_>,
) -> Result<String, String> {
    // A dependency is documented only when what rustdoc builds is what a release builds.
    let resolved = if target.is_dependency() {
        let manifest = target.manifest_path();
        let released = resolve_in_build(&manifest, workspace.release_package, target.name())?;
        let documented = resolve_in_build(&manifest, target.name(), target.name())?;
        require_release_features(
            target.name(),
            workspace.release_package,
            &released,
            &documented,
        )?;
        Some(released)
    } else {
        None
    };

    let rustdoc_json = rustdoc_json::Builder::default()
        .toolchain(RUSTDOC_TOOLCHAIN)
        .manifest_path(target.manifest_path())
        .package(target.name())
        .build()
        .map_err(|error| format!("build rustdoc JSON for {}: {error}", target.name()))?;

    let public_api = public_api::Builder::from_rustdoc_json(rustdoc_json)
        .omit_blanket_impls(true)
        .omit_auto_trait_impls(true)
        .omit_auto_derived_impls(true)
        .build()
        .map_err(|error| format!("derive public API for {}: {error}", target.name()))?;

    let missing_item_ids = public_api
        .missing_item_ids()
        .map(u32::to_string)
        .collect::<Vec<_>>();

    Ok(match resolved {
        None => render_api_artifact(target.name(), &public_api.to_string(), &missing_item_ids),
        Some(resolved) => render_dependency_artifact(
            target.name(),
            workspace.release_package,
            &resolved,
            describe_source(target.root(), &workspace.upstream, &resolved)?.as_deref(),
            &public_api.to_string(),
            &missing_item_ids,
        ),
    })
}

/// Says where a dependency is built from, without anything particular to
/// this machine: the upstream checkout and its commit for a crate inside it,
/// or the git revision Cargo names. `None` for the default registry, or for
/// a path outside the upstream checkout.
fn describe_source(
    root: &Path,
    upstream: &Upstream<'_>,
    resolved: &Resolved,
) -> Result<Option<String>, String> {
    let Some(source) = &resolved.source else {
        return Ok(None);
    };
    if !source.starts_with('/') {
        return Ok(Some(source.clone()));
    }
    let checkout = root.join(upstream.root);
    let inside = checkout
        .canonicalize()
        .is_ok_and(|checkout| Path::new(source).starts_with(&checkout))
        || Path::new(source).starts_with(&checkout);
    if !inside {
        return Ok(None);
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&checkout)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("run git in {}: {error}", checkout.display()))?;
    if !output.status.success() {
        return Err(format!(
            "read the commit of {}: {}",
            checkout.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(Some(format!("{} at {commit}", upstream.root)))
}

pub fn render_api_artifact(name: &str, api: &str, missing_item_ids: &[String]) -> String {
    let mut output = String::new();
    output.push_str("# ");
    output.push_str(name);
    output.push_str(" public API\n");
    output.push_str("# generated by Dageist public API artifact tooling\n");
    push_missing(&mut output, missing_item_ids);
    output.push('\n');
    output.push_str(api);
    output
}

/// Writes a dependency's audit: its version, where it is built from when
/// that is not the default registry, and the features a release builds it
/// with, one to a line, before its public API.
pub fn render_dependency_artifact(
    name: &str,
    release_package: &str,
    resolved: &Resolved,
    source: Option<&str>,
    api: &str,
    missing_item_ids: &[String],
) -> String {
    let mut output = String::new();
    output.push_str("# ");
    output.push_str(name);
    output.push_str(" public API\n");
    output.push_str("# generated by Dageist public API artifact tooling\n");
    output.push_str("# version: ");
    output.push_str(&resolved.version);
    output.push('\n');
    if let Some(source) = source {
        output.push_str("# source: ");
        output.push_str(source);
        output.push('\n');
    }
    output.push_str("# features a build of ");
    output.push_str(release_package);
    output.push_str(" enables:");
    if resolved.features.is_empty() {
        output.push_str(" none\n");
    } else {
        output.push('\n');
        for feature in &resolved.features {
            output.push_str("#   ");
            output.push_str(feature);
            output.push('\n');
        }
    }
    push_missing(&mut output, missing_item_ids);
    output.push('\n');
    output.push_str(api);
    output
}

fn push_missing(output: &mut String, missing_item_ids: &[String]) {
    if !missing_item_ids.is_empty() {
        output.push_str("# missing rustdoc item IDs: ");
        output.push_str(&missing_item_ids.join(", "));
        output.push('\n');
    }
}

pub fn compare_artifact(target: &ApiTarget, generated: &str) -> Result<ArtifactCheck, String> {
    let artifact = target.artifact_path();
    if !artifact.exists() {
        return Ok(ArtifactCheck::Missing);
    }
    let current = std::fs::read_to_string(&artifact)
        .map_err(|error| format!("read {}: {error}", artifact.display()))?;
    if current == generated {
        Ok(ArtifactCheck::Current)
    } else {
        Ok(ArtifactCheck::Different)
    }
}

pub fn write_artifact(target: &ApiTarget, generated: &str) -> Result<(), String> {
    let artifact = target.artifact_path();
    let directory = artifact
        .parent()
        .ok_or_else(|| format!("artifact path has no parent: {}", artifact.display()))?;
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    std::fs::write(&artifact, generated)
        .map_err(|error| format!("write {}: {error}", artifact.display()))
}

fn set_action(action: &mut Option<Action>, value: Action) -> Result<(), String> {
    if action.replace(value).is_some() {
        return Err("choose only one action".to_owned());
    }
    Ok(())
}

fn set_selection(
    selection: &mut Option<TargetSelection>,
    value: TargetSelection,
) -> Result<(), String> {
    if selection.replace(value).is_some() {
        return Err("choose only one target selector".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A workspace laid out as dageist's is, under the fixture's own names.
    const FIXTURE: Workspace<'static> = Workspace {
        root_package: "app",
        crate_prefix: "app-",
        release_package: "app-host",
        dependency_directory: "deps",
        dependencies: &["dep-facade", "dep"],
        upstream: NO_UPSTREAM,
    };

    /// An upstream that a fixture does not check out.
    const NO_UPSTREAM: Upstream<'static> = Upstream {
        root: "vendor/up",
        package: "up-runtime",
        left_out_features: &[],
        left_out_crates: &[],
        extra_packages: &[],
    };

    #[test]
    fn no_args_defaults_to_all_generation() {
        let cli = Cli::parse(["generator"]).unwrap();

        assert_eq!(cli.selection, TargetSelection::All);
        assert_eq!(cli.action, Action::Generate);
    }

    #[test]
    fn parses_single_crate_check() {
        let cli = Cli::parse(["generator", "--check", "--crate", "dageist-engine"]).unwrap();

        assert_eq!(
            cli.selection,
            TargetSelection::Crate("dageist-engine".to_owned())
        );
        assert_eq!(cli.action, Action::Check);
    }

    #[test]
    fn parses_the_polars_selection() {
        let cli = Cli::parse(["generator", "--polars", "--list"]).unwrap();

        assert_eq!(cli.selection, TargetSelection::Dependencies);
        assert_eq!(cli.action, Action::List);
    }

    #[test]
    fn rejects_conflicting_target_selection() {
        for args in [
            ["generator", "--root", "--crate", "dageist-engine"],
            ["generator", "--polars", "--crate", "dageist-engine"],
        ] {
            let error = Cli::parse(args).unwrap_err();

            assert!(error.contains("choose only one target selector"));
        }
    }

    #[test]
    fn rejects_missing_crate_name() {
        for args in [
            vec!["generator", "--crate"],
            vec!["generator", "--crate", "--check"],
        ] {
            let error = Cli::parse(args).unwrap_err();

            assert!(error.contains("--crate requires a crate name"));
        }
    }

    #[test]
    fn rejects_unknown_flags() {
        let error = Cli::parse(["generator", "--target", "dageist-engine"]).unwrap_err();

        assert!(error.contains("unknown flag --target"));
    }

    #[test]
    fn the_release_check_takes_no_target_selector() {
        let cli = Cli::parse(["generator", "--check-release"]).unwrap();
        assert_eq!(cli.action, Action::CheckRelease);

        let error = Cli::parse(["generator", "--check-release", "--polars"]).unwrap_err();
        assert!(error.contains("takes no target selector"));
    }

    #[test]
    fn rejects_conflicting_actions() {
        let error = Cli::parse(["generator", "--list", "--check"]).unwrap_err();

        assert!(error.contains("choose only one action"));
    }

    #[test]
    fn artifact_path_is_root_owned_for_crate_target() {
        let target = ApiTarget::new(
            "dageist-engine",
            PathBuf::from("crates/dageist-engine"),
            PathBuf::from("api/crates/dageist-engine.txt"),
        );

        assert_eq!(
            target.manifest_path(),
            PathBuf::from("crates/dageist-engine/Cargo.toml")
        );
        assert_eq!(
            target.artifact_path(),
            PathBuf::from("api/crates/dageist-engine.txt")
        );
    }

    #[test]
    fn discovers_root_then_library_crates_by_name_then_dependencies_in_order() {
        let fixture = TempFixture::new("dageist-api-targets");
        fixture.file("Cargo.toml", "[package]\nname = \"app\"\n");
        fixture.library("crates/app-zeta", "app-zeta");
        fixture.library("crates/app-alpha", "app-alpha");
        fixture.file(
            "crates/app-host/Cargo.toml",
            "[package]\nname = \"app-host\"\n",
        );
        fixture.file("crates/app-host/src/main.rs", "fn main() {}\n");
        fixture.library("crates/other", "other");

        let targets = discover_targets(fixture.path(), &FIXTURE).unwrap();
        let listed = targets
            .iter()
            .map(|target| render_list_line(fixture.path(), target))
            .collect::<Vec<_>>();

        assert_eq!(
            listed,
            [
                "app .",
                "app-alpha crates/app-alpha",
                "app-zeta crates/app-zeta",
                "dep-facade dependency",
                "dep dependency",
            ],
            "a crate with only a binary and a directory without the prefix are not audited",
        );
    }

    #[test]
    fn a_dependency_is_documented_from_the_workspace_root_into_its_own_directory() {
        let fixture = TempFixture::new("dageist-api-dependency-target");
        fixture.file("Cargo.toml", "[package]\nname = \"app\"\n");

        let targets = select_targets(
            fixture.path(),
            &FIXTURE,
            TargetSelection::Crate("dep".to_owned()),
        )
        .unwrap();

        assert_eq!(
            targets,
            [ApiTarget::dependency(
                "dep",
                fixture.path(),
                fixture.path().join("api/deps/dep.txt"),
            )],
        );
        assert_eq!(
            targets[0].manifest_path(),
            fixture.path().join("Cargo.toml")
        );
    }

    #[test]
    fn selections_pick_the_root_the_dependencies_or_one_crate() {
        let fixture = TempFixture::new("dageist-api-selection");
        fixture.file("Cargo.toml", "[package]\nname = \"app\"\n");
        fixture.library("crates/app-engine", "app-engine");

        let names = |selection| {
            select_targets(fixture.path(), &FIXTURE, selection)
                .unwrap()
                .iter()
                .map(|target| target.name().to_owned())
                .collect::<Vec<_>>()
        };

        assert_eq!(names(TargetSelection::Root), ["app"]);
        assert_eq!(names(TargetSelection::Dependencies), ["dep-facade", "dep"]);
        assert_eq!(
            names(TargetSelection::Crate("app-engine".to_owned())),
            ["app-engine"]
        );
        assert_eq!(
            names(TargetSelection::All),
            ["app", "app-engine", "dep-facade", "dep"]
        );
    }

    #[test]
    fn ignores_matching_files_under_crates_directory() {
        let fixture = TempFixture::new("dageist-api-target-files");
        fixture.file("Cargo.toml", "[package]\nname = \"app\"\n");
        fixture.file("crates/app-not-a-directory", "not a crate directory\n");

        let targets = select_targets(fixture.path(), &FIXTURE, TargetSelection::All).unwrap();
        let names = targets
            .iter()
            .filter(|target| !target.is_dependency())
            .map(ApiTarget::name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["app"]);
    }

    #[test]
    fn selecting_missing_crate_reports_available_targets() {
        let fixture = TempFixture::new("dageist-api-missing-target");
        fixture.file("Cargo.toml", "[package]\nname = \"app\"\n");
        fixture.library("crates/app-engine", "app-engine");

        let error = select_targets(
            fixture.path(),
            &FIXTURE,
            TargetSelection::Crate("missing".to_owned()),
        )
        .unwrap_err();

        assert_eq!(
            error,
            "unknown API target missing; available targets: app, app-engine, dep-facade, dep",
        );
    }

    #[test]
    fn crate_directory_without_manifest_reports_missing_source_manifest() {
        let fixture = TempFixture::new("dageist-api-missing-crate-manifest");
        fixture.file("Cargo.toml", "[package]\nname = \"app\"\n");
        std::fs::create_dir_all(fixture.path().join("crates/app-engine")).unwrap();

        let error = discover_targets(fixture.path(), &FIXTURE).unwrap_err();
        let manifest = fixture.path().join("crates/app-engine/Cargo.toml");

        assert!(error.contains("app-engine"));
        assert!(error.contains(&manifest.display().to_string()));
        assert!(error.contains("restore the crate manifest in the source workspace"));
    }

    #[test]
    fn missing_root_manifest_is_reported() {
        let fixture = TempFixture::new("dageist-api-missing-root");

        let error = discover_targets(fixture.path(), &FIXTURE).unwrap_err();

        assert!(error.starts_with("missing root manifest"));
    }

    #[test]
    fn finds_a_package_by_its_whole_name_in_a_cargo_tree() {
        let tree = concat!(
            "app-host v0.1.0 (/work/crates/app-host)|\n",
            "polars v0.55.2|lazy,parquet\n",
            "polars-plan v0.55.2|strings,abs,cse\n",
            "polars-plan v0.55.2|strings,abs,cse (*)\n",
        );

        assert_eq!(
            find_resolved(tree, "polars-plan"),
            Some(Resolved {
                version: "0.55.2".to_owned(),
                source: None,
                features: vec!["abs".to_owned(), "cse".to_owned(), "strings".to_owned()],
            }),
        );
        assert_eq!(
            find_resolved(tree, "polars"),
            Some(Resolved {
                version: "0.55.2".to_owned(),
                source: None,
                features: vec!["lazy".to_owned(), "parquet".to_owned()],
            }),
            "a name that starts another package's name must not match it",
        );
        assert_eq!(
            find_resolved(tree, "app-host"),
            Some(Resolved {
                version: "0.1.0".to_owned(),
                source: Some("/work/crates/app-host".to_owned()),
                features: Vec::new(),
            }),
        );
        assert_eq!(find_resolved(tree, "polars-lazy"), None);
    }

    #[test]
    fn features_that_differ_from_the_release_are_named() {
        let released = Resolved {
            version: "0.55.2".to_owned(),
            source: None,
            features: vec!["abs".to_owned(), "strings".to_owned()],
        };
        let documented = Resolved {
            version: "0.55.2".to_owned(),
            source: None,
            features: vec!["cum_agg".to_owned(), "strings".to_owned()],
        };

        assert_eq!(
            require_release_features("polars-plan", "app-host", &released, &released),
            Ok(())
        );
        let error = require_release_features("polars-plan", "app-host", &released, &documented)
            .unwrap_err();
        assert!(error.contains("polars-plan would not be documented as a release builds it"));
        assert!(error.contains("a build of app-host"));
        assert!(error.contains("features only in the audit: [cum_agg]"));
        assert!(error.contains("features only in the release: [abs]"));
    }

    #[test]
    fn list_line_formats_root_as_dot() {
        let fixture = TempFixture::new("dageist-api-list-root");
        let target = ApiTarget::new(
            "dageist",
            fixture.path(),
            fixture.path().join("api/public-api.txt"),
        );

        assert_eq!(render_list_line(fixture.path(), &target), "dageist .");
    }

    #[test]
    fn render_api_artifact_uses_target_name_header() {
        let artifact = render_api_artifact(
            "dageist-engine",
            "pub struct Engine;\n",
            &["2608".to_owned(), "2605".to_owned()],
        );

        assert_eq!(
            artifact,
            concat!(
                "# dageist-engine public API\n",
                "# generated by Dageist public API artifact tooling\n",
                "# missing rustdoc item IDs: 2608, 2605\n",
                "\n",
                "pub struct Engine;\n",
            ),
        );
    }

    #[test]
    fn a_dependency_artifact_records_its_version_and_release_features() {
        let resolved = Resolved {
            version: "0.55.2".to_owned(),
            source: None,
            features: vec!["abs".to_owned(), "strings".to_owned()],
        };

        let artifact = render_dependency_artifact(
            "polars-plan",
            "dageist-tauri",
            &resolved,
            Some("vendor/polars at 7f902bd3ffd27ff82d14278a09a295ff2f7aed92"),
            "pub enum polars_plan::dsl::Expr\n",
            &["4".to_owned()],
        );

        assert_eq!(
            artifact,
            concat!(
                "# polars-plan public API\n",
                "# generated by Dageist public API artifact tooling\n",
                "# version: 0.55.2\n",
                "# source: vendor/polars at 7f902bd3ffd27ff82d14278a09a295ff2f7aed92\n",
                "# features a build of dageist-tauri enables:\n",
                "#   abs\n",
                "#   strings\n",
                "# missing rustdoc item IDs: 4\n",
                "\n",
                "pub enum polars_plan::dsl::Expr\n",
            ),
        );
        let without = render_dependency_artifact(
            "dep",
            "app-host",
            &Resolved {
                version: "1.0.0".to_owned(),
                source: None,
                features: Vec::new(),
            },
            None,
            "pub struct dep::Shared\n",
            &[],
        );
        assert!(
            without.contains("# version: 1.0.0\n# features a build of app-host enables: none\n\n")
        );
    }

    #[test]
    fn check_artifact_reports_difference_without_writing() {
        let fixture = TempFixture::new("dageist-api-check-different");
        fixture.file("api/public-api.txt", "old artifact\n");
        let target = ApiTarget::new(
            "dageist",
            fixture.path(),
            fixture.path().join("api/public-api.txt"),
        );

        let check = compare_artifact(&target, "new artifact\n").unwrap();

        assert_eq!(check, ArtifactCheck::Different);
        assert_eq!(
            std::fs::read_to_string(target.artifact_path()).unwrap(),
            "old artifact\n"
        );
        assert_eq!(
            compare_artifact(&target, "old artifact\n").unwrap(),
            ArtifactCheck::Current
        );
    }

    #[test]
    fn check_artifact_reports_a_missing_artifact() {
        let fixture = TempFixture::new("dageist-api-check-missing");
        let target = ApiTarget::new(
            "dageist",
            fixture.path(),
            fixture.path().join("api/public-api.txt"),
        );

        assert_eq!(
            compare_artifact(&target, "artifact\n").unwrap(),
            ArtifactCheck::Missing
        );
    }

    #[test]
    fn write_artifact_creates_api_directory() {
        let fixture = TempFixture::new("dageist-api-write");
        let target = ApiTarget::dependency(
            "polars-plan",
            fixture.path(),
            fixture.path().join("api/polars/polars-plan.txt"),
        );

        write_artifact(&target, "generated artifact\n").unwrap();

        assert_eq!(
            std::fs::read_to_string(fixture.path().join("api/polars/polars-plan.txt")).unwrap(),
            "generated artifact\n"
        );
    }

    /// Lays out a workspace whose release package depends on `dep`, a crate
    /// outside the workspace, as a registry crate is. The release turns on
    /// `wanted`; `dep` turns on `base` by default; nothing turns on `unwanted`.
    /// `extra_member` adds a second member that turns on what it names.
    fn dependency_fixture(name: &str, extra_member: Option<&str>) -> TempFixture {
        let fixture = TempFixture::new(name);
        fixture.file(
            "dep/Cargo.toml",
            concat!(
                "[package]\nname = \"dep\"\nversion = \"1.2.3\"\nedition = \"2024\"\n",
                "[features]\ndefault = [\"base\"]\nbase = []\nwanted = []\nunwanted = []\n",
            ),
        );
        fixture.file(
            "dep/src/lib.rs",
            concat!(
                "pub struct Shared;\n",
                "#[cfg(feature = \"base\")]\npub struct BaseOnly;\n",
                "#[cfg(feature = \"wanted\")]\npub struct WantedOnly;\n",
                "#[cfg(feature = \"unwanted\")]\npub struct UnwantedOnly;\n",
            ),
        );
        let members = if extra_member.is_some() {
            "[\"crates/app-host\", \"crates/app-other\"]"
        } else {
            "[\"crates/app-host\"]"
        };
        fixture.file(
            "ws/Cargo.toml",
            &format!(
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
                 [workspace]\nmembers = {members}\nresolver = \"3\"\n"
            ),
        );
        fixture.file("ws/src/lib.rs", "pub struct App;\n");
        fixture.file(
            "ws/crates/app-host/Cargo.toml",
            concat!(
                "[package]\nname = \"app-host\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
                "[dependencies]\ndep = { path = \"../../../dep\", features = [\"wanted\"] }\n",
            ),
        );
        fixture.file("ws/crates/app-host/src/main.rs", "fn main() {}\n");
        if let Some(feature) = extra_member {
            fixture.file(
                "ws/crates/app-other/Cargo.toml",
                &format!(
                    "[package]\nname = \"app-other\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
                     [dependencies]\ndep = {{ path = \"../../../dep\", features = [\"{feature}\"] }}\n"
                ),
            );
            fixture.file("ws/crates/app-other/src/main.rs", "fn main() {}\n");
        }
        // The generator runs Cargo with `--locked`, so the workspace needs its lock file first.
        let status = Command::new("cargo")
            .args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(fixture.path().join("ws/Cargo.toml"))
            .status()
            .unwrap();
        assert!(status.success(), "the fixture's lock file must be written");
        fixture
    }

    /// The fixture's workspace, auditing only `dep`.
    const DEPENDENCY_FIXTURE: Workspace<'static> = Workspace {
        root_package: "app",
        crate_prefix: "app-",
        release_package: "app-host",
        dependency_directory: "deps",
        dependencies: &["dep"],
        upstream: NO_UPSTREAM,
    };

    #[test]
    #[ignore = "runs pinned nightly rustdoc on a fixture workspace; select explicitly with offline Cargo"]
    fn a_dependency_is_documented_with_the_features_the_release_turns_on() {
        let fixture = dependency_fixture("dageist-api-dependency-rustdoc", None);
        let root = fixture.path().join("ws");

        run(&root, &DEPENDENCY_FIXTURE, ["generator", "--polars"]).unwrap();

        let path = root.join("api/deps/dep.txt");
        let artifact = std::fs::read_to_string(&path).unwrap();
        assert!(artifact.starts_with(concat!(
            "# dep public API\n",
            "# generated by Dageist public API artifact tooling\n",
            "# version: 1.2.3\n",
            "# features a build of app-host enables:\n",
            "#   base\n",
            "#   default\n",
            "#   wanted\n",
        )));
        for item in ["Shared", "BaseOnly", "WantedOnly"] {
            assert!(
                artifact
                    .lines()
                    .any(|line| line == format!("pub struct dep::{item}")),
                "{item} is built by the release and must be in the audit",
            );
        }
        assert!(!artifact.contains("UnwantedOnly"));

        run(
            &root,
            &DEPENDENCY_FIXTURE,
            ["generator", "--check", "--polars"],
        )
        .unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            run(
                &root,
                &DEPENDENCY_FIXTURE,
                ["generator", "--check", "--polars"]
            )
            .unwrap_err(),
            "stale API artifacts: dep",
        );
        assert!(!path.exists(), "a check must not write the artifact");
    }

    #[test]
    #[ignore = "runs pinned nightly rustdoc on a fixture workspace; select explicitly with offline Cargo"]
    fn a_dependency_is_not_documented_with_features_the_release_does_not_build() {
        let fixture = dependency_fixture("dageist-api-dependency-mismatch", Some("unwanted"));
        let root = fixture.path().join("ws");

        let error = run(&root, &DEPENDENCY_FIXTURE, ["generator", "--polars"]).unwrap_err();

        assert!(error.contains("dep would not be documented as a release builds it"));
        assert!(error.contains("features only in the audit: [unwanted]"));
        assert!(!root.join("api/deps/dep.txt").exists());
    }

    /// The upstream directory the release-check tests name crates under.
    const UP: &str = "/work/vendor/up";

    /// An upstream whose release turns on `glue` in `up-core`, which the workspace leaves out.
    const UPSTREAM: Upstream<'static> = Upstream {
        root: "vendor/up",
        package: "up-runtime",
        left_out_features: &[LeftOutFeature {
            krate: "up-core",
            feature: "glue",
            reason: "test",
        }],
        left_out_crates: &[Listed {
            name: "up-binding",
            reason: "test",
        }],
        extra_packages: &[Listed {
            name: "host-only",
            reason: "test",
        }],
    };

    /// The release build: its runtime uses `up-core` and `up-binding`, which use registry crates.
    const RELEASE_TREE: &str = concat!(
        "0up-runtime v1.0.0 (/work/vendor/up/runtime)|default\n",
        "1up-core v1.0.0 (/work/vendor/up/core)|a,glue\n",
        "2hashbrown v0.17.1 (https://example.com/fork?rev=1#1)|default\n",
        "2libc v0.2.1|std\n",
        "1up-binding v1.0.0 (/work/vendor/up/binding)|\n",
        "2libc v0.2.1|std (*)\n",
    );

    /// The workspace's build: its host uses `up-core`, which here also uses
    /// `host-only` because a crate of the host turns on a feature that needs
    /// it, and `tauri`, which the check does not look at.
    fn our_tree(core: &str, extra: &str) -> String {
        format!(
            "0app-host v0.1.0 (/work/crates/app-host)|\n\
             1up-core v1.0.0 (/work/vendor/up/core)|{core}\n\
             2hashbrown v0.17.1 (https://example.com/fork?rev=1#1)|default\n\
             2libc v0.2.1|std\n\
             2host-only v1.0.0|\n\
             {extra}\
             1tauri v2.0.0|\n\
             2libc v0.2.1|std (*)\n\
             2tauri-only v1.0.0|\n"
        )
    }

    fn compared(ours: &str) -> Vec<String> {
        compare(
            &parse_graph(ours).unwrap(),
            &parse_graph(RELEASE_TREE).unwrap(),
            &UPSTREAM,
            Path::new(UP),
        )
    }

    #[test]
    fn reads_a_tree_line_of_each_source() {
        let (depth, package, features) =
            parse_tree_line("3hashbrown v0.17.1 (https://example.com/fork?rev=1#1)|a,b (*)")
                .unwrap();
        assert_eq!(depth, 3);
        assert_eq!(package.name, "hashbrown");
        assert_eq!(package.version, "0.17.1");
        assert_eq!(
            package.origin,
            Origin::Other("https://example.com/fork?rev=1#1".to_owned())
        );
        assert_eq!(features.into_iter().collect::<Vec<_>>(), ["a", "b"]);

        let (_, package, _) =
            parse_tree_line("12serde_derive v1.0.229 (proc-macro)|default").unwrap();
        assert_eq!(package.origin, Origin::Registry);
        assert_eq!(package.name, "serde_derive");

        let (_, package, features) = parse_tree_line("0up v1.0.0 (/work/vendor/up)|").unwrap();
        assert_eq!(
            package.origin,
            Origin::Path(PathBuf::from("/work/vendor/up"))
        );
        assert!(features.is_empty());

        assert!(parse_tree_line("[build-dependencies]").is_none());
    }

    #[test]
    fn a_graph_keeps_the_dependencies_of_a_package_printed_once() {
        let graph = parse_graph(RELEASE_TREE).unwrap();
        let binding = graph
            .uses
            .keys()
            .find(|unit| unit.package.name == "up-binding")
            .unwrap();

        assert!(
            graph.uses[binding]
                .iter()
                .any(|unit| unit.package.name == "libc"),
            "a repeated package is still used where it repeats"
        );
        assert_eq!(graph.uses.len(), 5);
        assert!(
            parse_graph("2deep v1.0.0|\n").is_err(),
            "a line deeper than its parent is refused"
        );
    }

    #[test]
    fn a_package_compiled_twice_with_different_features_is_two_units() {
        // The program's digest uses nothing; the build-time copy of it uses block-buffer.
        let graph = parse_graph(concat!(
            "0app v0.1.0|\n",
            "1digest v0.10.7|alloc\n",
            "1codegen v1.0.0|\n",
            "2digest v0.10.7|alloc,block-buffer\n",
            "3block-buffer v0.10.4|\n",
        ))
        .unwrap();

        let digests = graph
            .uses
            .iter()
            .filter(|(unit, _)| unit.package.name == "digest")
            .map(|(_, used)| used.len())
            .collect::<Vec<_>>();
        assert_eq!(digests, [0, 1]);
    }

    #[test]
    fn a_build_that_matches_the_release_apart_from_listed_exceptions_passes() {
        assert_eq!(compared(&our_tree("a", "")), Vec::<String>::new());
    }

    #[test]
    fn features_built_unlike_the_release_are_named() {
        let problems = compared(&our_tree("a,b", ""));
        assert_eq!(
            problems,
            ["up-core is built with features the release does not turn on: b"]
        );

        let problems = compared(&our_tree("", ""));
        assert!(
            problems
                .contains(&"up-core is built without features the release turns on: a".to_owned())
        );
    }

    #[test]
    fn a_package_at_another_version_or_source_is_named_with_the_release_version() {
        let ours = our_tree("a", "").replace(
            "2hashbrown v0.17.1 (https://example.com/fork?rev=1#1)",
            "2hashbrown v0.17.1",
        );

        assert_eq!(
            compared(&ours),
            [
                "hashbrown 0.17.1 is used here; the release uses hashbrown 0.17.1 (https://example.com/fork?rev=1#1)"
            ],
        );
    }

    #[test]
    fn a_package_the_release_does_not_use_is_named() {
        let problems = compared(&our_tree("a", "2zstd v0.13.4|\n"));

        assert_eq!(
            problems,
            ["zstd 0.13.4 is used here and not by the release"]
        );
    }

    #[test]
    fn a_crate_of_the_workspace_outside_the_upstream_crates_is_not_compared() {
        // tauri and tauri-only are not used by an upstream crate, so they are not checked.
        let problems = compared(&our_tree("a", ""));

        assert!(problems.iter().all(|problem| !problem.contains("tauri")));
    }

    #[test]
    fn listed_exceptions_that_no_longer_apply_are_named() {
        // The release no longer turns on glue, builds up-binding, or differs on host-only.
        let release = RELEASE_TREE.replace("|a,glue\n", "|a\n").replace(
            "1up-binding v1.0.0 (/work/vendor/up/binding)|\n2libc v0.2.1|std (*)\n",
            "",
        );
        let ours = our_tree("a", "").replace("2host-only v1.0.0|\n", "");
        let problems = compare(
            &parse_graph(&ours).unwrap(),
            &parse_graph(&release).unwrap(),
            &UPSTREAM,
            Path::new(UP),
        );

        assert_eq!(
            problems,
            [
                "the left-out feature glue of up-core no longer applies; remove it from the list",
                "the left-out crate up-binding no longer applies; remove it from the list",
                "the extra package host-only no longer applies; remove it from the list",
            ],
        );
    }

    #[test]
    fn a_crate_the_release_builds_and_the_workspace_does_not_must_be_listed() {
        let upstream = Upstream {
            left_out_crates: &[],
            ..UPSTREAM
        };

        let problems = compare(
            &parse_graph(&our_tree("a", "")).unwrap(),
            &parse_graph(RELEASE_TREE).unwrap(),
            &upstream,
            Path::new(UP),
        );

        assert_eq!(
            problems,
            ["up-binding is built by the release and not here"]
        );
    }

    /// Lays out a workspace whose host builds `up-core` from an upstream
    /// workspace under `vendor/up`, whose own runtime builds it with `a` and
    /// `glue`. The host turns on `host_features`.
    fn release_fixture(name: &str, host_features: &str) -> TempFixture {
        let fixture = TempFixture::new(name);
        fixture.file(
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [workspace]\nmembers = [\"crates/app-host\"]\nexclude = [\"vendor\"]\nresolver = \"3\"\n",
        );
        fixture.file("src/lib.rs", "pub struct App;\n");
        fixture.file(
            "crates/app-host/Cargo.toml",
            &format!(
                "[package]\nname = \"app-host\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
                 [dependencies]\nup-core = {{ path = \"../../vendor/up/core\", features = [{host_features}] }}\n"
            ),
        );
        fixture.file("crates/app-host/src/main.rs", "fn main() {}\n");
        fixture.file(
            "vendor/up/Cargo.toml",
            "[workspace]\nmembers = [\"core\", \"runtime\"]\nresolver = \"3\"\n",
        );
        fixture.file(
            "vendor/up/core/Cargo.toml",
            "[package]\nname = \"up-core\"\nversion = \"1.0.0\"\nedition = \"2024\"\n\
             [features]\na = []\nb = []\nglue = []\n",
        );
        fixture.file("vendor/up/core/src/lib.rs", "pub struct Core;\n");
        fixture.file(
            "vendor/up/runtime/Cargo.toml",
            "[package]\nname = \"up-runtime\"\nversion = \"1.0.0\"\nedition = \"2024\"\n\
             [dependencies]\nup-core = { path = \"../core\", features = [\"a\", \"glue\"] }\n",
        );
        fixture.file("vendor/up/runtime/src/lib.rs", "pub struct Runtime;\n");
        for manifest in ["Cargo.toml", "vendor/up/Cargo.toml"] {
            let status = Command::new("cargo")
                .args(["generate-lockfile", "--offline", "--manifest-path"])
                .arg(fixture.path().join(manifest))
                .status()
                .unwrap();
            assert!(status.success(), "the fixture's lock files must be written");
        }
        fixture
    }

    /// The fixture's upstream: the runtime's `glue` feature is the binding's own.
    const FIXTURE_UPSTREAM: Upstream<'static> = Upstream {
        root: "vendor/up",
        package: "up-runtime",
        left_out_features: &[LeftOutFeature {
            krate: "up-core",
            feature: "glue",
            reason: "test",
        }],
        left_out_crates: &[],
        extra_packages: &[],
    };

    #[test]
    #[ignore = "runs pinned nightly Cargo on a fixture workspace; select explicitly with offline Cargo"]
    fn a_build_of_the_upstream_crates_is_checked_against_the_release_build() {
        let fixture = release_fixture("dageist-api-release-check", "\"a\"");

        check_release(fixture.path(), "app-host", &FIXTURE_UPSTREAM).unwrap();

        let fixture = release_fixture("dageist-api-release-mismatch", "\"a\", \"b\"");
        let error = check_release(fixture.path(), "app-host", &FIXTURE_UPSTREAM).unwrap_err();
        assert_eq!(
            error,
            "app-host does not build the crates of vendor/up as up-runtime does: 1 differences",
        );
    }

    struct TempFixture {
        root: std::path::PathBuf,
    }

    impl TempFixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn path(&self) -> &std::path::Path {
            &self.root
        }

        fn file(&self, relative: &str, contents: &str) {
            let path = self.root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }

        /// Writes a crate with a library at `relative`.
        fn library(&self, relative: &str, name: &str) {
            self.file(
                &format!("{relative}/Cargo.toml"),
                &format!("[package]\nname = \"{name}\"\n"),
            );
            self.file(&format!("{relative}/src/lib.rs"), "\n");
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

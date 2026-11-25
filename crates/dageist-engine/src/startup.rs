//! Polars' set-up, done the way the Python Polars release does it when its
//! module loads.
//!
//! Building Polars as the release is built does not make it behave as the
//! release does: the release also runs a start-up function,
//! `register_startup_deps`, in `crates/polars-python/src/on_startup.rs` of the
//! pinned source. Most of that function connects Polars to Python. Four of
//! its parts do not, and the engine does them, in the release's order:
//!
//! 1. In a build with debug assertions, it raises the minimum size of the stack
//!    below which the `recursive` crate moves a recursive call to a new stack,
//!    to 1 MiB, and the size of each new stack to 16 MiB. The release does it
//!    because stack frames get very large in debug mode, and dageist's own debug
//!    and test builds are debug builds. A build without debug assertions keeps
//!    the crate's defaults, as the release does.
//! 2. It registers the SQL resolver, so a plan written in SQL can be resolved
//!    when it is collected, also one that was not planned in this process.
//! 3. It sets what happens to an extension type that nothing registered when
//!    it is read: it is loaded as a generic extension, or as its storage type
//!    when the environment variable `POLARS_UNKNOWN_EXTENSION_TYPE_BEHAVIOR`
//!    is `load_as_storage`.
//! 4. It starts the thread that deletes the files Polars' streaming engine
//!    spills to disk, which also deletes what a process that ended abnormally
//!    left behind. The release flushes that thread when the process exits;
//!    [`PolarsSession`] does it when it is dropped.
//!
//! Every engine entry that runs Polars calls [`ensure`] first, which does the
//! four once however often it is called, so nothing runs Polars through the
//! engine without them. [`PolarsSession`] is the host's part: it starts Polars
//! at the beginning of the program and flushes at its end.
//!
//! # What is left out
//!
//! The rest of the release's start-up is left out. All but the last two items
//! only connect Polars to Python, and have nothing to connect here:
//!
//! - The registries that carry Python values in and out of Polars: converters
//!   between Polars' types and Python's, the builder of `Object` columns, the
//!   resolver, reader, dataset and Delta deletion-vector providers that call
//!   into Python, and the callers of Python functions used as `map` and `apply`
//!   functions.
//! - The function that reports Polars' warnings through Python's `warnings`.
//!   Without it Polars prints a warning on the standard error stream.
//! - Allowing extension types to be created (`set_polars_allow_extension`),
//!   which lets `Object` columns hold Python objects. The engine has no
//!   Python objects.
//! - The name, version and build of the library (`set_polars_lib_name` and
//!   the two that go with it). Polars writes them into the Parquet files it
//!   writes, and the release's values name the Python package, which dageist is
//!   not.
//! - The panic hook and the interrupt handler of the query abort mechanism
//!   (`register_polars_abort_mechanism`). The release gives Python a
//!   `KeyboardInterrupt` by hooking `SIGINT` and aborting the running query
//!   with a panic that the catchers around the Python binding's queries turn
//!   into the exception. The same catchers turn a spill that runs out of disk
//!   (`OocOutOfDisk`) into an error. The engine has no catchers, so without
//!   them a spill that runs out of disk panics as any panic does, in the
//!   thread that spilled. Leaving the hook out also leaves `SIGINT` to end
//!   the process, which Ctrl-C does to the bridge; the release's hook would
//!   take it over, and with no catcher make it do nothing.
//! - The coloured backtrace printer for Polars' own panics, behind the
//!   `backtrace_filter` feature. The release's Makefile turns it on for
//!   development builds of Python Polars; the released wheels do not have it.
//! - Flushing at exit through Python's `atexit`, which [`PolarsSession`]
//!   replaces.
//!
//! When `vendor/polars` moves to a new release, the start-up code here is
//! checked against that release's `on_startup.rs`; the how-to says so.

use std::sync::Once;

use polars::datatypes::extension::{
    UnknownExtensionTypeBehavior, set_unknown_extension_type_behavior,
};

/// The environment variable that chooses what happens to an extension type
/// nothing registered, as in the release.
const UNKNOWN_EXTENSION_TYPE_BEHAVIOR: &str = "POLARS_UNKNOWN_EXTENSION_TYPE_BEHAVIOR";

/// What the release does with the value of
/// [`UNKNOWN_EXTENSION_TYPE_BEHAVIOR`]: `load_as_storage` loads an unknown
/// extension type as its storage type, and `load_as_extension`, any other value
/// and no value load it as a generic extension type.
fn unknown_extension_type_behavior(setting: Option<&str>) -> UnknownExtensionTypeBehavior {
    match setting {
        Some("load_as_storage") => UnknownExtensionTypeBehavior::LoadAsStorage,
        Some("load_as_extension") | Some(_) | None => UnknownExtensionTypeBehavior::LoadAsGeneric,
    }
}

static STARTED: Once = Once::new();

/// Sets Polars up, once for the process, however often it is called and from
/// however many threads: a call that comes while another is setting it up waits
/// for that one to finish.
///
/// Every engine entry that runs Polars calls it before running it. The set-up
/// reads the environment variables it uses when it first runs, so a variable
/// set later has no effect.
pub(crate) fn ensure() {
    STARTED.call_once(|| {
        // Stack frames can get really large in debug mode, as in the release.
        #[cfg(debug_assertions)]
        {
            recursive::set_minimum_stack_size(1024 * 1024);
            recursive::set_stack_allocation_size(1024 * 1024 * 16);
        }
        polars::sql::register_sql_resolver();
        let setting = std::env::var(UNKNOWN_EXTENSION_TYPE_BEHAVIOR).ok();
        set_unknown_extension_type_behavior(unknown_extension_type_behavior(setting.as_deref()));
        polars_ooc::init_ooc_cleaner();
    });
}

/// Polars, set up for the life of the program that holds this value.
///
/// A program that runs Polars through the engine starts one when it starts
/// and holds it until it ends. [`start`](Self::start) sets Polars up, as every
/// engine entry that runs Polars does when it runs, and dropping the value
/// waits until the files Polars spilled to disk are deleted, as the Python
/// release does when its process exits. Holding it for the whole of `main`
/// pairs the two, so no call to flush is left for a caller to remember.
///
/// # Ending abnormally
///
/// The files are deleted when the value is dropped, which happens when the
/// program ends normally and when a panic unwinds out of the function that
/// holds it. It does not happen when the process is killed or ended by a
/// signal, aborts, calls [`std::process::exit`] or crashes: nothing then runs
/// the program's destructors. The files stay in a directory named for the
/// process, under Polars' spill directory. The next program that starts Polars
/// deletes the directories of processes that no longer exist, as part of
/// starting the thread that deletes spilled files.
#[derive(Debug)]
#[must_use = "dropping the session at once flushes Polars' spill-file cleaner at once; hold it until the program ends"]
pub struct PolarsSession(());

impl PolarsSession {
    /// Sets Polars up if nothing has yet, and returns the value that flushes
    /// the spill-file cleaner when it is dropped.
    ///
    /// Starting more than one is allowed: each flushes when it is dropped, and
    /// Polars is set up only once.
    pub fn start() -> Self {
        ensure();
        Self(())
    }
}

impl Drop for PolarsSession {
    fn drop(&mut self) {
        polars_ooc::flush_ooc_cleanup();
    }
}

#[cfg(test)]
mod tests {
    use polars::datatypes::extension::UnknownExtensionTypeBehavior;

    use super::unknown_extension_type_behavior;

    fn loads_as_storage(setting: Option<&str>) -> bool {
        matches!(
            unknown_extension_type_behavior(setting),
            UnknownExtensionTypeBehavior::LoadAsStorage
        )
    }

    #[test]
    fn only_load_as_storage_loads_an_unknown_extension_type_as_its_storage() {
        assert!(loads_as_storage(Some("load_as_storage")));
    }

    #[test]
    fn load_as_extension_and_every_other_setting_load_it_as_a_generic_extension() {
        for setting in [
            Some("load_as_extension"),
            None,
            Some(""),
            Some("LOAD_AS_STORAGE"),
            Some("storage"),
        ] {
            assert!(!loads_as_storage(setting), "setting {setting:?}");
        }
    }
}

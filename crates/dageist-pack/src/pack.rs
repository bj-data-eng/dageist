//! Writing a pack whose objects are stored whole or as deltas against bases the caller names.

use std::collections::HashMap;
use std::fmt;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use gix_hash::ObjectId;
use gix_object::Kind;
use gix_pack::data::output::{self, bytes::FromEntriesIter};

use crate::delta::encode;

/// One object to write into a pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Object<'a> {
    /// What the object is.
    pub kind: Kind,
    /// Its bytes, as git stores them before compression.
    pub data: &'a [u8],
    /// The object it is to be stored against, by its place in the list, which must come before
    /// it and be of the same kind. It is stored against it only when the delta is smaller than
    /// the object stored whole and the base is not at the deepest a chain may go.
    pub base: Option<usize>,
}

/// How a pack is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// The most deltas an object may be read through: an object whose base is already this deep
    /// is stored whole. Git's own limit is 50.
    pub max_depth: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self { max_depth: 50 }
    }
}

/// What writing a pack made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The pack's data file.
    pub pack: PathBuf,
    /// The pack's index.
    pub index: PathBuf,
    /// The `.keep` file written beside the pack, if there is one, which keeps git from collecting
    /// the pack until a ref points at what it holds. The caller removes it once one does.
    pub keep: Option<PathBuf>,
    /// The id of each object, in the order they were given.
    pub ids: Vec<ObjectId>,
    /// How many objects were stored as deltas.
    pub deltas: usize,
}

/// Why a pack was not written.
#[derive(Debug)]
pub enum PackError {
    /// An object's base does not come before it.
    BaseNotBefore {
        /// The object, by its place in the list.
        object: usize,
        /// The base it names.
        base: usize,
    },
    /// An object's base is of another kind: a delta's object takes its base's kind.
    KindDiffers {
        /// The object, by its place in the list.
        object: usize,
        /// The base it names.
        base: usize,
    },
    /// An object is given twice: its bytes and kind are those of an earlier one, and git refuses
    /// a pack that holds an object twice.
    Repeated {
        /// The later object, by its place in the list.
        object: usize,
        /// The first object it repeats.
        first: usize,
    },
    /// No objects were given; a pack holds at least one.
    Empty,
    /// The pack would hold more objects than a pack's header can count.
    TooMany(usize),
    /// Writing the pack or its index failed; the text says why.
    Write {
        /// What was being done.
        context: &'static str,
        /// What went wrong.
        message: String,
    },
}

impl fmt::Display for PackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BaseNotBefore { object, base } => {
                write!(
                    formatter,
                    "object {object} names {base} as its base, which does not come before it"
                )
            }
            Self::KindDiffers { object, base } => {
                write!(
                    formatter,
                    "object {object} is of another kind than its base {base}"
                )
            }
            Self::Repeated { object, first } => {
                write!(formatter, "object {object} is the same object as {first}")
            }
            Self::Empty => formatter.write_str("a pack holds at least one object"),
            Self::TooMany(count) => write!(formatter, "{count} objects are more than a pack holds"),
            Self::Write { context, message } => write!(formatter, "{context}: {message}"),
        }
    }
}

impl std::error::Error for PackError {}

fn write_error(context: &'static str) -> impl Fn(&dyn fmt::Display) -> PackError {
    move |error| PackError::Write {
        context,
        message: error.to_string(),
    }
}

/// `data` compressed as a pack entry holds it.
fn compressed(data: &[u8]) -> Result<Vec<u8>, PackError> {
    let fail = write_error("compressing an object");
    let mut writer = gix_zlib::stream::deflate::Write::new(
        Vec::with_capacity(data.len() / 2),
        gix_zlib::Compression::DEFAULT,
    );
    writer.write_all(data).map_err(|error| fail(&error))?;
    writer.flush().map_err(|error| fail(&error))?;
    Ok(writer.into_inner())
}

/// Checks that each object's base comes before it and is of its kind.
fn check(objects: &[Object<'_>]) -> Result<(), PackError> {
    for (object, entry) in objects.iter().enumerate() {
        if let Some(base) = entry.base {
            if base >= object {
                return Err(PackError::BaseNotBefore { object, base });
            }
            if objects[base].kind != entry.kind {
                return Err(PackError::KindDiffers { object, base });
            }
        }
    }
    Ok(())
}

/// The entries the pack holds, each object whole or as a delta, with how many are deltas.
fn entries(
    objects: &[Object<'_>],
    ids: &[ObjectId],
    options: Options,
) -> Result<(Vec<output::Entry>, usize), PackError> {
    let mut depths = vec![0_u32; objects.len()];
    let mut entries = Vec::with_capacity(objects.len());
    let mut deltas = 0;
    for (place, object) in objects.iter().enumerate() {
        let whole = compressed(object.data)?;
        let delta = match object.base {
            Some(base) if depths[base] < options.max_depth => {
                match encode(objects[base].data, object.data) {
                    Ok(delta) => Some((base, compressed(&delta)?, delta.len())),
                    // A base a delta cannot reach all of is no base: the object is stored whole.
                    Err(_) => None,
                }
            }
            _ => None,
        };
        let entry = match delta {
            Some((base, delta, decompressed_size)) if delta.len() < whole.len() => {
                depths[place] = depths[base] + 1;
                deltas += 1;
                output::Entry {
                    id: ids[place],
                    kind: output::entry::Kind::DeltaRef { object_index: base },
                    decompressed_size,
                    compressed_data: delta,
                }
            }
            _ => output::Entry {
                id: ids[place],
                kind: output::entry::Kind::Base(object.kind),
                decompressed_size: object.data.len(),
                compressed_data: whole,
            },
        };
        entries.push(entry);
    }
    Ok((entries, deltas))
}

/// The id of each object, refusing an object given twice.
fn ids(objects: &[Object<'_>]) -> Result<Vec<ObjectId>, PackError> {
    let mut ids = Vec::with_capacity(objects.len());
    let mut places = HashMap::with_capacity(objects.len());
    for (object, entry) in objects.iter().enumerate() {
        let id = gix_object::compute_hash(gix_hash::Kind::Sha1, entry.kind, entry.data)
            .map_err(|error| write_error("naming an object")(&error))?;
        if let Some(&first) = places.get(&id) {
            return Err(PackError::Repeated { object, first });
        }
        places.insert(id, object);
        ids.push(id);
    }
    Ok(ids)
}

/// Writes `objects` as one pack with its index into `directory`, which is made when it is
/// missing, and returns where they are and what each object's id is.
///
/// An object that names a base is stored as a delta against it when that is smaller than the
/// object stored whole and the base is not already as deep in deltas as `options` allows. gix
/// then reads the pack back to write its index, resolving every delta as it does.
///
/// Each object is given once: a file put back to an earlier version is the same object as that
/// version, and the caller leaves it out rather than give it again.
///
/// # Errors
///
/// When no objects are given, an object's base does not come before it or is of another kind,
/// an object is given twice, the list is longer than a pack counts, or the folder, the pack or
/// its index cannot be written.
pub fn write_pack(
    objects: &[Object<'_>],
    directory: &Path,
    options: Options,
) -> Result<Written, PackError> {
    if objects.is_empty() {
        return Err(PackError::Empty);
    }
    check(objects)?;
    let count = u32::try_from(objects.len()).map_err(|_| PackError::TooMany(objects.len()))?;
    let ids = ids(objects)?;
    let (entries, deltas) = entries(objects, &ids, options)?;

    let mut bytes = Vec::new();
    let writer = FromEntriesIter::new(
        std::iter::once(Ok(entries)),
        &mut bytes,
        count,
        gix_pack::data::Version::V2,
        gix_hash::Kind::Sha1,
    );
    for written in writer {
        written.map_err(|error| write_error("writing the pack")(&error))?;
    }

    std::fs::create_dir_all(directory).map_err(|error| write_error("making the folder")(&error))?;
    let outcome = gix_pack::Bundle::write_to_directory(
        &mut bytes.as_slice(),
        Some(directory),
        &mut gix_utils::progress::Discard,
        &AtomicBool::new(false),
        None::<gix_object::find::Never>,
        gix_hash::Kind::Sha1,
        gix_pack::bundle::write::Options::default(),
    )
    .map_err(|error| write_error("indexing the pack")(&error))?;
    let missing = || PackError::Write {
        context: "indexing the pack",
        message: "no file was written".to_owned(),
    };
    Ok(Written {
        pack: outcome.data_path.ok_or_else(missing)?,
        index: outcome.index_path.ok_or_else(missing)?,
        keep: outcome.keep_path,
        ids,
        deltas,
    })
}

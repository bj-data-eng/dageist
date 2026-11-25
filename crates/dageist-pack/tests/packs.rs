//! Packs written with deltas, read back by gix and by git: every object reads back as it was
//! given, deltas are kept only where they are smaller, chains stop at their depth, and a history
//! of small edits packs small.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use dageist_pack::{Object, Options, PackError, Written, write_pack};
use gix_object::Kind;

/// A folder of its own for each test, so that tests running at once do not share one, removed
/// when the test ends.
struct Folder(PathBuf);

impl std::ops::Deref for Folder {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn folder(name: &str) -> Folder {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let folder = std::env::temp_dir().join(format!(
        "dageist-pack-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a test folder");
    Folder(folder)
}

/// Reads every object of `written` back through gix's own pack reader.
fn read_back(written: &Written) -> Vec<(Kind, Vec<u8>)> {
    let bundle =
        gix_pack::Bundle::at(&written.index, gix_hash::Kind::Sha1).expect("the pack opens");
    let mut inflate = gix_zlib::Inflate::default();
    written
        .ids
        .iter()
        .map(|id| {
            let mut out = Vec::new();
            let (data, _) = bundle
                .find(id, &mut out, &mut inflate, &mut gix_pack::cache::Never)
                .expect("the object reads")
                .expect("the pack holds the object");
            (data.kind, data.data.to_vec())
        })
        .collect()
}

/// A document of `steps` steps, as a node's file holds one, with `seed` in each step's value.
fn document(steps: u32, seed: u32) -> Vec<u8> {
    let mut text = String::from("{\n  \"version\": 1,\n  \"steps\": [\n");
    for step in 0..steps {
        text.push_str(&format!(
            "    {{ \"id\": \"step-{step}\", \"call\": {{ \"entry\": \"LazyFrame.with_columns\", \"arguments\": {{ \"value\": {} }} }} }},\n",
            (step * 7919 + seed) % 1000
        ));
    }
    text.push_str("  ]\n}\n");
    text.into_bytes()
}

/// `versions` versions of a document, each a small edit of the one before.
fn history(versions: u32) -> Vec<Vec<u8>> {
    let mut current = document(60, 0);
    let mut all = vec![current.clone()];
    for version in 1..versions {
        // Change one step's value, as setting an argument does.
        let text = String::from_utf8(current).expect("text");
        let step = (version * 13) % 60;
        let marker = format!("\"id\": \"step-{step}\"");
        let at = text.find(&marker).expect("the step") + marker.len();
        let value_at = text[at..].find("\"value\": ").expect("a value") + at + 9;
        let end = text[value_at..].find(' ').expect("the value ends") + value_at;
        current = format!("{}{}{}", &text[..value_at], version, &text[end..]).into_bytes();
        all.push(current.clone());
    }
    all
}

/// The objects of `versions`, each stored against the one before when `chained`.
fn objects(versions: &[Vec<u8>], chained: bool) -> Vec<Object<'_>> {
    versions
        .iter()
        .enumerate()
        .map(|(place, data)| Object {
            kind: Kind::Blob,
            data,
            base: (chained && place > 0).then(|| place - 1),
        })
        .collect()
}

#[test]
fn every_object_reads_back_through_gix_as_it_was_given() {
    let versions = history(40);
    let directory = folder("reads");
    let written = write_pack(&objects(&versions, true), &directory, Options::default())
        .expect("the pack is written");
    assert_eq!(written.deltas, 39);
    let read = read_back(&written);
    for (place, (kind, data)) in read.iter().enumerate() {
        assert_eq!(*kind, Kind::Blob);
        assert_eq!(data, &versions[place], "version {place}");
    }
}

#[test]
fn objects_of_every_kind_read_back_and_unrelated_objects_are_stored_whole() {
    let tree = [0_u8; 0];
    let commit = b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\nauthor A <a@example.com> 0 +0000\ncommitter A <a@example.com> 0 +0000\n\nfirst\n";
    let noise: Vec<u8> = (0..4000_u32)
        .map(|n| (n.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    let other: Vec<u8> = (0..4000_u32)
        .map(|n| (n.wrapping_mul(40_503) >> 7) as u8)
        .collect();
    let given = [
        Object {
            kind: Kind::Tree,
            data: &tree,
            base: None,
        },
        Object {
            kind: Kind::Commit,
            data: commit,
            base: None,
        },
        Object {
            kind: Kind::Blob,
            data: &noise,
            base: None,
        },
        Object {
            kind: Kind::Blob,
            data: &other,
            base: Some(2),
        },
    ];
    let directory = folder("kinds");
    let written = write_pack(&given, &directory, Options::default()).expect("the pack is written");
    // The two blobs share nothing, so the delta is no smaller and the second is stored whole.
    assert_eq!(written.deltas, 0);
    let read = read_back(&written);
    for (object, (kind, data)) in given.iter().zip(&read) {
        assert_eq!((object.kind, object.data), (*kind, data.as_slice()));
    }
}

#[test]
fn a_chain_of_deltas_stops_at_its_depth_and_starts_again_whole() {
    let repository = folder("depth");
    git(&repository, &["init", "-q"]);
    let versions = history(10);
    let written = write_pack(
        &objects(&versions, true),
        &repository.join(".git/objects/pack"),
        Options { max_depth: 3 },
    )
    .expect("the pack is written");
    // Versions 1 to 3 are deltas, 4 is whole, 5 to 7 deltas, 8 whole, 9 a delta.
    assert_eq!(written.deltas, 7);
    let read = read_back(&written);
    assert!(
        read.iter()
            .zip(&versions)
            .all(|((_, data), version)| data == version)
    );
    // Git counts the objects at each length of chain: three at one delta (versions 1, 5 and 9),
    // two at two and two at three.
    let index = written.index.to_str().expect("a path");
    let listing = String::from_utf8(git(&repository, &["verify-pack", "-v", index])).expect("text");
    let chains: Vec<&str> = listing
        .lines()
        .filter(|line| line.starts_with("chain length"))
        .collect();
    assert_eq!(
        chains,
        [
            "chain length = 1: 3 objects",
            "chain length = 2: 2 objects",
            "chain length = 3: 2 objects",
        ]
    );
}

#[test]
fn an_object_given_twice_is_refused_because_git_refuses_the_pack() {
    let first = b"the first version\n".to_vec();
    let second = b"the second version\n".to_vec();
    // A file put back to its first version gives the same object again.
    let given = [
        Object {
            kind: Kind::Blob,
            data: &first,
            base: None,
        },
        Object {
            kind: Kind::Blob,
            data: &second,
            base: Some(0),
        },
        Object {
            kind: Kind::Blob,
            data: &first,
            base: Some(1),
        },
    ];
    assert!(matches!(
        write_pack(&given, &folder("twice"), Options::default()),
        Err(PackError::Repeated {
            object: 2,
            first: 0
        })
    ));
}

#[test]
fn a_pack_of_no_objects_is_refused() {
    assert!(matches!(
        write_pack(&[], &folder("empty"), Options::default()),
        Err(PackError::Empty)
    ));
}

#[test]
fn a_missing_folder_is_made_and_the_pack_written_into_it() {
    let repository = folder("missing");
    git(&repository, &["init", "-q"]);
    let packs = repository.join(".git/objects/pack/new/deeper");
    let data = b"one object\n".to_vec();
    let given = [Object {
        kind: Kind::Blob,
        data: &data,
        base: None,
    }];
    let written = write_pack(&given, &packs, Options::default()).expect("the pack is written");
    assert_eq!(written.pack.parent(), Some(packs.as_path()));
    git(
        &repository,
        &["verify-pack", written.index.to_str().expect("a path")],
    );
}

#[test]
fn a_base_must_come_before_its_object_and_be_of_its_kind() {
    let data = b"some bytes".to_vec();
    let later = [
        Object {
            kind: Kind::Blob,
            data: &data,
            base: Some(1),
        },
        Object {
            kind: Kind::Blob,
            data: &data,
            base: None,
        },
    ];
    assert!(matches!(
        write_pack(&later, &folder("later"), Options::default()),
        Err(PackError::BaseNotBefore { object: 0, base: 1 })
    ));
    let kinds = [
        Object {
            kind: Kind::Commit,
            data: &data,
            base: None,
        },
        Object {
            kind: Kind::Blob,
            data: &data,
            base: Some(0),
        },
    ];
    assert!(matches!(
        write_pack(&kinds, &folder("kinds-differ"), Options::default()),
        Err(PackError::KindDiffers { object: 1, base: 0 })
    ));
}

#[test]
fn a_history_of_small_edits_packs_near_the_size_git_packs_it() {
    let versions = history(500);
    let size = |path: &Path| std::fs::metadata(path).expect("the pack").len();
    let repository = folder("size");
    git(&repository, &["init", "-q"]);
    let whole = write_pack(
        &objects(&versions, false),
        &repository.join(".git/objects/pack"),
        Options::default(),
    )
    .expect("the pack is written");
    let chained_folder = folder("chained");
    let chained = write_pack(
        &objects(&versions, true),
        &chained_folder,
        Options::default(),
    )
    .expect("the pack is written");
    assert_eq!(whole.deltas, 0);
    // Git's own delta search over the same objects, with its default window and depth.
    let ids: String = whole.ids.iter().map(|id| format!("{id}\n")).collect();
    let base = repository.join("git-packed");
    let mut packing = Command::new("git")
        .arg("-C")
        .arg(&*repository)
        .args(["pack-objects", "-q", "--no-reuse-object"])
        .arg(&base)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("git runs");
    std::io::Write::write_all(&mut packing.stdin.take().expect("stdin"), ids.as_bytes())
        .expect("the ids are sent");
    let packed = packing.wait_with_output().expect("git packs");
    assert!(packed.status.success());
    let name = String::from_utf8(packed.stdout).expect("a pack name");
    let by_git = size(&PathBuf::from(format!(
        "{}-{}.pack",
        base.display(),
        name.trim()
    )));
    let ours = size(&chained.pack);
    // The measurement, shown with `--nocapture`.
    eprintln!(
        "500 versions: {ours} bytes with our deltas, {by_git} with git's, {} stored whole",
        size(&whole.pack)
    );
    assert!(
        ours * 2 <= by_git * 3,
        "{ours} bytes with our deltas, {by_git} with git's, {} stored whole",
        size(&whole.pack)
    );
}

/// Runs git in `repository`, and gives what it printed.
fn git(repository: &Path, arguments: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .expect("git runs; the tests that check packs with git need it on the path");
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn git_verifies_the_pack_and_reads_every_object_back() {
    let repository = folder("git");
    git(&repository, &["init", "-q"]);
    let versions = history(30);
    let written = write_pack(
        &objects(&versions, true),
        &repository.join(".git/objects/pack"),
        Options::default(),
    )
    .expect("the pack is written");
    let index = written.index.to_str().expect("a path");
    git(&repository, &["verify-pack", index]);
    for (id, version) in written.ids.iter().zip(&versions) {
        let read = git(&repository, &["cat-file", "blob", &id.to_string()]);
        assert_eq!(&read, version, "{id}");
    }
}

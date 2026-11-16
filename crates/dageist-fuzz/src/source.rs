//! Source-evidence values: workspace-relative paths, digests and Git revisions.
//!
//! Each value is valid by construction, including when deserialized.
#![forbid(unsafe_code)]

use std::{
    cmp::Ordering,
    fmt,
    fs::{self, File},
    hash::{Hash, Hasher},
    io::Read,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Visitor};
use sha2::{Digest, Sha256};

use crate::{FuzzError, FuzzErrorKind, Result};

/// Message a value reports to a deserializer when its input is invalid.
fn serde_message(error: &FuzzError) -> String {
    format!("{:?}: {error}", error.kind())
}

fn is_lowercase_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// A normalized, UTF-8 path relative to a declared root.
#[derive(Clone, Debug, Eq)]
pub struct RelativePath(String);

impl RelativePath {
    /// Validates a forward-slash relative path.
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref();
        if value.is_empty()
            || value.trim() != value
            || value.contains('\0')
            || value.contains('\\')
            || value.starts_with('/')
            || value.ends_with('/')
        {
            return Err(invalid_path("validate relative path", value));
        }
        if value.as_bytes().get(1) == Some(&b':') && value.as_bytes()[0].is_ascii_alphabetic() {
            return Err(invalid_path("validate relative path", value));
        }

        for segment in value.split('/') {
            if segment.is_empty() || segment == "." || segment == ".." {
                return Err(invalid_path("validate relative path", value));
            }
        }

        let path = Path::new(value);
        if path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(invalid_path("validate relative path", value));
        }

        Ok(Self(value.to_owned()))
    }

    /// Returns the normalized forward-slash representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Joins this path to its declared root.
    #[must_use]
    pub fn join(&self, root: impl AsRef<Path>) -> PathBuf {
        self.0
            .split('/')
            .fold(root.as_ref().to_path_buf(), |mut path, segment| {
                path.push(segment);
                path
            })
    }

    /// Resolves an existing path and proves that symlinks do not escape `root`.
    pub fn resolve_existing(&self, root: impl AsRef<Path>) -> Result<PathBuf> {
        let root = fs::canonicalize(root.as_ref()).map_err(|_| {
            invalid_path(
                "canonicalize declared root",
                format!("unresolvable path: {}", root.as_ref().display()),
            )
        })?;
        if !root.is_dir() {
            return Err(invalid_path(
                "canonicalize declared root",
                format!("not a directory: {}", root.display()),
            ));
        }
        let candidate = self.join(&root);
        let resolved = fs::canonicalize(&candidate).map_err(|_| {
            invalid_path("canonicalize existing relative path", candidate.display())
        })?;
        if !resolved.starts_with(&root) {
            return Err(invalid_path(
                "resolve existing relative path",
                format!("{} escapes {}", resolved.display(), root.display()),
            ));
        }
        Ok(resolved)
    }
}

fn invalid_path(operation: &str, detail: impl fmt::Display) -> FuzzError {
    FuzzError::new(FuzzErrorKind::InvalidPath, operation, detail.to_string())
}

impl PartialEq for RelativePath {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialOrd for RelativePath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RelativePath {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl Hash for RelativePath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl Serialize for RelativePath {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RelativePathVisitor;

        impl Visitor<'_> for RelativePathVisitor {
            type Value = RelativePath;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a canonical relative-path string")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                RelativePath::new(value).map_err(|error| E::custom(serde_message(&error)))
            }

            fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                RelativePath::new(value).map_err(|error| E::custom(serde_message(&error)))
            }
        }

        deserializer.deserialize_str(RelativePathVisitor)
    }
}

/// A canonical lowercase SHA-256 digest.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    #[must_use]
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes.as_ref())))
    }

    pub fn from_text(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref();
        if value.len() != 64 || !is_lowercase_hex(value) {
            return Err(FuzzError::new(
                FuzzErrorKind::InvalidIdentity,
                "validate SHA-256 digest",
                "digest must contain exactly 64 lowercase hexadecimal characters",
            ));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut file = File::open(path).map_err(|error| {
            FuzzError::with_source(
                FuzzErrorKind::Io,
                "open file for SHA-256",
                path.display().to_string(),
                error,
            )
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer).map_err(|error| {
                FuzzError::with_source(
                    FuzzErrorKind::Io,
                    "read file for SHA-256",
                    path.display().to_string(),
                    error,
                )
            })?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(Self(format!("{:x}", hasher.finalize())))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DigestVisitor;

        impl Visitor<'_> for DigestVisitor {
            type Value = Sha256Digest;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a canonical lowercase SHA-256 string")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Sha256Digest::from_text(value).map_err(|error| E::custom(serde_message(&error)))
            }

            fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Sha256Digest::from_text(value).map_err(|error| E::custom(serde_message(&error)))
            }
        }

        deserializer.deserialize_str(DigestVisitor)
    }
}

/// Exact lowercase hexadecimal Git object revision.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceRevision(String);

impl SourceRevision {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref();
        if !matches!(value.len(), 40 | 64) || !is_lowercase_hex(value) {
            return Err(FuzzError::new(
                FuzzErrorKind::InvalidIdentity,
                "validate source revision",
                "revision must be a full 40- or 64-character lowercase hexadecimal object ID",
            ));
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for SourceRevision {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SourceRevision {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SourceRevisionVisitor;

        impl Visitor<'_> for SourceRevisionVisitor {
            type Value = SourceRevision;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a full lowercase hexadecimal Git object ID")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                SourceRevision::new(value).map_err(|error| E::custom(serde_message(&error)))
            }

            fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                SourceRevision::new(value).map_err(|error| E::custom(serde_message(&error)))
            }
        }

        deserializer.deserialize_str(SourceRevisionVisitor)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{RelativePath, Sha256Digest, SourceRevision};
    use crate::FuzzErrorKind;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "dageist-fuzz-source-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove test directory");
        }
    }

    #[test]
    fn strict_paths_reject_invalid_components() {
        for invalid in [
            "",
            " fixture.json",
            "fixture.json ",
            "/fixture.json",
            "C:/fixture.json",
            "fixtures\\fixture.json",
            "fixtures//fixture.json",
            "fixtures/./fixture.json",
            "fixtures/../fixture.json",
            "fixtures/",
            "fix\0ture.json",
        ] {
            let error = RelativePath::new(invalid).expect_err(invalid);
            assert_eq!(error.kind(), FuzzErrorKind::InvalidPath, "{invalid:?}");
        }
    }

    #[test]
    fn strict_paths_keep_normal_components_and_join_them_to_a_root() {
        let path = RelativePath::new("nested#source/Fixture#.json").unwrap();
        assert_eq!(path.as_str(), "nested#source/Fixture#.json");
        assert_eq!(
            path.join("/root"),
            Path::new("/root")
                .join("nested#source")
                .join("Fixture#.json")
        );
    }

    #[test]
    fn paths_deserialize_only_through_validation() {
        let path: RelativePath = serde_json::from_str("\"a/b.json\"").unwrap();
        assert_eq!(serde_json::to_string(&path).unwrap(), "\"a/b.json\"");
        let error = serde_json::from_str::<RelativePath>("\"../b.json\"").unwrap_err();
        assert!(
            error.to_string().contains("validate relative path"),
            "{error}"
        );
    }

    #[test]
    fn existing_paths_resolve_inside_their_root_and_missing_ones_do_not() {
        let root = TestDirectory::new("resolve");
        fs::create_dir(root.path().join("nested")).unwrap();
        fs::write(root.path().join("nested/case.json"), b"{}\n").unwrap();
        let present = RelativePath::new("nested/case.json").unwrap();
        assert_eq!(
            present.resolve_existing(root.path()).unwrap(),
            fs::canonicalize(root.path().join("nested/case.json")).unwrap()
        );
        let missing = RelativePath::new("nested/missing.json").unwrap();
        assert_eq!(
            missing.resolve_existing(root.path()).unwrap_err().kind(),
            FuzzErrorKind::InvalidPath
        );
        let not_a_directory = RelativePath::new("case.json").unwrap();
        assert_eq!(
            not_a_directory
                .resolve_existing(root.path().join("nested/case.json"))
                .unwrap_err()
                .kind(),
            FuzzErrorKind::InvalidPath
        );
    }

    #[cfg(unix)]
    #[test]
    fn existing_paths_reject_symlink_escapes() {
        use std::os::unix::fs::symlink;

        let root = TestDirectory::new("symlink-root");
        let outside = TestDirectory::new("symlink-outside");
        fs::write(outside.path().join("case.json"), b"{}\n").unwrap();
        symlink(outside.path(), root.path().join("escape")).unwrap();

        let relative = RelativePath::new("escape/case.json").expect("strict lexical path");
        let error = relative.resolve_existing(root.path()).unwrap_err();
        assert_eq!(error.kind(), FuzzErrorKind::InvalidPath);
        assert!(error.detail().contains("escapes"), "{error}");
    }

    #[test]
    fn digest_text_validation_rejects_noncanonical_text() {
        assert!(Sha256Digest::from_text("0".repeat(63)).is_err());
        assert!(Sha256Digest::from_text("0".repeat(65)).is_err());
        assert!(Sha256Digest::from_text("A".repeat(64)).is_err());
        assert!(Sha256Digest::from_text("g".repeat(64)).is_err());
        assert_eq!(
            Sha256Digest::from_text("0".repeat(63)).unwrap_err().kind(),
            FuzzErrorKind::InvalidIdentity
        );
        assert!(Sha256Digest::from_text("0123456789abcdef".repeat(4)).is_ok());
    }

    #[test]
    fn digests_match_the_published_sha256_vectors() {
        // FIPS 180-2 appendix B test vectors.
        assert_eq!(
            Sha256Digest::from_bytes(b"abc").as_str(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            Sha256Digest::from_bytes(b"").to_string(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn file_digests_equal_byte_digests_and_missing_files_are_io_errors() {
        let root = TestDirectory::new("digest");
        // Larger than one read buffer, so the digest spans several reads.
        let bytes = vec![0x5a_u8; 150_000];
        fs::write(root.path().join("big.bin"), &bytes).unwrap();
        assert_eq!(
            Sha256Digest::from_file(root.path().join("big.bin")).unwrap(),
            Sha256Digest::from_bytes(&bytes)
        );
        let error = Sha256Digest::from_file(root.path().join("absent.bin")).unwrap_err();
        assert_eq!(error.kind(), FuzzErrorKind::Io);
    }

    #[test]
    fn digests_deserialize_only_through_validation() {
        let text = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let digest: Sha256Digest = serde_json::from_str(&format!("\"{text}\"")).unwrap();
        assert_eq!(digest, Sha256Digest::from_bytes(b"abc"));
        assert!(serde_json::from_str::<Sha256Digest>("\"ABC\"").is_err());
    }

    #[test]
    fn revisions_accept_only_full_lowercase_object_ids() {
        assert!(SourceRevision::new("a".repeat(40)).is_ok());
        assert!(SourceRevision::new("0123456789abcdef".repeat(4)).is_ok());
        for invalid in [
            String::new(),
            "a".repeat(39),
            "a".repeat(41),
            "a".repeat(63),
            "A".repeat(40),
            "g".repeat(40),
            format!("{} ", "a".repeat(39)),
        ] {
            let error = SourceRevision::new(&invalid).expect_err(&invalid);
            assert_eq!(error.kind(), FuzzErrorKind::InvalidIdentity, "{invalid:?}");
        }
    }

    #[test]
    fn revisions_round_trip_and_deserialize_only_through_validation() {
        let revision = SourceRevision::new("b".repeat(40)).unwrap();
        let encoded = serde_json::to_string(&revision).unwrap();
        assert_eq!(encoded, format!("\"{}\"", "b".repeat(40)));
        assert_eq!(
            serde_json::from_str::<SourceRevision>(&encoded).unwrap(),
            revision
        );
        assert_eq!(revision.to_string(), "b".repeat(40));
        assert!(serde_json::from_str::<SourceRevision>("\"abc\"").is_err());
    }
}

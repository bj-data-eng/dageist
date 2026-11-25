//! Fuzzes the delta encoder through the fuzz framework's property runner. Each case is a base and
//! a target made from it by a run of edits, the way a file's next version is made from the one
//! before. The checked properties:
//!
//! - **The delta makes the target:** applying the delta to the base, by git's rules as written
//!   here apart from the encoder, gives the target exactly.
//! - **The delta is bounded:** a delta is never longer than its header and the target written as
//!   inserts, the most an encoder that copies nothing would write.

use dageist_fuzz::engine::{PropertyConfig, run_property};
use dageist_fuzz::generation::property;
use dageist_fuzz::registry::FuzzManifest;
use dageist_fuzz::{
    CaseOutcome, CaseTarget, FailureClass, FailureKey, FuzzError, FuzzErrorKind, Generation,
    PropertyId, Result, TargetDescriptor,
};
use dageist_pack::delta::encode;

const TARGET: &str = "dageist-pack::delta";
const MAKES_THE_TARGET: &str = "dageist-pack::delta-makes-the-target";
const BOUNDED: &str = "dageist-pack::delta-is-bounded";

/// A base and the target made from it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Case {
    base: Vec<u8>,
    target: Vec<u8>,
}

fn invalid(detail: &'static str) -> FuzzError {
    FuzzError::new(FuzzErrorKind::InvalidCase, "delta case", detail)
}

impl Case {
    /// The case as the base's length in four bytes, the base, and the target.
    fn encode(&self) -> Vec<u8> {
        let len = u32::try_from(self.base.len()).expect("a case's base is small");
        let mut bytes = len.to_le_bytes().to_vec();
        bytes.extend_from_slice(&self.base);
        bytes.extend_from_slice(&self.target);
        bytes
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let (len, rest) = bytes
            .split_first_chunk::<4>()
            .ok_or_else(|| invalid("the case has no base length"))?;
        let len = usize::try_from(u32::from_le_bytes(*len))
            .map_err(|_| invalid("the base is too long"))?;
        if len > rest.len() {
            return Err(invalid("the base is longer than the case"));
        }
        let (base, target) = rest.split_at(len);
        Ok(Self {
            base: base.to_vec(),
            target: target.to_vec(),
        })
    }

    /// Makes a case from generation entropy. The base is drawn from a small alphabet, so that
    /// it repeats itself as text does, and the target is the base after a run of edits:
    /// inserting, deleting, replacing and moving ranges.
    fn generate(entropy: &[u8]) -> Self {
        let mut bytes = entropy.iter().copied();
        let mut next = move || bytes.next().unwrap_or(0);
        let len = usize::from(next()) * 64 + usize::from(next());
        let alphabet = usize::from(next() % 16) + 2;
        let base: Vec<u8> = (0..len).map(|_| b'a' + (next() % alphabet as u8)).collect();
        let mut target = base.clone();
        for _ in 0..usize::from(next() % 12) {
            let at = if target.is_empty() {
                0
            } else {
                usize::from(next()) * 256 % (target.len() + 1)
            };
            let span = usize::from(next() % 64);
            let end = (at + span).min(target.len());
            match next() % 4 {
                0 => {
                    let inserted: Vec<u8> = (0..span).map(|_| next()).collect();
                    target.splice(at..at, inserted);
                }
                1 => {
                    target.drain(at..end);
                }
                2 => {
                    let replaced: Vec<u8> = (at..end).map(|_| next()).collect();
                    target.splice(at..end, replaced);
                }
                _ => {
                    let moved: Vec<u8> = target.drain(at..end).collect();
                    target.extend(moved);
                }
            }
        }
        Self { base, target }
    }
}

/// Makes the target a delta describes from its base, by git's rules: the base's and the target's
/// lengths seven bits a byte, then copies (high bit set; offset and size bytes named by the low
/// seven bits, a size of zero meaning 0x10000) and inserts (a count of 1 to 127 and its bytes).
fn apply(base: &[u8], delta: &[u8]) -> std::result::Result<Vec<u8>, String> {
    let mut at = 0;
    let mut byte = || -> std::result::Result<u8, String> {
        let value = *delta.get(at).ok_or("the delta ends early")?;
        at += 1;
        Ok(value)
    };
    let mut size = || -> std::result::Result<usize, String> {
        let (mut value, mut shift) = (0_usize, 0);
        loop {
            let next = byte()?;
            value |= usize::from(next & 0x7f) << shift;
            shift += 7;
            if next & 0x80 == 0 {
                return Ok(value);
            }
        }
    };
    if size()? != base.len() {
        return Err("the delta names another base length".to_owned());
    }
    let target_len = size()?;
    let mut out = Vec::new();
    while let Ok(op) = byte() {
        if op & 0x80 != 0 {
            let mut field = |bit: u8, shift: usize| -> std::result::Result<usize, String> {
                if op & bit == 0 {
                    return Ok(0);
                }
                Ok(usize::from(byte()?) << shift)
            };
            let offset = field(0x01, 0)? | field(0x02, 8)? | field(0x04, 16)? | field(0x08, 24)?;
            let mut len = field(0x10, 0)? | field(0x20, 8)? | field(0x40, 16)?;
            if len == 0 {
                len = 0x1_0000;
            }
            let copied = base
                .get(offset..offset + len)
                .ok_or("a copy reaches past the base")?;
            out.extend_from_slice(copied);
        } else if op == 0 {
            return Err("an instruction is zero".to_owned());
        } else {
            for _ in 0..op {
                out.push(byte()?);
            }
        }
    }
    if out.len() != target_len {
        return Err(format!(
            "the delta makes {} bytes, not the {target_len} it names",
            out.len()
        ));
    }
    Ok(out)
}

struct DeltaTarget {
    descriptor: TargetDescriptor,
}

impl DeltaTarget {
    fn new() -> Self {
        let manifest: FuzzManifest =
            toml::from_str(include_str!("descriptor.toml")).expect("the descriptor is valid");
        let descriptor = manifest
            .targets
            .into_iter()
            .find(|target| target.id().as_str() == TARGET)
            .expect("the descriptor declares the target");
        Self { descriptor }
    }
}

fn property(name: &str) -> PropertyId {
    PropertyId::new(name).expect("a declared property")
}

fn failure(name: &str, detail: String) -> Result<CaseOutcome> {
    CaseOutcome::failure(
        FailureKey::new(property(name), FailureClass::PropertyViolation, None)?,
        detail,
    )
}

impl CaseTarget for DeltaTarget {
    type Case = Case;

    fn descriptor(&self) -> &TargetDescriptor {
        &self.descriptor
    }

    fn decode(&self, input: &[u8]) -> Result<Case> {
        if input.len() as u64 > self.descriptor.limits().bytes() {
            return Err(invalid("the case is larger than the target admits"));
        }
        Case::decode(input)
    }

    fn encode(&self, case: &Case) -> Result<Vec<u8>> {
        Ok(case.encode())
    }

    fn generate(&self, input: &[u8]) -> Result<Generation<Case>> {
        Ok(Generation::Generated(Case::generate(input)))
    }

    fn evaluate(&self, case: &Case) -> Result<CaseOutcome> {
        let delta = match encode(&case.base, &case.target) {
            Ok(delta) => delta,
            Err(error) => return failure(MAKES_THE_TARGET, error.to_string()),
        };
        match apply(&case.base, &delta) {
            Ok(made) if made == case.target => {}
            Ok(_) => return failure(MAKES_THE_TARGET, "the delta makes other bytes".to_owned()),
            Err(why) => return failure(MAKES_THE_TARGET, why),
        }
        // A header of two sizes of at most ten bytes each, then each 127 bytes as one insert.
        let most = 20 + case.target.len() + case.target.len().div_ceil(127);
        if delta.len() > most {
            return failure(
                BOUNDED,
                format!(
                    "{} bytes of delta for a target of {}",
                    delta.len(),
                    case.target.len()
                ),
            );
        }
        CaseOutcome::evaluated(vec![property(MAKES_THE_TARGET), property(BOUNDED)])
    }
}

#[test]
fn every_delta_makes_its_target_and_is_bounded() {
    let target = DeltaTarget::new();
    let strategy = property::bytes(4096, target.descriptor().limits());
    let config = PropertyConfig {
        cases: 2000,
        max_rejections: 0,
        max_shrink_iterations: 2000,
        seed: 0x0da9_0001,
    };
    let report =
        run_property(&target, &strategy, config, &mut |_| Ok(())).expect("the run completes");
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert_eq!(report.counts.evaluated(), 2000);
}

#[test]
fn the_codec_reads_back_what_it_writes() {
    let case = Case::generate(&[3, 9, 7, 5, 1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(Case::decode(&case.encode()).expect("a case"), case);
}

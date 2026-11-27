//! Fuzzes the notation's reader and printer through the fuzz framework's property runner. Each
//! case is a text built from the notation's grammar, with spaces, lines and comments put between
//! its tokens inside brackets, and one case in four has a piece of the notation or of the Python
//! around it put in somewhere, so that many cases read and the rest are refused in many ways.
//! The checked properties:
//!
//! - **Refusals lie within the text:** a text that is refused is refused at a stretch of it whose
//!   lines and columns agree with its byte offsets, counted here apart from the reader.
//! - **Printing is a fixed point:** a text that reads prints as text that reads, and printing what
//!   that reads gives the same text again.

use dageist_fuzz::engine::{PropertyConfig, run_property};
use dageist_fuzz::generation::property;
use dageist_fuzz::registry::FuzzManifest;
use dageist_fuzz::{
    CaseOutcome, CaseTarget, FailureClass, FailureKey, FuzzError, FuzzErrorKind, Generation,
    PropertyId, Result, TargetDescriptor,
};
use dageist_syntax::{Position, parse, print};

const TARGET: &str = "dageist-syntax::notation";
const WITHIN: &str = "dageist-syntax::refusals-lie-within-the-text";
const FIXED: &str = "dageist-syntax::printing-is-a-fixed-point";

/// Pieces put into a text to break it: the notation's tokens, some of Python's that it refuses,
/// and lines and comments.
const PIECES: [&str; 32] = [
    ".",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    ",",
    "=",
    ":",
    "*",
    "**",
    "-",
    "...",
    "\"a\"",
    "1",
    " ",
    "\n",
    "# step: s\n",
    "\\\n",
    "+",
    "lambda",
    "for",
    "if",
    "[0]",
    "\"unclosed",
    "0x1",
    "'\\q'",
    "f\"x\"",
    "é",
    ";",
    "\r",
];

const NAMES: [&str; 8] = ["lf", "pl", "col", "abs", "when", "x", "_y", "str"];

const LITERALS: [&str; 14] = [
    "1",
    "-7",
    "1_000",
    "2.5",
    ".5",
    "5.",
    "1e+16",
    "-0.0",
    "True",
    "False",
    "None",
    "\"a\"",
    "'b\\n'",
    "r\"\\d+\"",
];

/// The entropy of a case, drawn a byte at a time; zero once it is spent.
struct Draw<'a>(std::slice::Iter<'a, u8>);

impl Draw<'_> {
    fn next(&mut self) -> usize {
        usize::from(self.0.next().copied().unwrap_or(0))
    }

    fn pick<'b>(&mut self, from: &[&'b str]) -> &'b str {
        from[self.next() % from.len()]
    }

    /// What stands between two tokens inside brackets.
    fn gap(&mut self, text: &mut String) {
        text.push_str(match self.next() % 8 {
            0 => " ",
            1 => "\n    ",
            2 => "  # note\n",
            _ => "",
        });
    }
}

fn expression(draw: &mut Draw<'_>, depth: usize, text: &mut String) {
    let choice = if depth > 3 {
        draw.next() % 3
    } else {
        draw.next() % 10
    };
    match choice {
        0 => text.push_str(draw.pick(&NAMES)),
        1 => text.push_str(draw.pick(&LITERALS)),
        2 => text.push_str("..."),
        3 => {
            text.push('[');
            for _ in 0..draw.next() % 4 {
                draw.gap(text);
                expression(draw, depth + 1, text);
                text.push(',');
            }
            draw.gap(text);
            text.push(']');
        }
        4 => {
            text.push('{');
            for _ in 0..draw.next() % 3 {
                draw.gap(text);
                expression(draw, depth + 1, text);
                text.push_str(": ");
                expression(draw, depth + 1, text);
                text.push(',');
            }
            text.push('}');
        }
        _ => chain(draw, depth, text),
    }
}

fn chain(draw: &mut Draw<'_>, depth: usize, text: &mut String) {
    match draw.next() % 4 {
        0 => {
            text.push('(');
            text.push_str(draw.pick(&LITERALS));
            text.push(')');
        }
        1 => {
            text.push('(');
            expression(draw, depth + 1, text);
            text.push(')');
        }
        _ => text.push_str(draw.pick(&NAMES)),
    }
    for _ in 0..1 + draw.next() % 4 {
        if depth > 0 && draw.next().is_multiple_of(6) {
            text.push_str("\n    # step: s\n    ");
        }
        text.push('.');
        text.push_str(draw.pick(&NAMES));
        if !draw.next().is_multiple_of(3) {
            arguments(draw, depth + 1, text);
        }
    }
}

fn arguments(draw: &mut Draw<'_>, depth: usize, text: &mut String) {
    text.push('(');
    for at in 0..draw.next() % 4 {
        if at > 0 {
            text.push(',');
        }
        draw.gap(text);
        match draw.next() % 6 {
            0 => {
                text.push_str(draw.pick(&NAMES));
                text.push('=');
            }
            1 => text.push('*'),
            _ => {}
        }
        expression(draw, depth, text);
    }
    draw.gap(text);
    text.push(')');
}

/// Makes a text from generation entropy: an expression from the grammar, in brackets or not, and
/// for one case in four a piece put in at a place the entropy picks.
fn generate(entropy: &[u8]) -> String {
    let mut draw = Draw(entropy.iter());
    let bracketed = draw.next().is_multiple_of(2);
    let mut text = String::new();
    if bracketed {
        text.push_str("(\n    ");
    }
    expression(&mut draw, 0, &mut text);
    if bracketed {
        text.push_str("\n)");
    }
    if draw.next().is_multiple_of(4) {
        let piece = draw.pick(&PIECES);
        let mut at = draw.next() * 256 % (text.len() + 1);
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        text.insert_str(at, piece);
    }
    text
}

struct NotationTarget {
    descriptor: TargetDescriptor,
}

impl NotationTarget {
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

fn invalid(detail: &'static str) -> FuzzError {
    FuzzError::new(FuzzErrorKind::InvalidCase, "notation case", detail)
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

/// The place `offset` bytes into `text`, counting lines and characters as the reader is meant to:
/// a line ends at `\n`, at `\r\n` and at a `\r` alone.
fn place(text: &str, offset: usize) -> Option<Position> {
    let before = text.get(..offset)?;
    let (mut line, mut column) = (1, 1);
    let mut characters = before.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' if characters.peek() == Some(&'\n') => column += 1,
            '\n' | '\r' => {
                line += 1;
                column = 1;
            }
            _ => column += 1,
        }
    }
    Some(Position::new(line, column, offset))
}

impl CaseTarget for NotationTarget {
    type Case = String;

    fn descriptor(&self) -> &TargetDescriptor {
        &self.descriptor
    }

    fn decode(&self, input: &[u8]) -> Result<String> {
        if input.len() as u64 > self.descriptor.limits().bytes() {
            return Err(invalid("the case is larger than the target admits"));
        }
        String::from_utf8(input.to_vec()).map_err(|_| invalid("the case is not UTF-8"))
    }

    fn encode(&self, case: &String) -> Result<Vec<u8>> {
        Ok(case.as_bytes().to_vec())
    }

    fn generate(&self, input: &[u8]) -> Result<Generation<String>> {
        Ok(Generation::Generated(generate(input)))
    }

    fn evaluate(&self, case: &String) -> Result<CaseOutcome> {
        match parse(case) {
            Err(error) => {
                let span = error.span();
                let (start, end) = (span.start(), span.end());
                if start.offset() > end.offset() || end.offset() > case.len() {
                    return failure(
                        WITHIN,
                        format!("{error:?} lies outside {} bytes", case.len()),
                    );
                }
                for position in [start, end] {
                    if place(case, position.offset()) != Some(position) {
                        return failure(
                            WITHIN,
                            format!("{position:?} is not where its offset is in {case:?}"),
                        );
                    }
                }
            }
            Ok(source) => {
                let printed = print(&source.expression);
                let again = match parse(&printed) {
                    Ok(again) => again,
                    Err(error) => {
                        return failure(FIXED, format!("{printed:?} does not read: {error}"));
                    }
                };
                let reprinted = print(&again.expression);
                if reprinted != printed {
                    return failure(FIXED, format!("{printed:?} prints as {reprinted:?}"));
                }
            }
        }
        CaseOutcome::evaluated(vec![property(WITHIN), property(FIXED)])
    }
}

#[test]
fn refusals_lie_within_the_text_and_printing_is_a_fixed_point() {
    let target = NotationTarget::new();
    let strategy = property::bytes(256, target.descriptor().limits());
    let config = PropertyConfig {
        cases: 4000,
        max_rejections: 0,
        max_shrink_iterations: 2000,
        seed: 0x0da9_0240,
    };
    let report =
        run_property(&target, &strategy, config, &mut |_| Ok(())).expect("the run completes");
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert_eq!(report.counts.evaluated(), 4000);
}

#[test]
fn a_generated_case_reads_back_through_its_codec() {
    let target = NotationTarget::new();
    let case = generate(&[1, 5, 2, 0, 0, 2, 1, 1, 3, 2, 1, 11, 3, 1]);
    assert_eq!(case, "lf.col(\"a\")");
    let bytes = target.encode(&case).expect("encodes");
    assert_eq!(target.decode(&bytes).expect("decodes"), case);
}

#[test]
fn a_third_or_more_of_generated_cases_read() {
    let target = NotationTarget::new();
    let mut state: u64 = 0x0da9_0240;
    let mut read = 0;
    for _ in 0..2000 {
        let entropy: Vec<u8> = (0..64)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                (state >> 33) as u8
            })
            .collect();
        let Generation::Generated(case) = target.generate(&entropy).expect("generates") else {
            panic!("the generator rejects no entropy");
        };
        read += usize::from(parse(&case).is_ok());
    }
    assert!(read * 3 > 2000, "{read} of 2000 cases read");
}

//! The literals a document can write, and the values at the edges of the dtypes that a rule which
//! reads a literal is crossed with. The edges are worked out from each class's bounds and
//! precision, so a class the tables gain brings its edges with it.

use dageist_engine::allowed::Representative;
use dageist_engine::document::Dtype;
use dageist_fuzz::grid::Choices;
use serde_json::{Value, json};

use crate::doc;

/// A value a literal is written as, which is one kind of the document's: a whole number, a number
/// with a fraction, a text, a flag or a null.
#[derive(Debug, Clone, PartialEq)]
pub enum Written {
    Integer(i64),
    Float(f64),
    Text(String),
    Flag(bool),
    Null,
}

impl Written {
    /// The value as a document writes an argument.
    #[must_use]
    pub fn json(&self) -> Value {
        match self {
            Self::Integer(number) => json!({ "integer": number }),
            Self::Float(number) => json!({ "float": number }),
            Self::Text(text) => json!({ "string": text }),
            Self::Flag(flag) => json!({ "flag": flag }),
            Self::Null => json!({ "flag": null }),
        }
    }

    /// The value for a label: `1.5`, `"-1"`, `true` or `null`, the kind kept apart by its
    /// writing, so a whole float is `300.0`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Integer(number) => number.to_string(),
            Self::Float(number) => format!("{number:?}"),
            Self::Text(text) => format!("{text:?}"),
            Self::Flag(flag) => flag.to_string(),
            Self::Null => "null".to_owned(),
        }
    }

    /// Whether the value is a number, or a text of one, that is at least `limit`.
    #[must_use]
    pub fn is_at_least(&self, limit: f64) -> bool {
        match self {
            Self::Integer(number) => (*number as f64) >= limit,
            Self::Float(number) => *number >= limit,
            Self::Text(text) => text
                .trim()
                .parse::<f64>()
                .is_ok_and(|number| number >= limit),
            Self::Flag(_) | Self::Null => false,
        }
    }

    /// The call `polars.lit` of the value, given the dtype `word` where it is one.
    #[must_use]
    pub fn call(&self, word: Option<&str>) -> Value {
        let mut arguments = json!({ "value": self.json() });
        if let Some(word) = word {
            arguments["dtype"] = doc::dtype(word);
        }
        doc::call("polars.lit", arguments)
    }

    /// The expression argument that is the literal.
    #[must_use]
    pub fn expression(&self, word: Option<&str>) -> Value {
        doc::chain(self.call(word), Vec::new())
    }

    /// The label of the literal of the value given the dtype `word`.
    #[must_use]
    pub fn label_as(&self, word: Option<&str>) -> String {
        word.map_or_else(
            || self.label(),
            |word| format!("{} as {word}", self.label()),
        )
    }
}

/// The literal a cell writes: its value, and the dtype it is given where it has one.
#[derive(Debug, Clone, Default)]
pub struct Literal {
    pub value: Option<Written>,
    pub word: Option<String>,
}

impl Literal {
    /// The call `polars.lit` of what the cell chose.
    ///
    /// # Panics
    ///
    /// When no value is chosen.
    #[must_use]
    pub fn call(&self) -> Value {
        self.written().call(self.word.as_deref())
    }

    /// The expression argument that is the literal.
    ///
    /// # Panics
    ///
    /// When no value is chosen.
    #[must_use]
    pub fn expression(&self) -> Value {
        self.written().expression(self.word.as_deref())
    }

    /// The value chosen.
    ///
    /// # Panics
    ///
    /// When no value is chosen.
    #[must_use]
    pub fn written(&self) -> &Written {
        self.value.as_ref().expect("a value is chosen")
    }
}

/// The lowest and the highest value of a class of whole numbers, and none for another class.
#[must_use]
pub const fn whole_bounds(class: Representative) -> Option<(i128, u128)> {
    Some(match class {
        Representative::Int8 => (i8::MIN as i128, i8::MAX as u128),
        Representative::Int16 => (i16::MIN as i128, i16::MAX as u128),
        Representative::Int32 => (i32::MIN as i128, i32::MAX as u128),
        Representative::Int64 => (i64::MIN as i128, i64::MAX as u128),
        Representative::Int128 => (i128::MIN, i128::MAX as u128),
        Representative::UInt8 => (0, u8::MAX as u128),
        Representative::UInt16 => (0, u16::MAX as u128),
        Representative::UInt32 => (0, u32::MAX as u128),
        Representative::UInt64 => (0, u64::MAX as u128),
        Representative::UInt128 => (0, u128::MAX),
        _ => return None,
    })
}

/// The decimal text of `text`, a whole number written in decimal, and one more than its magnitude,
/// which no width of number limits.
fn one_more(text: &str) -> String {
    let mut digits: Vec<u8> = text.bytes().rev().map(|digit| digit - b'0').collect();
    let mut carry = true;
    for digit in &mut digits {
        if !carry {
            break;
        }
        *digit += 1;
        carry = *digit == 10;
        if carry {
            *digit = 0;
        }
    }
    if carry {
        digits.push(1);
    }
    digits
        .iter()
        .rev()
        .map(|digit| char::from(b'0' + digit))
        .collect()
}

/// The edges of a class of whole numbers as the decimal text of each: its lowest and highest
/// value, and one past each.
#[must_use]
pub fn whole_edges_of(class: Representative) -> Vec<String> {
    let Some((lowest, highest)) = whole_bounds(class) else {
        return Vec::new();
    };
    let below = if lowest == 0 {
        "-1".to_owned()
    } else {
        format!("-{}", one_more(&lowest.unsigned_abs().to_string()))
    };
    vec![
        below,
        lowest.to_string(),
        highest.to_string(),
        one_more(&highest.to_string()),
    ]
}

/// The largest finite value of a class of floats, and the number of bits of its significand,
/// with the one it does not store; none for another class.
#[must_use]
pub const fn float_bounds(class: Representative) -> Option<(f64, u32)> {
    match class {
        Representative::Float16 => Some((65_504.0, 11)),
        Representative::Float32 => Some((f32::MAX as f64, f32::MANTISSA_DIGITS)),
        Representative::Float64 => Some((f64::MAX, f64::MANTISSA_DIGITS)),
        _ => None,
    }
}

/// The classes of whole numbers.
pub fn whole_classes() -> impl Iterator<Item = Representative> {
    Representative::MEASURED
        .into_iter()
        .filter(|class| whole_bounds(*class).is_some())
}

/// The classes of floats.
pub fn float_classes() -> impl Iterator<Item = Representative> {
    Representative::MEASURED
        .into_iter()
        .filter(|class| float_bounds(*class).is_some())
}

/// The edges of every class of whole numbers, as the decimal text of each.
fn whole_edges() -> Vec<String> {
    whole_classes().flat_map(whole_edges_of).collect()
}

/// The limits below which every whole number is a float of each class exactly, `2^bits`, and
/// the numbers beside them.
fn exact_limits() -> Vec<i128> {
    let mut numbers = Vec::new();
    for class in float_classes() {
        let (_, bits) = float_bounds(class).expect("a class of floats");
        let limit = 1_i128 << bits;
        numbers.extend([limit - 1, limit, limit + 1, -limit]);
    }
    numbers
}

/// The whole numbers a document writes that sit at an edge: the edges of every class of whole
/// numbers that an `i64` holds, the limits of exactness of the floats and the numbers beside them,
/// and the small numbers every rule meets. An edge no `i64` holds is a text of [`edge_texts`].
#[must_use]
pub fn edge_integers() -> Choices<Written> {
    let mut numbers: Vec<i64> = vec![-1, 0, 1, 2, 127, 128, 255, 256, 300, 70_000];
    numbers.extend(
        whole_edges()
            .iter()
            .filter_map(|edge| edge.parse::<i64>().ok())
            .chain(
                exact_limits()
                    .into_iter()
                    .filter_map(|number| i64::try_from(number).ok()),
            ),
    );
    numbers.sort_unstable();
    numbers.dedup();
    Choices::of(
        "value",
        numbers.into_iter().map(|number| {
            let written = Written::Integer(number);
            (written.label(), written)
        }),
    )
}

/// The numbers with a fraction a document writes at an edge: zero and minus zero, whole floats,
/// fractions that round to a whole number either way, the edges of every class of whole numbers as
/// floats, the limits of exactness of every class of floats and the numbers beside them, the
/// largest finite value of each class of floats and a number past it, and the smallest numbers
/// above zero. NaN and the infinities are left out: a document writes a number as JSON, which
/// holds neither.
#[must_use]
pub fn edge_floats() -> Choices<Written> {
    let mut numbers: Vec<f64> = vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        1.5,
        -1.5,
        2.5,
        0.999_999_9,
        100.0,
        300.0,
        f64::MIN_POSITIVE,
        5e-324,
        f64::from(f32::MIN_POSITIVE),
    ];
    for edge in whole_edges() {
        numbers.push(
            edge.parse::<f64>()
                .expect("a whole number reads as a float"),
        );
    }
    for limit in exact_limits() {
        numbers.push(limit as f64);
        numbers.push(limit as f64 + 0.5);
    }
    for class in float_classes() {
        let (largest, _) = float_bounds(class).expect("a class of floats");
        numbers.push(largest);
        numbers.push(-largest);
        if largest < f64::MAX {
            // Halfway to the next power of two rounds to the infinity of the class.
            numbers.push(largest * 1.000_5);
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut axis = Choices::new("value");
    for number in numbers {
        let written = Written::Float(number);
        if seen.insert(written.label()) {
            axis = axis.with(written.label(), written);
        }
    }
    axis.leave_out(
        "NaN",
        "a document writes a number as JSON, which holds no NaN",
    )
    .leave_out(
        "inf",
        "a document writes a number as JSON, which holds no infinity",
    )
    .leave_out(
        "-inf",
        "a document writes a number as JSON, which holds no infinity",
    )
}

/// The texts a rule that reads a text may meet: the edges of every class of whole numbers as
/// text, those no `i64` holds among them, numbers that are not whole, and words that are no number
/// or are one of another kind.
#[must_use]
pub fn edge_texts() -> Choices<Written> {
    let mut texts: Vec<String> = [
        "",
        "1d",
        "abc",
        "0",
        "-0",
        "1",
        "-1",
        "1.5",
        "300",
        "1e3",
        " 1",
        "true",
        "NaN",
        "inf",
        "-inf",
        "100000000",
        "9007199254740993",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    texts.extend(whole_edges());
    let mut seen = std::collections::BTreeSet::new();
    let mut axis = Choices::new("value");
    for text in texts {
        let written = Written::Text(text);
        if seen.insert(written.label()) {
            axis = axis.with(written.label(), written);
        }
    }
    axis
}

/// The flags and the null.
#[must_use]
pub fn flags_and_null() -> Choices<Written> {
    Choices::of(
        "value",
        [Written::Flag(true), Written::Flag(false), Written::Null]
            .into_iter()
            .map(|written| (written.label(), written)),
    )
}

/// Every value of every kind a literal is written as, at its edges.
#[must_use]
pub fn every_value() -> Choices<Written> {
    edge_integers()
        .and(edge_floats())
        .and(edge_texts())
        .and(flags_and_null())
}

/// The words of the dtypes a literal can be given in a document, each a scalar class whose dtype
/// the document can write, after `none`, a literal given no dtype. A class whose dtype the document
/// cannot write, and a nested one, which the editor has no literal for, are left out.
#[must_use]
pub fn dtype_words() -> Choices<Option<String>> {
    let mut axis = Choices::new("dtype").with("none", None);
    for class in Representative::MEASURED {
        if class.is_nested() || class == Representative::Struct {
            axis = axis.leave_out(
                class.name(),
                "the editor types a literal in of a scalar dtype only",
            );
            continue;
        }
        match Dtype::new(class.dtype()) {
            Ok(word) => axis = axis.with(class.name(), Some(word.to_string())),
            Err(error) => {
                axis = axis.leave_out(
                    class.name(),
                    format!("a document holds no dtype word for it: {error}"),
                );
            }
        }
    }
    axis
}

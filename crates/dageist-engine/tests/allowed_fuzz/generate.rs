//! Making a case from entropy: a step document that holds one chain of
//! methods in a position of a step, with the nestings the tables cover, and the
//! options its calls are given.
//!
//! Every document is valid as a document: each call stands where the catalogue
//! lets it, its arguments are accepted, and the links of a condition follow one
//! another. What is left to chance is what the tables are about: which dtypes
//! meet, which methods follow which, where an expression stands and which
//! options are set together. A call that the tables refuse is as likely to be
//! chosen as one they allow after a few tries that look for one they allow, so
//! that chains go several links deep and refusals are covered too.
//!
//! The generator asks [`allowed::offer`] only to steer; the verdict on a case
//! is [`super::predict`]'s, and Polars' is [`super::observe`]'s.

use dageist_engine::allowed::{
    self, After, Chosen, ConditionState, Context, Place as OfferPlace, Position as Slot,
    Representative, Shape, Verdict as OfferVerdict,
};
use dageist_engine::catalog::{
    self, Applies, Counting, Entry, Kind, ParameterDefault, Shape as ParamShape, ValueType,
};
use dageist_engine::document::{Call, Document, Dtype, Expression, Start, Step, StepId, Value};
use dageist_fuzz::generation::Entropy;

use super::case::{Case, STEP};
use super::frames::{ON_COLUMNS, OTHER, key_column};

/// Fewest bytes of entropy a case is made from. A shorter input is discarded,
/// for what it lacks would be read as zeros, the same choice every time.
pub const LEAST_ENTROPY: usize = 96;

/// Draws numbers from entropy, each below a count that may be of any size. Past its end, or
/// too near it to hold the bytes a draw needs, it gives zeros, the first choice of everything.
struct Draw<'a> {
    entropy: Entropy<'a>,
}

impl<'a> Draw<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            entropy: Entropy::new(bytes),
        }
    }

    /// A number below `count`, each as likely as any other. It reads as many bytes as `count`
    /// needs, as one number, and reads again where that number is at or above the largest
    /// multiple of `count` that the bytes can hold, since reducing it would give the numbers
    /// below the remainder once more than the rest. A count of one or of a power of two that a
    /// byte holds reads one byte and keeps its low bits.
    fn below(&mut self, count: usize) -> usize {
        debug_assert!(count > 0);
        let width = (usize::BITS - (count - 1).leading_zeros())
            .div_ceil(8)
            .max(1) as usize;
        let span = 1_u128 << (8 * width);
        let count = count as u128;
        let limit = span - span % count;
        loop {
            let Ok(bytes) = self.entropy.take(width) else {
                return 0;
            };
            let value = bytes
                .iter()
                .fold(0_u128, |number, byte| number << 8 | u128::from(*byte));
            if value < limit {
                return (value % count) as usize;
            }
        }
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }

    /// One of `items`, the first more often: the weights say how often.
    fn weighted<T: Copy>(&mut self, items: &[(usize, T)]) -> T {
        let total: usize = items.iter().map(|(weight, _)| weight).sum();
        let mut left = self.below(total);
        for (weight, item) in items {
            if left < *weight {
                return *item;
            }
            left -= weight;
        }
        items[0].1
    }
}

/// Where the chain under test stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Host {
    Select,
    WithColumns,
    Filter,
    Sort,
    JoinLeft,
    JoinRight,
    /// A join's `on`, which both tables read.
    JoinOn,
    Pivot,
    /// A step that holds no chain: the step itself is what is under test.
    Plain(Plain),
}

/// The steps that are drawn for themselves: they hold values and the names of columns, not
/// expressions, so what is left to chance is the arguments and the dtypes of the table they run
/// on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plain {
    Reverse,
    Head,
    Limit,
    Tail,
    Slice,
    First,
    Last,
    GatherEvery,
    Drop,
    DropNulls,
    DropNans,
    FillNan,
    WithRowIndex,
}

impl Plain {
    const ALL: [Self; 13] = [
        Self::Reverse,
        Self::Head,
        Self::Limit,
        Self::Tail,
        Self::Slice,
        Self::First,
        Self::Last,
        Self::GatherEvery,
        Self::Drop,
        Self::DropNulls,
        Self::DropNans,
        Self::FillNan,
        Self::WithRowIndex,
    ];

    const fn id(self) -> &'static str {
        match self {
            Self::Reverse => "LazyFrame.reverse",
            Self::Head => "LazyFrame.head",
            Self::Limit => "LazyFrame.limit",
            Self::Tail => "LazyFrame.tail",
            Self::Slice => "LazyFrame.slice",
            Self::First => "LazyFrame.first",
            Self::Last => "LazyFrame.last",
            Self::GatherEvery => "LazyFrame.gather_every",
            Self::Drop => "LazyFrame.drop",
            Self::DropNulls => "LazyFrame.drop_nulls",
            Self::DropNans => "LazyFrame.drop_nans",
            Self::FillNan => "LazyFrame.fill_nan",
            Self::WithRowIndex => "LazyFrame.with_row_index",
        }
    }
}

/// The weight of each step that is drawn for itself, beside the hundred and eight that the hosts of
/// a chain weigh together.
const PLAIN_WEIGHT: usize = 2;

impl Host {
    const ALL: [(usize, Self); 21] = [
        (22, Self::Select),
        (8, Self::WithColumns),
        (16, Self::Filter),
        (14, Self::Sort),
        (12, Self::JoinLeft),
        (10, Self::JoinRight),
        (8, Self::JoinOn),
        (18, Self::Pivot),
        (PLAIN_WEIGHT, Self::Plain(Plain::Reverse)),
        (PLAIN_WEIGHT, Self::Plain(Plain::Head)),
        (PLAIN_WEIGHT, Self::Plain(Plain::Limit)),
        (PLAIN_WEIGHT, Self::Plain(Plain::Tail)),
        (PLAIN_WEIGHT, Self::Plain(Plain::Slice)),
        (PLAIN_WEIGHT, Self::Plain(Plain::First)),
        (PLAIN_WEIGHT, Self::Plain(Plain::Last)),
        (PLAIN_WEIGHT, Self::Plain(Plain::GatherEvery)),
        (PLAIN_WEIGHT, Self::Plain(Plain::Drop)),
        (PLAIN_WEIGHT, Self::Plain(Plain::DropNulls)),
        (PLAIN_WEIGHT, Self::Plain(Plain::DropNans)),
        (PLAIN_WEIGHT, Self::Plain(Plain::FillNan)),
        (PLAIN_WEIGHT, Self::Plain(Plain::WithRowIndex)),
    ];

    fn step(self) -> &'static str {
        match self {
            Self::Plain(plain) => plain.id(),
            Self::Select => "LazyFrame.select",
            Self::WithColumns => "LazyFrame.with_columns",
            Self::Filter => "LazyFrame.filter",
            Self::Sort => "LazyFrame.sort",
            Self::JoinLeft | Self::JoinRight | Self::JoinOn => "LazyFrame.join",
            Self::Pivot => "LazyFrame.pivot",
        }
    }

    /// The parameter of the step that holds the chain.
    fn parameter(self) -> &'static str {
        match self {
            Self::Select | Self::WithColumns => "exprs",
            Self::Filter => "predicates",
            Self::Sort => "by",
            Self::JoinLeft => "left_on",
            Self::JoinRight => "right_on",
            Self::JoinOn => "on",
            Self::Pivot => "aggregate_function",
            Self::Plain(plain) => unreachable!("{} holds no chain", plain.id()),
        }
    }

    /// The names of the columns an expression reads here: the left table's,
    /// the other table's for `right_on`, and none for a pivot's group. `on`
    /// reads both tables, which share only `u`; a column of the left table alone
    /// is one the other table lacks, which is Polars' to refuse, and is drawn
    /// for the few cases that check it.
    fn columns(self) -> &'static [&'static str] {
        match self {
            Self::JoinRight => &["ra", "rb", "rf", "rk", "rg", "rh"],
            Self::JoinOn => &["u", "u", "u", "u", "u", "u", "u", "a", "k"],
            Self::Pivot | Self::Plain(_) => &[],
            _ => &["a", "b", "f", "k", "g", "h"],
        }
    }
}

/// What a chain must give where it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Goal {
    Anything,
    Boolean,
}

/// What a chain has after a call the tables allowed: its dtype class and shape;
/// `None` where they refused the call.
type Next = Option<(Option<Representative>, Shape)>;

/// The dtype class of each expression a call was given, by its parameter.
type Operands = Vec<(String, Option<Representative>)>;

/// A chain built, and what the tables say it has.
struct Built {
    expression: Expression,
    class: Option<Representative>,
}

struct Gen<'a> {
    draw: Draw<'a>,
    /// The methods a focused campaign draws more often.
    focus: Option<&'a Focus>,
    receiver: Representative,
    argument: Representative,
    host: Host,
    /// How many more calls may be added.
    budget: usize,
    /// Whether a number typed without a dtype may be drawn. It is not among the values of a
    /// condition: alone there Polars cannot resolve it where it runs, in a sort key, for one,
    /// which the tables cannot say.
    untyped_numbers: bool,
    /// What the tables said of the function that makes a moment that a chain was just started
    /// from: how many values it gives, or `None` where they refused it.
    started: Option<Option<Shape>>,
}

fn entry(id: &str) -> &'static Entry {
    catalog::entry(id).unwrap_or_else(|| panic!("the catalogue has no entry {id}"))
}

fn call(id: &str, given: Vec<(&str, Value)>) -> Call {
    Call::new(entry(id), given).unwrap_or_else(|refusal| {
        panic!("the generator built a call of {id} the document refuses: {refusal}")
    })
}

fn column(name: &str) -> Call {
    call(
        "polars.col",
        vec![("name", Value::Columns(vec![name.to_owned()]))],
    )
}

fn dtype_value(class: Representative) -> Option<Value> {
    Dtype::new(class.dtype()).ok().map(Value::Dtype)
}

/// The classes a document can hold as a dtype, which a literal may be given.
fn literal_classes() -> Vec<Representative> {
    Representative::MEASURED
        .into_iter()
        .filter(|class| Dtype::new(class.dtype()).is_ok())
        .collect()
}

/// How many of the first of the [`NUMBERED`] classes are numbers.
const NUMERIC_CLASSES: usize = 10;

/// The classes the whole number 1 is cast to as a literal, the numbers first.
const NUMBERED: [Representative; 17] = [
    Representative::Int8,
    Representative::Int16,
    Representative::Int32,
    Representative::Int64,
    Representative::UInt8,
    Representative::UInt16,
    Representative::UInt32,
    Representative::UInt64,
    Representative::Float32,
    Representative::Float64,
    Representative::Boolean,
    Representative::String,
    Representative::Date,
    Representative::Datetime,
    Representative::DatetimeZoned,
    Representative::Duration,
    Representative::Time,
];

/// The methods of an expression, which a chain draws its links from.
fn methods() -> Vec<&'static Entry> {
    catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
        .collect()
}

/// Whether a method gives a count of values that the data or the arguments decide, which a
/// campaign draws only where a focus names it: the shape of such a count is the first of several
/// that the catalogue gains, and the campaigns that check them are not a gate of the engine's tests
/// until every shape exists. [`Counting`](catalog::Counting) says which methods, and `is_in`, whose
/// count follows the dtype of the values it is given, is one too.
fn is_held_back(entry: &Entry) -> bool {
    catalog::Counting::of(entry, &[]).is_some() || entry.id().as_str() == "Expr.is_in"
}

/// The functions that start a chain and take expressions, which a chain may start from besides a
/// column, a literal, `element` and a condition: the ones that make a date, a time, a moment, a
/// length of time or a range of them.
fn function_starts() -> Vec<&'static Entry> {
    catalog::entries()
        .iter()
        .filter(|entry| {
            entry.applies() == Applies::Function
                && entry
                    .parameters()
                    .iter()
                    .any(|parameter| matches!(parameter.kind(), Kind::Expression(_)))
        })
        .collect()
}

/// The name of a function that starts a chain as Polars gives it, which is its identifier without
/// `polars.`: `date_range`.
fn start_name(entry: &Entry) -> &str {
    let id = entry.id().as_str();
    id.strip_prefix("polars.").unwrap_or(id)
}

/// The name of a method as Polars gives it, which is its identifier without `Expr.`:
/// `dt.replace`.
fn method_name(entry: &Entry) -> &str {
    let id = entry.id().as_str();
    id.strip_prefix("Expr.").unwrap_or(id)
}

/// How often, in a hundred, a focused campaign puts back a method out of its focus for one in it.
const FOCUS_PERCENT: usize = 25;

/// How often, in a hundred, a campaign focused on steps draws one of them for the case.
const FOCUS_STEP_PERCENT: usize = 60;

/// The steps a case may be drawn for, which a focus names by their identifiers.
fn plain_steps() -> Vec<&'static Entry> {
    Plain::ALL.iter().map(|plain| entry(plain.id())).collect()
}

/// The methods of an expression and the functions that start a chain a campaign focuses on. Where
/// a chain draws a method that is not one of them, it draws one that is, one time in four, as it
/// does where it draws a start, and the rest of the case is drawn as without a focus. A focus on
/// every method changes nothing. The target draws again a case that holds none of them.
#[derive(Debug, Clone)]
pub struct Focus {
    /// In the catalogue's order, each once.
    methods: Vec<&'static Entry>,
    /// The steps that are drawn for themselves, in the catalogue's order, each once.
    steps: Vec<&'static Entry>,
    /// The functions that start a chain, in the catalogue's order, each once.
    starts: Vec<&'static Entry>,
}

impl Focus {
    /// The focus that `text` names: methods, functions that start a chain and steps, separated by
    /// commas, each by its name, `dt.replace` or `date_range`, or by the catalogue's identifier,
    /// `Expr.dt.replace` or `polars.date_range`; a name that ends in a dot, `dt.`, names every
    /// method whose name it begins. A step that is drawn for itself is named by its identifier,
    /// `LazyFrame.head`, or by `LazyFrame.` for all of them, as a method of a table has the name of
    /// a method of an expression. Text that names nothing is no focus.
    ///
    /// # Errors
    ///
    /// Where a name is of no method of an expression, of no function that makes a moment and of no
    /// step that is drawn for itself in the catalogue, it says which.
    pub fn parse(text: &str) -> Result<Option<Self>, String> {
        let methods = methods();
        let starts = function_starts();
        let steps = plain_steps();
        let mut chosen: Vec<&'static Entry> = Vec::new();
        for name in text
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let short = name.strip_prefix("Expr.").unwrap_or(name);
            let mut named: Vec<&'static Entry> = methods
                .iter()
                .copied()
                .filter(|entry| {
                    if short.ends_with('.') {
                        method_name(entry).starts_with(short)
                    } else {
                        method_name(entry) == short
                    }
                })
                .collect();
            let function = name.strip_prefix("polars.").unwrap_or(name);
            named.extend(
                starts
                    .iter()
                    .copied()
                    .filter(|entry| !function.ends_with('.') && start_name(entry) == function),
            );
            named.extend(steps.iter().copied().filter(|entry| {
                let id = entry.id().as_str();
                if name == "LazyFrame." {
                    true
                } else {
                    id == name
                }
            }));
            if named.is_empty() {
                return Err(format!(
                    "DAGEIST_FUZZ_FOCUS names `{name}`, which is no method of an expression, no \
                     function that makes a moment and no step in the catalogue: name a method as \
                     `dt.replace`, the methods of a namespace as `dt.`, a function as \
                     `date_range` and a step as `LazyFrame.head`"
                ));
            }
            chosen.extend(named);
        }
        if chosen.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            methods: methods
                .into_iter()
                .filter(|entry| chosen.iter().any(|named| named.id() == entry.id()))
                .collect(),
            steps: steps
                .into_iter()
                .filter(|entry| chosen.iter().any(|named| named.id() == entry.id()))
                .collect(),
            starts: starts
                .into_iter()
                .filter(|entry| chosen.iter().any(|named| named.id() == entry.id()))
                .collect(),
        }))
    }

    /// The names of the methods and the functions in focus, separated by commas.
    pub fn names(&self) -> String {
        self.methods
            .iter()
            .map(|entry| method_name(entry))
            .chain(self.starts.iter().map(|entry| start_name(entry)))
            .chain(self.steps.iter().map(|entry| entry.id().as_str()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn holds(&self, entry: &Entry) -> bool {
        self.methods
            .iter()
            .chain(&self.starts)
            .chain(&self.steps)
            .any(|held| held.id() == entry.id())
    }

    /// Whether `document` calls a method in focus anywhere: in a step, a chain or what a call is
    /// given.
    pub fn is_held_by(&self, document: &Document) -> bool {
        document
            .steps()
            .iter()
            .any(|step| self.is_in_call(step.call()))
    }

    fn is_in_call(&self, call: &Call) -> bool {
        self.holds(call.entry())
            || call
                .arguments()
                .iter()
                .any(|given| self.is_in_value(given.value()))
    }

    fn is_in_value(&self, value: &Value) -> bool {
        match value {
            Value::Expression(expression) => {
                matches!(expression.start(), Start::Call(start) if self.is_in_call(start))
                    || expression.links().iter().any(|link| self.is_in_call(link))
            }
            Value::Many(items) => items.iter().any(|item| self.is_in_value(item)),
            Value::Pairs(pairs) => pairs
                .iter()
                .any(|(key, value)| self.is_in_value(key) || self.is_in_value(value)),
            _ => false,
        }
    }
}

pub fn generate(bytes: &[u8], focus: Option<&Focus>) -> Option<Case> {
    if bytes.len() < LEAST_ENTROPY {
        return None;
    }
    let mut draw = Draw::new(bytes);
    let receiver = draw.pick(&Representative::MEASURED);
    let argument = draw.pick(&Representative::MEASURED);
    let host = draw.weighted(&Host::ALL);
    // A campaign focused on steps draws one of them for most cases, and nothing more is drawn
    // for one that is focused on methods only.
    let host = match focus.filter(|focus| !focus.steps.is_empty()) {
        Some(focus) if draw.chance(FOCUS_STEP_PERCENT) => {
            let step = draw.pick(&focus.steps);
            Plain::ALL
                .into_iter()
                .find(|plain| plain.id() == step.id().as_str())
                .map_or(host, Host::Plain)
        }
        _ => host,
    };
    let mut generator = Gen {
        draw,
        focus,
        receiver,
        argument,
        host,
        budget: 18,
        untyped_numbers: true,
        started: None,
    };
    let document = generator.document();
    Some(Case {
        receiver,
        argument,
        document,
    })
}

impl Gen<'_> {
    fn document(&mut self) -> Document {
        if let Host::Plain(plain) = self.host {
            return self.plain_document(plain);
        }
        let host = self.host;
        let step_id = host.step();
        let mut slots = vec![
            Slot::of_parameter(step_id, host.parameter()).expect("a host holds a named position"),
        ];
        let goal = if host == Host::Filter {
            Goal::Boolean
        } else {
            Goal::Anything
        };
        let first = self.chain(&mut slots, 0, goal);
        let as_list = |expression: Expression| Value::Many(vec![Value::Expression(expression)]);
        let many = |expressions: Vec<Expression>| {
            Value::Many(expressions.into_iter().map(Value::Expression).collect())
        };
        let (held, others) = match host {
            Host::JoinLeft | Host::JoinRight | Host::JoinOn => {
                self.join_keys(first, &mut slots, host == Host::JoinOn)
            }
            _ => (vec![first.expression], Vec::new()),
        };
        let held_one = || held[0].clone();
        let others = || {
            many(
                others
                    .iter()
                    .map(|name| {
                        Expression::new(Start::Call(column(name)), Vec::new()).expect("a column")
                    })
                    .collect(),
            )
        };
        let mut given: Vec<(&str, Value)> = match host {
            Host::Select | Host::WithColumns => vec![("exprs", many(held.clone()))],
            Host::Filter => vec![("predicates", as_list(held_one()))],
            Host::Sort => vec![("by", as_list(held_one()))],
            Host::JoinLeft => vec![
                ("other", Value::Table(OTHER.to_owned())),
                ("left_on", many(held.clone())),
                ("right_on", others()),
            ],
            Host::JoinRight => vec![
                ("other", Value::Table(OTHER.to_owned())),
                ("left_on", others()),
                ("right_on", many(held.clone())),
            ],
            Host::JoinOn => vec![
                ("other", Value::Table(OTHER.to_owned())),
                ("on", many(held.clone())),
            ],
            Host::Pivot => vec![
                ("on", Value::Columns(vec!["h".to_owned()])),
                ("on_columns", Value::Table(ON_COLUMNS.to_owned())),
                ("index", Value::Columns(vec!["k".to_owned()])),
                ("values", Value::Columns(vec!["a".to_owned()])),
                ("aggregate_function", Value::Expression(held_one())),
            ],
            Host::Plain(_) => unreachable!("a step without a chain is drawn apart"),
        };
        given.extend(self.options(entry(step_id)));
        let step = Step::new(
            StepId::new(STEP).expect("an identifier"),
            call(step_id, given),
        )
        .expect("a step of a lazy frame");
        Document::new(vec![step]).expect("one step")
    }

    // -----------------------------------------------------------------------
    // Steps drawn for themselves

    /// The document of a step that holds no chain: its options, as the option table reads them,
    /// and arguments that Polars takes, drawn from the columns of the sample tables. A column the
    /// table lacks is drawn only where Polars is told to leave it: that a name is no column is
    /// Polars' to refuse, and no table says it.
    fn plain_document(&mut self, plain: Plain) -> Document {
        let id = plain.id();
        let options = self.options(entry(id));
        let strict = options
            .iter()
            .find(|(name, _)| *name == "strict")
            .and_then(|(_, value)| match value {
                Value::Flag(flag) => *flag,
                _ => None,
            })
            .unwrap_or(true);
        let mut given: Vec<(&str, Value)> = Vec::new();
        match plain {
            Plain::Reverse | Plain::First | Plain::Last => {}
            Plain::GatherEvery => {
                // Polars refuses an `n` of zero, which no table says.
                let every = self.draw.pick(&[1_i64, 2, 3, 9]);
                given.push(("n", Value::Integer(every)));
                if self.draw.chance(50) {
                    let offset = self.draw.pick(&[0_i64, 1, 2, 9]);
                    given.push(("offset", Value::Integer(offset)));
                }
            }
            Plain::Head | Plain::Limit | Plain::Tail => {
                if self.draw.chance(70) {
                    let count = self.draw.pick(&[0_i64, 1, 2, 3, 10]);
                    given.push(("n", Value::Integer(count)));
                }
            }
            Plain::Slice => {
                let offset = self.draw.pick(&[-5_i64, -3, -1, 0, 1, 2, 5]);
                given.push(("offset", Value::Integer(offset)));
                if self.draw.chance(60) {
                    let length = self.draw.pick(&[0_i64, 1, 2, 10]);
                    given.push(("length", Value::Integer(length)));
                }
            }
            Plain::Drop => {
                let items = self.draw.below(4);
                let mut columns = Vec::new();
                for _ in 0..items {
                    let names = self.names(!strict);
                    columns.push(Value::Columns(names));
                }
                given.push(("columns", Value::Many(columns)));
            }
            Plain::DropNulls | Plain::DropNans => {
                if self.draw.chance(65) {
                    let names = self.names(false);
                    given.push(("subset", Value::Columns(names)));
                }
            }
            Plain::FillNan => {
                let value = match self.draw.below(6) {
                    0 => Value::Integer(0),
                    1 => Value::Integer(-1),
                    2 => Value::Float(0.5),
                    3 => Value::String("x".to_owned()),
                    4 => Value::Flag(Some(true)),
                    _ => Value::Flag(None),
                };
                given.push(("value", value));
            }
            Plain::WithRowIndex => {
                if self.draw.chance(50) {
                    let name = self.draw.pick(&["i", "row_number", "n"]);
                    given.push(("name", Value::String(name.to_owned())));
                }
                if self.draw.chance(50) {
                    let offset = self.draw.pick(&[0_i64, 1, 7, 1000]);
                    given.push(("offset", Value::Integer(offset)));
                }
            }
        }
        given.extend(options);
        let step = Step::new(StepId::new(STEP).expect("an identifier"), call(id, given))
            .expect("a step of a lazy frame");
        Document::new(vec![step]).expect("one step")
    }

    /// Up to three names of columns of the sample table, each once. A name the table lacks may be
    /// among them where `missing` is true, which is where Polars is told to leave such a column.
    /// The dtype of a column is no reason to leave it out: the engine's walk says which columns
    /// `drop_nans` is refused for.
    fn names(&mut self, missing: bool) -> Vec<String> {
        let mut pool: Vec<&str> = vec!["a", "b", "f", "k", "g", "h", "u"];
        if missing {
            pool.push("zz");
        }
        let count = self.draw.below(4).min(pool.len());
        let mut names = Vec::new();
        for _ in 0..count {
            let at = self.draw.below(pool.len());
            names.push(pool.remove(at).to_owned());
        }
        names
    }

    // -----------------------------------------------------------------------
    // Join keys

    /// The keys a join holds on the side under test, the `first` and sometimes a
    /// second, and the names of the other table's columns they are paired with.
    /// The second is made to give the first's output name often, so that the
    /// pair of names repeats where the other side repeats its column too. A
    /// column of the other table is shared by two keys only where the dtypes of
    /// the keys are the same, for Polars refuses a join of different ones, which
    /// is no matter of the tables. `on` has no other side: it pairs each key with
    /// itself.
    fn join_keys(
        &mut self,
        first: Built,
        slots: &mut Vec<Slot>,
        on: bool,
    ) -> (Vec<Expression>, Vec<String>) {
        let mut held = vec![first.expression.clone()];
        let mut others = vec![key_column(0)];
        if self.draw.chance(30) {
            let (second, same_dtype) = self.sibling(&first, slots);
            held.push(second);
            let shared = same_dtype && self.draw.chance(70);
            others.push(if shared { key_column(0) } else { key_column(1) });
        }
        (held, if on { Vec::new() } else { others })
    }

    /// A key beside `first`: a copy of it, the column it starts from taken as one
    /// value, or its test for a null, which are named as it is; or a chain of its
    /// own. Whether it has the dtype `first` has, where the tables say so.
    fn sibling(&mut self, first: &Built, slots: &mut Vec<Slot>) -> (Expression, bool) {
        let named = match first.expression.start() {
            Start::Call(start)
                if matches!(start.entry().id().as_str(), "polars.col" | "polars.lit") =>
            {
                Some(start.clone())
            }
            _ => None,
        };
        let same_class = first.class.is_some();
        match (self.draw.below(10), named) {
            (0..=2, _) => (first.expression.clone(), same_class),
            (3..=5, Some(start)) if first.expression.links().is_empty() => {
                let link = if self.draw.chance(50) {
                    "Expr.first"
                } else {
                    "Expr.last"
                };
                let expression = Expression::new(Start::Call(start), vec![call(link, Vec::new())])
                    .expect("a column taken as one value");
                (expression, same_class)
            }
            (6, Some(start)) => {
                let expression = Expression::new(
                    Start::Call(start),
                    first
                        .expression
                        .links()
                        .iter()
                        .cloned()
                        .chain([call("Expr.is_null", Vec::new())])
                        .collect(),
                )
                .expect("a test for a null");
                (expression, first.class == Some(Representative::Boolean))
            }
            _ => {
                self.budget = 6;
                let built = self.chain(slots, 0, Goal::Anything);
                let same = built.class.is_some() && built.class == first.class;
                (built.expression, same)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Options

    /// The options an entry is given: those it must be given, and none or, as the option table
    /// reads them, one when the entry has only one that it need not be given, and two or more
    /// otherwise.
    fn options(&mut self, entry: &'static Entry) -> Vec<(&'static str, Value)> {
        let parameters: Vec<_> = entry
            .parameters()
            .iter()
            .filter(|parameter| matches!(parameter.kind(), Kind::Flag | Kind::Choice(_)))
            // Whether a join's keys fulfil `validate` depends on the rows' keys,
            // which the option table was measured with one way.
            .filter(|parameter| {
                !(entry.id().as_str() == "LazyFrame.join" && parameter.name() == "validate")
            })
            // A fill takes a strategy in place of a value, which `method_call` decides.
            .filter(|parameter| parameter.in_place_of().is_none())
            .collect();
        if !matches!(entry.applies(), Applies::Expression | Applies::Frame)
            && !function_starts()
                .iter()
                .any(|start| start.id() == entry.id())
        {
            return Vec::new();
        }
        // An option that a call must be given, as `cast_time_unit` its unit, is always given.
        let (needed, parameters): (Vec<&catalog::Parameter>, Vec<&catalog::Parameter>) = parameters
            .into_iter()
            .partition(|parameter| parameter.is_needed());
        let mut given: Vec<(&'static str, Value)> = needed
            .into_iter()
            .map(|parameter| (parameter.name(), self.option_value(parameter)))
            .collect();
        if parameters.is_empty() || !self.draw.chance(45) {
            return given;
        }
        let mut chosen: Vec<usize> = Vec::new();
        if parameters.len() == 1 {
            chosen.push(0);
        } else {
            let count = if self.draw.chance(70) {
                2
            } else {
                2 + self.draw.below(parameters.len() - 1)
            };
            let mut pool: Vec<usize> = (0..parameters.len()).collect();
            while chosen.len() < count.min(parameters.len()) {
                let at = self.draw.below(pool.len());
                chosen.push(pool.remove(at));
            }
        }
        chosen.sort_unstable();
        for at in chosen {
            let parameter = parameters[at];
            let value = self.option_value(parameter);
            // A difference that drops the values it has none before is a count that the data
            // decide, which a campaign draws only where a focus names the method.
            let held_back = entry.id().as_str() == "Expr.diff"
                && matches!(&value, Value::Choice(word) if word == "drop")
                && !self.focus.is_some_and(|focus| focus.holds(entry));
            let value = if held_back {
                Value::Choice("ignore".to_owned())
            } else {
                value
            };
            given.push((parameter.name(), value));
        }
        given
    }

    /// A word of a choice, or a setting of a flag, drawn for the option `parameter`.
    fn option_value(&mut self, parameter: &catalog::Parameter) -> Value {
        match parameter.kind() {
            Kind::Choice(words) => Value::Choice((*self.draw.pick(words)).to_owned()),
            _ => {
                let unsettable = parameter.default() == ParameterDefault::Python("None");
                match self.draw.below(if unsettable { 3 } else { 2 }) {
                    0 => Value::Flag(Some(true)),
                    1 => Value::Flag(Some(false)),
                    _ => Value::Flag(None),
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Operands

    /// A literal of a class: the whole number 1, or a null, of a dtype, or a
    /// text, a flag or a null of its own.
    fn literal(&mut self) -> (Call, Option<Representative>) {
        let classes = literal_classes();
        match self.draw.below(10) {
            0..=3 => {
                let class = self.draw.pick(&NUMBERED);
                match dtype_value(class) {
                    Some(dtype) => (
                        call(
                            "polars.lit",
                            vec![("value", Value::Integer(1)), ("dtype", dtype)],
                        ),
                        Some(class),
                    ),
                    None => self.literal_loose(),
                }
            }
            4..=5 => {
                let class = self.draw.pick(&classes);
                let dtype = dtype_value(class).expect("a class of the document");
                (
                    call(
                        "polars.lit",
                        vec![("value", Value::Flag(None)), ("dtype", dtype)],
                    ),
                    Some(class),
                )
            }
            _ => self.literal_loose(),
        }
    }

    fn literal_loose(&mut self) -> (Call, Option<Representative>) {
        match self.draw.below(if self.untyped_numbers { 5 } else { 3 }) {
            // A number typed without a dtype adapts to the operand it is used with, so its class
            // is not one the generator names: the walk says what it gives beside a column, and
            // the case is held to what Polars gives. The numbers are ones that fit a narrow
            // column and ones that do not.
            3 => {
                let number = self.draw.pick(&[1_i64, 1, 300, -1, 70_000]);
                (
                    call("polars.lit", vec![("value", Value::Integer(number))]),
                    None,
                )
            }
            4 => {
                let number = self.draw.pick(&[1.5_f64, 1.5, 1e40]);
                (
                    call("polars.lit", vec![("value", Value::Float(number))]),
                    None,
                )
            }
            0 => (
                call(
                    "polars.lit",
                    vec![("value", Value::String("1d".to_owned()))],
                ),
                Some(Representative::String),
            ),
            1 => (
                call("polars.lit", vec![("value", Value::Flag(Some(true)))]),
                Some(Representative::Boolean),
            ),
            _ => (
                call("polars.lit", vec![("value", Value::Flag(None))]),
                Some(Representative::Null),
            ),
        }
    }

    fn class_of(&self, name: &str) -> Option<Representative> {
        super::frames::column_class(name, self.receiver, self.argument)
    }

    /// A column the host can read, with its class.
    fn column(&mut self) -> (Call, Option<Representative>) {
        let names = self.host.columns();
        if names.is_empty() {
            return (call("polars.element", Vec::new()), Some(self.receiver));
        }
        let name: &str = self.draw.pick(names);
        (column(name), self.class_of(name))
    }

    /// An argument for a parameter of a method that follows a value: a column,
    /// a literal or a short chain.
    fn operand(&mut self, slots: &mut Vec<Slot>, depth: usize) -> Built {
        let single = |start: (Call, Option<Representative>)| Built {
            expression: Expression::new(Start::Call(start.0), Vec::new()).expect("a start"),
            class: start.1,
        };
        if self.host == Host::Pivot || self.host.columns().is_empty() {
            let literal = self.literal();
            return single(literal);
        }
        match self.draw.below(10) {
            0..=4 => {
                let column = self.column();
                single(column)
            }
            5..=6 => {
                let literal = self.literal();
                single(literal)
            }
            _ if depth < 2 && self.budget > 2 => self.chain(slots, depth + 1, Goal::Anything),
            _ => {
                let column = self.column();
                single(column)
            }
        }
    }

    /// A literal that stands for one value, as the parameters Polars takes
    /// only as a single value, such as the shift of `shift`, are given: the
    /// number 1 of a dtype, as the tables measured them. With `back`, a number
    /// of a signed dtype is sometimes the number below zero instead, a shift
    /// back: the tables measured the dtype, which the sign does not change, and
    /// a negative number of an unsigned dtype is not one.
    fn scalar(&mut self, back: bool) -> Built {
        self.scalar_among(&NUMBERED, back)
    }

    /// The same, of one of the `classes`.
    fn scalar_among(&mut self, classes: &[Representative], back: bool) -> Built {
        let class = self.draw.pick(classes);
        let signed = [
            Representative::Int8,
            Representative::Int16,
            Representative::Int32,
            Representative::Int64,
        ]
        .contains(&class);
        let whole = if back && signed && self.draw.chance(35) {
            -1
        } else {
            1
        };
        let literal = match dtype_value(class) {
            Some(dtype) => call(
                "polars.lit",
                vec![("value", Value::Integer(whole)), ("dtype", dtype)],
            ),
            None => call("polars.lit", vec![("value", Value::Flag(Some(true)))]),
        };
        Built {
            expression: Expression::new(Start::Call(literal), Vec::new()).expect("a start"),
            class: Some(if dtype_value(class).is_some() {
                class
            } else {
                Representative::Boolean
            }),
        }
    }

    /// What `parameter` of `method` that cuts a column is given: a count or a place, as a number
    /// typed in without a dtype or with one. A place may be counted from the end, which a
    /// signed dtype takes.
    fn cut(&mut self, method: &str, parameter: &str) -> Built {
        let from_the_end = method == "Expr.slice" && parameter == "offset";
        let number = if from_the_end && self.draw.chance(25) {
            -1
        } else {
            self.draw.pick(&[0_i64, 1, 1, 2, 2, 3, 5])
        };
        let typed = !from_the_end || number >= 0;
        let class = self.draw.pick(&NUMBERED[..4]);
        let literal = match dtype_value(class) {
            Some(dtype) if typed && self.draw.chance(50) => call(
                "polars.lit",
                vec![("value", Value::Integer(number)), ("dtype", dtype)],
            ),
            _ => call("polars.lit", vec![("value", Value::Integer(number))]),
        };
        let has_a_dtype = literal.argument("dtype").is_some();
        Built {
            expression: Expression::new(Start::Call(literal), Vec::new()).expect("a start"),
            class: has_a_dtype.then_some(class),
        }
    }

    /// A share a quantile reads: the number 1 of a numeric dtype, which is the share of the largest
    /// value, or a number from 0 to 1 typed without a dtype, which Polars reads as the number it is.
    /// A share is a number, which is all the editor offers: Polars reads text and moments as
    /// numbers in a select and refuses them where a window or a pivot's aggregate evaluates the
    /// values of a group.
    fn share(&mut self) -> Built {
        if self.draw.chance(35) {
            return self.scalar_among(&NUMBERED[..NUMERIC_CLASSES], false);
        }
        let number = self.draw.pick(&[0.0_f64, 0.25, 0.5, 0.9, 1.0]);
        Built {
            expression: Expression::new(
                Start::Call(call("polars.lit", vec![("value", Value::Float(number))])),
                Vec::new(),
            )
            .expect("a start"),
            class: Some(Representative::Float64),
        }
    }

    // -----------------------------------------------------------------------
    // Chains

    fn start(
        &mut self,
        slots: &mut Vec<Slot>,
        host_is_group: bool,
        depth: usize,
    ) -> (Call, Option<Representative>, bool) {
        // A focused campaign draws a function in focus that starts a chain, one time in four.
        let focused = self
            .focus
            .filter(|focus| !focus.starts.is_empty())
            .is_some_and(|_| self.draw.chance(FOCUS_PERCENT));
        // The start of a condition is the third element: whether it is one.
        let choice = if focused {
            5
        } else if host_is_group {
            self.draw
                .weighted(&[(70, 0_usize), (8, 1), (12, 2), (10, 3), (6, 5)])
        } else {
            self.draw
                .weighted(&[(52, 1_usize), (20, 2), (18, 3), (3, 0), (3, 4), (6, 5)])
        };
        // A condition inside a condition inside a condition is deeper than the
        // tables go, and than a test thread's stack takes in a debug build.
        let choice = if choice == 3 && depth >= 2 { 1 } else { choice };
        match choice {
            0 => (
                call("polars.element", Vec::new()),
                Some(self.receiver),
                false,
            ),
            1 => {
                let (call, class) = self.column();
                (call, class, false)
            }
            2 => {
                let (call, class) = self.literal();
                (call, class, false)
            }
            3 => (call("polars.when", Vec::new()), None, true),
            5 => {
                // A function that gives a count the arguments decide is drawn only where a focus
                // names it, as the methods that give one are.
                let starts = match self.focus {
                    Some(focus) if !focus.starts.is_empty() => focus.starts.clone(),
                    _ => function_starts()
                        .into_iter()
                        .filter(|entry| !is_held_back(entry))
                        .collect(),
                };
                let entry = self.draw.pick(&starts);
                match self.function_start(entry, slots, depth) {
                    Some((start, class)) => (start, class, false),
                    None => {
                        let (call, class) = self.column();
                        (call, class, false)
                    }
                }
            }
            _ => (
                call("polars.element", Vec::new()),
                Some(self.receiver),
                false,
            ),
        }
    }

    /// A function that makes a moment, given what it needs, with the class of what it gives where
    /// the tables say it. What it gives is for the chain to carry on from, and whether the tables
    /// allowed the call and how many values it gives are left in `started`.
    fn function_start(
        &mut self,
        entry: &'static Entry,
        slots: &mut Vec<Slot>,
        depth: usize,
    ) -> Option<(Call, Option<Representative>)> {
        let counted_by_arguments = Counting::of(entry, &[]) == Some(Counting::TheArguments);
        let (call, operands) = if counted_by_arguments {
            self.range_call(entry)
        } else {
            self.method_call(entry, slots, depth)?
        };
        let context = Context::from_positions(slots).expect("a position of a step comes first");
        let given: Vec<(&str, Representative)> = operands
            .iter()
            .filter_map(|(name, class)| class.map(|class| (name.as_str(), class)))
            .collect();
        let words = option_words(&call);
        let words: Vec<(&str, &str)> = words
            .iter()
            .map(|(name, word)| (name.as_str(), word.as_str()))
            .collect();
        let zones: Vec<(&str, Option<&str>)> = entry
            .parameters()
            .iter()
            .filter(|parameter| parameter.kind() == Kind::TimeZone)
            .filter_map(|parameter| match call.argument(parameter.name())? {
                Value::TimeZone(zone) => Some((parameter.name(), zone.as_deref())),
                _ => None,
            })
            .collect();
        // One value where every argument is a literal, which is one, and one for each row where
        // another is a chain, which may or may not be one value.
        let one = call.arguments().iter().all(|given| match given.value() {
            Value::Expression(expression) => {
                expression.links().is_empty()
                    && matches!(expression.start(), Start::Call(start)
                        if start.entry().id().as_str() == "polars.lit")
            }
            _ => true,
        });
        let chosen = Chosen::default()
            .with_arguments(&given)
            .with_options(&words)
            .with_time_zones(&zones);
        let chosen = if counted_by_arguments {
            chosen.giving_shape(Shape::Counted)
        } else if one {
            chosen.giving_one()
        } else {
            chosen.giving_many()
        };
        let verdict = allowed::offer(entry, &OfferPlace::new(context, After::Start), &chosen);
        self.started = Some(match &verdict {
            OfferVerdict::Allowed(allowance) => Some(allowance.shape()),
            OfferVerdict::Refused(_) => None,
        });
        let class = match verdict {
            OfferVerdict::Allowed(allowance) => {
                allowance.result().and_then(allowed::ResultDtype::class)
            }
            OfferVerdict::Refused(_) => None,
        };
        Some((call, class))
    }

    /// A number typed in without a dtype.
    fn typed_number(number: i64) -> Expression {
        Expression::new(
            Start::Call(call("polars.lit", vec![("value", Value::Integer(number))])),
            Vec::new(),
        )
        .expect("a start")
    }

    /// A function that makes a date, a moment or a time of day of numbers typed in: the values a
    /// range is counted from.
    fn made_of(id: &str, parts: &[(&'static str, i64)]) -> Expression {
        let given = parts
            .iter()
            .map(|(name, number)| (*name, Value::Expression(Self::typed_number(*number))))
            .collect();
        Expression::new(Start::Call(call(id, given)), Vec::new()).expect("a start")
    }

    /// What a range or a repeat is given for the parameter `name` that holds an expression: a
    /// value of the dtype the parameter takes typed in, mostly, which fixes the count of the range,
    /// and, as often as a method is given what it refuses, an aggregate of a column, whose count is
    /// not known, the rows, a count of values and a value of a dtype it does not take.
    fn range_argument(&mut self, id: &str, name: &str) -> Built {
        let typed =
            |expression: Expression, class: Option<Representative>| Built { expression, class };
        match self
            .draw
            .weighted(&[(62, 0_u8), (10, 1), (12, 2), (6, 3), (5, 4), (5, 5)])
        {
            0 | 1 => {
                let odd = self.draw.below(8) == 0;
                let year = self.draw.pick(&[2024_i64, 2023, 1970, 2000]);
                let month = self.draw.pick(&[1_i64, 2, 3, 6, 10, 12]);
                let day = self.draw.pick(&[1_i64, 5, 10, 28, 31]);
                let hour = self.draw.pick(&[0_i64, 1, 2, 5, 23]);
                let date = || [("year", year), ("month", month), ("day", day)];
                match (id, name) {
                    ("polars.int_range", _) => {
                        let bound = if odd {
                            self.draw.pick(&[-1_i64, 300, 256, 70_000])
                        } else {
                            self.draw.pick(&[0_i64, 1, 2, 3, 5, 10, 100])
                        };
                        typed(Self::typed_number(bound), None)
                    }
                    ("polars.linear_space", "num_samples") | ("polars.repeat", "n") => {
                        let count = if odd {
                            -1_i64
                        } else {
                            self.draw.pick(&[0_i64, 1, 2, 3, 5, 10, 1000])
                        };
                        typed(Self::typed_number(count), None)
                    }
                    ("polars.linear_space", _) => match self.draw.below(6) {
                        0 => typed(
                            Self::made_of("polars.date", &date()),
                            Some(Representative::Date),
                        ),
                        1 => typed(
                            Self::made_of(
                                "polars.datetime",
                                &[
                                    ("year", year),
                                    ("month", month),
                                    ("day", day),
                                    ("hour", hour),
                                ],
                            ),
                            Some(Representative::Datetime),
                        ),
                        _ => typed(Self::typed_number(self.draw.pick(&[0, 1, 2, 10, -5])), None),
                    },
                    ("polars.repeat", _) => match self.draw.below(6) {
                        0 => typed(
                            Self::made_of("polars.date", &date()),
                            Some(Representative::Date),
                        ),
                        1 => typed(
                            Expression::new(
                                Start::Call(call(
                                    "polars.lit",
                                    vec![("value", Value::String("a".to_owned()))],
                                )),
                                Vec::new(),
                            )
                            .expect("a start"),
                            Some(Representative::String),
                        ),
                        2 => typed(
                            Expression::new(
                                Start::Call(call("polars.lit", vec![("value", Value::Flag(None))])),
                                Vec::new(),
                            )
                            .expect("a start"),
                            Some(Representative::Null),
                        ),
                        _ => typed(Self::typed_number(self.draw.pick(&[0, 1, 7])), None),
                    },
                    ("polars.time_range", _) => {
                        if odd {
                            typed(
                                Self::made_of("polars.date", &date()),
                                Some(Representative::Date),
                            )
                        } else {
                            typed(
                                Self::made_of(
                                    "polars.time",
                                    &[("hour", hour), ("minute", self.draw.pick(&[0_i64, 30]))],
                                ),
                                Some(Representative::Time),
                            )
                        }
                    }
                    ("polars.datetime_range", _) if self.draw.chance(70) => typed(
                        Self::made_of(
                            "polars.datetime",
                            &[
                                ("year", year),
                                ("month", month),
                                ("day", day),
                                ("hour", hour),
                            ],
                        ),
                        Some(Representative::Datetime),
                    ),
                    // A date is the bound of a range of dates, and of moments too.
                    _ if odd => typed(Self::typed_number(0), None),
                    _ => typed(
                        Self::made_of("polars.date", &date()),
                        Some(Representative::Date),
                    ),
                }
            }
            2 => {
                let (start, class) = self.column();
                Built {
                    // The greatest value of the column, which is no null where the column holds
                    // one: the tables know no null, and the first value of a column may be one.
                    expression: Expression::new(
                        Start::Call(start),
                        vec![call("Expr.max", Vec::new())],
                    )
                    .expect("a chain"),
                    class,
                }
            }
            3 => {
                let (start, class) = self.column();
                Built {
                    expression: Expression::new(Start::Call(start), Vec::new()).expect("a start"),
                    class,
                }
            }
            4 => {
                let (start, class) = self.column();
                Built {
                    expression: Expression::new(
                        Start::Call(start),
                        vec![call(
                            "Expr.head",
                            vec![("n", Value::Expression(Self::typed_number(2)))],
                        )],
                    )
                    .expect("a chain"),
                    class,
                }
            }
            _ => {
                let text = Expression::new(
                    Start::Call(call(
                        "polars.lit",
                        vec![("value", Value::String("1".to_owned()))],
                    )),
                    Vec::new(),
                )
                .expect("a start");
                typed(text, Some(Representative::String))
            }
        }
    }

    /// The call of a function that makes a range or a repeat: each bound, the number of samples or
    /// of times and the value given as [`Self::range_argument`] says, and the step, the interval,
    /// the words and the dtype it takes, drawn from what Polars takes and what it refuses.
    fn range_call(&mut self, entry: &'static Entry) -> (Call, Operands) {
        let id = entry.id().as_str();
        let mut given: Vec<(&'static str, Value)> = Vec::new();
        let mut operands: Operands = Vec::new();
        for parameter in entry.parameters() {
            let name = parameter.name();
            match parameter.kind() {
                Kind::Expression(_) => {
                    if !parameter.is_needed() && self.draw.chance(30) {
                        continue;
                    }
                    let built = self.range_argument(id, name);
                    operands.push((name.to_owned(), built.class));
                    given.push((name, Value::Expression(built.expression)));
                }
                Kind::Value(ValueType::Integer) if self.draw.chance(50) => {
                    given.push((name, Value::Integer(self.draw.pick(&[1, 2, 3, -1, -2, 0]))));
                }
                Kind::Duration if self.draw.chance(70) => {
                    let texts: &[&str] = match id {
                        "polars.date_range" => &[
                            "1d", "2d", "1w", "1mo", "1y", "1q", "12h", "-1d", "0d", "1d1h",
                        ],
                        "polars.time_range" => &["1h", "30m", "15m", "1h30m", "1d", "-1h"],
                        _ => &["1d", "6h", "1h30m", "1mo", "2d12h", "xyz", "-1h", "1w"],
                    };
                    given.push((name, Value::Duration(self.draw.pick(texts).to_owned())));
                }
                Kind::Choice(words) if self.draw.chance(40) => {
                    given.push((name, Value::Choice((*self.draw.pick(words)).to_owned())));
                }
                Kind::TimeZone if self.draw.chance(35) => {
                    for (zone, value) in self.time_zones(entry) {
                        given.push((zone, value));
                    }
                }
                Kind::Dtype if self.draw.chance(35) => {
                    let classes = if id == "polars.int_range" {
                        vec![
                            Representative::Int8,
                            Representative::UInt8,
                            Representative::Int32,
                            Representative::Int64,
                            Representative::UInt64,
                            Representative::Float64,
                        ]
                    } else {
                        literal_classes()
                    };
                    let class = self.draw.pick(&classes);
                    given.push((name, dtype_value(class).expect("a class of the document")));
                }
                // A parameter that is not drawn this time, and the series that no step makes.
                _ => {}
            }
        }
        (call(id, given), operands)
    }

    /// A chain in the position `slots` end in, `depth` levels inside others.
    fn chain(&mut self, slots: &mut Vec<Slot>, depth: usize, goal: Goal) -> Built {
        let context = Context::from_positions(slots).expect("a position of a step comes first");
        self.budget = self.budget.saturating_sub(1);
        let group = self.host == Host::Pivot;
        let (start, class, is_condition) = if self.budget == 0 {
            let (call, class) = if group {
                (call("polars.element", Vec::new()), Some(self.receiver))
            } else {
                self.column()
            };
            (call, class, false)
        } else {
            self.started = None;
            self.start(slots, group, depth)
        };
        let mut links: Vec<Call> = Vec::new();
        let mut cur_class = class;
        let mut shape = Shape::Many;
        let mut alive = true;
        let mut values: Vec<Option<Representative>> = Vec::new();
        let mut start = start;
        if is_condition {
            // `when(predicate).then(value)`, further `when`s, then maybe
            // `otherwise(value)`; a condition left at its `then` is one too.
            let mut condition = self.condition(slots, depth, &mut values);
            start = condition.remove(0);
            links = condition;
            cur_class = unify(&values);
        } else if let Some(started) = self.started.take() {
            // A function that makes a moment was judged for what it was given.
            match started {
                Some(given) => shape = given,
                None => alive = false,
            }
        } else {
            match allowed::offer(
                start.entry(),
                &OfferPlace::new(context, After::Start),
                &Chosen::default(),
            ) {
                OfferVerdict::Allowed(allowance) => shape = allowance.shape(),
                OfferVerdict::Refused(_) => alive = false,
            }
        }
        let ended_condition = is_condition
            && links
                .last()
                .is_some_and(|link| link.entry().id().as_str().ends_with("otherwise"));
        if is_condition && !ended_condition {
            alive = false;
        }
        let count = if self.budget == 0 {
            0
        } else {
            self.draw
                .weighted(&[(15, 0_usize), (25, 1), (25, 2), (20, 3), (10, 4), (5, 5)])
        };
        for _ in 0..count {
            if !alive || cur_class.is_none() || self.budget == 0 {
                break;
            }
            let Some((link, next)) =
                self.method(slots, depth, context, cur_class.expect("a class"), shape)
            else {
                break;
            };
            links.push(link);
            match next {
                Some((next_class, next_shape)) => {
                    cur_class = next_class;
                    shape = next_shape;
                }
                None => alive = false,
            }
            // The groups of a pattern are a struct that is often tested for unique values, which a
            // struct with no fields, as the groups of a pattern with none are, cannot be.
            if alive
                && links
                    .last()
                    .is_some_and(|link| link.entry().id().as_str() == "Expr.str.extract_groups")
                && self.draw.chance(35)
            {
                links.push(call(
                    self.draw.pick(&["Expr.is_unique", "Expr.is_duplicated"]),
                    Vec::new(),
                ));
                cur_class = Some(Representative::Boolean);
                shape = Shape::Many;
            }
        }
        if goal == Goal::Boolean
            && alive
            && cur_class.is_some_and(|class| class != Representative::Boolean)
            && self.budget > 0
            && self.draw.chance(80)
            && let Some((link, next)) =
                self.compare(slots, depth, context, cur_class.expect("a class"), shape)
        {
            links.push(link);
            cur_class = next.and_then(|(class, _)| class);
        }
        Built {
            expression: Expression::new(Start::Call(start), links).unwrap_or_else(|refusal| {
                panic!("the generator built a chain the document refuses: {refusal}")
            }),
            class: cur_class,
        }
    }

    /// `polars.when` and the calls that make a condition, with the dtypes of
    /// its values in `values`.
    fn condition(
        &mut self,
        slots: &mut Vec<Slot>,
        depth: usize,
        values: &mut Vec<Option<Representative>>,
    ) -> Vec<Call> {
        let mut calls = Vec::new();
        let predicate = |this: &mut Self, slots: &mut Vec<Slot>| {
            slots.push(Slot::Nested(allowed::Nested::ConditionPredicate));
            let built = this.chain(slots, depth + 1, Goal::Boolean);
            slots.pop();
            Value::Many(vec![Value::Expression(built.expression)])
        };
        // The tables measured a condition whose values are the same
        // expression, so the values here are of one class: a copy of the first,
        // or a chain that has its class, which is looked for a few times.
        let mut model: Option<(Expression, Option<Representative>)> = None;
        let mut value = |this: &mut Self, slots: &mut Vec<Slot>, values: &mut Vec<_>| {
            slots.push(Slot::Nested(allowed::Nested::ConditionValue));
            let untyped = std::mem::replace(&mut this.untyped_numbers, false);
            let chosen = match &model {
                None => {
                    let built = this.chain(slots, depth + 1, Goal::Anything);
                    model = Some((built.expression.clone(), built.class));
                    (built.expression, built.class)
                }
                Some((expression, class)) => {
                    // A copy of the first would leave Polars to see that both
                    // values are one expression, and to drop the predicates.
                    let mut found = (expression.clone(), *class);
                    if class.is_some() {
                        for _ in 0..5 {
                            let built = this.chain(slots, depth + 1, Goal::Anything);
                            if built.class == *class {
                                found = (built.expression, built.class);
                                break;
                            }
                        }
                    }
                    found
                }
            };
            this.untyped_numbers = untyped;
            slots.pop();
            values.push(chosen.1);
            Value::Expression(chosen.0)
        };
        let first = predicate(self, slots);
        calls.push(call("polars.when", vec![("predicates", first)]));
        let statement = value(self, slots, values);
        calls.push(call("When.then", vec![("statement", statement)]));
        let mut state = ConditionState::Then;
        while self.budget > 2 && self.draw.chance(25) {
            let next = predicate(self, slots);
            let (when, then) = match state {
                ConditionState::Then => ("Then.when", "ChainedWhen.then"),
                _ => ("ChainedThen.when", "ChainedWhen.then"),
            };
            calls.push(call(when, vec![("predicates", next)]));
            let statement = value(self, slots, values);
            calls.push(call(then, vec![("statement", statement)]));
            state = ConditionState::ChainedThen;
        }
        if self.draw.chance(90) {
            let statement = value(self, slots, values);
            let otherwise = match state {
                ConditionState::Then => "Then.otherwise",
                _ => "ChainedThen.otherwise",
            };
            calls.push(call(otherwise, vec![("statement", statement)]));
        }
        calls
    }

    /// A method that follows a value of `class`, asked of the tables a few
    /// times for one they allow. It gives the call and what the chain has
    /// after it, or `None` for that when the tables refused the call.
    fn method(
        &mut self,
        slots: &mut Vec<Slot>,
        depth: usize,
        context: Context,
        class: Representative,
        shape: Shape,
    ) -> Option<(Call, Next)> {
        let methods = methods();
        for attempt in 0..6 {
            let mut chosen = methods[self.draw.below(methods.len())];
            if is_held_back(chosen) && !self.focus.is_some_and(|focus| focus.holds(chosen)) {
                continue;
            }
            // Without a focus, or with the method drawn in it, nothing more is drawn: the case is
            // the one drawn without a focus.
            if let Some(focus) = self.focus
                && !focus.methods.is_empty()
                && !focus.holds(chosen)
                && self.draw.chance(FOCUS_PERCENT)
            {
                chosen = self.draw.pick(&focus.methods);
            }
            let (call, arguments) = self.method_call(chosen, slots, depth)?;
            let given: Vec<(&str, Representative)> = arguments
                .iter()
                .filter_map(|(name, class)| class.map(|class| (name.as_str(), class)))
                .collect();
            let words = option_words(&call);
            let words: Vec<(&str, &str)> = words
                .iter()
                .map(|(name, word)| (name.as_str(), word.as_str()))
                .collect();
            let verdict = allowed::offer(
                chosen,
                &OfferPlace::new(
                    context,
                    After::Value {
                        dtype: class,
                        shape,
                    },
                ),
                &Chosen::default()
                    .with_arguments(&given)
                    .with_options(&words),
            );
            match verdict {
                OfferVerdict::Allowed(allowance) => {
                    let next = allowance.result().and_then(allowed::ResultDtype::class);
                    return Some((call, Some((next, allowance.shape()))));
                }
                OfferVerdict::Refused(_) if attempt == 5 || self.draw.chance(15) => {
                    return Some((call, None));
                }
                OfferVerdict::Refused(_) => {}
            }
        }
        None
    }

    /// A comparison or a test for null, which gives a `Boolean`, where one is
    /// wanted.
    fn compare(
        &mut self,
        slots: &mut Vec<Slot>,
        depth: usize,
        context: Context,
        class: Representative,
        shape: Shape,
    ) -> Option<(Call, Next)> {
        let ids = [
            "Expr.is_null",
            "Expr.is_not_null",
            "Expr.eq",
            "Expr.ne",
            "Expr.gt",
            "Expr.ge",
            "Expr.lt",
            "Expr.le",
        ];
        let chosen = entry(self.draw.pick(&ids));
        let (call, arguments) = self.method_call(chosen, slots, depth)?;
        let given: Vec<(&str, Representative)> = arguments
            .iter()
            .filter_map(|(name, class)| class.map(|class| (name.as_str(), class)))
            .collect();
        let words = option_words(&call);
        let words: Vec<(&str, &str)> = words
            .iter()
            .map(|(name, word)| (name.as_str(), word.as_str()))
            .collect();
        match allowed::offer(
            chosen,
            &OfferPlace::new(
                context,
                After::Value {
                    dtype: class,
                    shape,
                },
            ),
            &Chosen::default()
                .with_arguments(&given)
                .with_options(&words),
        ) {
            OfferVerdict::Allowed(allowance) => Some((
                call,
                Some((
                    allowance.result().and_then(allowed::ResultDtype::class),
                    allowance.shape(),
                )),
            )),
            OfferVerdict::Refused(_) => Some((call, None)),
        }
    }

    /// A call of the method `entry` with the arguments its parameters take,
    /// and the dtype of each expression among them.
    fn method_call(
        &mut self,
        entry: &'static Entry,
        slots: &mut Vec<Slot>,
        depth: usize,
    ) -> Option<(Call, Operands)> {
        let id = entry.id().as_str();
        let mut given: Vec<(&'static str, Value)> = Vec::new();
        let mut dtypes: Vec<(String, Option<Representative>)> = Vec::new();
        // Whether a window is given `partition_by`, which Polars takes without
        // `order_by` and `order_by` without it, and refuses neither.
        let mut partitioned = true;
        // Which bounds a `clip` is given: one of them, or both, for a step gives at least one and
        // Polars builds each alone differently.
        let bounds = self.draw.below(3);
        // Whether a fill is given a strategy, in place of a value: Python takes one of them and
        // refuses both and neither.
        let fill_by_strategy = id == "Expr.fill_null" && self.draw.chance(45);
        // Whether what a replacement replaces is a literal, which is one value, so what it is
        // replaced by must be one value too: Polars refuses values of two other lengths.
        let mut replaces_a_literal = self.host == Host::Pivot;
        for parameter in entry.parameters() {
            if !matches!(parameter.kind(), Kind::Expression(_)) {
                continue;
            }
            let name = parameter.name();
            let required = parameter.is_needed();
            let given_it = match (id, name) {
                ("Expr.clip", "lower_bound") => bounds != 1,
                ("Expr.clip", "upper_bound") => bounds != 0,
                ("Expr.fill_null", "value") => !fill_by_strategy,
                // A strict replacement without a default is refused where a value is not listed,
                // which depends on the rows, so one is sometimes given none and a limit holds the
                // refusal.
                ("Expr.replace_strict", "default") => !self.draw.chance(12),
                (_, _) if required => true,
                ("Expr.over", "partition_by") => {
                    partitioned = self.draw.chance(80);
                    partitioned
                }
                // A window that is not partitioned is ordered, and `*more_exprs`
                // extends `partition_by` and means nothing without it.
                ("Expr.over", "order_by") => !partitioned || self.draw.chance(35),
                ("Expr.over", "more_exprs") => partitioned && self.draw.chance(6),
                _ => self.draw.chance(30),
            };
            if !given_it {
                continue;
            }
            let position = Slot::of_parameter(id, name);
            slots.extend(position);
            let built: Vec<Built> = match (id, name) {
                ("Expr.shift" | "Expr.diff" | "Expr.pct_change", _) => {
                    vec![self.scalar(name == "n")]
                }
                // How many values a method keeps, or where it starts: a number typed in, of a
                // dtype or not, which fixes the count that the walk states for the call.
                (
                    "Expr.head" | "Expr.limit" | "Expr.tail" | "Expr.top_k" | "Expr.bottom_k"
                    | "Expr.slice",
                    _,
                ) => vec![self.cut(id, name)],
                // A quantile is one share from 0 to 1, which Polars refuses a column of for the
                // exact quantile, and a column of numbers outside the range or holding a null for
                // the approximate one: it is a number, and the tables measure it as one.
                ("Expr.quantile" | "Expr.approx_quantile", "quantile") => vec![self.share()],
                // Polars builds a text of as many characters as the length converts to, which for
                // a timestamp is the microseconds since the epoch: a length that is a chain of
                // any dtype can fill the memory, so it is the number 1 of a dtype, as a shift's.
                ("Expr.str.pad_start" | "Expr.str.pad_end" | "Expr.str.zfill", "length") => {
                    vec![self.scalar(false)]
                }
                ("Expr.replace" | "Expr.replace_strict", "old") => {
                    let built = self.replaced();
                    replaces_a_literal = built.expression.links().is_empty()
                        && matches!(built.expression.start(), Start::Call(start)
                            if start.entry().id().as_str() == "polars.lit");
                    vec![built]
                }
                ("Expr.replace" | "Expr.replace_strict", "new") if replaces_a_literal => {
                    let literal = self.literal();
                    vec![Built {
                        expression: Expression::new(Start::Call(literal.0), Vec::new())
                            .expect("a start"),
                        class: literal.1,
                    }]
                }
                ("Expr.dt.offset_by", "by") => {
                    // The durations are text of the table's: `b`, whose rows
                    // are durations where it is text, as the tables measured it.
                    let (literal, class) = if self.draw.chance(70)
                        || matches!(self.host, Host::Pivot | Host::JoinOn)
                    {
                        self.literal_loose_text()
                    } else {
                        let name = if self.host == Host::JoinRight {
                            "rb"
                        } else {
                            "b"
                        };
                        (column(name), self.class_of(name))
                    };
                    vec![Built {
                        expression: Expression::new(Start::Call(literal), Vec::new())
                            .expect("a start"),
                        class,
                    }]
                }
                // A separator is often text typed in: a pattern where `literal` is false, which
                // is then one Polars cannot compile now and then, and plain text where it is not.
                ("Expr.str.split", "by") if self.draw.chance(50) => {
                    let text = if self.draw.chance(40) {
                        self.draw.pick(&INVALID_PATTERNS)
                    } else {
                        self.draw.pick(&VALID_PATTERNS)
                    };
                    let literal = call(
                        "polars.lit",
                        vec![("value", Value::String(text.to_owned()))],
                    );
                    vec![Built {
                        expression: Expression::new(Start::Call(literal), Vec::new())
                            .expect("a start"),
                        class: Some(Representative::String),
                    }]
                }
                // A duration is often text typed in, which Polars reads as a duration string: one it
                // reads for the period of a cut or the amount of a move, or one it does not. Where
                // it is a column it is the one that stands for an argument, whose text is the
                // durations the tables were measured with: the text of the other columns is no
                // duration, which Polars refuses when the rows run, and is no mistake of the
                // tables but of what a duration is given.
                _ if parameter.duration_string().is_some() => {
                    let argument = match self.host {
                        Host::JoinRight => Some("rb"),
                        Host::Pivot | Host::JoinOn => None,
                        _ => Some("b"),
                    };
                    match argument {
                        Some(name) if !self.draw.chance(55) => {
                            vec![Built {
                                expression: Expression::new(Start::Call(column(name)), Vec::new())
                                    .expect("a start"),
                                class: self.class_of(name),
                            }]
                        }
                        _ => {
                            let text = if self.draw.chance(25) {
                                self.draw.pick(&INVALID_DURATIONS)
                            } else {
                                self.draw.pick(&VALID_DURATIONS)
                            };
                            let literal = call(
                                "polars.lit",
                                vec![("value", Value::String(text.to_owned()))],
                            );
                            vec![Built {
                                expression: Expression::new(Start::Call(literal), Vec::new())
                                    .expect("a start"),
                                class: Some(Representative::String),
                            }]
                        }
                    }
                }
                // What a replacement puts in place of a match is often text, with the references to
                // a group of the match that a pattern gives it meaning for.
                ("Expr.str.replace" | "Expr.str.replace_all", "value") if self.draw.chance(50) => {
                    let text = self.draw.pick(&["_", "$1", "${g}", "<$0>", "$$", ""]);
                    let literal = call(
                        "polars.lit",
                        vec![("value", Value::String(text.to_owned()))],
                    );
                    vec![Built {
                        expression: Expression::new(Start::Call(literal), Vec::new())
                            .expect("a start"),
                        class: Some(Representative::String),
                    }]
                }
                ("Expr.over", "partition_by" | "order_by" | "more_exprs") => {
                    let count = if name == "more_exprs" {
                        1
                    } else {
                        1 + self.draw.below(2)
                    };
                    (0..count).map(|_| self.operand(slots, depth)).collect()
                }
                _ => vec![self.operand(slots, depth)],
            };
            if position.is_some() {
                slots.pop();
            }
            let many = matches!(parameter.shape(), ParamShape::Many);
            for item in &built {
                dtypes.push((name.to_owned(), item.class));
            }
            let value = if many {
                Value::Many(
                    built
                        .into_iter()
                        .map(|item| Value::Expression(item.expression))
                        .collect(),
                )
            } else {
                Value::Expression(built.into_iter().next().expect("an operand").expression)
            };
            given.push((name, value));
        }
        given.extend(self.options(entry));
        if fill_by_strategy
            && let Some(Kind::Choice(words)) = entry
                .parameters()
                .iter()
                .find(|parameter| parameter.name() == "strategy")
                .map(catalog::Parameter::kind)
        {
            given.push((
                "strategy",
                Value::Choice((*self.draw.pick(words)).to_owned()),
            ));
        }
        given.extend(self.dtype_arguments(entry));
        given.extend(self.numbers(entry, &given));
        given.extend(self.texts(entry));
        given.extend(self.patterns(entry));
        given.extend(self.time_zones(entry));
        if id == "Expr.over" {
            let ordered = given.iter().any(|(name, _)| *name == "order_by");
            let optioned = given
                .iter()
                .any(|(name, _)| matches!(*name, "descending" | "nulls_last" | "mapping_strategy"));
            // The option table measures `over`'s options with something to
            // order by.
            if optioned && !ordered {
                let key = if self.host == Host::Pivot {
                    call(
                        "polars.lit",
                        vec![
                            ("value", Value::Integer(1)),
                            ("dtype", dtype_value(Representative::Int64).expect("Int64")),
                        ],
                    )
                } else if self.host == Host::JoinRight {
                    column("rk")
                } else if self.host == Host::JoinOn {
                    column("u")
                } else {
                    column("k")
                };
                given.push((
                    "order_by",
                    Value::Many(vec![Value::Expression(
                        Expression::new(Start::Call(key), Vec::new()).expect("a start"),
                    )]),
                ));
                dtypes.push(("order_by".to_owned(), Some(Representative::Int64)));
            }
        }
        let call = Call::new(entry, given).ok()?;
        Some((call, dtypes))
    }

    /// The numbers a method is given for the parameters that take one: the places a rounding
    /// keeps and the significant figures it keeps, how many nulls in a row a fill fills, and
    /// the tolerances of a comparison. The values are those that Polars accepts: a number of
    /// significant figures below one is refused when rows run, for every dtype alike, and so is
    /// a tolerance below zero, which the tables, measured with two figures and the defaults, do
    /// not say. The places include those from which Polars leaves a float as it is, which differ
    /// for each width of float. A limit is given only to a fill that takes one: a fill by a
    /// strategy that moves along the rows, which Python refuses it beside any other.
    fn numbers(
        &mut self,
        entry: &'static Entry,
        given: &[(&'static str, Value)],
    ) -> Vec<(&'static str, Value)> {
        let moves_along = given.iter().any(|(name, value)| {
            *name == "strategy"
                && matches!(value, Value::Choice(word) if word == "forward" || word == "backward")
        });
        let mut numbers = Vec::new();
        for parameter in entry.parameters() {
            let kind = parameter.kind();
            if kind != Kind::Value(ValueType::Integer) && kind != Kind::Value(ValueType::Float) {
                continue;
            }
            if !parameter.is_needed() && !self.draw.chance(40) {
                continue;
            }
            let name = parameter.name();
            if entry.id().as_str() == "Expr.fill_null" && name == "limit" && !moves_along {
                continue;
            }
            let value = match name {
                "digits" => Value::Integer(self.draw.pick(&[1, 2, 3, 6, 15, 40])),
                "decimals" => Value::Integer(self.draw.pick(&[0, 1, 2, 3, 5, 12, 50, 310, 400])),
                "limit" => Value::Integer(self.draw.pick(&[0, 1, 2, 5])),
                // The index of the group `extract` takes: the whole match, a group, or one the
                // pattern lacks, which Polars gives a null for.
                "group_index" => Value::Integer(self.draw.pick(&[0, 1, 2, 5])),
                // Every nth value from a place that `gather_every` takes: Polars refuses every 0th.
                "n" if entry.id().as_str() == "Expr.gather_every" => {
                    Value::Integer(self.draw.pick(&[1, 2, 2, 3, 5]))
                }
                "offset" if entry.id().as_str() == "Expr.gather_every" => {
                    Value::Integer(self.draw.pick(&[0, 0, 1, 2, 4]))
                }
                // How many matches `replace` replaces: none, some, or all of them below zero.
                "n" => Value::Integer(self.draw.pick(&[-1, 0, 1, 1, 2, 3])),
                // The seed of a random rank: Polars takes any unsigned number.
                "seed" => Value::Integer(self.draw.pick(&[0, 1, 7, 12345])),
                "abs_tol" | "rel_tol" => Value::Float(self.draw.pick(&[0.0, 0.5, 1e-9, 1.0])),
                // The delta degrees of freedom of a variance: none, the default, and counts that
                // leave no value to divide by, up to the largest an unsigned byte holds.
                "ddof" => Value::Integer(self.draw.pick(&[0, 1, 1, 2, 3, 255])),
                // The base of an entropy's logarithm.
                "base" => Value::Float(self.draw.pick(&[2.0, 10.0, std::f64::consts::E, 0.5])),
                // The rank error of an approximate quantile, as a share of the rows: Polars takes
                // one from 2^-32 up to but not including 1.
                "error" => Value::Float(self.draw.pick(&[0.001, 0.01, 0.1, 0.5])),
                other => panic!("the generator does not know what to give {other}"),
            };
            numbers.push((name, value));
        }
        numbers
    }

    /// The texts a method is given for the parameters that take one: what `join` puts between the
    /// values it joins, and the one character that `pad_start` and `pad_end` add. A fill character
    /// is one character, as Python's binding takes it, and the others, which differ in how many
    /// bytes they take, show the lengths in characters and in bytes apart.
    fn texts(&mut self, entry: &'static Entry) -> Vec<(&'static str, Value)> {
        let mut texts = Vec::new();
        for parameter in entry.parameters() {
            if parameter.kind() != Kind::Value(ValueType::String)
                || (!parameter.is_needed() && !self.draw.chance(50))
            {
                continue;
            }
            let name = parameter.name();
            let value = match name {
                "delimiter" => self.draw.pick(&["", "-", ", ", "é", "ab"]),
                "fill_char" => self.draw.pick(&["*", "0", " ", "é", "-"]),
                // How a moment is written: ISO in its two forms, and the formats of the calendar
                // and of the clock, some of which a date or a time of day has no part for.
                "format" => self.draw.pick(&FORMATS),
                other => panic!("the generator does not know what to give {other}"),
            };
            texts.push((name, Value::String(value.to_owned())));
        }
        texts
    }

    /// The patterns a method is given for the parameters that take one, drawn from a small fixed
    /// set: regular expressions with groups, named groups, classes, anchors and flags, and some
    /// that Polars cannot compile, so that the refusal of a pattern is checked as the pattern is.
    /// One that is no regular expression is plain text where `literal` is set, which is drawn
    /// apart from it.
    fn patterns(&mut self, entry: &'static Entry) -> Vec<(&'static str, Value)> {
        entry
            .parameters()
            .iter()
            .filter(|parameter| parameter.kind() == Kind::Pattern)
            .map(|parameter| {
                let text = if self.draw.chance(25) {
                    self.draw.pick(&INVALID_PATTERNS)
                } else {
                    self.draw.pick(&VALID_PATTERNS)
                };
                (parameter.name(), Value::Pattern(text.to_owned()))
            })
            .collect()
    }

    /// The time zones a method is given for the parameters that take one, which it must be given:
    /// names of the database and fixed offsets that Polars reads, text that it does not, and none,
    /// for a method that takes it off.
    fn time_zones(&mut self, entry: &'static Entry) -> Vec<(&'static str, Value)> {
        entry
            .parameters()
            .iter()
            .filter(|parameter| parameter.kind() == Kind::TimeZone)
            .map(|parameter| {
                let value = if parameter.takes_no_time_zone() && self.draw.chance(15) {
                    Value::TimeZone(None)
                } else if self.draw.chance(18) {
                    Value::TimeZone(Some(self.draw.pick(&INVALID_TIME_ZONES).to_owned()))
                } else {
                    Value::TimeZone(Some(self.draw.pick(&VALID_TIME_ZONES).to_owned()))
                };
                (parameter.name(), value)
            })
            .collect()
    }

    /// The dtypes a method is given for the parameters that take one: the dtype a cast gives,
    /// which it must be given, and the dtype a replacement is asked to give, which it sometimes is.
    fn dtype_arguments(&mut self, entry: &'static Entry) -> Vec<(&'static str, Value)> {
        let mut dtypes = Vec::new();
        for parameter in entry.parameters() {
            if parameter.kind() != Kind::Dtype {
                continue;
            }
            if !parameter.is_needed() && !self.draw.chance(25) {
                continue;
            }
            let class = self.draw.pick(&literal_classes());
            dtypes.push((
                parameter.name(),
                dtype_value(class).expect("a class of the document"),
            ));
        }
        dtypes
    }

    /// What a replacement replaces: a column of the table whose values are all different, which
    /// is the only kind Polars accepts as what is replaced, or a literal. A literal is one
    /// value. A column that holds a value twice is refused when rows run, which depends on them.
    fn replaced(&mut self) -> Built {
        let names: &[&str] = match self.host {
            Host::JoinRight => &["ra", "rb"],
            Host::JoinOn => &["u", "a"],
            Host::Pivot => &[],
            _ => &["a", "b", "u"],
        };
        if names.is_empty() || self.draw.chance(40) {
            let literal = self.literal();
            return Built {
                expression: Expression::new(Start::Call(literal.0), Vec::new()).expect("a start"),
                class: literal.1,
            };
        }
        let name: &str = self.draw.pick(names);
        Built {
            expression: Expression::new(Start::Call(column(name)), Vec::new()).expect("a column"),
            class: self.class_of(name).or(Some(Representative::Int64)),
        }
    }

    fn literal_loose_text(&mut self) -> (Call, Option<Representative>) {
        (
            call(
                "polars.lit",
                vec![("value", Value::String("1d".to_owned()))],
            ),
            Some(Representative::String),
        )
    }
}

/// Patterns Polars compiles: with and without groups, named groups in both of Rust's spellings,
/// classes, anchors, alternation, flags, an empty pattern and plain text.
const VALID_PATTERNS: [&str; 14] = [
    "a",
    "(a)",
    "(?<g>a)",
    r"(\d)(\w)?",
    "[a-z]+",
    "^1",
    "d$",
    r"\d+",
    ".",
    "(?i)D",
    "(?P<x>\\d)(?P<y>d)",
    "a|d",
    "",
    "x*",
];

/// Patterns Polars cannot compile, each for a reason of its own: an unclosed group and class, a
/// repetition that ends before it begins, a name that is no name, a group named twice, a flag and
/// a property that do not exist, an escape with nothing after it, and a repetition with nothing to
/// repeat.
const INVALID_PATTERNS: [&str; 10] = [
    "(",
    "[a",
    "a{2,1}",
    "(?<1>a)",
    "(?<n>a)(?<n>d)",
    "(?z)a",
    r"\p{Nothing}",
    "\\",
    "*a",
    "a)",
];

/// Durations Polars reads: a single unit of each kind, two units, a sign and a zero. A method
/// that cuts or rounds meets some of them with a reason of its own, which a limit holds.
const VALID_DURATIONS: [&str; 12] = [
    "1d", "1h", "30m", "1mo", "1w", "1y", "1q", "500ms", "1s", "2d12h", "+1d", "-1d",
];

/// Durations Polars does not read, each for a reason of its own: no amount, no unit, an amount
/// with a fraction, a unit that does not exist and one in the wrong case, a second sign, a space,
/// and an amount too large to hold.
const INVALID_DURATIONS: [&str; 9] = [
    "d",
    "1",
    "1.5d",
    "1x",
    "1D",
    "1d-1h",
    "1 d",
    "1d ",
    "99999999999y",
];

/// Time zones Polars reads: with and without daylight saving, with a half hour, as a fixed offset
/// and as the zone of the database that has it.
const VALID_TIME_ZONES: [&str; 8] = [
    "UTC",
    "Europe/Berlin",
    "America/New_York",
    "Asia/Kolkata",
    "Australia/Lord_Howe",
    "Etc/GMT+5",
    "+01:00",
    "EST",
];

/// Time zones Polars does not read: a name that is none, one in the wrong case, one with a space
/// and one that is the letter of an offset.
const INVALID_TIME_ZONES: [&str; 4] = ["Not/AZone", "europe/berlin", "America/New_York ", "Z"];

/// Formats a moment is written by: the two ISO forms, the date, the clock, the names of the day and
/// of the month, the day of the year, text with no field in it, and two that Polars reads no field
/// in: a percent sign with nothing after it and a field that does not exist.
const FORMATS: [&str; 11] = [
    "iso",
    "iso:strict",
    "%Y-%m-%d",
    "%H:%M:%S",
    "%A %d %B %Y",
    "%j",
    "%F %T%.f",
    "text",
    "%Y",
    "%",
    "%Q",
];

/// The class every value of a condition has, when they have the same one.
fn unify(values: &[Option<Representative>]) -> Option<Representative> {
    let first = (*values.first()?)?;
    values
        .iter()
        .all(|value| *value == Some(first))
        .then_some(first)
}

/// The options `call` was given, each by the parameter and its word as the
/// option table writes it, which `offer` is asked with: a flag, a choice, and a dtype by the name
/// of its class.
fn option_words(call: &Call) -> Vec<(String, String)> {
    let entry = call.entry();
    call.arguments()
        .iter()
        .filter(|given| {
            entry.parameters().iter().any(|parameter| {
                parameter.name() == given.parameter()
                    && matches!(parameter.kind(), Kind::Flag | Kind::Choice(_) | Kind::Dtype)
            })
        })
        .filter_map(|given| {
            let word = match given.value() {
                Value::Flag(Some(true)) => "true".to_owned(),
                Value::Flag(Some(false)) => "false".to_owned(),
                Value::Flag(None) => "unset".to_owned(),
                Value::Choice(word) => word.clone(),
                Value::Dtype(dtype) => Representative::of(dtype.as_polars())?.name().to_owned(),
                _ => return None,
            };
            Some((given.parameter().to_owned(), word))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 512 bytes of entropy for each `offset`, as the output of a mixing function of the offset
    /// and the position, so that the bytes of one input are not a pattern the size of the
    /// catalogue's list of methods can hide a choice in.
    fn entropy(offset: u64) -> Vec<u8> {
        (0..512_u64)
            .map(|at| {
                let mut mixed = (offset << 16 | at).wrapping_add(0x9e37_79b9_7f4a_7c15);
                mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                (mixed ^ (mixed >> 31)) as u8
            })
            .collect()
    }

    /// `length` bytes from a mixing function of the position, so that no pattern the size of a
    /// count can hide in them.
    fn stream(length: usize) -> Vec<u8> {
        (0..length as u64)
            .map(|at| {
                let mut mixed = at.wrapping_add(0x9e37_79b9_7f4a_7c15);
                mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                (mixed ^ (mixed >> 31)) as u8
            })
            .collect()
    }

    /// How many of `draws` numbers below `count`, drawn from a long stream, fall in each of `bins`
    /// equal ranges of `0..count`, which `bins` divides. Sixteen bytes of stream for each draw is
    /// more than a draw that rejects some of what it reads comes to, so a stream that ran out
    /// would show as the zeros it is read as.
    fn tally(count: usize, bins: usize, draws: usize) -> Vec<usize> {
        assert_eq!(count % bins, 0, "{bins} bins do not divide {count}");
        let bytes = stream(draws * 16);
        let mut draw = Draw::new(&bytes);
        let mut tally = vec![0; bins];
        for _ in 0..draws {
            let number = draw.below(count);
            assert!(number < count, "{number} is not below {count}");
            tally[number / (count / bins)] += 1;
        }
        tally
    }

    /// Asserts that `tally` of `draws` draws is even: the chance of each bin is one over the
    /// number of bins, so a bin holds `draws / bins` on average with the standard deviation of a
    /// binomial count, and five of them is how far a bin of a fair draw is allowed to stray.
    fn assert_even(tally: &[usize], draws: usize) {
        let bins = tally.len() as f64;
        let chance = 1.0 / bins;
        let mean = draws as f64 * chance;
        let deviation = (draws as f64 * chance * (1.0 - chance)).sqrt();
        for (bin, &found) in tally.iter().enumerate() {
            assert!(
                (found as f64 - mean).abs() <= 5.0 * deviation,
                "bin {bin} of {} holds {found} of {draws} draws, where {mean:.0} +- {:.0} is even",
                tally.len(),
                5.0 * deviation
            );
        }
    }

    #[test]
    fn a_draw_below_a_count_that_does_not_divide_256_gives_every_number_as_often() {
        // 157 is a count of methods the catalogue has held; 256 mod 157 is 99, so a draw that
        // reduces a byte gives each of the first 99 numbers twice as often as the others.
        let draws = 157 * 2000;
        assert_even(&tally(157, 157, draws), draws);
    }

    #[test]
    fn a_draw_reads_again_where_its_bytes_are_at_or_above_the_largest_multiple_of_the_count() {
        // One byte holds 256 numbers and one copy of 157, so 157 to 255 are read again.
        assert_eq!(Draw::new(&[156]).below(157), 156);
        assert_eq!(Draw::new(&[157, 3]).below(157), 3);
        assert_eq!(Draw::new(&[255, 200, 0]).below(157), 0);
        // Two bytes hold 65536 numbers and 218 copies of 300, which end at 65400.
        assert_eq!(Draw::new(&[0xff, 0x77]).below(300), 299);
        assert_eq!(Draw::new(&[0xff, 0x78, 0x01, 0x2c]).below(300), 0);
    }

    #[test]
    fn a_draw_below_a_count_above_256_reaches_every_number_and_gives_each_as_often() {
        for count in [257, 300, 600] {
            let draws = count * 1000;
            let tally = tally(count, count, draws);
            assert!(tally[0] > 0, "0 is never drawn below {count}");
            assert!(
                tally[count - 1] > 0,
                "{} is never drawn below {count}",
                count - 1
            );
            assert_even(&tally, draws);
        }
    }

    #[test]
    fn a_draw_below_a_count_of_many_bytes_is_even_across_the_whole_range() {
        // 1_000_000 takes three bytes and 4_000_000_000 four, the size of a count of a `u32`.
        for count in [1_000_000, 4_000_000_000] {
            let draws = 100 * 2000;
            assert_even(&tally(count, 100, draws), draws);
        }
    }

    #[test]
    fn a_pick_from_a_list_of_more_than_256_items_draws_the_first_and_the_last_as_often_as_any() {
        let items: Vec<usize> = (0..300).collect();
        let draws = 300 * 1000;
        let bytes = stream(draws * 16);
        let mut draw = Draw::new(&bytes);
        let mut tally = vec![0; items.len()];
        for _ in 0..draws {
            tally[draw.pick(&items)] += 1;
        }
        assert!(
            tally[0] > 0 && tally[299] > 0,
            "{} and {}",
            tally[0],
            tally[299]
        );
        assert_even(&tally, draws);
    }

    #[test]
    fn a_weighted_pick_keeps_its_weights_for_a_total_above_256() {
        // Weights of 1000 and 3000 give the first a quarter of the draws, of which there are
        // 40_000: the count of a binomial with that chance, which has a standard deviation of
        // the square root of 40_000 * 1/4 * 3/4.
        let draws = 40_000;
        let bytes = stream(draws * 16);
        let mut draw = Draw::new(&bytes);
        let first = (0..draws)
            .filter(|_| draw.weighted(&[(1000, 'a'), (3000, 'b')]) == 'a')
            .count();
        let deviation = (40_000.0_f64 * 0.25 * 0.75).sqrt();
        assert!(
            (first as f64 - 10_000.0).abs() <= 5.0 * deviation,
            "{first} of {draws} draws are the first, where 10000 +- {:.0} is a quarter",
            5.0 * deviation
        );
        // Every one of 300 weights of one is drawn, the last among them.
        let items: Vec<(usize, usize)> = (0..300).map(|at| (1, at)).collect();
        let draws = 300 * 1000;
        let bytes = stream(draws * 16);
        let mut draw = Draw::new(&bytes);
        let mut tally = vec![0; items.len()];
        for _ in 0..draws {
            tally[draw.weighted(&items)] += 1;
        }
        assert!(
            tally[0] > 0 && tally[299] > 0,
            "{} and {}",
            tally[0],
            tally[299]
        );
        assert_even(&tally, draws);
    }

    #[test]
    fn the_same_entropy_gives_the_same_numbers_and_past_its_end_a_draw_gives_zero() {
        let bytes = stream(400);
        let (mut one, mut other) = (Draw::new(&bytes), Draw::new(&bytes));
        for count in (1..200).cycle().take(300) {
            assert_eq!(one.below(count), other.below(count));
        }
        // Too few bytes for a draw below a count of three bytes, or none at all, is zero, the
        // first choice of everything, whatever the count.
        for count in [1, 7, 157, 300, 1_000_000] {
            assert_eq!(Draw::new(&[]).below(count), 0, "{count}");
        }
        assert_eq!(Draw::new(&[200]).below(1_000_000), 0);
    }

    /// The identifiers of the methods `focus` holds, in the catalogue's order.
    fn focused_ids(focus: &Focus) -> Vec<&'static str> {
        focus
            .methods
            .iter()
            .map(|entry| entry.id().as_str())
            .collect()
    }

    /// The identifiers of the catalogue's methods of an expression that begin with `prefix`.
    fn ids_beginning(prefix: &str) -> Vec<&'static str> {
        catalog::entries()
            .iter()
            .filter(|entry| entry.applies() == Applies::Expression)
            .map(|entry| entry.id().as_str())
            .filter(|id| id.starts_with(prefix))
            .collect()
    }

    #[test]
    fn a_focus_names_methods_by_name_or_identifier_and_namespaces_by_a_name_that_ends_in_a_dot() {
        let one = Focus::parse("dt.replace").unwrap().unwrap();
        assert_eq!(focused_ids(&one), ["Expr.dt.replace"]);
        let by_id = Focus::parse("Expr.dt.replace").unwrap().unwrap();
        assert_eq!(focused_ids(&by_id), ["Expr.dt.replace"]);
        let namespace = Focus::parse("dt.").unwrap().unwrap();
        let dates = ids_beginning("Expr.dt.");
        assert!(dates.len() > 1, "the catalogue has methods of `dt`");
        assert_eq!(focused_ids(&namespace), dates);
        // Names are separated by commas, and a method named twice is in focus once.
        let several = Focus::parse(" over , replace_strict,over ")
            .unwrap()
            .unwrap();
        assert_eq!(focused_ids(&several), ["Expr.over", "Expr.replace_strict"]);
        assert_eq!(several.names(), "over, replace_strict");
    }

    #[test]
    fn text_that_names_nothing_is_no_focus() {
        for text in ["", "   ", " , "] {
            assert!(Focus::parse(text).unwrap().is_none(), "{text:?}");
        }
    }

    #[test]
    fn a_focus_refuses_a_name_that_is_of_no_method_of_an_expression() {
        // A method the catalogue lacks, a namespace named without its dot, a namespace it lacks,
        // a function that starts a chain, a link of a condition and a step are no methods.
        for name in [
            "dt.no_such_method",
            "dt",
            "nowhere.",
            "col",
            "polars.when",
            "Then.otherwise",
            "LazyFrame.join",
        ] {
            let refusal = Focus::parse(&format!("over, {name}")).unwrap_err();
            assert!(
                refusal.contains("DAGEIST_FUZZ_FOCUS") && refusal.contains(&format!("`{name}`")),
                "{name}: {refusal}"
            );
        }
    }

    #[test]
    fn a_focus_names_a_function_that_starts_a_chain_by_name_or_identifier() {
        for name in ["date", "polars.date", "from_epoch"] {
            let focus = Focus::parse(name)
                .unwrap_or_else(|refusal| panic!("{name}: {refusal}"))
                .unwrap_or_else(|| panic!("{name} names a function"));
            assert_eq!(focus.names(), name.strip_prefix("polars.").unwrap_or(name));
        }
        // Methods and functions are named together, each once.
        let both = Focus::parse("dt.replace, datetime, polars.datetime")
            .unwrap()
            .unwrap();
        assert_eq!(both.names(), "dt.replace, datetime");
        // A function that takes no expression stays no focus: a column and a literal are drawn
        // evenly as they were, and a function is not named as a namespace.
        for name in ["col", "polars.lit", "date."] {
            assert!(Focus::parse(name).is_err(), "{name}");
        }
    }

    #[test]
    fn a_focus_on_a_function_draws_it_as_the_start_of_a_chain_more_often() {
        let focus = Focus::parse("duration").unwrap().unwrap();
        let holding = |focus: Option<&Focus>| {
            (0..2000_u64)
                .filter_map(|offset| generate(&entropy(offset), focus))
                .filter(|case| {
                    case.document
                        .write()
                        .contains("\"entry\":\"polars.duration\"")
                })
                .count()
        };
        let (without, with) = (holding(None), holding(Some(&focus)));
        assert!(without > 0, "no case starts a chain from a function");
        assert!(
            with > 4 * without.max(10),
            "{with} focused cases against {without}"
        );
    }

    #[test]
    fn every_function_that_starts_a_chain_and_takes_expressions_is_drawn_without_a_focus() {
        let mut drawn = std::collections::BTreeSet::new();
        for offset in 0..6000_u64 {
            let Some(case) = generate(&entropy(offset), None) else {
                continue;
            };
            let written = case.document.write();
            for name in ["date", "time", "datetime", "duration", "from_epoch"] {
                if written.contains(&format!("\"entry\":\"polars.{name}\"")) {
                    drawn.insert(name);
                }
            }
        }
        assert_eq!(
            drawn.len(),
            5,
            "the cases start a chain from {drawn:?} and not from every function"
        );
    }

    #[test]
    fn a_function_that_gives_a_count_the_arguments_decide_is_drawn_only_where_a_focus_names_it() {
        let ranges = [
            "int_range",
            "date_range",
            "datetime_range",
            "time_range",
            "linear_space",
            "repeat",
        ];
        for name in ranges {
            assert!(
                is_held_back(entry(&format!("polars.{name}"))),
                "{name} is not held back"
            );
        }
        let calls =
            |written: &str, name: &str| written.contains(&format!("\"entry\":\"polars.{name}\""));
        let focus = Focus::parse("int_range,repeat,date_range")
            .unwrap()
            .unwrap();
        let (mut without, mut with) = (0, 0);
        let mut typed = 0;
        for offset in 0..3000_u64 {
            let bytes = entropy(offset);
            if let Some(case) = generate(&bytes, None) {
                let written = case.document.write();
                without += ranges.iter().filter(|name| calls(&written, name)).count();
            }
            if let Some(case) = generate(&bytes, Some(&focus)) {
                let written = case.document.write();
                let drawn = ["int_range", "repeat", "date_range"]
                    .iter()
                    .any(|name| calls(&written, name));
                with += usize::from(drawn);
                // A range that is given numbers typed in has a count that the walk states.
                typed += usize::from(
                    drawn && written.contains("\"entry\":\"polars.lit\"")
                        || written.contains("\"entry\":\"polars.date\""),
                );
                // Only the functions named are drawn of those held back.
                for name in ["datetime_range", "time_range", "linear_space"] {
                    assert!(!calls(&written, name), "{name} drawn at offset {offset}");
                }
            }
        }
        assert_eq!(without, 0, "a function held back was drawn without a focus");
        assert!(with > 500, "{with} cases of 3000 hold a function in focus");
        assert!(typed > 200, "{typed} cases give a range numbers typed in");
    }

    #[test]
    fn a_focus_on_every_method_leaves_each_case_as_it_is_drawn_without_one() {
        // A method held back is drawn only where it is named, so a focus on it is not as none, and
        // nor is a case that calls `diff`, whose form that drops values is held back too.
        let drawn: Vec<&'static str> = ids_beginning("")
            .into_iter()
            .filter(|id| !is_held_back(entry(id)))
            .collect();
        let every = drawn
            .iter()
            .map(|id| id.strip_prefix("Expr.").unwrap_or(id))
            .collect::<Vec<_>>()
            .join(",");
        let every = Focus::parse(&every).unwrap().unwrap();
        assert_eq!(focused_ids(&every), drawn);
        let mut compared = 0;
        for offset in 0..2000_u64 {
            let bytes = entropy(offset);
            let unfocused = generate(&bytes, None);
            let diffs = unfocused
                .as_ref()
                .is_some_and(|case| case.document.write().contains("Expr.diff"));
            if !diffs {
                assert_eq!(generate(&bytes, Some(&every)), unfocused, "offset {offset}");
            }
            compared += usize::from(unfocused.is_some() && !diffs);
        }
        assert!(compared > 1800, "{compared} cases compared");
    }

    #[test]
    fn a_method_that_gives_a_count_the_data_decide_is_drawn_only_where_a_focus_names_it() {
        let held_back: Vec<&'static str> = ids_beginning("")
            .into_iter()
            .filter(|id| is_held_back(entry(id)))
            .collect();
        assert!(held_back.len() >= 10, "{held_back:?}");
        let calls = |written: &str, id: &str| written.contains(&format!("\"entry\":\"{id}\""));
        let focus = Focus::parse("head,filter").unwrap().unwrap();
        let (mut without, mut with) = (0, 0);
        for offset in 0..3000_u64 {
            let bytes = entropy(offset);
            if let Some(case) = generate(&bytes, None) {
                let written = case.document.write();
                without += held_back.iter().filter(|id| calls(&written, id)).count();
            }
            if let Some(case) = generate(&bytes, Some(&focus)) {
                let written = case.document.write();
                with += usize::from(calls(&written, "Expr.head") || calls(&written, "Expr.filter"));
                // Only the methods named are drawn of those held back.
                for id in held_back
                    .iter()
                    .filter(|id| !matches!(**id, "Expr.head" | "Expr.filter"))
                {
                    assert!(!calls(&written, id), "{id} drawn at offset {offset}");
                }
            }
        }
        assert_eq!(without, 0, "a method held back was drawn without a focus");
        assert!(
            with > 100,
            "{with} cases draw a method that the focus names"
        );
    }

    /// The identifiers of the steps drawn for themselves.
    const STEPS: [&str; 13] = [
        "LazyFrame.reverse",
        "LazyFrame.head",
        "LazyFrame.limit",
        "LazyFrame.tail",
        "LazyFrame.slice",
        "LazyFrame.first",
        "LazyFrame.last",
        "LazyFrame.gather_every",
        "LazyFrame.drop",
        "LazyFrame.drop_nulls",
        "LazyFrame.drop_nans",
        "LazyFrame.fill_nan",
        "LazyFrame.with_row_index",
    ];

    #[test]
    fn a_focus_names_a_step_by_its_identifier_and_all_of_them_by_the_owner_and_a_dot() {
        let one = Focus::parse("LazyFrame.head").unwrap().unwrap();
        assert_eq!(one.names(), "LazyFrame.head");
        let all = Focus::parse("LazyFrame.").unwrap().unwrap();
        assert_eq!(all.names().split(", ").collect::<Vec<_>>(), STEPS);
        // A step is named with methods and functions, each once.
        let mixed = Focus::parse("over, LazyFrame.head, LazyFrame.head, date")
            .unwrap()
            .unwrap();
        assert_eq!(mixed.names(), "over, date, LazyFrame.head");
        // The name of a method of an expression that a step shares is of the method alone.
        let method = Focus::parse("reverse").unwrap().unwrap();
        assert_eq!(method.names(), "reverse");
        assert_eq!(focused_ids(&method), ["Expr.reverse"]);
    }

    #[test]
    fn every_step_that_is_drawn_for_itself_is_drawn_without_a_focus() {
        let mut drawn = std::collections::BTreeSet::new();
        for offset in 0..4000_u64 {
            let Some(case) = generate(&entropy(offset), None) else {
                continue;
            };
            let written = case.document.write();
            for id in STEPS {
                if written.contains(&format!("\"entry\":\"{id}\"")) {
                    drawn.insert(id);
                }
            }
        }
        assert_eq!(drawn.into_iter().collect::<Vec<_>>(), {
            let mut every = STEPS.to_vec();
            every.sort_unstable();
            every
        });
    }

    #[test]
    fn a_focus_on_a_step_draws_it_more_often_and_the_case_holds_it() {
        let focus = Focus::parse("LazyFrame.slice").unwrap().unwrap();
        let holding = |focus: Option<&Focus>| {
            (0..2000_u64)
                .filter_map(|offset| generate(&entropy(offset), focus))
                .filter(|case| {
                    case.document
                        .write()
                        .contains("\"entry\":\"LazyFrame.slice\"")
                })
                .count()
        };
        let (without, with) = (holding(None), holding(Some(&focus)));
        assert!(
            with > 4 * without.max(10),
            "{with} focused cases against {without}"
        );
        for offset in 0..200_u64 {
            if let Some(case) = generate(&entropy(offset), Some(&focus)) {
                // A case that holds the step is judged to hold the focus.
                assert_eq!(
                    focus.is_held_by(&case.document),
                    case.document.write().contains("LazyFrame.slice"),
                    "offset {offset}"
                );
            }
        }
    }

    #[test]
    fn a_focus_draws_its_methods_more_often() {
        let focus = Focus::parse("dt.replace").unwrap().unwrap();
        let holding = |focus: Option<&Focus>| {
            (0..2000_u64)
                .filter_map(|offset| generate(&entropy(offset), focus))
                .filter(|case| {
                    case.document
                        .write()
                        .contains("\"entry\":\"Expr.dt.replace\"")
                })
                .count()
        };
        let (without, with) = (holding(None), holding(Some(&focus)));
        assert!(
            with > 4 * without.max(10),
            "{with} focused cases against {without}"
        );
    }

    /// Whether the document of `case` holds a `shift` by a whole number below
    /// zero, which is written as a literal of the number.
    fn shifts_back(case: &Case) -> bool {
        let written = case.document.write();
        written.contains("\"Expr.shift\"") && written.contains("\"integer\":-")
    }

    /// Whether every call of `id` in the JSON `value` is given as `length` a literal alone.
    fn lengths_are_literals(value: &serde_json::Value, ids: &[&str], found: &mut usize) -> bool {
        match value {
            serde_json::Value::Object(map) => {
                let mut fine = true;
                if map
                    .get("entry")
                    .and_then(|entry| entry.as_str())
                    .is_some_and(|entry| ids.contains(&entry))
                    && let Some(length) = map.get("arguments").and_then(|given| given.get("length"))
                {
                    *found += 1;
                    let expression = &length["expression"];
                    fine = expression["then"].as_array().is_some_and(Vec::is_empty)
                        && expression["start"]["entry"] == "polars.lit";
                }
                fine && map
                    .values()
                    .all(|inside| lengths_are_literals(inside, ids, found))
            }
            serde_json::Value::Array(items) => items
                .iter()
                .all(|inside| lengths_are_literals(inside, ids, found)),
            _ => true,
        }
    }

    #[test]
    fn the_length_a_text_is_padded_to_is_a_literal_that_cannot_be_a_huge_number() {
        // A date, a time or a timestamp that is a length converts to the number it counts, which
        // for a timestamp is the microseconds since the epoch, and Polars builds a text of that many
        // characters: a case that never ends and fills the memory, which is no table's business.
        // The generator gives a length as a literal of the number 1, as it gives the shift of `shift`.
        let mut found = 0;
        for offset in 0..8000_u64 {
            let Some(case) = generate(&entropy(offset), None) else {
                continue;
            };
            let document: serde_json::Value =
                serde_json::from_str(case.document.write().as_str()).expect("a document");
            assert!(
                lengths_are_literals(
                    &document,
                    &["Expr.str.pad_start", "Expr.str.pad_end", "Expr.str.zfill"],
                    &mut found
                ),
                "{}",
                case.document.write()
            );
        }
        assert!(found > 5, "the cases pad text only {found} times");
    }

    /// Whether every share a quantile is given in the JSON `value` is a literal alone of a numeric
    /// dtype, or of none, which is a number typed in.
    fn shares_are_numbers(value: &serde_json::Value, found: &mut usize) -> bool {
        const NUMERIC: [&str; 10] = [
            "Int8", "Int16", "Int32", "Int64", "UInt8", "UInt16", "UInt32", "UInt64", "Float32",
            "Float64",
        ];
        match value {
            serde_json::Value::Object(map) => {
                let mut fine = true;
                if map
                    .get("entry")
                    .and_then(|entry| entry.as_str())
                    .is_some_and(|entry| {
                        entry == "Expr.quantile" || entry == "Expr.approx_quantile"
                    })
                    && let Some(share) =
                        map.get("arguments").and_then(|given| given.get("quantile"))
                {
                    *found += 1;
                    let expression = &share["expression"];
                    let literal = &expression["start"];
                    fine = expression["then"].as_array().is_some_and(Vec::is_empty)
                        && literal["entry"] == "polars.lit"
                        && literal["arguments"]["dtype"]["dtype"]
                            .as_str()
                            .is_none_or(|dtype| NUMERIC.contains(&dtype));
                }
                fine && map.values().all(|inside| shares_are_numbers(inside, found))
            }
            serde_json::Value::Array(items) => {
                items.iter().all(|inside| shares_are_numbers(inside, found))
            }
            _ => true,
        }
    }

    #[test]
    fn the_share_of_a_quantile_is_a_number_as_the_editor_offers_it() {
        // Polars reads a text or a moment as the share it writes in a select and refuses it where a
        // window or a pivot's aggregate evaluates the values of a group, so the generator gives the
        // share the editor offers: a number, typed in or the number 1 of a numeric dtype.
        let mut found = 0;
        for offset in 0..8000_u64 {
            let Some(case) = generate(&entropy(offset), None) else {
                continue;
            };
            let document: serde_json::Value =
                serde_json::from_str(case.document.write().as_str()).expect("a document");
            assert!(
                shares_are_numbers(&document, &mut found),
                "{}",
                case.document.write()
            );
        }
        assert!(found > 5, "the cases take a quantile only {found} times");
    }

    #[test]
    fn the_cases_include_a_shift_back_and_a_shift_forward() {
        // The limit for what a shift leaves null follows the direction of the
        // shift, so the target must reach both for a campaign to check it.
        let cases: Vec<Case> = (0..6000_u64)
            .filter_map(|offset| generate(&entropy(offset), None))
            .collect();
        let back = cases.iter().filter(|case| shifts_back(case)).count();
        let forward = cases
            .iter()
            .filter(|case| {
                let written = case.document.write();
                written.contains("\"Expr.shift\"") && !shifts_back(case)
            })
            .count();
        assert!(back > 0, "no case of {} shifts back", cases.len());
        assert!(forward > 0, "no case of {} shifts forward", cases.len());
    }

    /// The argument names of every `over` in `value`, one list for each.
    fn windows(value: &serde_json::Value, found: &mut Vec<Vec<String>>) {
        match value {
            serde_json::Value::Object(object) => {
                if object.get("entry").and_then(serde_json::Value::as_str) == Some("Expr.over") {
                    found.push(
                        object
                            .get("arguments")
                            .and_then(serde_json::Value::as_object)
                            .map(|arguments| arguments.keys().cloned().collect())
                            .unwrap_or_default(),
                    );
                }
                object.values().for_each(|inner| windows(inner, found));
            }
            serde_json::Value::Array(items) => items.iter().for_each(|inner| windows(inner, found)),
            _ => {}
        }
    }

    #[test]
    fn the_cases_include_windows_with_no_partition_by_and_none_with_nothing_to_group_or_order_by() {
        // Polars gives a window ordered by a column and given no `partition_by` a
        // dtype, with options or without, and refuses a window given neither, so a
        // campaign checks the first and never draws the second.
        let mut all = Vec::new();
        for case in (0..6000_u64).filter_map(|offset| generate(&entropy(offset), None)) {
            let document: serde_json::Value =
                serde_json::from_str(case.document.write().lines().last().expect("a document"))
                    .expect("a document is JSON");
            windows(&document, &mut all);
        }
        let has = |window: &[String], name: &str| window.iter().any(|given| given == name);
        let options = ["descending", "nulls_last", "mapping_strategy"];
        let unpartitioned: Vec<_> = all
            .iter()
            .filter(|window| !has(window, "partition_by"))
            .collect();
        assert!(
            unpartitioned.iter().any(|window| has(window, "order_by")
                && !options.iter().any(|option| has(window, option))),
            "no window is ordered without a partition or options, of {}",
            all.len()
        );
        for option in options {
            assert!(
                unpartitioned
                    .iter()
                    .any(|window| has(window, "order_by") && has(window, option)),
                "no window is ordered with {option} and without a partition, of {}",
                all.len()
            );
        }
        assert!(
            all.iter()
                .all(|window| has(window, "partition_by") || has(window, "order_by")),
            "a window is given neither partition_by nor order_by"
        );
        assert!(
            all.iter()
                .all(|window| !has(window, "more_exprs") || has(window, "partition_by")),
            "a window is given more_exprs without partition_by"
        );
    }
}

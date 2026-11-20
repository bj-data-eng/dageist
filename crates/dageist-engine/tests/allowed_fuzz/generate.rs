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
    self, Applies, Entry, Kind, ParameterDefault, Shape as ParamShape, ValueType,
};
use dageist_engine::document::{Call, Document, Dtype, Expression, Start, Step, StepId, Value};
use dageist_fuzz::generation::Entropy;

use super::case::{Case, STEP};
use super::frames::{ON_COLUMNS, OTHER, key_column};

/// Fewest bytes of entropy a case is made from. A shorter input is discarded,
/// for what it lacks would be read as zeros, the same choice every time.
pub const LEAST_ENTROPY: usize = 96;

/// Draws small numbers from entropy, one byte for each. Past its end it gives
/// zeros, the first choice of everything.
struct Draw<'a> {
    entropy: Entropy<'a>,
}

impl<'a> Draw<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            entropy: Entropy::new(bytes),
        }
    }

    /// A number below `count`, which is at most 256.
    fn below(&mut self, count: usize) -> usize {
        debug_assert!((1..=256).contains(&count));
        let byte = self
            .entropy
            .take(1)
            .map_or(0, |bytes| usize::from(bytes[0]));
        byte % count
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
        let mut left = self.below(total.min(256));
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
}

impl Host {
    const ALL: [(usize, Self); 8] = [
        (22, Self::Select),
        (8, Self::WithColumns),
        (16, Self::Filter),
        (14, Self::Sort),
        (12, Self::JoinLeft),
        (10, Self::JoinRight),
        (8, Self::JoinOn),
        (18, Self::Pivot),
    ];

    fn step(self) -> &'static str {
        match self {
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
            Self::Pivot => &[],
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
    receiver: Representative,
    argument: Representative,
    host: Host,
    /// How many more calls may be added.
    budget: usize,
    /// Whether a number typed without a dtype may be drawn. It is not among the values of a
    /// condition: alone there Polars cannot resolve it where it runs, in a sort key, for one,
    /// which the tables cannot say.
    untyped_numbers: bool,
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
    Representative::ALL
        .into_iter()
        .filter(|class| Dtype::new(class.dtype()).is_ok())
        .collect()
}

/// The classes the whole number 1 is cast to as a literal.
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

pub fn generate(bytes: &[u8]) -> Option<Case> {
    if bytes.len() < LEAST_ENTROPY {
        return None;
    }
    let mut draw = Draw::new(bytes);
    let receiver = draw.pick(&Representative::ALL);
    let argument = draw.pick(&Representative::ALL);
    let host = draw.weighted(&Host::ALL);
    let mut generator = Gen {
        draw,
        receiver,
        argument,
        host,
        budget: 18,
        untyped_numbers: true,
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

    /// The options an entry is given: none, or, as the option table reads
    /// them, one when the entry has only one, and two or more otherwise.
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
            || parameters.is_empty()
            || !self.draw.chance(45)
        {
            return Vec::new();
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
        chosen
            .into_iter()
            .map(|at| {
                let parameter = parameters[at];
                let value = match parameter.kind() {
                    Kind::Choice(words) => Value::Choice((*self.draw.pick(words)).to_owned()),
                    _ => {
                        let unsettable = parameter.default() == ParameterDefault::Python("None");
                        match self.draw.below(if unsettable { 3 } else { 2 }) {
                            0 => Value::Flag(Some(true)),
                            1 => Value::Flag(Some(false)),
                            _ => Value::Flag(None),
                        }
                    }
                };
                (parameter.name(), value)
            })
            .collect()
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
        let class = self.draw.pick(&NUMBERED);
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

    // -----------------------------------------------------------------------
    // Chains

    fn start(&mut self, host_is_group: bool, depth: usize) -> (Call, Option<Representative>, bool) {
        // The start of a condition is the third element: whether it is one.
        let choice = if host_is_group {
            self.draw
                .weighted(&[(70, 0_usize), (8, 1), (12, 2), (10, 3)])
        } else {
            self.draw
                .weighted(&[(56, 1_usize), (20, 2), (18, 3), (3, 0), (3, 4)])
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
            _ => (
                call("polars.element", Vec::new()),
                Some(self.receiver),
                false,
            ),
        }
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
            self.start(group, depth)
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
        let methods: Vec<&'static Entry> = catalog::entries()
            .iter()
            .filter(|entry| entry.applies() == Applies::Expression)
            .collect();
        for attempt in 0..6 {
            let chosen = methods[self.draw.below(methods.len())];
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
                ("Expr.shift", _) => vec![self.scalar(name == "n")],
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
                // How many matches `replace` replaces: none, some, or all of them below zero.
                "n" => Value::Integer(self.draw.pick(&[-1, 0, 1, 1, 2, 3])),
                "abs_tol" | "rel_tol" => Value::Float(self.draw.pick(&[0.0, 0.5, 1e-9, 1.0])),
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
            if parameter.kind() != Kind::Value(ValueType::String) || !self.draw.chance(50) {
                continue;
            }
            let name = parameter.name();
            let value = match name {
                "delimiter" => self.draw.pick(&["", "-", ", ", "é", "ab"]),
                "fill_char" => self.draw.pick(&["*", "0", " ", "é", "-"]),
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
            let Some(case) = generate(&entropy(offset)) else {
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

    #[test]
    fn the_cases_include_a_shift_back_and_a_shift_forward() {
        // The limit for what a shift leaves null follows the direction of the
        // shift, so the target must reach both for a campaign to check it.
        let cases: Vec<Case> = (0..6000_u64)
            .filter_map(|offset| generate(&entropy(offset)))
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
        for case in (0..6000_u64).filter_map(|offset| generate(&entropy(offset))) {
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

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
use dageist_engine::catalog::{self, Applies, Entry, Kind, ParameterDefault, Shape as ParamShape};
use dageist_engine::document::{Call, Document, Dtype, Expression, Start, Step, StepId, Value};
use dageist_fuzz::generation::Entropy;

use super::case::{Case, STEP};
use super::frames::{ON_COLUMNS, OTHER};

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
    Pivot,
}

impl Host {
    const ALL: [(usize, Self); 7] = [
        (22, Self::Select),
        (8, Self::WithColumns),
        (16, Self::Filter),
        (14, Self::Sort),
        (12, Self::JoinLeft),
        (10, Self::JoinRight),
        (18, Self::Pivot),
    ];

    fn step(self) -> &'static str {
        match self {
            Self::Select => "LazyFrame.select",
            Self::WithColumns => "LazyFrame.with_columns",
            Self::Filter => "LazyFrame.filter",
            Self::Sort => "LazyFrame.sort",
            Self::JoinLeft | Self::JoinRight => "LazyFrame.join",
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
            Self::Pivot => "aggregate_function",
        }
    }

    /// The names of the columns an expression reads here: the left table's,
    /// the other table's for `right_on`, and none for a pivot's group.
    fn columns(self) -> &'static [&'static str] {
        match self {
            Self::JoinRight => &["ra", "rb", "rf", "rk", "rg", "rh"],
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
        let held = self.chain(&mut slots, 0, goal).expression;
        let as_list = |expression: Expression| Value::Many(vec![Value::Expression(expression)]);
        let key =
            || as_list(Expression::new(Start::Call(column("key")), Vec::new()).expect("a column"));
        let mut given: Vec<(&str, Value)> = match host {
            Host::Select | Host::WithColumns => vec![("exprs", as_list(held))],
            Host::Filter => vec![("predicates", as_list(held))],
            Host::Sort => vec![("by", as_list(held))],
            Host::JoinLeft => vec![
                ("other", Value::Table(OTHER.to_owned())),
                ("left_on", as_list(held)),
                ("right_on", key()),
            ],
            Host::JoinRight => vec![
                ("other", Value::Table(OTHER.to_owned())),
                ("left_on", key()),
                ("right_on", as_list(held)),
            ],
            Host::Pivot => vec![
                ("on", Value::Columns(vec!["h".to_owned()])),
                ("on_columns", Value::Table(ON_COLUMNS.to_owned())),
                ("index", Value::Columns(vec!["k".to_owned()])),
                ("values", Value::Columns(vec!["a".to_owned()])),
                ("aggregate_function", Value::Expression(held)),
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
        match self.draw.below(3) {
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
    /// number 1 of a dtype, as the tables measured them.
    fn scalar(&mut self) -> Built {
        let class = self.draw.pick(&NUMBERED);
        let literal = match dtype_value(class) {
            Some(dtype) => call(
                "polars.lit",
                vec![("value", Value::Integer(1)), ("dtype", dtype)],
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
        for parameter in entry.parameters() {
            if !matches!(parameter.kind(), Kind::Expression(_)) {
                continue;
            }
            let name = parameter.name();
            let required = parameter.default() == ParameterDefault::Required;
            let given_it = match (id, name) {
                (_, _) if required => true,
                ("Expr.over", "partition_by") => true,
                ("Expr.over", "order_by") => self.draw.chance(35),
                ("Expr.over", "more_exprs") => self.draw.chance(6),
                _ => self.draw.chance(30),
            };
            if !given_it {
                continue;
            }
            let position = Slot::of_parameter(id, name);
            slots.extend(position);
            let built: Vec<Built> = match (id, name) {
                ("Expr.shift", _) => vec![self.scalar()],
                ("Expr.dt.offset_by", "by") => {
                    // The durations are text of the table's: `b`, whose rows
                    // are durations where it is text, as the tables measured it.
                    let (literal, class) = if self.draw.chance(70) || self.host == Host::Pivot {
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

/// The class every value of a condition has, when they have the same one.
fn unify(values: &[Option<Representative>]) -> Option<Representative> {
    let first = (*values.first()?)?;
    values
        .iter()
        .all(|value| *value == Some(first))
        .then_some(first)
}

/// The options `call` was given, each by the parameter and its word as the
/// option table writes it, which `offer` is asked with.
fn option_words(call: &Call) -> Vec<(String, String)> {
    let entry = call.entry();
    call.arguments()
        .iter()
        .filter(|given| {
            entry.parameters().iter().any(|parameter| {
                parameter.name() == given.parameter()
                    && matches!(parameter.kind(), Kind::Flag | Kind::Choice(_))
            })
        })
        .filter_map(|given| {
            let word = match given.value() {
                Value::Flag(Some(true)) => "true".to_owned(),
                Value::Flag(Some(false)) => "false".to_owned(),
                Value::Flag(None) => "unset".to_owned(),
                Value::Choice(word) => word.clone(),
                _ => return None,
            };
            Some((given.parameter().to_owned(), word))
        })
        .collect()
}

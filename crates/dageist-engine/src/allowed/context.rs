//! The contexts an expression can stand in.
//!
//! A context is a named position that holds an expression: the items a
//! `select` computes, a `filter`'s predicate, a window's `partition_by`. The
//! tables record what Polars accepts in each. A context is at most two
//! positions: a position of a step, and a position inside the expression that
//! stands there, such as a window's `order_by` inside a `select` item. A place
//! deeper than that is judged by its outer position and the innermost nested
//! one.

use std::fmt;

/// A position of a step that holds an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Outer {
    /// An item of `select`, or of `with_columns`.
    Select,
    /// The predicate of `filter`.
    Filter,
    /// A key of `sort`.
    Sort,
    /// A key of `join` evaluated against the table it starts from: `left_on`,
    /// and `on`, which is also evaluated as [`Outer::JoinRight`] is, against the
    /// other table.
    JoinLeft,
    /// `join`'s `right_on`, evaluated against the other table's columns, not
    /// those of the table the step starts from, and `on` too.
    JoinRight,
    /// A pivot's aggregate, a chain that starts from `element()`, which
    /// stands for the values of one group.
    PivotAggregate,
    /// A key of `group_by`, a column or an expression, which names the group each row is in.
    GroupKey,
    /// An aggregate of `agg`, evaluated on the rows of each group: a chain that aggregates gives one
    /// value for each group, and one that does not, such as `col("a") + 1`, gives a list of its
    /// values for each group.
    Aggregate,
    /// A predicate of `having`, evaluated on the rows of each group as an aggregate is, and
    /// giving one value for each group, which Polars casts to a `Boolean` to keep the group or not.
    Having,
}

/// A position inside an expression that holds another expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Nested {
    /// What `over` groups by.
    PartitionBy,
    /// What `over` orders each group by.
    OrderBy,
    /// A predicate of a condition: of `when`, or of a further `when`.
    ConditionPredicate,
    /// A value of a condition: of `then`, or of `otherwise`.
    ConditionValue,
}

impl Outer {
    /// Every position of a step.
    pub const ALL: [Self; 9] = [
        Self::Select,
        Self::Filter,
        Self::Sort,
        Self::JoinLeft,
        Self::JoinRight,
        Self::PivotAggregate,
        Self::GroupKey,
        Self::Aggregate,
        Self::Having,
    ];

    /// The name the tables give the position.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Filter => "filter",
            Self::Sort => "sort",
            Self::JoinLeft => "join_left",
            Self::JoinRight => "join_right",
            Self::PivotAggregate => "pivot_aggregate",
            Self::GroupKey => "group_key",
            Self::Aggregate => "aggregate",
            Self::Having => "having",
        }
    }

    /// The step and parameter that hold the position.
    #[must_use]
    pub const fn host(self) -> (&'static str, &'static str) {
        match self {
            Self::Select => ("LazyFrame.select", "exprs"),
            Self::Filter => ("LazyFrame.filter", "predicates"),
            Self::Sort => ("LazyFrame.sort", "by"),
            Self::JoinLeft => ("LazyFrame.join", "left_on"),
            Self::JoinRight => ("LazyFrame.join", "right_on"),
            Self::PivotAggregate => ("LazyFrame.pivot", "aggregate_function"),
            Self::GroupKey => ("LazyFrame.group_by", "by"),
            Self::Aggregate => ("LazyGroupBy.agg", "aggs"),
            Self::Having => ("LazyGroupBy.having", "predicates"),
        }
    }

    /// The parameter of the same step that holds the table whose columns an
    /// expression in this position is evaluated against, when that is not the
    /// table the step starts from.
    #[must_use]
    pub const fn evaluated_against(self) -> Option<&'static str> {
        match self {
            Self::JoinRight => Some("other"),
            _ => None,
        }
    }
}

impl Nested {
    /// Every position inside an expression.
    pub const ALL: [Self; 4] = [
        Self::PartitionBy,
        Self::OrderBy,
        Self::ConditionPredicate,
        Self::ConditionValue,
    ];

    /// The name the tables give the position.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PartitionBy => "over.partition_by",
            Self::OrderBy => "over.order_by",
            Self::ConditionPredicate => "condition.predicate",
            Self::ConditionValue => "condition.value",
        }
    }

    /// The entry and parameter that hold the position.
    #[must_use]
    pub const fn host(self) -> (&'static str, &'static str) {
        match self {
            Self::PartitionBy => ("Expr.over", "partition_by"),
            Self::OrderBy => ("Expr.over", "order_by"),
            Self::ConditionPredicate => ("polars.when", "predicates"),
            Self::ConditionValue => ("When.then", "statement"),
        }
    }
}

/// A position that holds an expression, of a step or inside an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Position {
    /// A position of a step.
    Outer(Outer),
    /// A position inside an expression.
    Nested(Nested),
}

impl Position {
    /// The position the parameter `parameter` of the entry `entry` holds an
    /// expression in, if it is one of the named positions.
    #[must_use]
    pub fn of_parameter(entry: &str, parameter: &str) -> Option<Self> {
        Some(match (entry, parameter) {
            ("LazyFrame.select" | "LazyFrame.with_columns", "exprs" | "named_exprs") => {
                Self::Outer(Outer::Select)
            }
            ("LazyFrame.filter", "predicates") => Self::Outer(Outer::Filter),
            ("LazyFrame.sort", "by" | "more_by") => Self::Outer(Outer::Sort),
            ("LazyFrame.join", "on" | "left_on") => Self::Outer(Outer::JoinLeft),
            ("LazyFrame.join", "right_on") => Self::Outer(Outer::JoinRight),
            ("LazyFrame.pivot", "aggregate_function") => Self::Outer(Outer::PivotAggregate),
            ("LazyFrame.group_by", "by" | "named_by") => Self::Outer(Outer::GroupKey),
            ("LazyGroupBy.agg", "aggs" | "named_aggs") => Self::Outer(Outer::Aggregate),
            ("LazyGroupBy.having", "predicates") => Self::Outer(Outer::Having),
            ("Expr.over", "partition_by" | "more_exprs") => Self::Nested(Nested::PartitionBy),
            ("Expr.over", "order_by") => Self::Nested(Nested::OrderBy),
            ("polars.when" | "Then.when" | "ChainedThen.when", "predicates") => {
                Self::Nested(Nested::ConditionPredicate)
            }
            (
                "When.then" | "Then.otherwise" | "ChainedWhen.then" | "ChainedThen.otherwise",
                "statement",
            ) => Self::Nested(Nested::ConditionValue),
            _ => return None,
        })
    }
}

/// Where an expression stands: a position of a step, and when it is inside
/// another expression, the position in that one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Context {
    outer: Outer,
    nested: Option<Nested>,
}

impl Context {
    /// Every context the tables cover: the nine positions of a step, and each
    /// of them with each of the four positions inside an expression.
    #[must_use]
    pub fn all() -> Vec<Self> {
        Outer::ALL
            .into_iter()
            .flat_map(|outer| {
                std::iter::once(Self::step(outer)).chain(
                    Nested::ALL
                        .into_iter()
                        .map(move |nested| Self::inside(outer, nested)),
                )
            })
            .collect()
    }

    /// The context of a position of a step.
    #[must_use]
    pub const fn step(outer: Outer) -> Self {
        Self {
            outer,
            nested: None,
        }
    }

    /// The context of a position inside the expression that stands at
    /// `outer`.
    #[must_use]
    pub const fn inside(outer: Outer, nested: Nested) -> Self {
        Self {
            outer,
            nested: Some(nested),
        }
    }

    /// The context of an expression found by following `positions`, the
    /// outermost first. The tables go two positions deep, so a place deeper
    /// than that is judged by its outer position and the innermost nested one:
    /// an `over` key inside a condition's value is a key, and a predicate
    /// inside a condition's value is a predicate. `None` when the way does not
    /// start at a position of a step.
    #[must_use]
    pub fn from_positions(positions: &[Position]) -> Option<Self> {
        let Some(Position::Outer(outer)) = positions.first().copied() else {
            return None;
        };
        let nested = positions
            .iter()
            .skip(1)
            .rev()
            .find_map(|position| match position {
                Position::Nested(nested) => Some(*nested),
                Position::Outer(_) => None,
            });
        Some(Self { outer, nested })
    }

    /// Whether an expression that stands here is one of a join's own keys,
    /// where Polars needs it to be elementwise. The key of a window is not: it
    /// stands in a select, a filter or a sort key, and Polars accepts one that
    /// is not elementwise.
    #[must_use]
    pub const fn is_join_key(&self) -> bool {
        self.nested.is_none() && matches!(self.outer, Outer::JoinLeft | Outer::JoinRight)
    }

    /// The position of the step.
    #[must_use]
    pub const fn outer(&self) -> Outer {
        self.outer
    }

    /// The position inside the expression, if the context has one.
    #[must_use]
    pub const fn nested(&self) -> Option<Nested> {
        self.nested
    }

    /// The name the tables give the context: `select`, or
    /// `select > over.order_by`.
    #[must_use]
    pub fn name(&self) -> String {
        self.to_string()
    }

    /// The context named `name` in a table.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|context| context.name() == name)
    }
}

impl fmt::Display for Context {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.outer.name())?;
        match self.nested {
            Some(nested) => write!(formatter, " > {}", nested.name()),
            None => Ok(()),
        }
    }
}

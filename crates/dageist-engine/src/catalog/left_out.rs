//! The Python Polars items the catalogue does not have an entry for, each with
//! the reason. Together with the entries they are every item of
//! `api/polars/python.txt`; a test proves it, so a new, renamed or removed
//! Polars item shows up as a name to place here or in the entries.

/// Why an item of the inventory has no entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    /// Polars marks the item deprecated.
    Deprecated,
    /// A parameter takes a Python function. Its place is the custom
    /// expression language, not an entry.
    TakesPythonFunction,
    /// It only makes sense with an object that exists in Python alone, such
    /// as a pyarrow dataset or a Delta table.
    PythonObject,
    /// It is not an operation a step could use, and never will be: it only
    /// raises, is Python's own syntax for an operation that has its own entry,
    /// returns what it is given, or runs, reads or shows a query, which the
    /// host does itself. Each such item is justified where `LEFT_OUT` lists it.
    NeverOffered,
    /// It could be offered but is not yet. The list of these shrinks as the
    /// catalogue grows, so its length measures how far the catalogue is from
    /// complete.
    NotYetCatalogued,
}

/// One item of the inventory without an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeftOut {
    id: &'static str,
    reason: Reason,
}

impl LeftOut {
    const fn new(id: &'static str, reason: Reason) -> Self {
        Self { id, reason }
    }

    /// The item's name in the inventory, such as `Expr.abs`.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// Why the item has no entry.
    #[must_use]
    pub const fn reason(&self) -> Reason {
        self.reason
    }
}

/// Every item of the inventory without an entry, ordered by name.
#[must_use]
pub fn left_out() -> &'static [LeftOut] {
    LEFT_OUT
}

// Items marked `NeverOffered`, and why each is not an operation a step could use:
//
// - `LazyFrame.__eq__`, `__ne__`, `__lt__`, `__le__`, `__gt__` and `__ge__` only raise
//   a `TypeError`: a lazy frame has no value to compare until it is collected.
// - `LazyFrame.__contains__` is Python's `in`: it asks whether a column name is in the
//   schema and gives a bool, not a table or an expression.
// - `LazyFrame.__getitem__` is Python's subscript syntax for `LazyFrame.slice`, which
//   is the item to catalogue for that operation; it supports no other subscript.
// - `LazyFrame.clone` and `LazyFrame.lazy` return the frame they are given, unchanged.
// - `LazyFrame.collect`, `collect_async`, `collect_batches`, `execute`, and
//   `polars.collect_all` and `collect_all_async` run a query and hand the rows to
//   Python. The host runs queries and reads their rows itself.
// - `LazyFrame.collect_schema`, `columns`, `dtypes`, `schema` and `width` read a
//   schema, which the host reads itself to know what each step gives.
// - `LazyFrame.explain`, `show`, `show_graph`, `inspect` and `polars.explain_all` print,
//   draw or describe a plan or its rows and leave the table as it was.
// - The dunder methods of `Expr` are Python's operator syntax, `+`, `~`, `<` and the rest. Each
//   spells an operation that has its own entry, and offering both would show one operation
//   twice:
//   - `__add__`, `__sub__`, `__mul__`, `__truediv__`, `__floordiv__`, `__mod__` and `__pow__`
//     spell `Expr.add`, `sub`, `mul`, `truediv`, `floordiv`, `mod` and `pow`;
//   - `__neg__` spells `Expr.neg` and `__abs__`, which is Python's `abs(expr)`, `Expr.abs`;
//   - `__and__`, `__or__` and `__xor__` spell `Expr.and_`, `or_` and `xor`, and `__invert__`,
//     Python's `~`, spells `Expr.not_`;
//   - `__eq__`, `__ne__`, `__lt__`, `__le__`, `__gt__` and `__ge__` spell `Expr.eq`, `ne`,
//     `lt`, `le`, `gt` and `ge`;
//   - the reflected forms, `__radd__`, `__rsub__`, `__rmul__`, `__rtruediv__`, `__rfloordiv__`,
//     `__rmod__`, `__rpow__`, `__rand__`, `__ror__` and `__rxor__`, are the same operations
//     with the sides swapped, which is what Python does when the value on the left of the
//     operator is not an expression. A step puts a value on the left by starting its chain
//     from that value, as `lit(2)` and then `Expr.sub`, so each reflected form is the entry of
//     its own operation with the chain on the other side;
//   - `__pos__` returns the expression it is given, as `LazyFrame.clone` returns its frame.
// - `polars.escape_regex` gives a Python string, the text with the characters of a pattern
//   escaped, and not an expression, so no start of the catalogue is its form. What a step needs
//   of it is there already: every method that takes a pattern takes `literal`, which makes the
//   text plain text, and `Expr.str.escape_regex` escapes the text of a column or a literal.
//
// Items marked `NotYetCatalogued` that wait for something, and what:
//
// - `Expr.str.contains_any`, `replace_many`, `extract_many` and `find_many` take a list of literal
//   patterns, which Polars matches all at once with Aho-Corasick. A pattern here is one regular
//   expression typed in; a list of texts, typed or connected from another node, is a kind of its
//   own, and the many-pattern methods wait for it.
// - `Expr.dt.add_business_days` and `is_business_day` take `week_mask` and `holidays`, a week of seven
//   flags and a list of dates, which are lists of literals, and a list of literals typed in, of
//   flags or of dates, is a kind of its own that does not exist yet.
// - The `*_ranges` functions, `polars.date_ranges`, `datetime_ranges`, `time_ranges`, `int_ranges` and
//   `linear_spaces`, give a list of values for each row, which waits for a list kind: a step has no
//   way yet to hold or draw a list that is made of its rows. The functions that give one range, with a
//   count of values their arguments decide, are catalogued, and `polars.arange` is Python's `int_range`
//   under another name.
// - `Expr.interpolate` gives as many values as it follows, but whether Polars takes a column depends
//   on its rows: it interpolates a column of `Boolean` that holds no null to a `Float64` and panics
//   on one that holds a null, and the method `nearest` does the same for text and arrays, where the
//   tables measured the sample's rows, which hold a null, and refuse them. A column that holds no
//   null is accepted by Polars and refused by the tables, so it waits for a way to say that a
//   column holds no null.
// - `Expr.item` gives the single value of an expression and refuses one that has none or more than
//   one, with "aggregation 'item' expected a single value, got 4 values". The shapes of the walk say
//   when it is accepted without the rows: after one value, which it gives again, and after a count of
//   exactly one; and when it is refused: after a count of more than one, and after none unless
//   `allow_empty` says so. What it is for is to find out that a table holds one row, and the rows are
//   no shape of the walk: the editor, which is strict where the data decide, would refuse it there,
//   so what it would accept is a call that gives back the value it is given. It waits for a way to
//   say how many rows a table has; `allow_empty` only admits a table of no rows.
// - `Expr.str.split_exact` and `splitn` give a struct, whose fields a step reads with
//   `Expr.struct.field`, which is not catalogued yet. `Expr.str.extract_groups` gives one too, and
//   is catalogued, for its fields are named by the pattern and Polars says them.
// - `LazyFrame.unique` keeps one of the rows that repeat, and its `keep` is `any` unless it is told
//   another, which is whichever of them Polars meets first; without `maintain_order` the rows are
//   in no order Polars promises either. What the step gives is not a table the engine can state
//   twice, which a step that shows its rows needs, so it waits for a decision on what a step of
//   rows that nothing fixes shows. Its `subset` also takes expressions beside names, which is a
//   position no table is measured in.
// - `LazyFrame.fill_null` and `shift` run a method of an expression over every column. Python
//   picks the columns `fill_null` fills by the Python type of its value, a number the numbers and
//   a text the text, and a strategy goes to every column, and `shift` takes its rows and its fill
//   as expressions. Whether Polars takes a column depends on its dtype and on what each is given,
//   as the tables of the method say, so the step is judged by the table of that method for every
//   column. `drop_nans` is the one step that is, for it gives `is_nan` no argument; a step whose
//   arguments meet each column is a shape of its own.
// - `LazyFrame.top_k` and `bottom_k` take `by` as expressions, which is a position none of the
//   tables is measured in, as the keys of a sort are.
// - `LazyFrame.max`, `min`, `sum`, `mean`, `median`, `std`, `var`, `quantile`, `count` and
//   `null_count` aggregate every column by a function of the table of their own, and not by the
//   method of an expression of that name: the sum of a column of text is a null, which `Expr.sum`
//   refuses. No table measures what each gives for each dtype, and `quantile` takes its quantile
//   as an expression.
// - `LazyFrame.cast` takes a mapping of columns to dtypes, a pairs parameter of a column and a
//   dtype, and a dtype alone for every column. What Polars takes depends on the dtype of each
//   column and the one it is cast to, which the tables say of `Expr.cast` for one column; a step
//   is judged by it for each pair, which is the shape `fill_null` waits for as well.
// - The methods of `Expr.list` and `Expr.arr` that are left take more than the list or the array
//   they follow, and wait for what each takes. The methods that take no expression argument are
//   catalogued, on a column of every element dtype:
//   - `list.concat`, `contains`, `count_matches`, `diff`, `gather`, `gather_every`, `get`, `head`,
//     `join`, `shift`, `slice`, `tail`, `to_array` and the four `set_*` methods, and `arr.contains`,
//     `count_matches`, `dot`, `get`, `head`, `join`, `shift`, `slice` and `tail`, take a value, a
//     place, a count or another list, which is a typed list of literals or a column of the lists'
//     own element dtype, and a list of literals typed in is a kind that does not exist yet.
//     `list.__getitem__` is Python's subscript for `get`. The `set_*` methods also panic on a list
//     of `Null`;
//   - `arr.reverse`, which Python builds as an evaluation of `element().reverse()` over each array,
//     makes an array that Polars panics on where an aggregate, a cut or a window follows it, with
//     "not all elements have the specified width", for every element dtype, so it is left out until
//     Polars evaluates it; `Expr.reverse` of an array reverses it as it does a list;
//   - `list.explode` and `arr.explode` give a value for each element, so how many values there
//     are is no longer what the table has, nor what the receiver has, which no count of the walk
//     says yet;
//   - `list.agg`, `eval` and `filter` and `arr.agg` and `eval` take a body whose `element()` is
//     one list's elements, a position that no table is measured in and that gives what its body
//     gives; `list.item` is the body `element().item()`;
//   - `list.sample` draws at random, which two runs do not repeat, `list.to_map` gives a dtype
//     that is not a class, and `list.to_struct` and `arr.to_struct` give a struct whose fields a
//     step reads with `Expr.struct.field`, which is not catalogued either.
// - `LazyFrame.explode` takes the columns of lists and arrays and refuses any other dtype, which
//   no table measures, for `Expr.explode` is not catalogued either.
// - `polars.concat` takes several tables, which are no receiver of a step: a step is a method
//   of the table before it, and a function that gives a table from a list of them is not an
//   entry of any applies-to the catalogue has.
const LEFT_OUT: &[LeftOut] = &[
    LeftOut::new("Expr.__abs__", Reason::NeverOffered),
    LeftOut::new("Expr.__add__", Reason::NeverOffered),
    LeftOut::new("Expr.__and__", Reason::NeverOffered),
    LeftOut::new("Expr.__eq__", Reason::NeverOffered),
    LeftOut::new("Expr.__floordiv__", Reason::NeverOffered),
    LeftOut::new("Expr.__ge__", Reason::NeverOffered),
    LeftOut::new("Expr.__gt__", Reason::NeverOffered),
    LeftOut::new("Expr.__invert__", Reason::NeverOffered),
    LeftOut::new("Expr.__le__", Reason::NeverOffered),
    LeftOut::new("Expr.__lt__", Reason::NeverOffered),
    LeftOut::new("Expr.__mod__", Reason::NeverOffered),
    LeftOut::new("Expr.__mul__", Reason::NeverOffered),
    LeftOut::new("Expr.__ne__", Reason::NeverOffered),
    LeftOut::new("Expr.__neg__", Reason::NeverOffered),
    LeftOut::new("Expr.__or__", Reason::NeverOffered),
    LeftOut::new("Expr.__pos__", Reason::NeverOffered),
    LeftOut::new("Expr.__pow__", Reason::NeverOffered),
    LeftOut::new("Expr.__radd__", Reason::NeverOffered),
    LeftOut::new("Expr.__rand__", Reason::NeverOffered),
    LeftOut::new("Expr.__rfloordiv__", Reason::NeverOffered),
    LeftOut::new("Expr.__rmod__", Reason::NeverOffered),
    LeftOut::new("Expr.__rmul__", Reason::NeverOffered),
    LeftOut::new("Expr.__ror__", Reason::NeverOffered),
    LeftOut::new("Expr.__rpow__", Reason::NeverOffered),
    LeftOut::new("Expr.__rsub__", Reason::NeverOffered),
    LeftOut::new("Expr.__rtruediv__", Reason::NeverOffered),
    LeftOut::new("Expr.__rxor__", Reason::NeverOffered),
    LeftOut::new("Expr.__sub__", Reason::NeverOffered),
    LeftOut::new("Expr.__truediv__", Reason::NeverOffered),
    LeftOut::new("Expr.__xor__", Reason::NeverOffered),
    LeftOut::new("Expr.alias", Reason::NotYetCatalogued),
    LeftOut::new("Expr.append", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arg_true", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.agg", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.contains", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.count_matches", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.dot", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.eval", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.explode", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.get", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.head", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.join", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.reverse", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.shift", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.slice", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.tail", Reason::NotYetCatalogued),
    LeftOut::new("Expr.arr.to_struct", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin.contains", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin.decode", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin.encode", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin.ends_with", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin.get", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin.head", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin.reinterpret", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin.size", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin.slice", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin.starts_with", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin.tail", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin_intervals", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin_quantiles", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bin_ranks", Reason::NotYetCatalogued),
    LeftOut::new("Expr.bottom_k_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.cat.ends_with", Reason::NotYetCatalogued),
    LeftOut::new("Expr.cat.len_bytes", Reason::NotYetCatalogued),
    LeftOut::new("Expr.cat.len_chars", Reason::NotYetCatalogued),
    LeftOut::new("Expr.cat.physical", Reason::NotYetCatalogued),
    LeftOut::new("Expr.cat.slice", Reason::NotYetCatalogued),
    LeftOut::new("Expr.cat.starts_with", Reason::NotYetCatalogued),
    LeftOut::new("Expr.cat.to", Reason::NotYetCatalogued),
    LeftOut::new("Expr.cumulative_eval", Reason::NotYetCatalogued),
    LeftOut::new("Expr.cut", Reason::Deprecated),
    LeftOut::new("Expr.deserialize", Reason::NotYetCatalogued),
    LeftOut::new("Expr.dot", Reason::NotYetCatalogued),
    LeftOut::new("Expr.dt.add_business_days", Reason::NotYetCatalogued),
    LeftOut::new("Expr.dt.is_business_day", Reason::NotYetCatalogued),
    LeftOut::new("Expr.ewm_mean", Reason::NotYetCatalogued),
    LeftOut::new("Expr.ewm_mean_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.ewm_std", Reason::NotYetCatalogued),
    LeftOut::new("Expr.ewm_sum", Reason::NotYetCatalogued),
    LeftOut::new("Expr.ewm_sum_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.ewm_var", Reason::NotYetCatalogued),
    LeftOut::new("Expr.exclude", Reason::NotYetCatalogued),
    LeftOut::new("Expr.explode", Reason::NotYetCatalogued),
    LeftOut::new("Expr.ext.storage", Reason::NotYetCatalogued),
    LeftOut::new("Expr.ext.to", Reason::NotYetCatalogued),
    LeftOut::new("Expr.extend_constant", Reason::NotYetCatalogued),
    LeftOut::new("Expr.get", Reason::NotYetCatalogued),
    LeftOut::new("Expr.hash", Reason::NotYetCatalogued),
    LeftOut::new("Expr.hist", Reason::NotYetCatalogued),
    LeftOut::new("Expr.implode", Reason::NotYetCatalogued),
    LeftOut::new("Expr.index_of", Reason::NotYetCatalogued),
    LeftOut::new("Expr.inspect", Reason::NotYetCatalogued),
    LeftOut::new("Expr.interpolate", Reason::NotYetCatalogued),
    LeftOut::new("Expr.interpolate_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.is_empty", Reason::NotYetCatalogued),
    LeftOut::new("Expr.is_sorted", Reason::NotYetCatalogued),
    LeftOut::new("Expr.item", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.__getitem__", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.agg", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.concat", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.contains", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.count_matches", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.diff", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.eval", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.explode", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.filter", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.gather", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.gather_every", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.get", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.head", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.item", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.join", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.sample", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.set_difference", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.set_intersection", Reason::NotYetCatalogued),
    LeftOut::new(
        "Expr.list.set_symmetric_difference",
        Reason::NotYetCatalogued,
    ),
    LeftOut::new("Expr.list.set_union", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.shift", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.slice", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.tail", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.to_array", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.to_map", Reason::NotYetCatalogued),
    LeftOut::new("Expr.list.to_struct", Reason::NotYetCatalogued),
    LeftOut::new("Expr.log10", Reason::NotYetCatalogued),
    LeftOut::new("Expr.lower_bound", Reason::NotYetCatalogued),
    LeftOut::new("Expr.map.contains_key", Reason::NotYetCatalogued),
    LeftOut::new("Expr.map.entries", Reason::NotYetCatalogued),
    LeftOut::new("Expr.map.get", Reason::NotYetCatalogued),
    LeftOut::new("Expr.map.keys", Reason::NotYetCatalogued),
    LeftOut::new("Expr.map.len", Reason::NotYetCatalogued),
    LeftOut::new("Expr.map.values", Reason::NotYetCatalogued),
    LeftOut::new("Expr.map_batches", Reason::TakesPythonFunction),
    LeftOut::new("Expr.map_elements", Reason::TakesPythonFunction),
    LeftOut::new("Expr.max_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.__eq__", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.__ne__", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.as_expression", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.as_selector", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.eq", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.has_multiple_outputs", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.is_column", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.is_column_selection", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.is_known_length", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.is_length_preserving", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.is_literal", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.is_regex_projection", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.is_row_separable", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.is_scalar", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.ne", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.output_name", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.pop", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.root_names", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.serialize", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.show_graph", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.tree_format", Reason::NotYetCatalogued),
    LeftOut::new("Expr.meta.undo_aliases", Reason::NotYetCatalogued),
    LeftOut::new("Expr.min_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.name.keep", Reason::NotYetCatalogued),
    LeftOut::new("Expr.name.map", Reason::TakesPythonFunction),
    LeftOut::new("Expr.name.map_fields", Reason::TakesPythonFunction),
    LeftOut::new("Expr.name.prefix", Reason::NotYetCatalogued),
    LeftOut::new("Expr.name.prefix_fields", Reason::NotYetCatalogued),
    LeftOut::new("Expr.name.replace", Reason::NotYetCatalogued),
    LeftOut::new("Expr.name.strip_prefix", Reason::NotYetCatalogued),
    LeftOut::new("Expr.name.strip_suffix", Reason::NotYetCatalogued),
    LeftOut::new("Expr.name.suffix", Reason::NotYetCatalogued),
    LeftOut::new("Expr.name.suffix_fields", Reason::NotYetCatalogued),
    LeftOut::new("Expr.name.to_lowercase", Reason::NotYetCatalogued),
    LeftOut::new("Expr.name.to_uppercase", Reason::NotYetCatalogued),
    LeftOut::new("Expr.peak_max", Reason::NotYetCatalogued),
    LeftOut::new("Expr.peak_min", Reason::NotYetCatalogued),
    LeftOut::new("Expr.pipe", Reason::TakesPythonFunction),
    LeftOut::new("Expr.pipe_with_dtype", Reason::TakesPythonFunction),
    LeftOut::new("Expr.qcut", Reason::Deprecated),
    LeftOut::new("Expr.reinterpret", Reason::NotYetCatalogued),
    LeftOut::new("Expr.repeat_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.reshape", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rle", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_kurtosis", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_map", Reason::TakesPythonFunction),
    LeftOut::new("Expr.rolling_max", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_max_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_mean", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_mean_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_median", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_median_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_min", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_min_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_quantile", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_quantile_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_rank", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_rank_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_skew", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_std", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_std_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_sum", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_sum_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_var", Reason::NotYetCatalogued),
    LeftOut::new("Expr.rolling_var_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.sample", Reason::NotYetCatalogued),
    LeftOut::new("Expr.set_sorted", Reason::NotYetCatalogued),
    LeftOut::new("Expr.shuffle", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.contains_any", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.decode", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.encode", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.extract_many", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.find_many", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.json_decode", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.json_path_match", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.normalize", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.replace_many", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.split_exact", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.splitn", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.strptime", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.to_date", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.to_datetime", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.to_decimal", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.to_integer", Reason::NotYetCatalogued),
    LeftOut::new("Expr.str.to_time", Reason::NotYetCatalogued),
    LeftOut::new("Expr.struct.__getitem__", Reason::NotYetCatalogued),
    LeftOut::new("Expr.struct.drop", Reason::NotYetCatalogued),
    LeftOut::new("Expr.struct.eval", Reason::NotYetCatalogued),
    LeftOut::new("Expr.struct.field", Reason::NotYetCatalogued),
    LeftOut::new("Expr.struct.json_encode", Reason::NotYetCatalogued),
    LeftOut::new("Expr.struct.rename_fields", Reason::NotYetCatalogued),
    LeftOut::new("Expr.struct.unnest", Reason::NotYetCatalogued),
    LeftOut::new("Expr.struct.with_fields", Reason::NotYetCatalogued),
    LeftOut::new("Expr.to_physical", Reason::NotYetCatalogued),
    LeftOut::new("Expr.top_k_by", Reason::NotYetCatalogued),
    LeftOut::new("Expr.truncdiv", Reason::NotYetCatalogued),
    LeftOut::new("Expr.upper_bound", Reason::NotYetCatalogued),
    LeftOut::new("Expr.value_counts", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.__contains__", Reason::NeverOffered),
    LeftOut::new("LazyFrame.__eq__", Reason::NeverOffered),
    LeftOut::new("LazyFrame.__ge__", Reason::NeverOffered),
    LeftOut::new("LazyFrame.__getitem__", Reason::NeverOffered),
    LeftOut::new("LazyFrame.__gt__", Reason::NeverOffered),
    LeftOut::new("LazyFrame.__le__", Reason::NeverOffered),
    LeftOut::new("LazyFrame.__lt__", Reason::NeverOffered),
    LeftOut::new("LazyFrame.__ne__", Reason::NeverOffered),
    LeftOut::new("LazyFrame.bottom_k", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.cache", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.cast", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.clear", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.clone", Reason::NeverOffered),
    LeftOut::new("LazyFrame.collect", Reason::NeverOffered),
    LeftOut::new("LazyFrame.collect_async", Reason::NeverOffered),
    LeftOut::new("LazyFrame.collect_batches", Reason::NeverOffered),
    LeftOut::new("LazyFrame.collect_schema", Reason::NeverOffered),
    LeftOut::new("LazyFrame.columns", Reason::NeverOffered),
    LeftOut::new("LazyFrame.count", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.describe", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.deserialize", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.dtypes", Reason::NeverOffered),
    LeftOut::new("LazyFrame.execute", Reason::NeverOffered),
    LeftOut::new("LazyFrame.explain", Reason::NeverOffered),
    LeftOut::new("LazyFrame.explode", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.fill_null", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.from_lazyframe_resolver", Reason::PythonObject),
    LeftOut::new("LazyFrame.gather", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.group_by", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.group_by_dynamic", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.inspect", Reason::NeverOffered),
    LeftOut::new("LazyFrame.interpolate", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.join_asof", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.join_where", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.lazy", Reason::NeverOffered),
    LeftOut::new("LazyFrame.map_batches", Reason::TakesPythonFunction),
    LeftOut::new("LazyFrame.match_to_schema", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.max", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.mean", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.median", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.merge_sorted", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.min", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.null_count", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.pipe", Reason::TakesPythonFunction),
    LeftOut::new("LazyFrame.pipe_with_schema", Reason::TakesPythonFunction),
    LeftOut::new("LazyFrame.quantile", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.remote", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.remove", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.rolling", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.schema", Reason::NeverOffered),
    LeftOut::new("LazyFrame.select_seq", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.serialize", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.set_sorted", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.shift", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.show", Reason::NeverOffered),
    LeftOut::new("LazyFrame.show_graph", Reason::NeverOffered),
    LeftOut::new("LazyFrame.sink_batches", Reason::TakesPythonFunction),
    LeftOut::new("LazyFrame.sink_csv", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.sink_delta", Reason::PythonObject),
    LeftOut::new("LazyFrame.sink_iceberg", Reason::PythonObject),
    LeftOut::new("LazyFrame.sink_ipc", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.sink_ndjson", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.sink_parquet", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.sql", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.std", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.sum", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.top_k", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.unique", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.unnest", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.unpivot", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.update", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.var", Reason::NotYetCatalogued),
    LeftOut::new("LazyFrame.width", Reason::NeverOffered),
    LeftOut::new("LazyFrame.with_columns_seq", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.agg", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.all", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.first", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.having", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.head", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.last", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.len", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.map_groups", Reason::TakesPythonFunction),
    LeftOut::new("LazyGroupBy.max", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.mean", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.median", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.min", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.n_unique", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.quantile", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.sum", Reason::NotYetCatalogued),
    LeftOut::new("LazyGroupBy.tail", Reason::NotYetCatalogued),
    LeftOut::new("polars.align_frames", Reason::NotYetCatalogued),
    LeftOut::new("polars.all", Reason::NotYetCatalogued),
    LeftOut::new("polars.all_horizontal", Reason::NotYetCatalogued),
    LeftOut::new("polars.any", Reason::NotYetCatalogued),
    LeftOut::new("polars.any_horizontal", Reason::NotYetCatalogued),
    LeftOut::new("polars.approx_n_unique", Reason::NotYetCatalogued),
    LeftOut::new("polars.arange", Reason::NotYetCatalogued),
    LeftOut::new("polars.arctan2", Reason::NotYetCatalogued),
    LeftOut::new("polars.arg_sort_by", Reason::NotYetCatalogued),
    LeftOut::new("polars.arg_where", Reason::NotYetCatalogued),
    LeftOut::new("polars.business_day_count", Reason::NotYetCatalogued),
    LeftOut::new("polars.coalesce", Reason::NotYetCatalogued),
    LeftOut::new("polars.collect_all", Reason::NeverOffered),
    LeftOut::new("polars.collect_all_async", Reason::NeverOffered),
    LeftOut::new("polars.concat", Reason::NotYetCatalogued),
    LeftOut::new("polars.concat_arr", Reason::NotYetCatalogued),
    LeftOut::new("polars.concat_list", Reason::NotYetCatalogued),
    LeftOut::new("polars.concat_str", Reason::NotYetCatalogued),
    LeftOut::new("polars.corr", Reason::NotYetCatalogued),
    LeftOut::new("polars.count", Reason::NotYetCatalogued),
    LeftOut::new("polars.cov", Reason::NotYetCatalogued),
    LeftOut::new("polars.cum_count", Reason::NotYetCatalogued),
    LeftOut::new("polars.cum_fold", Reason::TakesPythonFunction),
    LeftOut::new("polars.cum_reduce", Reason::TakesPythonFunction),
    LeftOut::new("polars.cum_sum", Reason::NotYetCatalogued),
    LeftOut::new("polars.cum_sum_horizontal", Reason::NotYetCatalogued),
    LeftOut::new("polars.date_ranges", Reason::NotYetCatalogued),
    LeftOut::new("polars.datetime_ranges", Reason::NotYetCatalogued),
    LeftOut::new("polars.dtype_of", Reason::NotYetCatalogued),
    LeftOut::new("polars.escape_regex", Reason::NeverOffered),
    LeftOut::new("polars.exclude", Reason::NotYetCatalogued),
    LeftOut::new("polars.explain_all", Reason::NeverOffered),
    LeftOut::new("polars.field", Reason::NotYetCatalogued),
    LeftOut::new("polars.first", Reason::NotYetCatalogued),
    LeftOut::new("polars.fold", Reason::TakesPythonFunction),
    LeftOut::new("polars.format", Reason::NotYetCatalogued),
    LeftOut::new("polars.head", Reason::NotYetCatalogued),
    LeftOut::new("polars.implode", Reason::NotYetCatalogued),
    LeftOut::new("polars.int_ranges", Reason::NotYetCatalogued),
    LeftOut::new("polars.last", Reason::NotYetCatalogued),
    LeftOut::new("polars.len", Reason::NotYetCatalogued),
    LeftOut::new("polars.linear_spaces", Reason::NotYetCatalogued),
    LeftOut::new("polars.list", Reason::NotYetCatalogued),
    LeftOut::new("polars.map_batches", Reason::TakesPythonFunction),
    LeftOut::new("polars.map_groups", Reason::TakesPythonFunction),
    LeftOut::new("polars.max", Reason::NotYetCatalogued),
    LeftOut::new("polars.max_horizontal", Reason::NotYetCatalogued),
    LeftOut::new("polars.mean", Reason::NotYetCatalogued),
    LeftOut::new("polars.mean_horizontal", Reason::NotYetCatalogued),
    LeftOut::new("polars.median", Reason::NotYetCatalogued),
    LeftOut::new("polars.merge_sorted", Reason::NotYetCatalogued),
    LeftOut::new("polars.min", Reason::NotYetCatalogued),
    LeftOut::new("polars.min_horizontal", Reason::NotYetCatalogued),
    LeftOut::new("polars.n_unique", Reason::NotYetCatalogued),
    LeftOut::new("polars.nth", Reason::NotYetCatalogued),
    LeftOut::new("polars.ones", Reason::NotYetCatalogued),
    LeftOut::new("polars.pipe_with_dtype", Reason::TakesPythonFunction),
    LeftOut::new("polars.quantile", Reason::NotYetCatalogued),
    LeftOut::new("polars.reduce", Reason::TakesPythonFunction),
    LeftOut::new("polars.rolling_corr", Reason::NotYetCatalogued),
    LeftOut::new("polars.rolling_cov", Reason::NotYetCatalogued),
    LeftOut::new("polars.row_index", Reason::NotYetCatalogued),
    LeftOut::new("polars.scan_arrow_c_stream", Reason::PythonObject),
    LeftOut::new("polars.scan_csv", Reason::TakesPythonFunction),
    LeftOut::new("polars.scan_delta", Reason::PythonObject),
    LeftOut::new("polars.scan_external_reader", Reason::PythonObject),
    LeftOut::new("polars.scan_iceberg", Reason::PythonObject),
    LeftOut::new("polars.scan_ipc", Reason::NotYetCatalogued),
    LeftOut::new("polars.scan_lance", Reason::PythonObject),
    LeftOut::new("polars.scan_lines", Reason::NotYetCatalogued),
    LeftOut::new("polars.scan_ndjson", Reason::NotYetCatalogued),
    LeftOut::new("polars.scan_parquet", Reason::NotYetCatalogued),
    LeftOut::new("polars.scan_pyarrow_dataset", Reason::PythonObject),
    LeftOut::new("polars.select", Reason::NotYetCatalogued),
    LeftOut::new("polars.self_dtype", Reason::NotYetCatalogued),
    LeftOut::new("polars.set_random_seed", Reason::NotYetCatalogued),
    LeftOut::new("polars.sql_expr", Reason::NotYetCatalogued),
    LeftOut::new("polars.std", Reason::NotYetCatalogued),
    LeftOut::new("polars.struct", Reason::NotYetCatalogued),
    LeftOut::new("polars.struct_with_fields", Reason::NotYetCatalogued),
    LeftOut::new("polars.sum", Reason::NotYetCatalogued),
    LeftOut::new("polars.sum_horizontal", Reason::NotYetCatalogued),
    LeftOut::new("polars.tail", Reason::NotYetCatalogued),
    LeftOut::new("polars.time_ranges", Reason::NotYetCatalogued),
    LeftOut::new("polars.union", Reason::NotYetCatalogued),
    LeftOut::new("polars.var", Reason::NotYetCatalogued),
    LeftOut::new("polars.zeros", Reason::NotYetCatalogued),
];

//! The structural check: every call in the source names a Python item that
//! `api/polars/python.txt` traces, gives it only parameters it has, binds each
//! parameter once, leaves none that is required unbound, and gives each an
//! argument of the kind the catalogue says it takes.
//!
//! The Python is read with a small parser of the forms the source uses: names,
//! calls, attributes, strings, numbers, lists and dictionaries. The inventory
//! is the authority for the items and their signatures; the catalogue, which a
//! test of its own holds to the inventory, for the kind of each argument.

use std::collections::BTreeMap;

use dageist_engine::catalog::{Kind, Shape, ValueType, entry};

const INVENTORY: &str = include_str!("../../../../../api/polars/python.txt");

// ---------------------------------------------------------------------------
// The inventory

/// One item's signature, as the inventory writes it.
struct Signature {
    /// The parameters Python takes by position, in order.
    positional: Vec<String>,
    /// The starred parameter that takes the rest of the positional arguments.
    star: Option<String>,
    /// The parameters Python takes by name only.
    by_name: Vec<String>,
    /// The double-starred parameter.
    double_star: Option<String>,
    /// The parameters that must be given.
    required: Vec<String>,
}

fn split_parameters(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut depth, mut quote, mut start) = (0_i32, None::<char>, 0);
    for (at, character) in text.char_indices() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '(' | '[' | '{') => depth += 1,
            (None, ')' | ']' | '}') => depth -= 1,
            (None, ',') if depth == 0 => {
                parts.push(text[start..at].trim().to_owned());
                start = at + 1;
            }
            _ => {}
        }
    }
    let last = text[start..].trim();
    if !last.is_empty() {
        parts.push(last.to_owned());
    }
    parts
}

fn inventory() -> BTreeMap<String, Signature> {
    let mut items = BTreeMap::new();
    for line in INVENTORY
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
    {
        let signature = line.split(" | ").next().expect("a signature");
        let Some(open) = signature.find('(') else {
            continue;
        };
        let close = signature.rfind(')').expect("a signature closes");
        let mut found = Signature {
            positional: Vec::new(),
            star: None,
            by_name: Vec::new(),
            double_star: None,
            required: Vec::new(),
        };
        let mut by_name = false;
        for parameter in split_parameters(&signature[open + 1..close]) {
            if parameter == "*" {
                by_name = true;
            } else if let Some(name) = parameter.strip_prefix("**") {
                found.double_star = Some(name.to_owned());
            } else if let Some(name) = parameter.strip_prefix('*') {
                found.star = Some(name.to_owned());
                by_name = true;
            } else if parameter == "/" {
            } else {
                let (name, default) = match parameter.split_once('=') {
                    Some((name, _)) => (name.to_owned(), true),
                    None => (parameter.clone(), false),
                };
                if !default {
                    found.required.push(name.clone());
                }
                if by_name {
                    found.by_name.push(name);
                } else {
                    found.positional.push(name);
                }
            }
        }
        items.insert(signature[..open].to_owned(), found);
    }
    items
}

// ---------------------------------------------------------------------------
// The Python

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Name(String),
    Text(String),
    Number(String),
    Punct(&'static str),
}

fn tokens(source: &str) -> Result<Vec<Token>, String> {
    let characters: Vec<char> = source.chars().collect();
    let mut found = Vec::new();
    let mut at = 0;
    while at < characters.len() {
        let character = characters[at];
        if character.is_whitespace() {
            at += 1;
        } else if character.is_alphabetic() || character == '_' {
            let end = characters[at..]
                .iter()
                .position(|c| !(c.is_alphanumeric() || *c == '_'))
                .map_or(characters.len(), |length| at + length);
            found.push(Token::Name(characters[at..end].iter().collect()));
            at = end;
        } else if character.is_ascii_digit()
            || (character == '-' && characters.get(at + 1).is_some_and(char::is_ascii_digit))
        {
            let mut end = at + 1;
            while end < characters.len()
                && (characters[end].is_ascii_alphanumeric()
                    || characters[end] == '.'
                    || (matches!(characters[end], '+' | '-')
                        && matches!(characters[end - 1], 'e' | 'E')))
            {
                end += 1;
            }
            found.push(Token::Number(characters[at..end].iter().collect()));
            at = end;
        } else if character == '"' || character == '\'' {
            let mut text = String::new();
            at += 1;
            loop {
                match characters.get(at) {
                    None => return Err("a string is not closed".to_owned()),
                    Some(close) if *close == character => break,
                    Some('\\') => {
                        text.push(characters[at + 1]);
                        at += 2;
                    }
                    Some(other) => {
                        text.push(*other);
                        at += 1;
                    }
                }
            }
            at += 1;
            found.push(Token::Text(text));
        } else if character == '*' && characters.get(at + 1) == Some(&'*') {
            found.push(Token::Punct("**"));
            at += 2;
        } else {
            let punct = match character {
                '(' => "(",
                ')' => ")",
                '[' => "[",
                ']' => "]",
                '{' => "{",
                '}' => "}",
                ',' => ",",
                ':' => ":",
                '=' => "=",
                '.' => ".",
                other => return Err(format!("unexpected {other:?}")),
            };
            found.push(Token::Punct(punct));
            at += 1;
        }
    }
    Ok(found)
}

/// What a value is, as far as Python's classes matter here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    LazyFrame,
    DataFrame,
    Expr,
    When,
    Then,
    ChainedWhen,
    ChainedThen,
}

impl Class {
    const fn name(self) -> &'static str {
        match self {
            Self::LazyFrame => "LazyFrame",
            Self::DataFrame => "DataFrame",
            Self::Expr => "Expr",
            Self::When => "When",
            Self::Then => "Then",
            Self::ChainedWhen => "ChainedWhen",
            Self::ChainedThen => "ChainedThen",
        }
    }

    /// The class a call of the item `id` gives.
    fn after(self, item: &str) -> Self {
        match item {
            "polars.when" => Self::When,
            "When.then" | "ChainedWhen.then" => {
                if self == Self::When {
                    Self::Then
                } else {
                    Self::ChainedThen
                }
            }
            "Then.when" | "ChainedThen.when" => Self::ChainedWhen,
            "LazyFrame.collect" => Self::DataFrame,
            _ => match self {
                Self::LazyFrame => Self::LazyFrame,
                _ => Self::Expr,
            },
        }
    }
}

/// A piece of Python.
#[derive(Debug, Clone)]
enum Node {
    Text(String),
    Number(String),
    /// `None`, `True` or `False`.
    Constant(String),
    /// A name bound by the function's parameters.
    Name(String),
    /// `pl.Int64`: an attribute of `pl`, not called.
    Attribute(String),
    List(Vec<Node>),
    Dict(Vec<(Node, Node)>),
    Call(Box<Called>),
}

#[derive(Debug, Clone)]
struct Argument {
    /// The name it is given by, or `None` when it is by position.
    name: Option<String>,
    /// Whether it is `**{...}`.
    unpacked: bool,
    value: Node,
}

#[derive(Debug, Clone)]
struct Called {
    /// The item's identifier in the inventory.
    item: String,
    /// What the call gives.
    gives: Class,
    arguments: Vec<Argument>,
    /// The calls before it in its chain, which it is called on.
    before: Vec<Called>,
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    /// What the name the chain starts from is, by its name.
    start: Class,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn eat(&mut self, punct: &'static str) -> bool {
        if self.peek() == Some(&Token::Punct(punct)) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, punct: &'static str) -> Result<(), String> {
        if self.eat(punct) {
            Ok(())
        } else {
            Err(format!("expected {punct:?}, found {:?}", self.peek()))
        }
    }

    /// A value: a literal, a list, a dictionary, or a chain of calls.
    fn value(&mut self) -> Result<Node, String> {
        match self.peek().cloned() {
            Some(Token::Text(text)) => {
                self.at += 1;
                Ok(Node::Text(text))
            }
            Some(Token::Number(number)) => {
                self.at += 1;
                Ok(Node::Number(number))
            }
            Some(Token::Punct("[")) => {
                self.at += 1;
                let mut items = Vec::new();
                while !self.eat("]") {
                    items.push(self.value()?);
                    if !self.eat(",") {
                        self.expect("]")?;
                        break;
                    }
                }
                Ok(Node::List(items))
            }
            Some(Token::Punct("{")) => {
                self.at += 1;
                let mut entries = Vec::new();
                while !self.eat("}") {
                    let key = self.value()?;
                    self.expect(":")?;
                    entries.push((key, self.value()?));
                    if !self.eat(",") {
                        self.expect("}")?;
                        break;
                    }
                }
                Ok(Node::Dict(entries))
            }
            Some(Token::Name(name)) => {
                self.at += 1;
                if matches!(name.as_str(), "None" | "True" | "False") {
                    return Ok(Node::Constant(name));
                }
                self.chain(name)
            }
            other => Err(format!("unexpected {other:?}")),
        }
    }

    /// A chain that starts with the name `first`: `pl.col("a").shift(1)`.
    fn chain(&mut self, first: String) -> Result<Node, String> {
        let mut class = if first == "pl" {
            None
        } else {
            Some(self.start)
        };
        let mut before: Vec<Called> = Vec::new();
        let mut path: Vec<String> = Vec::new();
        let mut bare = first == "pl";
        while self.eat(".") {
            let Some(Token::Name(name)) = self.peek().cloned() else {
                return Err("a name is expected after a dot".to_owned());
            };
            self.at += 1;
            path.push(name);
            if self.eat("(") {
                let arguments = self.arguments()?;
                let dotted = path.join(".");
                let item = match class {
                    None => format!("polars.{dotted}"),
                    Some(class) => format!("{}.{dotted}", class.name()),
                };
                let gives = class.unwrap_or(Class::Expr).after(&item);
                before.push(Called {
                    item,
                    gives,
                    arguments,
                    before: before.clone(),
                });
                class = Some(gives);
                path.clear();
                bare = false;
            }
        }
        if bare || !path.is_empty() {
            let attribute = format!("{first}.{}", path.join("."));
            return Ok(if first == "pl" {
                Node::Attribute(attribute)
            } else {
                Node::Name(attribute)
            });
        }
        match before.pop() {
            Some(last) => Ok(Node::Call(Box::new(last))),
            None => Ok(Node::Name(first)),
        }
    }

    /// The arguments of a call, after its opening bracket.
    fn arguments(&mut self) -> Result<Vec<Argument>, String> {
        let mut arguments = Vec::new();
        while !self.eat(")") {
            let argument = if self.eat("**") {
                Argument {
                    name: None,
                    unpacked: true,
                    value: self.value()?,
                }
            } else if let (Some(Token::Name(name)), Some(Token::Punct("="))) =
                (self.peek().cloned(), self.tokens.get(self.at + 1))
            {
                self.at += 2;
                Argument {
                    name: Some(name),
                    unpacked: false,
                    value: self.value()?,
                }
            } else {
                Argument {
                    name: None,
                    unpacked: false,
                    value: self.value()?,
                }
            };
            arguments.push(argument);
            if !self.eat(",") {
                self.expect(")")?;
                break;
            }
        }
        Ok(arguments)
    }
}

/// Reads one chain of calls that starts from the name `start` (or `pl`).
fn parse(source: &str, start: Class) -> Result<Node, String> {
    let mut parser = Parser {
        tokens: tokens(source)?,
        at: 0,
        start,
    };
    // A step line starts with a dot: it is called on a lazy frame.
    let node = if parser.peek() == Some(&Token::Punct(".")) {
        parser.chain("frame".to_owned())?
    } else {
        parser.value()?
    };
    if parser.at != parser.tokens.len() {
        return Err(format!("{:?} is left over", &parser.tokens[parser.at..]));
    }
    Ok(node)
}

// ---------------------------------------------------------------------------
// The checks

type Items = BTreeMap<String, Signature>;

/// What a parameter of this kind and shape takes of the argument `node`, or why
/// it does not.
fn takes(
    name: &str,
    (kind, shape): (Kind, Shape),
    node: &Node,
    items: &Items,
    inside_list: bool,
) -> Result<(), String> {
    // A parameter that holds a list takes the list or, as Python does, one item.
    if let (Node::List(list), true, false) = (
        node,
        matches!(shape, Shape::Many | Shape::PerItem(_)),
        inside_list,
    ) {
        return list
            .iter()
            .try_for_each(|item| takes(name, (kind, shape), item, items, true));
    }
    let wrong = |expected: &str| Err(format!("{name} takes {expected}, not {node:?}"));
    match (kind, node) {
        (Kind::Expression(_), Node::Call(called)) => {
            if matches!(called.gives, Class::Expr) {
                check(called, items)
            } else {
                wrong("an expression")
            }
        }
        // A field's name stands for the field where an expression is one of a list of keys.
        (Kind::Expression(_), Node::Text(_)) if matches!(shape, Shape::Many) => Ok(()),
        (Kind::Flag, Node::Constant(word))
            if matches!(word.as_str(), "True" | "False" | "None") =>
        {
            Ok(())
        }
        (Kind::Choice(words), Node::Text(word)) => {
            if words.contains(&word.as_str()) {
                Ok(())
            } else {
                Err(format!("{word:?} is not a choice of {name}"))
            }
        }
        (Kind::Value(ValueType::Integer), Node::Number(number))
            if number.parse::<i64>().is_ok() =>
        {
            Ok(())
        }
        (Kind::Value(ValueType::Float), Node::Number(number)) if number.parse::<f64>().is_ok() => {
            Ok(())
        }
        (
            Kind::Value(ValueType::String | ValueType::Date)
            | Kind::Duration
            | Kind::Path
            | Kind::Column,
            Node::Text(_),
        )
        | (
            Kind::Value(ValueType::Any),
            // A name is a value or a list the function is given.
            Node::Number(_) | Node::Text(_) | Node::Constant(_) | Node::Name(_),
        ) => Ok(()),
        (Kind::Column, Node::List(names))
            if names.iter().all(|name| matches!(name, Node::Text(_))) =>
        {
            Ok(())
        }
        (Kind::Dtype, Node::Attribute(attribute)) if DTYPES.contains(&attribute.as_str()) => Ok(()),
        (Kind::Dtype, Node::Call(called))
            if DTYPES.contains(
                &format!("pl.{}", called.item.trim_start_matches("polars.")).as_str(),
            ) =>
        {
            Ok(())
        }
        // A table is a name the function is given, or collected.
        (Kind::Table, Node::Name(_)) => Ok(()),
        (Kind::Table, Node::Call(called)) if called.item == "LazyFrame.collect" => Ok(()),
        (Kind::Pairs(key, value), Node::Dict(entries)) => {
            entries.iter().try_for_each(|(first, second)| {
                takes(name, (*key, Shape::One), first, items, true)?;
                takes(name, (*value, Shape::One), second, items, true)
            })
        }
        _ => wrong(&format!("{kind:?}")),
    }
}

/// The dtypes a document names, as attributes of `pl`.
const DTYPES: [&str; 20] = [
    "pl.Int8",
    "pl.Int16",
    "pl.Int32",
    "pl.Int64",
    "pl.UInt8",
    "pl.UInt16",
    "pl.UInt32",
    "pl.UInt64",
    "pl.Float32",
    "pl.Float64",
    "pl.String",
    "pl.Boolean",
    "pl.Date",
    "pl.Time",
    "pl.Null",
    "pl.Datetime",
    "pl.Duration",
    "pl.List",
    "pl.Int128",
    "pl.UInt128",
];

/// The names a piece of Python uses, which the function must be given.
fn names_in(node: &Node, found: &mut Vec<String>) {
    match node {
        Node::Name(name) => found.push(name.split('.').next().unwrap_or(name).to_owned()),
        Node::List(items) => items.iter().for_each(|item| names_in(item, found)),
        Node::Dict(entries) => entries.iter().for_each(|(key, value)| {
            names_in(key, found);
            names_in(value, found);
        }),
        Node::Call(called) => {
            for each in called.before.iter().chain(std::iter::once(&**called)) {
                each.arguments
                    .iter()
                    .for_each(|argument| names_in(&argument.value, found));
            }
        }
        Node::Text(_) | Node::Number(_) | Node::Constant(_) | Node::Attribute(_) => {}
    }
}

/// Checks one call and the calls before it in its chain.
fn check(called: &Called, items: &Items) -> Result<(), String> {
    for earlier in &called.before {
        check(earlier, items)?;
    }
    let signature = items
        .get(&called.item)
        .ok_or_else(|| format!("{} is not an item of Python Polars", called.item))?;
    let entry = entry(&called.item);
    let mut bound: Vec<String> = Vec::new();
    let mut by_position = 0;
    let parameter_of = |name: &str| {
        entry.and_then(|entry| {
            entry
                .parameters()
                .iter()
                .find(|parameter| parameter.name() == name)
        })
    };
    let kind_check = |name: &str, node: &Node| match parameter_of(name) {
        // An item the catalogue does not hold has no kinds to check against.
        None if entry.is_none() => Ok(()),
        None => Err(format!(
            "the catalogue has no parameter {name} of {}",
            called.item
        )),
        Some(parameter) => takes(
            name,
            (parameter.kind(), parameter.shape()),
            node,
            items,
            false,
        ),
    };
    for argument in &called.arguments {
        if argument.unpacked {
            let Some(name) = &signature.double_star else {
                return Err(format!("{} takes no **", called.item));
            };
            kind_check(name, &argument.value)?;
            continue;
        }
        let name = match &argument.name {
            Some(name) => {
                let known = signature.positional.contains(name) || signature.by_name.contains(name);
                if !known {
                    return Err(format!("{} has no parameter {name}", called.item));
                }
                name.clone()
            }
            None => {
                let name = signature
                    .positional
                    .get(by_position)
                    .or(signature.star.as_ref())
                    .ok_or_else(|| {
                        format!("{} takes no further argument by position", called.item)
                    })?
                    .clone();
                by_position += 1;
                name
            }
        };
        let star = signature.star.as_deref() == Some(name.as_str());
        if !star && bound.contains(&name) {
            return Err(format!("{} gives {name} twice", called.item));
        }
        kind_check(&name, &argument.value)?;
        bound.push(name);
    }
    for required in &signature.required {
        if !bound.contains(required) {
            return Err(format!("{} lacks {required}", called.item));
        }
    }
    Ok(())
}

/// Checks the source of a step: a method call on a lazy frame.
pub fn check_step(source: &str) -> Result<(), String> {
    let items = inventory();
    match parse(source, Class::LazyFrame)? {
        Node::Call(called) => check(&called, &items),
        other => Err(format!("a step is a call, not {other:?}")),
    }
}

/// Checks every step of a module the engine wrote.
pub fn check_module(module: &dageist_engine::document::Source) -> Result<(), String> {
    // The names the function is given: its parameters, one to a line.
    let lines: Vec<&str> = module.lines().iter().map(|line| line.text()).collect();
    let open = lines
        .iter()
        .position(|line| *line == "def run(")
        .ok_or("the module has no function run")?;
    let given: Vec<String> = lines[open + 1..]
        .iter()
        .take_while(|line| !line.starts_with(')'))
        .map(|line| {
            line.trim()
                .trim_end_matches(',')
                .split(':')
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    for line in module
        .lines()
        .iter()
        .filter(|line| line.step().is_some() && !line.text().trim().starts_with('#'))
    {
        let text = line.text().trim();
        check_step(text).map_err(|why| format!("{text}: {why}"))?;
        let Node::Call(called) = parse(text, Class::LazyFrame)? else {
            return Err(format!("{text} is not a call"));
        };
        let mut used = Vec::new();
        names_in(&Node::Call(called), &mut used);
        if let Some(unbound) = used.iter().find(|name| !given.contains(name)) {
            return Err(format!("{text}: {unbound} is not a parameter of run"));
        }
    }
    Ok(())
}

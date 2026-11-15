//! Writes the audit of the Python API of the upstream release: each public
//! Python method, property, operator and function, the binding calls it
//! makes, and the public Rust items those reach.
//!
//! The Python package is read with Python's own parser by
//! `python/inventory.py`, so no Python package needs installing. The binding
//! is read with `syn`. A binding body's calls are followed by the Rust type
//! of the value they are made on: the type the binding object wraps, then the
//! return type the Rust audit gives each method or function it calls. A call
//! counts only when that type has the method, so a call on an `Option`, or a
//! method of the same name on another type, is not mistaken for it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use syn::visit::{self, Visit};

/// What the Python audit reads and where it writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythonAudit<'a> {
    /// The Python package, relative to the workspace root.
    pub package: &'a str,
    /// The source of the Python binding, relative to the workspace root.
    pub binding: &'a str,
    /// The dependency audits the Rust API is read from, relative to the workspace root.
    pub rust_audits: &'a [&'a str],
}

/// One public Python item, as `python/inventory.py` reads it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PythonItem {
    /// The class it belongs to, such as `Expr.str`, or `polars` for a function.
    pub owner: String,
    pub name: String,
    /// `method`, `property`, `operator` or `function`.
    pub kind: String,
    /// Its parameters as written, with their defaults.
    pub parameters: Vec<String>,
    pub deprecated: bool,
    /// The binding calls it makes: the binding type, or `module`, and the name.
    pub binding: Vec<(String, String)>,
    /// For an item that makes no binding call, the Polars calls it is built from.
    pub uses: Vec<String>,
}

/// The public Rust API: each type's methods and each free function, with the
/// type each returns when the audit names one the API has.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RustApi {
    pub methods: BTreeMap<String, BTreeMap<String, Option<String>>>,
    pub functions: BTreeMap<String, Option<String>>,
}

/// Returns the index of the parenthesis that closes the one at `open`.
fn closing(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, character) in text.char_indices().skip_while(|(at, _)| *at < open) {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// Returns the type a return type names, looking through `Result`, `Option`
/// and `Box`: `PolarsResult<polars_lazy::frame::LazyFrame>` names `LazyFrame`.
fn named_type(text: &str, owner: Option<&str>) -> Option<String> {
    let mut text = text
        .trim()
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim();
    loop {
        let (head, generics) = match text.find('<') {
            Some(at) => (&text[..at], Some(&text[at + 1..text.rfind('>')?])),
            None => (text, None),
        };
        let name = head.rsplit("::").next()?.trim();
        match (name, generics) {
            ("Result" | "PolarsResult" | "Option" | "Box", Some(inner)) => {
                let mut depth = 0usize;
                let end = inner
                    .char_indices()
                    .find(|(_, character)| match character {
                        '<' => {
                            depth += 1;
                            false
                        }
                        '>' => {
                            depth = depth.saturating_sub(1);
                            false
                        }
                        ',' => depth == 0,
                        _ => false,
                    })
                    .map_or(inner.len(), |(at, _)| at);
                text = inner[..end].trim();
            }
            ("Self", _) => return owner.map(str::to_owned),
            _ => {
                return name
                    .chars()
                    .all(|character| character.is_alphanumeric() || character == '_')
                    .then(|| name.to_owned());
            }
        }
    }
}

/// Splits an audit line such as `pub fn polars_plan::dsl::Expr::abs(self) -> Self`
/// into its path and its return type, as written.
fn split_signature(rest: &str) -> Option<(&str, Option<&str>)> {
    // The parameters open at the first parenthesis outside the generics.
    let mut depth = 0usize;
    let mut open = None;
    let bytes = rest.as_bytes();
    for (at, character) in rest.char_indices() {
        match character {
            '<' => depth += 1,
            '>' if at > 0 && bytes[at - 1] != b'-' => depth = depth.saturating_sub(1),
            '(' if depth == 0 => {
                open = Some(at);
                break;
            }
            _ => {}
        }
    }
    let open = open?;
    let path = rest[..open].split('<').next()?;
    let after = rest[closing(rest, open)? + 1..].trim_start();
    let returned = after.strip_prefix("->").map(|returned| {
        let returned = returned.trim();
        returned.split(" where ").next().unwrap_or(returned)
    });
    Some((path, returned))
}

impl RustApi {
    /// Reads the public functions of an audit.
    pub fn read(&mut self, audit: &str) {
        for line in audit.lines() {
            let Some(rest) = [
                "pub fn ",
                "pub const fn ",
                "pub async fn ",
                "pub unsafe fn ",
            ]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix)) else {
                continue;
            };
            let Some((path, returned)) = split_signature(rest) else {
                continue;
            };
            let segments = path.split("::").collect::<Vec<_>>();
            let [.., parent, name] = segments.as_slice() else {
                continue;
            };
            if parent.starts_with(|character: char| character.is_ascii_uppercase()) {
                let returned = returned.and_then(|returned| named_type(returned, Some(parent)));
                self.methods
                    .entry((*parent).to_owned())
                    .or_default()
                    .insert((*name).to_owned(), returned);
            } else {
                let returned = returned.and_then(|returned| named_type(returned, None));
                self.functions.insert((*name).to_owned(), returned);
            }
        }
    }

    /// Returns what `owner::method` returns, if the API has it: the type, when that is a type of the API.
    fn method(&self, owner: &str, method: &str) -> Option<Option<&str>> {
        let returned = self.methods.get(owner)?.get(method)?;
        Some(
            returned
                .as_deref()
                .filter(|returned| self.methods.contains_key(*returned)),
        )
    }

    fn function(&self, name: &str) -> Option<Option<&str>> {
        let returned = self.functions.get(name)?;
        Some(
            returned
                .as_deref()
                .filter(|returned| self.methods.contains_key(*returned)),
        )
    }
}

/// What a binding method or function reaches: public Rust items, written as
/// `Type::method` or `function()`, and whether it builds Polars' internals itself.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Reached {
    pub items: BTreeSet<String>,
    pub internals: bool,
}

/// Methods that hand on the value they are called on, such as cloning it.
const PASSING_ON: &[&str] = &[
    "clone", "read", "write", "into", "to_owned", "as_ref", "borrow", "unwrap", "expect", "take",
];

/// The fields of a binding object that hold the Polars value it wraps, and the
/// type that value has when the field is not on `self`.
const WRAPPED: &[(&str, &str)] = &[
    ("inner", "Expr"),
    ("ldf", "LazyFrame"),
    ("lgb", "LazyGroupBy"),
];

/// Public fields of Polars types the binding reads, and the type each holds.
const FIELDS: &[(&str, &str, &str)] = &[("LazyFrame", "logical_plan", "DslPlan")];

/// Follows one binding body.
struct Trace<'a> {
    api: &'a RustApi,
    /// The Rust type the binding object's own wrapped field holds, such as `Expr` for `PyExpr`.
    own: Option<String>,
    /// The other methods of the same binding type, which a body may call on `self`.
    siblings: &'a HashMap<String, &'a syn::Block>,
    /// The sibling methods already followed, so that recursion ends.
    followed: BTreeSet<String>,
    locals: HashMap<String, Option<String>>,
    reached: Reached,
}

impl Trace<'_> {
    /// Returns the Rust type of the API that `expr` evaluates to, if it is one.
    fn type_of(&self, expr: &syn::Expr) -> Option<String> {
        match expr {
            syn::Expr::Field(field) => {
                let syn::Member::Named(name) = &field.member else {
                    return None;
                };
                if let Some((_, _, held)) = FIELDS.iter().find(|(owner, field_name, _)| {
                    name == field_name && self.type_of(&field.base).as_deref() == Some(*owner)
                }) {
                    return self
                        .api
                        .methods
                        .contains_key(*held)
                        .then(|| (*held).to_owned());
                }
                let (_, other) = WRAPPED.iter().find(|(wrapped, _)| name == wrapped)?;
                let on_self = matches!(field.base.as_ref(), syn::Expr::Path(path) if path.path.is_ident("self"))
                    || matches!(field.base.as_ref(), syn::Expr::MethodCall(call)
                        if matches!(call.receiver.as_ref(), syn::Expr::Path(path) if path.path.is_ident("self")));
                if on_self {
                    self.own.clone()
                } else {
                    Some((*other).to_owned())
                }
            }
            syn::Expr::Path(path) => path
                .path
                .get_ident()
                .and_then(|ident| self.locals.get(&ident.to_string()).cloned().flatten()),
            syn::Expr::Call(call) => match call.func.as_ref() {
                syn::Expr::Path(path) => match self.associated(&path.path) {
                    Some((owner, function)) => self
                        .api
                        .method(&owner, &function)
                        .flatten()
                        .map(str::to_owned),
                    None => self
                        .free_function(&path.path)
                        .and_then(|name| self.api.function(&name).flatten().map(str::to_owned)),
                },
                _ => None,
            },
            syn::Expr::MethodCall(call) => {
                let owner = self.type_of(&call.receiver)?;
                let method = call.method.to_string();
                if PASSING_ON.contains(&method.as_str()) {
                    return Some(owner);
                }
                self.api
                    .method(&owner, &method)
                    .flatten()
                    .map(str::to_owned)
            }
            syn::Expr::Paren(inner) => self.type_of(&inner.expr),
            syn::Expr::Group(inner) => self.type_of(&inner.expr),
            syn::Expr::Reference(inner) => self.type_of(&inner.expr),
            syn::Expr::Try(inner) => self.type_of(&inner.expr),
            syn::Expr::Unary(inner) => self.type_of(&inner.expr),
            syn::Expr::Cast(inner) => self.type_of(&inner.expr),
            _ => None,
        }
    }

    /// Returns the type and function when `path` names an associated function of
    /// the Rust API, such as `LazyCsvReader::new_with_sources`.
    fn associated(&self, path: &syn::Path) -> Option<(String, String)> {
        let segments = path.segments.iter().collect::<Vec<_>>();
        let [.., owner, function] = segments.as_slice() else {
            return None;
        };
        let (owner, function) = (owner.ident.to_string(), function.ident.to_string());
        self.api
            .method(&owner, &function)
            .map(|_| (owner, function))
    }

    /// Returns the name of a free function of the Rust API that `path` calls. A path
    /// through a type, such as `DataFrame::empty`, is that type's function, not a free one.
    fn free_function(&self, path: &syn::Path) -> Option<String> {
        let segments = path.segments.iter().collect::<Vec<_>>();
        if let [.., parent, _] = segments.as_slice()
            && parent
                .ident
                .to_string()
                .starts_with(|character: char| character.is_ascii_uppercase())
        {
            return None;
        }
        let name = segments.last()?.ident.to_string();
        self.api.functions.contains_key(&name).then_some(name)
    }

    fn bind(&mut self, pattern: &syn::Pat, value: Option<String>) {
        match pattern {
            syn::Pat::Ident(ident) => {
                self.locals.insert(ident.ident.to_string(), value);
            }
            syn::Pat::Type(typed) => self.bind(&typed.pat, value),
            _ => {}
        }
    }
}

impl<'ast> Visit<'ast> for Trace<'_> {
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        // A call to another method of the same binding type is followed into it.
        let method = call.method.to_string();
        if matches!(call.receiver.as_ref(), syn::Expr::Path(path) if path.path.is_ident("self"))
            && let Some(body) = self.siblings.get(&method).copied()
            && self.followed.insert(method)
        {
            let outer = std::mem::take(&mut self.locals);
            self.visit_block(body);
            self.locals = outer;
        }
        if let Some(owner) = self.type_of(&call.receiver) {
            let method = call.method.to_string();
            if self.api.method(&owner, &method).is_some() {
                self.reached.items.insert(format!("{owner}::{method}"));
            }
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = call.func.as_ref() {
            if let Some((owner, function)) = self.associated(&path.path) {
                self.reached.items.insert(format!("{owner}::{function}"));
            } else if let Some(name) = self.free_function(&path.path) {
                self.reached.items.insert(format!("{name}()"));
            }
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let Some(init) = &local.init {
            let value = self.type_of(&init.expr);
            self.bind(&local.pat, value);
        }
        visit::visit_local(self, local);
    }

    fn visit_expr_assign(&mut self, assign: &'ast syn::ExprAssign) {
        if let syn::Expr::Path(path) = assign.left.as_ref()
            && let Some(ident) = path.path.get_ident()
        {
            let value = self.type_of(&assign.right);
            self.locals.insert(ident.to_string(), value);
        }
        visit::visit_expr_assign(self, assign);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        // Using Polars' plan builder or its arena, or naming a variant of one of its
        // expression or plan enums, such as `Expr::Function` or `DataTypeExpr::OfExpr`.
        let builds = segments.iter().enumerate().any(|(at, segment)| {
            matches!(segment.as_str(), "DslBuilder" | "AExpr")
                || ((segment.ends_with("Expr") || segment == "DslPlan")
                    && segments.get(at + 1).is_some_and(|variant| {
                        variant.starts_with(|character: char| character.is_ascii_uppercase())
                    }))
        });
        self.reached.internals |= builds;
        visit::visit_path(self, path);
    }
}

/// Returns the Rust type a binding type wraps: its name without `Py`, when the
/// API has that type, as `Expr` for `PyExpr`.
fn wrapped_type(binding_type: &str, api: &RustApi) -> Option<String> {
    let name = binding_type.strip_prefix("Py")?;
    api.methods.contains_key(name).then(|| name.to_owned())
}

/// Follows a binding body and returns what it reaches. `siblings` are the other
/// methods of the same binding type, followed when the body calls them on `self`.
pub fn trace_body(
    body: &syn::Block,
    binding_type: &str,
    api: &RustApi,
    siblings: &HashMap<String, &syn::Block>,
) -> Reached {
    let mut trace = Trace {
        api,
        own: wrapped_type(binding_type, api),
        siblings,
        followed: BTreeSet::new(),
        locals: HashMap::new(),
        reached: Reached::default(),
    };
    trace.visit_block(body);
    trace.reached
}

/// The binding: what each of its methods and module functions reaches, keyed
/// by the binding type, or `module`, and the name Python calls it by. A
/// constructor is also listed as `__new__`.
pub type Binding = BTreeMap<(String, String), Reached>;

/// Returns whether `attributes` hold the attribute `name`, such as `pymethods`.
fn has_attribute(attributes: &[syn::Attribute], name: &str) -> bool {
    attributes.iter().any(|attribute| {
        attribute
            .path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == name)
    })
}

/// Returns the name Python calls a binding item by: its own, or the one
/// `#[pyo3(name = "…")]` or `#[pyfunction(name = "…")]` gives.
fn python_name(attributes: &[syn::Attribute], own: &syn::Ident) -> String {
    for attribute in attributes {
        let path = attribute.path();
        if !(path.is_ident("pyo3") || path.is_ident("pyfunction"))
            || !matches!(attribute.meta, syn::Meta::List(_))
        {
            continue;
        }
        let mut renamed = None;
        let _ = attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let value: syn::LitStr = meta.value()?.parse()?;
                renamed = Some(value.value());
            } else if meta.input.peek(syn::Token![=]) {
                let _: syn::Expr = meta.value()?.parse()?;
            }
            Ok(())
        });
        if let Some(renamed) = renamed {
            return renamed;
        }
    }
    own.to_string()
}

/// Adds what an item reaches under `key`, joining it with anything already there.
fn add(binding: &mut Binding, key: (String, String), reached: Reached) {
    let entry = binding.entry(key).or_default();
    entry.items.extend(reached.items);
    entry.internals |= reached.internals;
}

/// The methods of each binding type, wherever they are defined.
type Methods<'a> = BTreeMap<String, Vec<&'a syn::ImplItemFn>>;

fn collect_items<'a>(
    items: &'a [syn::Item],
    methods: &mut Methods<'a>,
    functions: &mut Vec<&'a syn::ItemFn>,
) {
    for item in items {
        match item {
            syn::Item::Impl(block) if has_attribute(&block.attrs, "pymethods") => {
                let syn::Type::Path(owner) = block.self_ty.as_ref() else {
                    continue;
                };
                let Some(owner) = owner.path.segments.last() else {
                    continue;
                };
                let entry = methods.entry(owner.ident.to_string()).or_default();
                for member in &block.items {
                    if let syn::ImplItem::Fn(method) = member {
                        entry.push(method);
                    }
                }
            }
            syn::Item::Fn(function) if has_attribute(&function.attrs, "pyfunction") => {
                functions.push(function);
            }
            syn::Item::Mod(module) => {
                if let Some((_, inner)) = &module.content {
                    collect_items(inner, methods, functions);
                }
            }
            _ => {}
        }
    }
}

fn trace_items(
    methods: &Methods<'_>,
    functions: &[&syn::ItemFn],
    api: &RustApi,
    binding: &mut Binding,
) {
    for (owner, members) in methods {
        let siblings = members
            .iter()
            .map(|method| (method.sig.ident.to_string(), &method.block))
            .collect::<HashMap<_, _>>();
        for method in members {
            let reached = trace_body(&method.block, owner, api, &siblings);
            if has_attribute(&method.attrs, "new") {
                add(
                    binding,
                    (owner.clone(), "__new__".to_owned()),
                    reached.clone(),
                );
            }
            let name = python_name(&method.attrs, &method.sig.ident);
            add(binding, (owner.clone(), name), reached);
        }
    }
    let none = HashMap::new();
    for function in functions {
        let name = python_name(&function.attrs, &function.sig.ident);
        add(
            binding,
            ("module".to_owned(), name),
            trace_body(&function.block, "module", api, &none),
        );
    }
}

/// Reads binding source files, given as their text. A binding type's methods
/// may be spread over several files, and each can call the others on `self`.
pub fn read_binding_sources(sources: &[String], api: &RustApi) -> Result<Binding, String> {
    let files = sources
        .iter()
        .map(|source| {
            syn::parse_file(source).map_err(|error| format!("parse the binding: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut methods = Methods::new();
    let mut functions = Vec::new();
    for file in &files {
        collect_items(&file.items, &mut methods, &mut functions);
    }
    let mut binding = Binding::new();
    trace_items(&methods, &functions, api, &mut binding);
    Ok(binding)
}

fn rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?;
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

/// Reads every source file of the binding under `directory`.
pub fn read_binding(directory: &Path, api: &RustApi) -> Result<Binding, String> {
    let mut files = Vec::new();
    rust_files(directory, &mut files)?;
    files.sort();
    let sources = files
        .iter()
        .map(|file| {
            std::fs::read_to_string(file)
                .map_err(|error| format!("read {}: {error}", file.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    read_binding_sources(&sources, api)
}

/// Runs `python/inventory.py` on the package at `package` and reads what it prints.
pub fn read_python(package: &Path) -> Result<Vec<PythonItem>, String> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("python")
        .join("inventory.py");
    let output = Command::new("python3")
        .arg("-I")
        .arg(&script)
        .arg(package)
        .output()
        .map_err(|error| format!("run python3 on {}: {error}", script.display()))?;
    if !output.status.success() {
        return Err(format!(
            "reading the Python package failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_inventory(&String::from_utf8_lossy(&output.stdout))
}

fn strings(value: &serde_json::Value, field: &str) -> Result<Vec<String>, String> {
    value[field]
        .as_array()
        .ok_or_else(|| format!("the inventory has no list {field}"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("the inventory's {field} holds a value that is not text"))
        })
        .collect()
}

fn text(value: &serde_json::Value, field: &str) -> Result<String, String> {
    value[field]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("an inventory item has no {field}"))
}

/// Reads the JSON `python/inventory.py` prints.
pub fn parse_inventory(json: &str) -> Result<Vec<PythonItem>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| format!("read the inventory: {error}"))?;
    let items = value.as_array().ok_or("the inventory is not a list")?;
    items
        .iter()
        .map(|item| {
            let binding = item["binding"]
                .as_array()
                .ok_or("an inventory item has no binding list")?
                .iter()
                .map(|call| match call.as_array().map(Vec::as_slice) {
                    Some([kind, name]) => match (kind.as_str(), name.as_str()) {
                        (Some(kind), Some(name)) => Ok((kind.to_owned(), name.to_owned())),
                        _ => Err("a binding call is not two names".to_owned()),
                    },
                    _ => Err("a binding call is not two names".to_owned()),
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(PythonItem {
                owner: text(item, "owner")?,
                name: text(item, "name")?,
                kind: text(item, "kind")?,
                parameters: strings(item, "parameters")?,
                deprecated: item["deprecated"]
                    .as_bool()
                    .ok_or("an inventory item has no deprecated flag")?,
                binding,
                uses: strings(item, "uses")?,
            })
        })
        .collect()
}

/// How a Python item reaches Rust.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Kind {
    /// It reaches a public Rust item of the same name.
    SameName,
    /// It reaches public Rust items, none of the same name.
    OtherName,
    /// It makes no binding call; Python builds it from other calls.
    InPython,
    /// Its binding calls reach no public Rust item.
    NotResolved,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::SameName => "same name",
            Kind::OtherName => "other name",
            Kind::InPython => "in Python",
            Kind::NotResolved => "not resolved",
        }
    }
}

/// One line of the audit, with how the item reaches Rust.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Traced {
    pub kind: Kind,
    pub line: String,
}

/// Traces one Python item through the binding.
pub fn trace_item(item: &PythonItem, binding: &Binding) -> Traced {
    let mut signature = if item.kind == "property" {
        format!("{}.{} property", item.owner, item.name)
    } else {
        format!(
            "{}.{}({})",
            item.owner,
            item.name,
            item.parameters.join(", ")
        )
    };
    if item.deprecated {
        signature.push_str(" deprecated");
    }
    if item.binding.is_empty() {
        let uses = if item.uses.is_empty() {
            String::new()
        } else {
            format!(" from {}", item.uses.join(", "))
        };
        return Traced {
            kind: Kind::InPython,
            line: format!("{signature} | in Python{uses}"),
        };
    }
    let mut reached = BTreeSet::new();
    let mut internals = false;
    let mut calls = Vec::new();
    for (kind, name) in &item.binding {
        let shown = if kind == "module" {
            format!("plr.{name}")
        } else {
            format!("{kind}.{name}")
        };
        match binding.get(&(kind.clone(), name.clone())) {
            Some(found) => {
                reached.extend(found.items.iter().cloned());
                internals |= found.internals;
                calls.push(shown);
            }
            None => calls.push(format!("{shown} (not in the binding)")),
        }
    }
    let same = reached.iter().any(|reached| {
        reached == &format!("{}()", item.name) || reached.ends_with(&format!("::{}", item.name))
    });
    let kind = if same {
        Kind::SameName
    } else if reached.is_empty() {
        Kind::NotResolved
    } else {
        Kind::OtherName
    };
    let mut rust = reached.into_iter().collect::<Vec<_>>();
    if internals {
        rust.push("builds internals".to_owned());
    }
    let rust = if rust.is_empty() {
        "-".to_owned()
    } else {
        rust.join(", ")
    };
    let uses = if item.uses.is_empty() {
        String::new()
    } else {
        format!(" | also uses {}", item.uses.join(", "))
    };
    Traced {
        kind,
        line: format!(
            "{signature} | {} | {rust} | {}{uses}",
            calls.join(", "),
            kind.label()
        ),
    }
}

/// Writes the audit from the traced items, with a count of each kind.
pub fn render_python_artifact(source: Option<&str>, traced: &[Traced]) -> String {
    let mut output = String::new();
    output.push_str("# Python Polars public API, traced through its binding to the Rust API\n");
    output.push_str("# generated by Dageist public API artifact tooling\n");
    if let Some(source) = source {
        output.push_str("# source: ");
        output.push_str(source);
        output.push('\n');
    }
    output.push_str(
        "# covers: the public methods, properties and operators of Expr and its namespaces str,\n\
         #   dt, list, arr, cat, name, struct, bin, meta, map and ext; of when/then, LazyFrame and\n\
         #   LazyGroupBy; and the functions polars.functions exports and the scan_* functions.\n\
         #   Not the eager DataFrame and Series, selectors, configuration or the read_* functions.\n",
    );
    output.push_str(
        "# each line: item(parameters) | binding calls | public Rust items reached | how\n\
         #   [| also uses: the other Polars calls it is built from]\n",
    );
    for kind in [
        Kind::SameName,
        Kind::OtherName,
        Kind::InPython,
        Kind::NotResolved,
    ] {
        let count = traced.iter().filter(|traced| traced.kind == kind).count();
        output.push_str(&format!("# {}: {count}\n", kind.label()));
    }
    output.push('\n');
    for traced in traced {
        output.push_str(&traced.line);
        output.push('\n');
    }
    output
}

/// Generates the Python audit for the workspace at `root`.
pub fn generate(
    root: &Path,
    audit: &PythonAudit<'_>,
    source: Option<&str>,
) -> Result<String, String> {
    let mut api = RustApi::default();
    for relative in audit.rust_audits {
        let path = root.join(relative);
        let text = std::fs::read_to_string(&path).map_err(|error| {
            format!(
                "read {}: {error}; generate the dependency audits before the Python audit",
                path.display()
            )
        })?;
        api.read(&text);
    }
    let binding = read_binding(&root.join(audit.binding), &api)?;
    let items = read_python(&root.join(audit.package))?;
    let traced = items
        .iter()
        .map(|item| trace_item(item, &binding))
        .collect::<Vec<_>>();
    Ok(render_python_artifact(source, &traced))
}

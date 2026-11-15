//! Writes the audit of the Python API of the upstream release: each public
//! Python method and function, the binding methods it calls, and the public
//! Rust items those reach.
//!
//! The Python package is read with Python's own parser by
//! `python/inventory.py`, so no Python package needs installing. The binding
//! is read with `syn`. A binding method's calls are followed only on values
//! that come from the Polars object it wraps, so a call on anything else,
//! such as an `Option`, is not mistaken for a Polars method.

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

/// One public Python method or function, as `python/inventory.py` reads it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PythonItem {
    /// The class it belongs to, such as `Expr.str`, or `polars` for a function.
    pub owner: String,
    pub name: String,
    /// Its parameters as written, with their defaults.
    pub parameters: Vec<String>,
    pub deprecated: bool,
    /// The binding calls it makes: the binding type, or `module`, and the name.
    pub binding: Vec<(String, String)>,
    /// For an item that makes no binding call, the Polars calls it is built from.
    pub uses: Vec<String>,
}

/// The public Rust API: methods by the type they belong to, and free functions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RustApi {
    pub methods: BTreeMap<String, BTreeSet<String>>,
    pub functions: BTreeSet<String>,
}

impl RustApi {
    /// Reads the public functions of an audit, such as `pub fn polars_plan::dsl::Expr::abs(self) -> Self`.
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
            let path = rest.split(['<', '(']).next().unwrap_or_default();
            let segments = path.split("::").collect::<Vec<_>>();
            let [.., parent, name] = segments.as_slice() else {
                continue;
            };
            if parent.starts_with(|character: char| character.is_ascii_uppercase()) {
                self.methods
                    .entry((*parent).to_owned())
                    .or_default()
                    .insert((*name).to_owned());
            } else {
                self.functions.insert((*name).to_owned());
            }
        }
    }

    fn has_method(&self, owner: &str, name: &str) -> bool {
        self.methods
            .get(owner)
            .is_some_and(|methods| methods.contains(name))
    }
}

/// What a binding method or function reaches: public Rust items, written as
/// `Type::method` or `function()`, and whether it builds Polars' internals itself.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Reached {
    pub items: BTreeSet<String>,
    pub internals: bool,
}

/// The accessors that open an expression namespace, and the namespace type each opens.
const NAMESPACES: &[(&str, &str)] = &[
    ("str", "StringNameSpace"),
    ("dt", "DateLikeNameSpace"),
    ("list", "ListNameSpace"),
    ("arr", "ArrayNameSpace"),
    ("cat", "CategoricalNameSpace"),
    ("name", "ExprNameNameSpace"),
    ("struct_", "StructNameSpace"),
    ("binary", "BinaryNameSpace"),
    ("meta", "MetaNameSpace"),
];

/// Methods that hand on the value they are called on, such as cloning it.
const PASSING_ON: &[&str] = &[
    "clone", "read", "write", "into", "to_owned", "as_ref", "borrow", "unwrap", "expect", "take",
];

/// Fields of a binding object that hold the Polars value it wraps.
const WRAPPED: &[&str] = &["inner", "ldf", "lgb"];

/// Returns the Rust types whose methods a binding type's methods call.
fn rust_types(binding_type: &str) -> &'static [&'static str] {
    match binding_type {
        "PyExpr" => &["Expr"],
        "PyLazyFrame" => &[
            "LazyFrame",
            "JoinBuilder",
            "LazyGroupBy",
            "LazyCsvReader",
            "LazyJsonLineReader",
            "LazyFileListReader",
        ],
        "PyLazyGroupBy" => &["LazyGroupBy"],
        "PyWhen" | "PyThen" | "PyChainedWhen" | "PyChainedThen" => {
            &["When", "Then", "ChainedWhen", "ChainedThen"]
        }
        _ => &["Expr", "LazyFrame"],
    }
}

/// What an expression in a binding method evaluates to, as far as the trace needs to know.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Value {
    /// A Polars value, such as the expression a binding object wraps.
    Polars,
    /// An expression namespace of the given type.
    Namespace(&'static str),
    /// Anything else.
    Other,
}

/// Follows one binding method's body.
struct Trace<'a> {
    api: &'a RustApi,
    types: &'static [&'static str],
    locals: HashMap<String, Value>,
    reached: Reached,
}

impl Trace<'_> {
    fn value_of(&self, expr: &syn::Expr) -> Value {
        match expr {
            syn::Expr::Field(field) => match &field.member {
                syn::Member::Named(name) if WRAPPED.contains(&name.to_string().as_str()) => {
                    Value::Polars
                }
                _ => Value::Other,
            },
            syn::Expr::Path(path) => path
                .path
                .get_ident()
                .and_then(|ident| self.locals.get(&ident.to_string()).cloned())
                .unwrap_or(Value::Other),
            syn::Expr::Call(call) => match call.func.as_ref() {
                syn::Expr::Path(path)
                    if self.is_function(&path.path) || self.associated(&path.path).is_some() =>
                {
                    Value::Polars
                }
                _ => Value::Other,
            },
            syn::Expr::MethodCall(call) => {
                let method = call.method.to_string();
                match self.value_of(&call.receiver) {
                    Value::Polars if PASSING_ON.contains(&method.as_str()) => Value::Polars,
                    Value::Polars if call.args.is_empty() => namespace(&method)
                        .map(Value::Namespace)
                        .or_else(|| self.method_owner(&method).map(|_| Value::Polars))
                        .unwrap_or(Value::Other),
                    Value::Polars if self.method_owner(&method).is_some() => Value::Polars,
                    Value::Namespace(owner) if self.api.has_method(owner, &method) => Value::Polars,
                    _ => Value::Other,
                }
            }
            syn::Expr::Paren(inner) => self.value_of(&inner.expr),
            syn::Expr::Group(inner) => self.value_of(&inner.expr),
            syn::Expr::Reference(inner) => self.value_of(&inner.expr),
            syn::Expr::Try(inner) => self.value_of(&inner.expr),
            syn::Expr::Unary(inner) => self.value_of(&inner.expr),
            syn::Expr::Cast(inner) => self.value_of(&inner.expr),
            _ => Value::Other,
        }
    }

    /// Returns the Rust type among this binding type's whose method `method` is.
    fn method_owner(&self, method: &str) -> Option<&'static str> {
        self.types
            .iter()
            .copied()
            .find(|owner| self.api.has_method(owner, method))
    }

    /// Returns `Type::function` when `path` names an associated function of the Rust API,
    /// such as `LazyCsvReader::new_with_sources`.
    fn associated(&self, path: &syn::Path) -> Option<String> {
        let segments = path.segments.iter().collect::<Vec<_>>();
        let [.., owner, function] = segments.as_slice() else {
            return None;
        };
        let (owner, function) = (owner.ident.to_string(), function.ident.to_string());
        self.api
            .has_method(&owner, &function)
            .then(|| format!("{owner}::{function}"))
    }

    fn is_function(&self, path: &syn::Path) -> bool {
        path.segments
            .last()
            .is_some_and(|segment| self.api.functions.contains(&segment.ident.to_string()))
    }

    fn bind(&mut self, pattern: &syn::Pat, value: Value) {
        match pattern {
            syn::Pat::Ident(ident) => {
                self.locals.insert(ident.ident.to_string(), value);
            }
            syn::Pat::Type(typed) => self.bind(&typed.pat, value),
            _ => {}
        }
    }
}

/// Returns the namespace type an accessor opens, if it is one.
fn namespace(accessor: &str) -> Option<&'static str> {
    NAMESPACES
        .iter()
        .find(|(name, _)| *name == accessor)
        .map(|(_, owner)| *owner)
}

impl<'ast> Visit<'ast> for Trace<'_> {
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let method = call.method.to_string();
        match self.value_of(&call.receiver) {
            Value::Polars if !(call.args.is_empty() && namespace(&method).is_some()) => {
                if let Some(owner) = self.method_owner(&method) {
                    self.reached.items.insert(format!("{owner}::{method}"));
                }
            }
            Value::Namespace(owner) if self.api.has_method(owner, &method) => {
                self.reached.items.insert(format!("{owner}::{method}"));
            }
            _ => {}
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = call.func.as_ref() {
            if let Some(associated) = self.associated(&path.path) {
                self.reached.items.insert(associated);
            } else if self.is_function(&path.path)
                && let Some(segment) = path.path.segments.last()
            {
                self.reached.items.insert(format!("{}()", segment.ident));
            }
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_assign(&mut self, assign: &'ast syn::ExprAssign) {
        if let syn::Expr::Path(path) = assign.left.as_ref()
            && let Some(ident) = path.path.get_ident()
        {
            let value = self.value_of(&assign.right);
            self.locals.insert(ident.to_string(), value);
        }
        visit::visit_expr_assign(self, assign);
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let Some(init) = &local.init {
            let value = self.value_of(&init.expr);
            self.bind(&local.pat, value);
        }
        visit::visit_local(self, local);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        let builds = segments.iter().enumerate().any(|(at, segment)| {
            matches!(
                segment.as_str(),
                "FunctionExpr" | "DslPlan" | "DslBuilder" | "AExpr"
            ) || (segment == "Expr"
                && segments.get(at + 1).is_some_and(|variant| {
                    variant.starts_with(|character: char| character.is_ascii_uppercase())
                }))
        });
        self.reached.internals |= builds;
        visit::visit_path(self, path);
    }
}

/// Follows a binding body and returns what it reaches.
pub fn trace_body(body: &syn::Block, binding_type: &str, api: &RustApi) -> Reached {
    let mut trace = Trace {
        api,
        types: rust_types(binding_type),
        locals: HashMap::new(),
        reached: Reached::default(),
    };
    trace.visit_block(body);
    trace.reached
}

/// The binding: what each of its methods and module functions reaches, keyed
/// by the binding type, or `module`, and the name Python calls it by.
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

fn read_items(items: &[syn::Item], api: &RustApi, binding: &mut Binding) {
    for item in items {
        match item {
            syn::Item::Impl(block) if has_attribute(&block.attrs, "pymethods") => {
                let syn::Type::Path(owner) = block.self_ty.as_ref() else {
                    continue;
                };
                let Some(owner) = owner
                    .path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string())
                else {
                    continue;
                };
                for member in &block.items {
                    if let syn::ImplItem::Fn(method) = member {
                        let name = python_name(&method.attrs, &method.sig.ident);
                        binding.insert(
                            (owner.clone(), name),
                            trace_body(&method.block, &owner, api),
                        );
                    }
                }
            }
            syn::Item::Fn(function) if has_attribute(&function.attrs, "pyfunction") => {
                let name = python_name(&function.attrs, &function.sig.ident);
                binding.insert(
                    ("module".to_owned(), name),
                    trace_body(&function.block, "module", api),
                );
            }
            syn::Item::Mod(module) => {
                if let Some((_, inner)) = &module.content {
                    read_items(inner, api, binding);
                }
            }
            _ => {}
        }
    }
}

/// Reads one source file of the binding.
pub fn read_binding_source(
    source: &str,
    api: &RustApi,
    binding: &mut Binding,
) -> Result<(), String> {
    let file = syn::parse_file(source).map_err(|error| format!("parse the binding: {error}"))?;
    read_items(&file.items, api, binding);
    Ok(())
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
    let mut binding = Binding::new();
    for file in files {
        let source = std::fs::read_to_string(&file)
            .map_err(|error| format!("read {}: {error}", file.display()))?;
        read_binding_source(&source, api, &mut binding)
            .map_err(|error| format!("{}: {error}", file.display()))?;
    }
    Ok(binding)
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
                owner: item["owner"]
                    .as_str()
                    .ok_or("an inventory item has no owner")?
                    .to_owned(),
                name: item["name"]
                    .as_str()
                    .ok_or("an inventory item has no name")?
                    .to_owned(),
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
    let signature = format!(
        "{}.{}({}){}",
        item.owner,
        item.name,
        item.parameters.join(", "),
        if item.deprecated { " deprecated" } else { "" }
    );
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
    Traced {
        kind,
        line: format!(
            "{signature} | {} | {rust} | {}",
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
        "# covers: Expr and its namespaces, when/then, LazyFrame, LazyGroupBy, the functions of\n\
         #   polars.functions and the scan_* functions; not the eager DataFrame and Series\n",
    );
    output.push_str(
        "# each line: item(parameters) | binding calls | public Rust items reached | how\n",
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

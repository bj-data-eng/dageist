"""Lists the public Python API of a Polars source tree, read with Python's own parser.

For each public method, property, operator and function it prints the
parameters with their defaults, whether it is deprecated, the calls it makes
into the Rust binding, and, for one that makes no such call, the Polars calls
it is built from instead.

A binding call is a call on a binding object (such as ``self._pyexpr``, or a
local variable holding one), on a binding class (``PyLazyFrame.new_from_csv``,
``plr.PyDataTypeExpr.of_expr``), to a binding class itself (a constructor,
recorded as ``__new__``), or to a binding module function (``plr.col``, or a
name imported from ``polars._plr``). Private helpers are followed, in the same
module or imported from another, as are the methods of objects such helpers
return: ``_select_engine(engine).collect(self)`` follows ``collect`` on the
classes of the module ``_select_engine`` is defined in.

Usage: python3 -I inventory.py <path to py-polars/src/polars>

Nothing is imported from the tree, so no Python package needs installing. The
parser must read the syntax Polars is written in, so the Python running this
must be at least the oldest Python Polars supports. The output is JSON, sorted
so that it does not depend on the order of files.
"""

import ast
import json
import pathlib
import sys

# Python Polars requires this version at the release this reader was written for.
MINIMUM_PYTHON = (3, 11)

# The classes read: the name the audit gives them, their file and the class.
CLASSES = [
    ("Expr", "expr/expr.py", "Expr"),
    ("Expr.str", "expr/string.py", "ExprStringNameSpace"),
    ("Expr.dt", "expr/datetime.py", "ExprDateTimeNameSpace"),
    ("Expr.list", "expr/list.py", "ExprListNameSpace"),
    ("Expr.arr", "expr/array.py", "ExprArrayNameSpace"),
    ("Expr.cat", "expr/categorical.py", "ExprCatNameSpace"),
    ("Expr.name", "expr/name.py", "ExprNameNameSpace"),
    ("Expr.struct", "expr/struct.py", "ExprStructNameSpace"),
    ("Expr.bin", "expr/binary.py", "ExprBinaryNameSpace"),
    ("Expr.meta", "expr/meta.py", "ExprMetaNameSpace"),
    ("Expr.map", "expr/map.py", "ExprMapNameSpace"),
    ("Expr.ext", "expr/ext.py", "ExprExtensionNameSpace"),
    ("When", "expr/whenthen.py", "When"),
    ("Then", "expr/whenthen.py", "Then"),
    ("ChainedWhen", "expr/whenthen.py", "ChainedWhen"),
    ("ChainedThen", "expr/whenthen.py", "ChainedThen"),
    ("LazyFrame", "lazyframe/frame.py", "LazyFrame"),
    ("LazyGroupBy", "lazyframe/group_by.py", "LazyGroupBy"),
]

# The attributes that hold a binding object, and the binding type each holds.
BINDING_ATTRIBUTES = {
    "_pyexpr": "PyExpr",
    "_ldf": "PyLazyFrame",
    "lgb": "PyLazyGroupBy",
    "_when": "PyWhen",
    "_then": "PyThen",
    "_chained_when": "PyChainedWhen",
    "_chained_then": "PyChainedThen",
}

# The module the binding is, and the name it is imported under.
BINDING_MODULE = "polars._plr"
BINDING_MODULE_NAME = "plr"

# The modules Polars' own Python API is used through inside the package.
POLARS_MODULES = {"F", "pl"}

# The operators a class can define, listed as part of its public API.
OPERATORS = {
    "__add__", "__radd__", "__sub__", "__rsub__", "__mul__", "__rmul__",
    "__truediv__", "__rtruediv__", "__floordiv__", "__rfloordiv__", "__mod__",
    "__rmod__", "__pow__", "__rpow__", "__neg__", "__pos__", "__abs__",
    "__invert__", "__and__", "__rand__", "__or__", "__ror__", "__xor__",
    "__rxor__", "__eq__", "__ne__", "__lt__", "__le__", "__gt__", "__ge__",
    "__getitem__", "__contains__",
}


def decorator_names(function):
    names = []
    for decorator in function.decorator_list:
        called = decorator.func if isinstance(decorator, ast.Call) else decorator
        names.append(called.attr if isinstance(called, ast.Attribute) else getattr(called, "id", ""))
    return names


def is_overload(function):
    return "overload" in decorator_names(function)


def is_property(function):
    return "property" in decorator_names(function)


def is_deprecated(function):
    """Returns whether `function` itself is deprecated, by `@deprecated`; deprecating one
    of its parameters does not count."""
    return "deprecated" in decorator_names(function)


def is_listed(function):
    """Returns whether `function` belongs in the inventory: public or an operator, not an
    overload stub or a property setter."""
    if function.name.startswith("_") and function.name not in OPERATORS:
        return False
    if is_overload(function):
        return False
    return not any(
        isinstance(decorator, ast.Attribute) and decorator.attr in ("setter", "deleter")
        for decorator in function.decorator_list
    )


def parameters(function):
    """Returns the parameters of `function` as written, without `self` or `cls` and without annotations."""
    arguments = function.args
    positional = arguments.posonlyargs + arguments.args
    defaults = [None] * (len(positional) - len(arguments.defaults)) + list(arguments.defaults)
    out = []
    for index, (argument, default) in enumerate(zip(positional, defaults)):
        if not (index == 0 and argument.arg in ("self", "cls")):
            out.append(argument.arg if default is None else f"{argument.arg}={ast.unparse(default)}")
        if arguments.posonlyargs and argument is arguments.posonlyargs[-1] and out:
            out.append("/")
    if arguments.vararg:
        out.append(f"*{arguments.vararg.arg}")
    elif arguments.kwonlyargs:
        out.append("*")
    for argument, default in zip(arguments.kwonlyargs, arguments.kw_defaults):
        out.append(argument.arg if default is None else f"{argument.arg}={ast.unparse(default)}")
    if arguments.kwarg:
        out.append(f"**{arguments.kwarg.arg}")
    return out


class Module:
    """One module of the package: its tree, its functions and classes, and what it imports."""

    def __init__(self, root, name):
        self.name = name
        path = root.joinpath(*name.split(".")[1:])
        path = path.with_suffix(".py") if path.with_suffix(".py").exists() else path / "__init__.py"
        self.tree = ast.parse(path.read_text(encoding="utf-8"))
        self.functions = {}
        self.classes = {}
        for node in self.tree.body:
            if isinstance(node, ast.FunctionDef) and not is_overload(node):
                self.functions[node.name] = node
            elif isinstance(node, ast.ClassDef):
                self.classes[node.name] = node
        # Imports anywhere in the module, including under `if TYPE_CHECKING:` and in functions.
        self.imports = {}
        self.binding_names = {}
        for node in ast.walk(self.tree):
            if isinstance(node, ast.ImportFrom) and node.module:
                for alias in node.names:
                    local = alias.asname or alias.name
                    if node.module == BINDING_MODULE:
                        self.binding_names[local] = alias.name
                    elif node.module.startswith("polars."):
                        self.imports[local] = (node.module, alias.name)


class Package:
    """The package, with its modules read once each."""

    def __init__(self, root):
        self.root = root
        self.modules = {}

    def module(self, name):
        if name not in self.modules:
            self.modules[name] = Module(self.root, name)
        return self.modules[name]

    def resolve(self, module, name):
        """Returns the module and definition `name` means in `module`, following imports."""
        seen = set()
        while (module.name, name) not in seen:
            seen.add((module.name, name))
            if name in module.functions:
                return module, module.functions[name]
            if name in module.classes:
                return module, module.classes[name]
            if name not in module.imports:
                return None
            target, original = module.imports[name]
            try:
                module = self.module(target)
            except FileNotFoundError:
                return None
            name = original
        return None


def binding_class(name):
    return name.startswith("Py")


def binding_root(node):
    """Returns the binding type an expression's value comes from, such as
    `self._ldf.with_optimizations(...)`, or None."""
    while True:
        if isinstance(node, ast.Call):
            node = node.func
        elif isinstance(node, ast.Attribute):
            if node.attr in BINDING_ATTRIBUTES:
                return BINDING_ATTRIBUTES[node.attr]
            node = node.value
        else:
            return None


# Python's operators, and the method of the binding object each calls.
BINARY = {
    ast.Add: "add", ast.Sub: "sub", ast.Mult: "mul", ast.Div: "truediv",
    ast.FloorDiv: "floordiv", ast.Mod: "mod", ast.Pow: "pow", ast.BitAnd: "and",
    ast.BitOr: "or", ast.BitXor: "xor",
}
UNARY = {ast.USub: "__neg__", ast.UAdd: "__pos__", ast.Invert: "__invert__"}


def binding_type_of(node, held):
    """Returns the binding type `node` evaluates to: a binding object, or a local holding one."""
    if isinstance(node, ast.Name):
        return held.get(node.id)
    return binding_root(node)


def binding_calls(node, module):
    """Returns the binding calls in `node`, as (binding type or "module", name) pairs."""
    held = {}
    for child in ast.walk(node):
        if isinstance(child, ast.Assign) and len(child.targets) == 1 and isinstance(child.targets[0], ast.Name):
            kind = binding_root(child.value)
            if kind is not None:
                held[child.targets[0].id] = kind
    calls = set()
    for child in ast.walk(node):
        if isinstance(child, ast.Attribute):
            holder = child.value
            if isinstance(holder, ast.Attribute) and holder.attr in BINDING_ATTRIBUTES:
                calls.add((BINDING_ATTRIBUTES[holder.attr], child.attr))
            elif isinstance(holder, ast.Name) and holder.id in held:
                calls.add((held[holder.id], child.attr))
            elif isinstance(holder, ast.Name) and holder.id == BINDING_MODULE_NAME:
                if not binding_class(child.attr):
                    calls.add(("module", child.attr))
            elif (
                isinstance(holder, ast.Attribute)
                and isinstance(holder.value, ast.Name)
                and holder.value.id == BINDING_MODULE_NAME
                and binding_class(holder.attr)
            ):
                calls.add((holder.attr, child.attr))
            elif isinstance(holder, ast.Name) and binding_class(module.binding_names.get(holder.id, "")):
                calls.add((module.binding_names[holder.id], child.attr))
            elif isinstance(holder, ast.Name) and holder.id in BINDING_ATTRIBUTES.values():
                calls.add((holder.id, child.attr))
        elif isinstance(child, ast.BinOp) and type(child.op) in BINARY:
            # Python turns the other operand into a binding object first, so the binding's
            # own operator method is called whichever side the binding object is on.
            kind = binding_type_of(child.left, held) or binding_type_of(child.right, held)
            if kind is not None:
                calls.add((kind, f"__{BINARY[type(child.op)]}__"))
        elif isinstance(child, ast.UnaryOp) and type(child.op) in UNARY:
            operand = binding_type_of(child.operand, held)
            if operand is not None:
                calls.add((operand, UNARY[type(child.op)]))
        elif isinstance(child, ast.Call):
            func = child.func
            if isinstance(func, ast.Name) and func.id in module.binding_names:
                original = module.binding_names[func.id]
                calls.add((original, "__new__") if binding_class(original) else ("module", original))
            elif (
                isinstance(func, ast.Attribute)
                and isinstance(func.value, ast.Name)
                and func.value.id == BINDING_MODULE_NAME
                and binding_class(func.attr)
            ):
                calls.add((func.attr, "__new__"))
    return calls


def polars_calls_used(node, own):
    """Returns the Polars calls `node` is built from: functions called through `F` or `pl`,
    methods called on their results, and the public methods in `own` it calls on `self`."""
    used = set()
    for child in ast.walk(node):
        if isinstance(child, ast.Call) and isinstance(child.func, ast.Attribute):
            holder = child.func.value
            if isinstance(holder, ast.Name) and holder.id in POLARS_MODULES:
                used.add(child.func.attr)
            elif isinstance(holder, ast.Call):
                used.add(child.func.attr)
            elif isinstance(holder, ast.Name) and holder.id == "self" and child.func.attr in own:
                used.add(child.func.attr)
    return used


def subclasses(module, cls):
    """Returns `cls` and the classes of `module` that derive from it, directly or not."""
    found, names, changed = [cls], {cls.name}, True
    while changed:
        changed = False
        for candidate in module.classes.values():
            if candidate.name in names:
                continue
            bases = {base.id for base in candidate.bases if isinstance(base, ast.Name)}
            if bases & names:
                found.append(candidate)
                names.add(candidate.name)
                changed = True
    return found


def methods_named(classes, name):
    for cls in classes:
        for item in cls.body:
            if isinstance(item, ast.FunctionDef) and item.name == name and not is_overload(item):
                yield item, cls


def returned_classes(package, module, helper):
    """Returns the module and classes a helper's return annotation names, with their
    subclasses there; otherwise every class of the helper's own module."""
    annotation = helper.returns
    if isinstance(annotation, ast.Constant) and isinstance(annotation.value, str):
        annotation = ast.parse(annotation.value, mode="eval").body
    if isinstance(annotation, ast.Name):
        found = package.resolve(module, annotation.id)
        if found and isinstance(found[1], ast.ClassDef):
            return found[0], subclasses(found[0], found[1])
    return module, list(module.classes.values())


def follow(package, module, node, own_class=None):
    """Returns the binding calls of `node` and of the private code it reaches, transitively:
    private functions of its module or imported from another, the private methods of its
    class called on `self`, and the methods called on what such a function returns."""
    calls, seen, waiting = set(), set(), [(module, node, own_class)]
    while waiting:
        current_module, current, cls = waiting.pop()
        calls |= binding_calls(current, current_module)
        private = {}
        if cls is not None:
            private = {
                item.name: item
                for item in cls.body
                if isinstance(item, ast.FunctionDef) and item.name.startswith("_") and not is_overload(item)
            }
        # Locals that hold what a private helper returns.
        helper_results = {}
        for child in ast.walk(current):
            if (
                isinstance(child, ast.Assign)
                and len(child.targets) == 1
                and isinstance(child.targets[0], ast.Name)
                and isinstance(child.value, ast.Call)
                and isinstance(child.value.func, ast.Name)
                and child.value.func.id.startswith("_")
            ):
                helper_results[child.targets[0].id] = child.value.func.id
        for child in ast.walk(current):
            if not isinstance(child, ast.Call):
                continue
            func = child.func
            targets = []
            helper = None
            if isinstance(func, ast.Attribute):
                if (
                    isinstance(func.value, ast.Call)
                    and isinstance(func.value.func, ast.Name)
                    and func.value.func.id.startswith("_")
                ):
                    helper = func.value.func.id
                elif isinstance(func.value, ast.Name) and func.value.id in helper_results:
                    helper = helper_results[func.value.id]
            if isinstance(func, ast.Name) and func.id.startswith("_"):
                found = package.resolve(current_module, func.id)
                if found and isinstance(found[1], ast.FunctionDef):
                    targets.append((found[0], found[1], None))
            elif (
                isinstance(func, ast.Attribute)
                and isinstance(func.value, ast.Name)
                and func.value.id == "self"
                and func.attr in private
            ):
                targets.append((current_module, private[func.attr], cls))
            elif helper is not None:
                # A method called on what a private helper returns: that method of the
                # class the helper's return annotation names and of its subclasses.
                found = package.resolve(current_module, helper)
                if found and isinstance(found[1], ast.FunctionDef):
                    targets.append((found[0], found[1], None))
                    home, classes = returned_classes(package, found[0], found[1])
                    for item, candidate in methods_named(classes, func.attr):
                        targets.append((home, item, candidate))
            for target in targets:
                key = (target[0].name, id(target[1]))
                if key not in seen:
                    seen.add(key)
                    waiting.append(target)
    return calls


def entry(owner, name, function, calls, uses, kind):
    return {
        "owner": owner,
        "name": name,
        "kind": kind,
        "parameters": [] if kind == "property" else parameters(function),
        "deprecated": is_deprecated(function),
        "binding": sorted([binding, called] for binding, called in calls),
        "uses": sorted(uses),
    }


def read_classes(package):
    entries = []
    namespaces = {owner.split(".")[1] for owner, _, _ in CLASSES if owner.startswith("Expr.")}
    for owner, relative, class_name in CLASSES:
        module = package.module("polars." + relative.removesuffix(".py").replace("/", "."))
        cls = module.classes.get(class_name)
        if cls is None:
            raise SystemExit(f"class {class_name} not found in {relative}")
        methods = [node for node in cls.body if isinstance(node, ast.FunctionDef) and is_listed(node)]
        public = {method.name for method in methods}
        for method in methods:
            kind = "property" if is_property(method) else "operator" if method.name in OPERATORS else "method"
            if kind == "property" and owner == "Expr" and method.name in namespaces:
                continue  # a namespace accessor; the namespace is listed as an owner of its own
            calls = follow(package, module, method, cls)
            uses = set() if calls else polars_calls_used(method, public) - {method.name}
            entries.append(entry(owner, method.name, method, calls, uses, kind))
    return entries


def exported(package, name, prefix=""):
    """Returns the public names a package's `__init__.py` imports, with where each comes from."""
    module = package.module(name)
    names = []
    for node in module.tree.body:
        if isinstance(node, ast.ImportFrom) and node.module and node.module.startswith("polars."):
            for alias in node.names:
                public = alias.asname or alias.name
                if not public.startswith("_") and public.startswith(prefix):
                    names.append((public, node.module, alias.name))
    return names


def callable_of(package, module, name):
    """Returns the module, function and class of what `name` is in `module`: a function, or
    the `__call__` of the class an instance `name` is made from."""
    found = package.resolve(module, name)
    if found and isinstance(found[1], ast.FunctionDef):
        return found[0], found[1], None
    home = found[0] if found else module
    for node in home.tree.body:
        target = node.target if isinstance(node, ast.AnnAssign) else None
        if isinstance(node, ast.Assign) and len(node.targets) == 1:
            target = node.targets[0]
        value = getattr(node, "value", None)
        if (
            isinstance(target, ast.Name)
            and target.id == name
            and isinstance(value, ast.Call)
            and isinstance(value.func, ast.Name)
            and value.func.id in home.classes
        ):
            cls = home.classes[value.func.id]
            for method in cls.body:
                if isinstance(method, ast.FunctionDef) and method.name == "__call__":
                    return home, method, cls
    return None


def read_functions(package, name, prefix=""):
    entries = []
    for public, module_name, original in exported(package, name, prefix):
        located = callable_of(package, package.module(module_name), original)
        if located is None:
            raise SystemExit(f"{public} is exported from {name} but no function was found for it")
        module, function, cls = located
        calls = follow(package, module, function, cls)
        uses = set() if calls else polars_calls_used(function, set())
        entries.append(entry("polars", public, function, calls, uses, "function"))
    return entries


def main():
    if sys.version_info < MINIMUM_PYTHON:
        raise SystemExit(
            "reading Polars needs Python {}.{} or newer, the oldest Python Polars supports".format(*MINIMUM_PYTHON)
        )
    package = Package(pathlib.Path(sys.argv[1]))
    entries = read_classes(package)
    entries += read_functions(package, "polars.functions")
    entries += read_functions(package, "polars.io", prefix="scan_")
    order = {owner: index for index, (owner, _, _) in enumerate(CLASSES)}
    order["polars"] = len(order)
    seen, unique = set(), []
    for item in entries:
        key = (item["owner"], item["name"])
        if key not in seen:
            seen.add(key)
            unique.append(item)
    unique.sort(key=lambda item: (order[item["owner"]], item["name"]))
    json.dump(unique, sys.stdout, indent=1, sort_keys=True)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()

"""Lists the public Python API of a Polars source tree, read with Python's own parser.

For each public method and function it prints the parameters with their
defaults, whether it is deprecated, the calls it makes into the Rust binding
(through a binding object such as ``self._pyexpr``, a binding class such as
``PyLazyFrame``, or the binding module ``plr``), and, for one that makes no
such call, the public methods of its own class it uses instead.

Usage: python3 -I inventory.py <path to py-polars/src/polars>

Nothing is imported from the tree, so no Python package needs installing.
The output is JSON, sorted so that it does not depend on the order of files.
"""

import ast
import json
import pathlib
import sys

# The classes read, in the order they are written: the name the audit gives
# them, their file, the class, and the attributes that hold their binding object.
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

# The names under which the binding module and binding classes are used.
BINDING_MODULE = "plr"
BINDING_CLASSES = set(BINDING_ATTRIBUTES.values())


def parameters(function):
    """Returns the parameters of `function` as written, without `self` or `cls` and without annotations."""
    arguments = function.args
    positional = arguments.posonlyargs + arguments.args
    defaults = [None] * (len(positional) - len(arguments.defaults)) + list(arguments.defaults)
    out = []
    for index, (argument, default) in enumerate(zip(positional, defaults)):
        if index == 0 and argument.arg in ("self", "cls"):
            continue
        out.append(argument.arg if default is None else f"{argument.arg}={ast.unparse(default)}")
        if arguments.posonlyargs and argument is arguments.posonlyargs[-1]:
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


def is_deprecated(function):
    """Returns whether `function` itself is deprecated, by `@deprecated`; deprecating one
    of its parameters does not count."""
    for decorator in function.decorator_list:
        called = decorator.func if isinstance(decorator, ast.Call) else decorator
        name = called.attr if isinstance(called, ast.Attribute) else getattr(called, "id", "")
        if name == "deprecated":
            return True
    return False


def is_listed(function):
    """Returns whether `function` belongs in the inventory: public, not a property, not an overload stub."""
    if function.name.startswith("_"):
        return False
    for decorator in function.decorator_list:
        text = ast.unparse(decorator)
        if text in ("property", "overload", "typing.overload") or text.endswith(".setter"):
            return False
    return True


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


def binding_locals(node):
    """Returns the local names `node` assigns a binding object to, with its binding type."""
    held = {}
    for child in ast.walk(node):
        if isinstance(child, ast.Assign) and len(child.targets) == 1 and isinstance(child.targets[0], ast.Name):
            kind = binding_root(child.value)
            if kind is not None:
                held[child.targets[0].id] = kind
    return held


def binding_calls(node):
    """Returns the binding calls in `node`, as (binding type or "module", name) pairs."""
    calls = set()
    held = binding_locals(node)
    for child in ast.walk(node):
        if not isinstance(child, ast.Attribute):
            continue
        holder = child.value
        if isinstance(holder, ast.Attribute) and holder.attr in BINDING_ATTRIBUTES:
            calls.add((BINDING_ATTRIBUTES[holder.attr], child.attr))
        elif isinstance(holder, ast.Name) and holder.id in held:
            calls.add((held[holder.id], child.attr))
        elif isinstance(holder, ast.Name) and holder.id == BINDING_MODULE:
            # A binding class reached through the module, as plr.PyExpr, is not a module function.
            if not child.attr.startswith("Py"):
                calls.add(("module", child.attr))
        elif (
            isinstance(holder, ast.Attribute)
            and isinstance(holder.value, ast.Name)
            and holder.value.id == BINDING_MODULE
            and holder.attr.startswith("Py")
        ):
            calls.add((holder.attr, child.attr))
        elif isinstance(holder, ast.Name) and holder.id in BINDING_CLASSES:
            calls.add((holder.id, child.attr))
    return calls


def own_methods_used(node, public):
    """Returns the public methods of the same class that `node` calls on `self`."""
    return {
        child.attr
        for child in ast.walk(node)
        if isinstance(child, ast.Attribute)
        and isinstance(child.value, ast.Name)
        and child.value.id == "self"
        and child.attr in public
    }


def module_helpers(tree):
    """Returns the private top-level functions of a module by name."""
    return {
        node.name: node
        for node in tree.body
        if isinstance(node, ast.FunctionDef) and node.name.startswith("_")
    }


def with_helpers(node, helpers, own=None):
    """Returns the binding calls of `node` and of the private functions it calls,
    transitively: module functions, and the private methods in `own` it calls on `self`."""
    own = own or {}
    calls, seen, waiting = set(), set(), [node]
    while waiting:
        current = waiting.pop()
        calls |= binding_calls(current)
        for child in ast.walk(current):
            if not isinstance(child, ast.Call):
                continue
            func = child.func
            if isinstance(func, ast.Name) and func.id in helpers:
                found = ("module", func.id, helpers[func.id])
            elif (
                isinstance(func, ast.Attribute)
                and isinstance(func.value, ast.Name)
                and func.value.id == "self"
                and func.attr in own
            ):
                found = ("own", func.attr, own[func.attr])
            else:
                continue
            if found[:2] not in seen:
                seen.add(found[:2])
                waiting.append(found[2])
    return calls


def entry(owner, name, function, calls, uses):
    return {
        "owner": owner,
        "name": name,
        "parameters": parameters(function),
        "deprecated": is_deprecated(function),
        "binding": sorted([kind, called] for kind, called in calls),
        "uses": sorted(uses),
    }


def read_classes(root):
    entries = []
    for owner, relative, class_name in CLASSES:
        tree = ast.parse((root / relative).read_text(encoding="utf-8"))
        helpers = module_helpers(tree)
        cls = next(
            (node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == class_name),
            None,
        )
        if cls is None:
            raise SystemExit(f"class {class_name} not found in {relative}")
        methods = [node for node in cls.body if isinstance(node, ast.FunctionDef) and is_listed(node)]
        public = {method.name for method in methods}
        private = {
            node.name: node
            for node in cls.body
            if isinstance(node, ast.FunctionDef) and node.name.startswith("_") and not node.name.startswith("__")
        }
        for method in methods:
            calls = with_helpers(method, helpers, private)
            uses = set() if calls else own_methods_used(method, public) - {method.name}
            entries.append(entry(owner, method.name, method, calls, uses))
    return entries


def module_file(root, module):
    """Returns the file of a module of the package, such as `polars.functions.col`."""
    parts = module.split(".")[1:]
    path = root.joinpath(*parts)
    return path.with_suffix(".py") if path.with_suffix(".py").exists() else path / "__init__.py"


def exported(root, package, prefix=""):
    """Returns the public names a package's `__init__.py` imports, with the module and name each comes from."""
    tree = ast.parse(module_file(root, package).read_text(encoding="utf-8"))
    names = []
    for node in tree.body:
        if isinstance(node, ast.ImportFrom) and node.module and node.module.startswith("polars."):
            for alias in node.names:
                public = alias.asname or alias.name
                if not public.startswith("_") and public.startswith(prefix):
                    names.append((public, node.module, alias.name))
    return names


def is_overload(function):
    return any(ast.unparse(decorator) in ("overload", "typing.overload") for decorator in function.decorator_list)


def find_function(root, module, name):
    """Returns the module tree and the function `name` defines in `module`, following
    re-exports; or the `__call__` of the class an instance `name` is made from."""
    tree = ast.parse(module_file(root, module).read_text(encoding="utf-8"))
    for node in tree.body:
        if isinstance(node, ast.FunctionDef) and node.name == name and not is_overload(node):
            return tree, node
    for node in tree.body:
        if isinstance(node, ast.ImportFrom) and node.module and node.module.startswith("polars."):
            for alias in node.names:
                if (alias.asname or alias.name) == name:
                    return find_function(root, node.module, alias.name)
    for node in tree.body:
        target = node.target if isinstance(node, ast.AnnAssign) else None
        if isinstance(node, ast.Assign) and len(node.targets) == 1:
            target = node.targets[0]
        value = getattr(node, "value", None)
        if (
            isinstance(target, ast.Name)
            and target.id == name
            and isinstance(value, ast.Call)
            and isinstance(value.func, ast.Name)
        ):
            for cls in tree.body:
                if isinstance(cls, ast.ClassDef) and cls.name == value.func.id:
                    for method in cls.body:
                        if isinstance(method, ast.FunctionDef) and method.name == "__call__":
                            return tree, (method, cls)
    return None


# The names under which Polars' own Python API is used inside the package.
POLARS_MODULES = {"F", "pl"}


def polars_calls_used(node):
    """Returns the Polars functions `node` calls through `F` or `pl`, and the methods it calls on their results."""
    used = set()
    for child in ast.walk(node):
        if isinstance(child, ast.Call) and isinstance(child.func, ast.Attribute):
            holder = child.func.value
            if isinstance(holder, ast.Name) and holder.id in POLARS_MODULES:
                used.add(child.func.attr)
            elif isinstance(holder, ast.Call):
                used.add(child.func.attr)
    return used


def read_functions(root, package, prefix=""):
    entries = []
    for public, module, name in exported(root, package, prefix):
        located = find_function(root, module, name)
        if located is None:
            raise SystemExit(f"{public} is exported from {package} but no function was found for it")
        tree, found = located
        helpers = module_helpers(tree)
        if isinstance(found, tuple):
            function, cls = found
            # An instance called as a function also reaches the binding through its class's helpers.
            for method in cls.body:
                if isinstance(method, ast.FunctionDef) and method.name.startswith("_"):
                    helpers.setdefault(method.name, method)
            calls = with_helpers(function, helpers)
            for child in ast.walk(function):
                if (
                    isinstance(child, ast.Attribute)
                    and isinstance(child.value, ast.Name)
                    and child.value.id == "self"
                    and child.attr in helpers
                ):
                    calls |= with_helpers(helpers[child.attr], helpers)
        else:
            function = found
            calls = with_helpers(function, helpers)
        uses = set() if calls else polars_calls_used(function) - {public}
        entries.append(entry("polars", public, function, calls, uses))
    return entries


def main():
    root = pathlib.Path(sys.argv[1])
    entries = read_classes(root)
    entries += read_functions(root, "polars.functions")
    entries += read_functions(root, "polars.io", prefix="scan_")
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

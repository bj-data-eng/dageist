"""Runs the Python Polars source the engine wrote for a document and compares what it gives
with what the engine's own run of the document gave.

Usage: execute.py CASE_DIRECTORY

The directory holds what the test wrote:

- `source.py`: the module the engine wrote, a function `run` that is given the table the
  document starts from and each input it names, by their names, and gives a lazy frame;
- `case.json`: the name of the starting table, and the inputs: each a name, a kind
  (`table`, `value` or `list`) and the Arrow file that holds it;
- `frame.arrow`: the rows the document is run on;
- `expected.arrow`: the table the engine's run gave.

Exit status 0 means the same schema and the same rows. Anything else prints why.
"""

import inspect
import json
import sys
from pathlib import Path

import polars as pl
from polars.testing import assert_frame_equal


def inputs_of(case: Path, description: dict) -> dict:
    """Returns every input of the case, bound as `run` takes it."""
    bound = {description["frame"]: pl.scan_ipc(case / "frame.arrow")}
    for item in description["inputs"]:
        loaded = pl.read_ipc(case / item["file"])
        kind = item["kind"]
        if kind == "table":
            bound[item["name"]] = loaded.lazy()
        elif kind == "value":
            bound[item["name"]] = loaded.get_column("value").item()
        elif kind == "list":
            bound[item["name"]] = loaded.get_column("value")
        else:
            raise SystemExit(f"unknown input kind {kind!r}")
    return bound


def main() -> None:
    case = Path(sys.argv[1])
    description = json.loads((case / "case.json").read_text(encoding="utf-8"))
    bound = inputs_of(case, description)
    source = (case / "source.py").read_text(encoding="utf-8")
    module: dict[str, object] = {}
    exec(compile(source, str(case / "source.py"), "exec"), module)  # noqa: S102 - under test
    # `run` takes the table the document starts from and the inputs it names, and no others.
    wanted = list(inspect.signature(module["run"]).parameters)
    missing = [name for name in wanted if name not in bound]
    if missing:
        raise SystemExit(f"run takes inputs the case does not have: {missing}")
    result = module["run"](**{name: bound[name] for name in wanted}).collect()
    expected = pl.read_ipc(case / "expected.arrow")
    assert_frame_equal(result, expected, check_dtypes=True, check_exact=True)


if __name__ == "__main__":
    main()

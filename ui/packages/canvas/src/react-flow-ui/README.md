# Components copied from React Flow UI

These files are copied from [React Flow UI](https://reactflow.dev/ui), React
Flow's components built with shadcn/ui, by webkid GmbH. They were taken from
its registry at `https://ui.reactflow.dev/registry/<name>.json` on
2026-11-24. The component pages give the MIT License and link to the React Flow
repository's licence; the registry's own source repository is not public.

| File                 | Registry item    |
| -------------------- | ---------------- |
| `base-node.tsx`      | `base-node`      |
| `base-handle.tsx`    | `base-handle`    |
| `labeled-handle.tsx` | `labeled-handle` |
| `button-handle.tsx`  | `button-handle`  |

They are kept close to the originals so a later version can be compared with
them. The changes made here, each also noted in its file:

- **Imports** point at the files beside them and at `cn` from the design
  package.
- **`base-node.tsx`** draws the canvas's floating look and fills the size
  React Flow gives a resized node. It takes no tab stop of its own, since
  React Flow's wrapper around it is focusable. Its title passes its content
  through explicitly.
- **`base-handle.tsx`** draws the pin in the muted colour on the panel colour,
  not in slate greys.
- **`labeled-handle.tsx`** takes any content as its label, such as a name with
  its dtype, with `title` as the name a screen reader gives it.
- **`button-handle.tsx`** draws its stalk in the strong line colour, and its
  position has no default, since React Flow requires one.

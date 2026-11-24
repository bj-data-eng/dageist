# Components copied from shadcn/ui

These files are copied from [shadcn/ui](https://ui.shadcn.com), MIT License,
Copyright (c) 2023 shadcn. They come from the repository's `new-york-v4`
registry, which is built on Radix Primitives and written in plain Tailwind
classes, at commit `055aa6b7837b1808456a24beb4c97a677864aa82`:

| File                                                                                                      | From                                                                   |
| --------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `sidebar.tsx`                                                                                             | `apps/v4/registry/new-york-v4/ui/sidebar.tsx`                          |
| `button.tsx`, `collapsible.tsx`, `input.tsx`, `separator.tsx`, `sheet.tsx`, `skeleton.tsx`, `tooltip.tsx` | `apps/v4/registry/new-york-v4/ui/`                                     |
| `use-mobile.ts`                                                                                           | `apps/v4/registry/new-york-v4/hooks/use-mobile.ts`                     |
| `cn.ts`                                                                                                   | shadcn/ui's `cn` helper, written here with `clsx` and `tailwind-merge` |

They are kept close to the originals so a later version can be compared with
them. The changes made here:

- **Imports** point at the files beside them, and Next.js's `"use client"`
  lines are removed.
- **Colours.** shadcn/ui's `accent` is a quiet hover fill, while this
  application's `accent` is its orange. `button.tsx` uses `hover` and `ink` in
  its place, and `skeleton.tsx` uses `line`. The shell's stylesheet sets
  shadcn/ui's other colour names, such as `sidebar` and `foreground`, to the
  application's tokens.
- **Lint.** `sidebar.tsx` declares its context as an interface, gives its
  styles that set CSS properties a type instead of an assertion, returns no
  value from its toggle, and checks for a missing tooltip explicitly.
- **No `Math.random` while drawing.** `SidebarMenuSkeleton` derives its
  varying width from `useId` instead.
- **`use-mobile.ts`** reads the window's width with `useSyncExternalStore`
  instead of setting state from an effect.
- **`skeleton.tsx`** is hidden from screen readers.

The sheet's and tooltip's `animate-in` classes come from `tw-animate-css`,
which is not installed, so those parts appear without animating.

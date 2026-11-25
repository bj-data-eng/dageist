/**
 * @file What a box asks the canvas to do: each is an edit the canvas sends to
 * the host, or a request to the shell, such as opening a transform's steps.
 */

import { createContext, useContext } from 'react';

import type { FieldView, FilePurpose, FrameView, NodeKind, SlotView } from '@dageist/host';

/** What a box can ask for. */
export interface GraphActions {
  readonly rename: (node: string, name: string) => void;
  readonly remove: (node: string) => void;
  readonly chooseFile: (node: string, purpose: FilePurpose) => void;
  readonly editSteps: (node: string) => void;
  /** Adds a node of `kind` that reads the table of `node`. */
  readonly addFollowing: (node: string, kind: NodeKind) => void;
  readonly setParameter: (node: string, name: string, dtype: string, text: string) => void;
  readonly removeParameter: (node: string, name: string) => void;
  /** Fills a list slot of `node` from the column `column` of the table `from` gives. */
  readonly connectColumn: (node: string, slot: SlotView, from: string, column: string) => void;
  readonly disconnect: (node: string, slot: SlotView) => void;
  readonly removeInput: (node: string, name: string) => void;
  /** Keeps the box of `node` where it was resized to, at its new size. */
  readonly resized: (node: string, frame: FrameView) => void;
  /** The dtypes a parameter may have. */
  readonly parameterDtypes: readonly string[];
  /** The columns the node `node` gives, when they are known. */
  readonly columnsOf: (node: string) => readonly FieldView[] | null;
  /** The name the node `node` shows. */
  readonly nameOf: (node: string) => string;
}

const ActionsContext = createContext<GraphActions | null>(null);

/** Provides what the boxes can ask for. */
export const ActionsProvider = ActionsContext.Provider;

/** Returns what a box can ask for. */
export function useGraphActions(): GraphActions {
  const actions = useContext(ActionsContext);
  if (actions === null) throw new Error('A box is drawn outside the canvas that edits it.');
  return actions;
}

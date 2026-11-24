/**
 * @file What a window of the editor is made of: the node's document from the host, what the engine
 * reported of it, the chosen step and the rows after it. Both windows hold the same parts, so that
 * what is changed in one shows in the other.
 */

import { useEffect, useMemo, useRef } from 'react';

import type { DocumentStepView, Host } from '@dageist/host';

import type { Drafts } from './drafts';
import { stillWanted, useDraftStore } from './drafts';
import type { Editor } from './editor';
import { useEditor } from './editor';
import type { RowsState } from './engine';
import { useRows } from './engine';
import type { PreparedRun } from './model/report';
import { prepareRun } from './model/report';
import type { Role, Selection } from './selection';
import { useSelection } from './selection';

/** What a window holds. */
export interface EditorWindow {
  readonly editor: Editor;
  readonly drafts: Drafts;
  readonly selection: Selection;
  /** The steps of the document, or none until the host has sent it. */
  readonly steps: readonly DocumentStepView[];
  /** What the engine reported of the steps. `null` until the host has sent them. */
  readonly run: PreparedRun | null;
  /** The position of the chosen step, or `-1`. */
  readonly selectedIndex: number;
  readonly rows: RowsState;
}

const NO_STEPS: readonly DocumentStepView[] = [];

/** Holds the document, what the engine reported of it and the chosen step, for one window. */
export function useEditorWindow(host: Host, role: Role): EditorWindow {
  const editor = useEditor(host);
  const drafts = useDraftStore();
  const selection = useSelection(host, role);
  const { state } = editor;
  const steps = state?.document.steps ?? NO_STEPS;
  const run = useMemo(
    () => (state === null ? null : prepareRun(state.document.steps, state.run)),
    [state],
  );
  const { selectedId, choose } = selection;
  const selectedIndex = steps.findIndex((step) => step.id === selectedId);
  const revision = state?.revision ?? null;
  const lastIndex = useRef(0);

  // What is half done stays until its place is gone from the document or it is given its value, so
  // that a change another window makes elsewhere does not take away what the person has begun.
  const { retain } = drafts;
  const document = state?.document ?? null;
  useEffect(() => {
    if (document !== null) retain((draft) => stillWanted(document, draft));
  }, [document, retain]);

  // A chosen step that is gone is replaced by the step in its place, and none chosen is the first.
  useEffect(() => {
    if (state === null) return;
    if (selectedIndex >= 0) {
      lastIndex.current = selectedIndex;
      return;
    }
    const beside = steps[Math.min(lastIndex.current, steps.length - 1)];
    choose(beside?.id ?? null);
  }, [state, selectedIndex, steps, choose]);

  const rows = useRows(host, revision, selectedId, run?.steps[selectedIndex] ?? null);
  return { editor, drafts, selection, steps, run, selectedIndex, rows };
}

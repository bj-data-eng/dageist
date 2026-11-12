/**
 * @file The table viewer: opens the sample table in one of two modes and
 * shows it in a grid.
 *
 * A working window is a limited number of whole keys the engine holds in
 * memory. The whole result stays in its Parquet files, and the engine reads
 * each block from disk as the grid asks for it.
 */

import { useCallback, useEffect, useState } from 'react';

import { Panel, ToggleButton } from '@dageist/design';
import type { Host, TableView } from '@dageist/host';

import { ArrowGrid } from './ArrowGrid';

/** Whole keys held in the working window. */
const WINDOW_KEYS = 500;

/** A way of looking at the sample table. */
type Mode = 'window' | 'result';

/** What one mode is called and how it is opened and described. */
interface ModeInfo {
  readonly label: string;
  readonly title: string;
  readonly source: string;
  readonly open: (host: Host) => Promise<TableView>;
}

const MODES: Readonly<Record<Mode, ModeInfo>> = {
  window: {
    label: 'Working window',
    title: 'Sample claims · working window',
    source: `${String(WINDOW_KEYS)} whole keys held in memory`,
    open: async (host) => host.openSampleWindow(WINDOW_KEYS),
  },
  result: {
    label: 'Whole result',
    title: 'Sample claims · whole result',
    source: 'read from Parquet as you scroll',
    open: async (host) => host.openSampleResult(),
  },
};

const MODE_ORDER: readonly Mode[] = ['window', 'result'];

/** How opening a table ended. */
type Outcome =
  | { readonly status: 'open'; readonly view: TableView; readonly millis: number }
  | { readonly status: 'failed'; readonly message: string };

/** The outcome of opening one mode, and what its grid last reported. */
interface Settled {
  readonly mode: Mode;
  readonly outcome: Outcome;
  readonly report: string;
}

/** What the table viewer needs. */
export interface TableViewerProps {
  /** Where the table's shape and blocks come from. */
  readonly host: Host;
}

/** Shows the sample table, with a switch between the working window and the whole result. */
export function TableViewer({ host }: TableViewerProps): React.JSX.Element {
  const [mode, setMode] = useState<Mode>('window');
  const [settled, setSettled] = useState<Settled | null>(null);
  const info = MODES[mode];
  // The host holds one open table, so a mode is only shown once its own open
  // has finished, and the switch waits until then.
  const current = settled?.mode === mode ? settled : null;

  useEffect(() => {
    let live = true;
    const started = performance.now();
    const settle = (outcome: Outcome): void => {
      if (live) setSettled({ mode, outcome, report: '' });
    };
    const open = async (): Promise<void> => {
      try {
        const view = await MODES[mode].open(host);
        settle({ status: 'open', view, millis: performance.now() - started });
      } catch (error) {
        settle({ status: 'failed', message: String(error) });
      }
    };
    void open();
    return () => {
      live = false;
    };
  }, [host, mode]);

  const onReport = useCallback((report: string) => {
    setSettled((previous) => (previous === null ? previous : { ...previous, report }));
  }, []);

  const modeBar = MODE_ORDER.map((entry) => (
    <ToggleButton
      key={entry}
      pressed={entry === mode}
      disabled={current === null}
      onPress={() => {
        setMode(entry);
      }}
    >
      {MODES[entry].label}
    </ToggleButton>
  ));

  if (current === null) {
    return (
      <Panel
        mark="▦"
        title={`Opening the ${info.label.toLowerCase()}…`}
        details={['The first run generates the sample table, which takes a while.']}
        actions={modeBar}
      >
        {null}
      </Panel>
    );
  }
  if (current.outcome.status === 'failed') {
    return (
      <Panel
        mark="▦"
        title="The sample table could not be opened"
        details={[current.outcome.message]}
        actions={modeBar}
      >
        {null}
      </Panel>
    );
  }
  const { view, millis } = current.outcome;
  const shape =
    `${view.rows.toLocaleString('en-US')} rows × ${String(view.columns.length)} columns` +
    ` · ${info.source} · opened in ${millis.toFixed(0)} ms`;
  return (
    <Panel
      mark="▦"
      title={info.title}
      details={current.report === '' ? [shape] : [shape, current.report]}
      actions={modeBar}
    >
      <ArrowGrid key={mode} view={view} fetchBlock={host.fetchTableBlock} onReport={onReport} />
    </Panel>
  );
}

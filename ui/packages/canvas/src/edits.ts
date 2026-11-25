/**
 * @file The canvas's edits of the graph, sent one at a time. Each edit names
 * the revision the host is at, and the host refuses one made against an older
 * revision; so an edit is sent only once the one before it was answered, with
 * the revision that answer gave. A drag of several boxes or a delete of a box
 * and its wires is then several edits that each land, in the order they were
 * asked for.
 */

import type { FrameView, GraphEdit, GraphView, Host } from '@dageist/host';

/** What the queue sends to the host. */
export type EditPort = Pick<Host, 'editGraph' | 'placeNode'>;

/** Where a box added by an edit is to stand, once the host has added it. */
export type PlaceAdded = (added: string) => { readonly x: number; readonly y: number } | null;

/** What the queue reports. */
export interface EditListener {
  /** The graph as the host now holds it. */
  readonly view: (view: GraphView) => void;
  /** The host refused an edit, with why; the graph is as it was. */
  readonly refused: (message: string) => void;
  /** An edit failed, as one made against another revision does. */
  readonly failed: (error: unknown) => void;
}

/** Sends the canvas's edits to the host one at a time. */
export class GraphEdits {
  private current: GraphView;
  private tail: Promise<void> = Promise.resolve();

  constructor(
    private readonly port: EditPort,
    view: GraphView,
    private readonly listener: EditListener,
  ) {
    this.current = view;
  }

  /** Takes `view` as the graph the host holds, as after the graph was asked for again. */
  seen(view: GraphView): void {
    this.current = view;
  }

  /**
   * Sends `edit` once every edit before it is answered. A box it adds is
   * placed where `place` says, as an edit of the layout that follows it.
   */
  async edit(edit: GraphEdit, place?: PlaceAdded): Promise<void> {
    return this.queue(async () => {
      const answer = await this.port.editGraph(this.current.revision, edit);
      if (answer.kind === 'refused') {
        this.listener.refused(answer.message);
        return;
      }
      this.show(answer.graph);
      const at = answer.added === null ? null : (place?.(answer.added) ?? null);
      if (answer.added !== null && at !== null) {
        await this.placeNow(answer.added, { ...at, width: null, height: null });
      }
    });
  }

  /** Keeps where the box of `node` stands, and its size, once every edit before it is answered. */
  async place(node: string, frame: FrameView): Promise<void> {
    return this.queue(async () => this.placeNow(node, frame));
  }

  private async placeNow(node: string, frame: FrameView): Promise<void> {
    const placed = await this.port.placeNode(this.current.layoutRevision, node, frame);
    this.show({
      ...this.current,
      layoutRevision: placed.layoutRevision,
      nodes: this.current.nodes.map((box) => (box.id === node ? { ...box, frame } : box)),
    });
  }

  private show(view: GraphView): void {
    this.current = view;
    this.listener.view(view);
  }

  /** Runs `work` after everything queued before it, and reports a failure without stopping the queue. */
  private async queue(work: () => Promise<void>): Promise<void> {
    const next = this.tail.then(work).catch((error: unknown) => {
      this.listener.failed(error);
    });
    this.tail = next;
    return next;
  }
}

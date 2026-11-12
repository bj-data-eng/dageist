/**
 * @file The framed surface a feature draws itself on: a header with a mark,
 * a title and supporting lines, then the feature's own content.
 */

/** What a panel's header shows. */
export interface PanelProps {
  /** A short glyph that identifies the feature. */
  readonly mark: string;
  readonly title: string;
  /** Lines of supporting text under the title. */
  readonly details: readonly string[];
  /** Controls placed at the right of the header. */
  readonly actions?: React.ReactNode;
  readonly children: React.ReactNode;
}

/** Frames `children` under a header. */
export function Panel({ mark, title, details, actions, children }: PanelProps): React.JSX.Element {
  return (
    <article className="flex h-full w-full flex-col overflow-hidden rounded-xl border border-line bg-panel">
      <header className="flex items-center gap-3.5 border-b border-line px-5 py-4">
        <span aria-hidden="true" className="text-2xl text-accent">
          {mark}
        </span>
        <div className="min-w-0">
          <h1 className="m-0 text-[17px] font-semibold">{title}</h1>
          {details.map((detail) => (
            <p key={detail} className="m-0 mt-0.5 text-[13px] text-muted tabular-nums">
              {detail}
            </p>
          ))}
        </div>
        {actions !== undefined && <div className="ml-auto flex gap-1.5">{actions}</div>}
      </header>
      {children}
    </article>
  );
}

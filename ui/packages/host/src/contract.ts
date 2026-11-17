/**
 * @file What the view may ask of the host, and the shapes the host answers
 * in. Features depend on this contract, never on Tauri.
 */

/** Mirrors `IdentityView` in crates/dageist-tauri/src/identity.rs. */
export interface IdentityView {
  readonly name: string;
  readonly version: string;
}

/** Mirrors `ColumnView` in crates/dageist-tauri/src/sample_table.rs. */
export interface ColumnView {
  readonly name: string;
  readonly dataType: string;
}

/** Mirrors `TableView` in crates/dageist-tauri/src/sample_table.rs. */
export interface TableView {
  readonly rows: number;
  readonly columns: readonly ColumnView[];
}

/** A block of rows and columns to fetch from the open table. */
export interface BlockRequest {
  readonly offset: number;
  readonly length: number;
  readonly columns: readonly string[];
}

/**
 * What a parameter holds, mirroring `KindView` in
 * crates/dageist-tauri/src/catalogue.rs. Each kind has one drawing.
 */
export type KindView =
  /** A chain; `dtypes` names the dtype groups it may have. */
  | { readonly kind: 'expression'; readonly dtypes: readonly string[] }
  | { readonly kind: 'column' }
  /** A literal typed in; `type` is the type of literal it takes. */
  | { readonly kind: 'value'; readonly type: ValueTypeView }
  | { readonly kind: 'flag' }
  | { readonly kind: 'choice'; readonly words: readonly ChoiceView[] }
  | { readonly kind: 'dtype' }
  | { readonly kind: 'duration' }
  | { readonly kind: 'table' }
  | { readonly kind: 'path' }
  | { readonly kind: 'pairs'; readonly key: KindView; readonly value: KindView };

/** The types of literal a parameter takes, mirroring `ValueType` in the engine's catalogue. */
export const VALUE_TYPES = ['integer', 'float', 'string', 'date', 'any'] as const;

/** The type of a literal typed in. */
export type ValueTypeView = (typeof VALUE_TYPES)[number];

/** One word of a choice: as Polars spells it, and as it is shown. */
export interface ChoiceView {
  readonly word: string;
  readonly shown: string;
}

/** How many of its kind a parameter holds; `of` names the parameter a per-item one follows. */
export type ShapeView =
  | { readonly shape: 'one' }
  | { readonly shape: 'many' }
  | { readonly shape: 'perItem'; readonly of: string };

/**
 * What a parameter holds when it is not given, mirroring `DefaultView` in
 * crates/dageist-tauri/src/catalogue.rs. `none` is Python's `None`: unset,
 * which leaves the choice to Polars. `empty` is an empty `*` or `**`
 * parameter. `other` is a default the engine does not read as a value.
 */
export type DefaultView =
  | { readonly kind: 'required' }
  | { readonly kind: 'none' }
  | { readonly kind: 'flag'; readonly value: boolean }
  | { readonly kind: 'integer'; readonly value: number }
  | { readonly kind: 'float'; readonly value: number }
  | { readonly kind: 'text'; readonly value: string }
  | { readonly kind: 'empty' }
  | { readonly kind: 'other' };

/** Mirrors `ParameterView` in crates/dageist-tauri/src/catalogue.rs. */
export interface ParameterView {
  /** Python's name, which identifies the parameter. */
  readonly name: string;
  /** The name shown for it. */
  readonly shown: string;
  readonly label: string;
  readonly kind: KindView;
  readonly shape: ShapeView;
  /** Polars' default, read by the engine. */
  readonly default: DefaultView;
  /** Polars' default as Python writes it, or `null` when it must be given. */
  readonly python: string | null;
}

/** What an entry is applied to, mirroring `Applies` in the engine's catalogue; `condition` is one link of `when`, `then` and `otherwise`. */
export const APPLIES = ['expression', 'frame', 'function', 'condition'] as const;

/** What an entry is applied to. */
export type AppliesView = (typeof APPLIES)[number];

/** Mirrors `EntryView` in crates/dageist-tauri/src/catalogue.rs. */
export interface EntryView {
  /** The stable identifier saved steps keep, such as `Expr.shift`. */
  readonly id: string;
  /** `condition` is one link of `when`, `then` and `otherwise`. */
  readonly applies: AppliesView;
  /** The key of the family it is swapped within. */
  readonly family: string;
  /** The name shown for it; in English, Polars' own name. */
  readonly name: string;
  readonly hint: string;
  /** The plain head of a step, such as "Add column"; `null` for anything but a step. */
  readonly head: string | null;
  /** The dtype groups an expression method accepts on what it follows. */
  readonly accepts: readonly string[];
  /** Whether it works on a lazy frame without a collect. */
  readonly lazy: boolean;
  readonly parameters: readonly ParameterView[];
}

/** A family or dtype group: its key and the words shown for it. */
export interface WordsView {
  readonly key: string;
  readonly title: string;
}

/** Mirrors `CatalogueView` in crates/dageist-tauri/src/catalogue.rs. */
export interface CatalogueView {
  /** Every family, in the order they are offered. */
  readonly families: readonly WordsView[];
  /** Every dtype group. */
  readonly dtypes: readonly WordsView[];
  /** Every entry, ordered by identifier. */
  readonly entries: readonly EntryView[];
}

/** A column and the short name of its dtype, such as `Datetime(us)`. Mirrors `FieldView` in crates/dageist-tauri/src/report.rs. */
export interface FieldView {
  readonly name: string;
  readonly dtype: string;
}

/** The table a node's steps start from. Mirrors `SourceView` in crates/dageist-tauri/src/node.rs. */
export interface SourceView {
  readonly name: string;
  readonly columns: readonly FieldView[];
}

/** One input of a node, by the name a step document uses for it. Mirrors `InputView` in crates/dageist-tauri/src/node.rs. */
export type InputView =
  | { readonly kind: 'table'; readonly name: string; readonly columns: readonly FieldView[] }
  | { readonly kind: 'value'; readonly name: string; readonly dtype: string }
  | { readonly kind: 'list'; readonly name: string; readonly dtype: string };

/** What a node is connected to. Mirrors `NodeView` in crates/dageist-tauri/src/node.rs. */
export interface NodeView {
  readonly source: SourceView;
  /** Every other input, in the order of their names. */
  readonly inputs: readonly InputView[];
}

/**
 * The rows after a step, every cell written by the engine as text, and a null
 * as `null`. Each row has one cell for each column. Mirrors `RowsView` in
 * crates/dageist-tauri/src/node.rs.
 */
export interface RowsView {
  readonly columns: readonly FieldView[];
  readonly rows: ReadonlyArray<ReadonlyArray<string | null>>;
}

/** One step of the way from a step of a document to the thing inside it. */
export type SegmentView =
  /** The argument for a parameter, by Python's name for it. */
  | { readonly kind: 'parameter'; readonly name: string }
  /** An item of a list. */
  | { readonly kind: 'item'; readonly index: number }
  /** The key of an entry of pairs. */
  | { readonly kind: 'key'; readonly index: number }
  /** The value of an entry of pairs. */
  | { readonly kind: 'value'; readonly index: number }
  /** The start of an expression. */
  | { readonly kind: 'start' }
  /** A link of an expression, counted from the first after its start. */
  | { readonly kind: 'link'; readonly index: number };

/**
 * A place in a document: a step, then the way down through its arguments.
 * The step is absent for a place in the document outside its steps.
 */
export interface PlaceView {
  /** Where the step is in the document, counted from zero. */
  readonly stepIndex: number | null;
  readonly stepId: string | null;
  readonly path: readonly SegmentView[];
}

/** What an expression has, up to one of its links. */
export type ResolutionView =
  /** A single column of this dtype. */
  | { readonly kind: 'dtype'; readonly dtype: string }
  /** More than one column, with their dtypes. */
  | { readonly kind: 'several'; readonly dtypes: readonly string[] }
  /** A condition that still lacks its `then`. */
  | { readonly kind: 'unfinished' }
  /** An expression that starts from `element`, which has no dtype outside a pivot. */
  | { readonly kind: 'noContext' }
  /** Polars refuses the expression up to here, with this reason. */
  | { readonly kind: 'refused'; readonly reason: string };

/** The dtype of an expression at its start and after each of its links. */
export interface ExpressionDtypesView {
  readonly place: PlaceView;
  readonly start: ResolutionView;
  readonly links: readonly ResolutionView[];
}

/** How many values a chain gives, mirroring `Shape` in the engine: `one` as an aggregate does, or `many`. */
export const VALUE_SHAPES = ['one', 'many'] as const;

/** How many values a chain gives. */
export type ValueShapeView = (typeof VALUE_SHAPES)[number];

/** How far a condition has come, mirroring `ConditionState` in the engine: the call that was last made. */
export const CONDITION_STATES = ['when', 'then', 'chainedWhen', 'chainedThen'] as const;

/** How far a condition has come. */
export type ConditionStateView = (typeof CONDITION_STATES)[number];

/** What an expression has so far, where a call would follow it. */
export type AfterView =
  /** Nothing: the call starts an expression. */
  | { readonly kind: 'start' }
  /** A value, which an expression method follows. `dtype` is the class of its dtype. */
  | { readonly kind: 'value'; readonly dtype: string; readonly shape: ValueShapeView }
  /** A condition that is not finished. */
  | { readonly kind: 'condition'; readonly state: ConditionStateView };

/** The positions of a step that hold an expression, as the engine's tables name them. */
export const OUTER_POSITIONS = [
  'select',
  'filter',
  'sort',
  'join_left',
  'join_right',
  'pivot_aggregate',
] as const;

/** The position of a step that holds an expression. */
export type OuterPosition = (typeof OUTER_POSITIONS)[number];

/** The positions inside an expression that hold another one, as the engine's tables name them. */
export const NESTED_POSITIONS = [
  'over.partition_by',
  'over.order_by',
  'condition.predicate',
  'condition.value',
] as const;

/** A position inside an expression that holds another one. */
export type NestedPosition = (typeof NESTED_POSITIONS)[number];

/** Where an expression stands. */
export interface ContextView {
  readonly outer: OuterPosition;
  readonly nested: NestedPosition | null;
}

/** Where a call would stand: in which context, and after what. */
export interface OfferPlaceView {
  readonly context: ContextView;
  readonly after: AfterView;
}

/** Where Polars refused a combination, as the tables record it. */
export const STAGES = ['schema', 'rows', 'empty'] as const;

/** Where Polars refused a combination. */
export type StageView = (typeof STAGES)[number];

/** The rules the engine holds that Polars does not, by the name the tables give them. */
export const RULE_NAMES = [
  'pivot-aggregate-gives-one-value',
  'condition-predicate-is-boolean',
  'element-only-in-pivot-aggregate',
  'filter-predicate-is-boolean',
  'key-is-elementwise',
] as const;

/** The name of a rule the engine holds that Polars does not. */
export type RuleName = (typeof RULE_NAMES)[number];

/** Who refused a call. */
export type RefusalSourceView =
  | { readonly by: 'polars'; readonly stage: StageView }
  | { readonly by: 'rule'; readonly rule: RuleName }
  /** The shape of an expression: a step is not part of one, a method needs a value before it. */
  | { readonly by: 'structure' };

/** Why a call may not stand somewhere: Polars' reason, or the rule's statement. */
export interface RefusalView {
  readonly source: RefusalSourceView;
  readonly reason: string;
}

/** Whether an expression may end where it does. */
export type EndingView =
  | { readonly kind: 'allowed' }
  | { readonly kind: 'refused'; readonly refusal: RefusalView }
  /** The tables do not say, for this reason. */
  | { readonly kind: 'notKnown'; readonly because: string };

/** Whether a call may stand where it does. */
export type VerdictView =
  | {
      readonly kind: 'allowed';
      /** The dtype the chain has after it, where the call decides it. */
      readonly result: string | null;
      readonly shape: ValueShapeView;
      readonly ending: EndingView;
    }
  | { readonly kind: 'refused'; readonly refusal: RefusalView };

/** A call of an expression: what it follows, and whether it may stand there. Both are `null` where the tables were not asked. */
export interface StandingView {
  readonly follows: AfterView | null;
  readonly verdict: VerdictView | null;
}

/** An expression of a step, as the tables of allowed combinations see it. */
export interface WalkedExpressionView {
  readonly place: PlaceView;
  /** `null` where no named position holds the expression. */
  readonly context: ContextView | null;
  /** The start, then each link. */
  readonly calls: readonly StandingView[];
  /**
   * The place of each link, and after them the place a link added at the end
   * would have; `null` where the context or what comes before is not known.
   */
  readonly places: ReadonlyArray<OfferPlaceView | null>;
  readonly ending: EndingView | null;
}

/** A refusal the tables found, and where. */
export interface WalkRefusalView {
  readonly place: PlaceView;
  readonly refusal: RefusalView;
  /** Whether the expression there may not end where it does, and not that a call may not stand. */
  readonly atEnd: boolean;
}

/** What the tables of allowed combinations say of a step. */
export interface WalkView {
  readonly expressions: readonly WalkedExpressionView[];
  readonly refusal: WalkRefusalView | null;
  /** Why the tables could not be asked past a point, where they could not. */
  readonly notKnown: string | null;
}

/** Why a step was refused. */
export type CauseView =
  /** The step names a table that is not connected. */
  | { readonly kind: 'unknownTable'; readonly name: string }
  /** An expression starts from an input that is not a connected value or list. */
  | { readonly kind: 'unknownInput'; readonly name: string }
  /** The catalogue could not build the call. */
  | { readonly kind: 'build'; readonly reason: string }
  /** A named rule does not let a call stand where it does. */
  | { readonly kind: 'notAllowed'; readonly refusal: RefusalView }
  /** Polars refused, with this reason. */
  | { readonly kind: 'polars'; readonly reason: string };

/** What running one step came to. */
export type OutcomeView =
  /** Polars accepts the step, and its table has these columns. */
  | { readonly status: 'valid'; readonly columns: readonly FieldView[] }
  /** The step was refused, here, for this reason. */
  | { readonly status: 'refused'; readonly place: PlaceView; readonly cause: CauseView }
  /** A step before it was refused, so it was not run. */
  | { readonly status: 'notRun'; readonly blockedBy: string };

/** What running one step reported. Mirrors `StepReportView` in crates/dageist-tauri/src/report.rs. */
export interface StepReportView {
  readonly id: string;
  /** Whether the step stays in a lazy plan; a step that does not collected the data it needs when it was built. */
  readonly lazy: boolean;
  readonly outcome: OutcomeView;
  /** The dtypes of every expression in the step's arguments, as far as they could be built. */
  readonly expressions: readonly ExpressionDtypesView[];
  readonly walk: WalkView;
}

/** Why text is not a document the engine reads. */
export type ReadProblemView =
  /** The text is not JSON by RFC 8259, or nests too deep. Lines and columns count from 1. */
  | {
      readonly kind: 'syntax';
      readonly line: number;
      readonly column: number;
      readonly reason: string;
    }
  /** The JSON is not a document, here, for this reason. */
  | { readonly kind: 'refused'; readonly place: PlaceView; readonly reason: string };

/**
 * What running a document came to: its steps reported one by one, or the
 * reason it could not be read. Mirrors `RunView` in
 * crates/dageist-tauri/src/report.rs.
 */
export type RunView =
  | {
      readonly status: 'run';
      /** The columns of the table the steps start from. */
      readonly source: readonly FieldView[];
      /** One report for each step, in the document's order. */
      readonly steps: readonly StepReportView[];
    }
  | { readonly status: 'unreadable'; readonly problem: ReadProblemView };

/** Mirrors `ViewWindow` in crates/dageist-tauri/src/windows.rs. */
export type WindowName = 'steps';

/** Every command the view can send to the host. */
export interface Host {
  /** Asks the core who it is. */
  readonly coreIdentity: () => Promise<IdentityView>;
  /** Fetches the catalogue of operations the step editor offers, with its English words. */
  readonly operationCatalogue: () => Promise<CatalogueView>;
  /** Reports what the node is connected to: the table its steps start from and every other input. */
  readonly nodeInputs: () => Promise<NodeView>;
  /**
   * Runs the step document `document`, written as JSON, on the node and
   * reports each of its steps. Text that is not a document is reported as
   * such, not rejected.
   */
  readonly runDocument: (document: string) => Promise<RunView>;
  /**
   * Gives the rows after the step `step` of `document`, at most `limit` of
   * them, as text. It rejects when the document cannot be read, the step is
   * not in it or was refused, or `limit` is over what a preview may hold.
   */
  readonly rowsAfterStep: (document: string, step: string, limit: number) => Promise<RowsView>;
  /** Opens a working window of `keys` whole keys of the sample table, held in memory. */
  readonly openSampleWindow: (keys: number) => Promise<TableView>;
  /** Opens the whole sample table where it is stored, without loading it. */
  readonly openSampleResult: () => Promise<TableView>;
  /** Fetches a block of the open table as the bytes of an Arrow IPC stream. */
  readonly fetchTableBlock: (request: BlockRequest) => Promise<ArrayBuffer>;
  /** Opens the window called `name`, or brings it forward when it is already open. */
  readonly openWindow: (name: WindowName) => Promise<void>;
  /**
   * Sends `message` to every other window of the application that listens to
   * `topic`. The window that sends it does not receive it.
   */
  readonly broadcast: (topic: string, message: unknown) => void;
  /**
   * Calls `listener` with every message another window sends on `topic`.
   * Returns a function that stops listening.
   */
  readonly listen: (topic: string, listener: (message: unknown) => void) => () => void;
}

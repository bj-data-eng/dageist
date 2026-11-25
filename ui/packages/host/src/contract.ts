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
  /** The column's type, as Polars prints it. */
  readonly dataType: string;
  /** Whether the column holds numbers, which are drawn against the right of a cell. */
  readonly numeric: boolean;
}

/** The figures of an opened table, written by the host to be shown as they are. Mirrors `FiguresView` in crates/dageist-tauri/src/sample_table.rs. */
export interface FiguresView {
  /** The row count and what it counts, such as `2,552 rows`. */
  readonly rows: string;
  /** The column count and what it counts, such as `300 columns`. */
  readonly columns: string;
  /** How long the host took to open the table, such as `12.3 ms`. */
  readonly opened: string;
}

/** Mirrors `TableView` in crates/dageist-tauri/src/sample_table.rs. */
export interface TableView {
  /** The number of rows, for placing them. A person is shown `figures.rows`. */
  readonly rows: number;
  readonly columns: readonly ColumnView[];
  readonly figures: FiguresView;
}

/** A block of rows and columns to fetch from the open table. */
export interface BlockRequest {
  readonly offset: number;
  readonly length: number;
  readonly columns: readonly string[];
}

/**
 * A block of the open table: the cells of the columns asked for, each written
 * by the host as text, with the numbers of the rows and a report of the block
 * also written by it. The host sends them as the frame in crates/dageist-tauri/src/block.rs.
 */
export interface BlockView {
  /** The number of each row, counting from 1, such as `1,234`. */
  readonly numbers: readonly string[];
  /** The block in a line, such as `7 rows × 5 columns · 1.6 KB · built in 850 µs`. */
  readonly report: string;
  /** The cells of each column asked for, in the order asked, a missing value as `null`. Each column has a cell for every number. */
  readonly columns: ReadonlyArray<ReadonlyArray<string | null>>;
}

/**
 * What a parameter holds, mirroring `KindView` in
 * crates/dageist-tauri/src/catalogue.rs. Each kind has one drawing.
 */
export type KindView =
  /** A chain. What may stand in it is the engine's to say through its offers. */
  | { readonly kind: 'expression' }
  | { readonly kind: 'column' }
  /** A literal typed in; `type` is the type of literal it takes. */
  | { readonly kind: 'value'; readonly type: ValueTypeView }
  | { readonly kind: 'flag' }
  | { readonly kind: 'choice'; readonly words: readonly ChoiceView[] }
  | { readonly kind: 'dtype' }
  | { readonly kind: 'duration' }
  /**
   * A regular expression, as Polars reads one. `plainTextFlag` names the flag of the same entry
   * that makes the text plain text where it is true, such as `literal`, or is `null`.
   */
  | { readonly kind: 'pattern'; readonly plainTextFlag: string | null }
  /**
   * A time zone, as Polars reads one. `none` is set where the parameter may also be given no zone at
   * all, which is Python's `None`: the word for it and what it does, as the language file gives them.
   */
  | { readonly kind: 'timeZone'; readonly none: NoneView | null }
  | { readonly kind: 'table' }
  | { readonly kind: 'path' }
  | { readonly kind: 'pairs'; readonly key: KindView; readonly value: KindView };

/** The types of literal a parameter takes, mirroring `ValueType` in the engine's catalogue. */
export const VALUE_TYPES = ['integer', 'float', 'string', 'date', 'any'] as const;

/** The type of a literal typed in. */
export type ValueTypeView = (typeof VALUE_TYPES)[number];

/** The word for none and what it does, for a parameter that may be given none. */
export interface NoneView {
  readonly shown: string;
  readonly hint: string;
}

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

/** The kinds of setting the engine reports, mirroring `Setting` in the engine's catalogue. */
export const SETTING_KINDS = [
  'unset',
  'flag',
  'integer',
  'float',
  'text',
  'word',
  'field',
  'element',
  'comparison',
  'list',
  'pairs',
] as const;

/**
 * What a parameter holds, in the one shape every kind of parameter is drawn from. The engine
 * decides every setting it reports, such as what a parameter holds when it is not given. A number
 * or text comes with the words the engine shows it as. Mirrors `SettingView` in crates/dageist-tauri/src/catalogue.rs.
 */
export type SettingView =
  /** Still to be given. */
  | { readonly kind: 'unset' }
  | { readonly kind: 'flag'; readonly value: boolean }
  | { readonly kind: 'integer'; readonly value: number; readonly shown: string }
  /** A number with a fraction, which stays one though it is whole. */
  | { readonly kind: 'float'; readonly value: number; readonly shown: string }
  /** Text that is typed: a string, a duration or a path. */
  | { readonly kind: 'text'; readonly value: string; readonly shown: string }
  /** A name picked from a list: a choice's word, a dtype, a column or a table. */
  | { readonly kind: 'word'; readonly value: string }
  /** An expression that reads this column. */
  | { readonly kind: 'field'; readonly column: string }
  /** An expression that starts from `polars.element` and calls this method, named without its owner. */
  | { readonly kind: 'element'; readonly method: string }
  /** A comparison of a field with something else, by the entry that compares, such as `Expr.eq`. */
  | {
      readonly kind: 'comparison';
      readonly entry: string;
      readonly field: SettingView;
      readonly other: SettingView;
    }
  | { readonly kind: 'list'; readonly items: readonly SettingView[] }
  | { readonly kind: 'pairs'; readonly entries: readonly PairView[] };

/** One entry of pairs. */
export interface PairView {
  readonly key: SettingView;
  readonly value: SettingView;
}

/**
 * A position that holds an expression, as the engine's tables name it: of a
 * step, or inside an expression. Mirrors `PositionView` in
 * crates/dageist-tauri/src/catalogue.rs.
 */
export type PositionView =
  | { readonly kind: 'outer'; readonly name: OuterPosition }
  | { readonly kind: 'nested'; readonly name: NestedPosition };

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
  /**
   * What the parameter holds when it is not given, as a setting, or `null` where it must be
   * given, Polars decides or the default is not a value.
   */
  readonly defaultSetting: SettingView | null;
  /**
   * Where the expression the parameter holds stands, when a named position
   * holds it; the engine's tables are asked at that place.
   */
  readonly position: PositionView | null;
  /**
   * Whether the editor offers the parameter among a method's options. One that is not is in
   * Python's signature and changes nothing here.
   */
  readonly offered: boolean;
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
  /** Whether it works on a lazy frame without a collect. */
  readonly lazy: boolean;
  readonly parameters: readonly ParameterView[];
}

/** A family: its key and the words shown for it. */
export interface WordsView {
  readonly key: string;
  readonly title: string;
}

/**
 * A comparison of a field with a value: the entry that makes it, such as `Expr.eq`, and its sign
 * as the language file writes it. Mirrors `ComparisonView` in crates/dageist-tauri/src/catalogue.rs.
 */
export interface ComparisonView {
  readonly entry: string;
  readonly sign: string;
}

/** Mirrors `CatalogueView` in crates/dageist-tauri/src/catalogue.rs. */
export interface CatalogueView {
  /** Every family, in the order they are offered. */
  readonly families: readonly WordsView[];
  /** The dtypes a document names by one word, in the order a list gives them. */
  readonly listedDtypes: readonly string[];
  /** The comparisons a test makes, in the order they are offered. */
  readonly comparisons: readonly ComparisonView[];
  /** Every entry, ordered by identifier. */
  readonly entries: readonly EntryView[];
}

/** A column and the short name of its dtype, such as `Datetime(us)`. Mirrors `FieldView` in crates/dageist-tauri/src/report.rs. */
export interface FieldView {
  readonly name: string;
  readonly dtype: string;
}

/**
 * What supplies an input to a node. Mirrors `OriginView` in
 * crates/dageist-tauri/src/node.rs.
 */
export type OriginView =
  /** The table a node gives, by the node's name. */
  | { readonly kind: 'node'; readonly name: string }
  /** One column of the table a node gives. */
  | { readonly kind: 'column'; readonly node: string; readonly column: string }
  /** A value set on the graph itself, which no node gives. */
  | { readonly kind: 'parameter' };

/**
 * The first values of a column or a list, each written by the engine as text
 * and a null as `null`, and how many more follow them. Mirrors `ValuesView` in
 * crates/dageist-tauri/src/report.rs.
 */
export interface ValuesView {
  readonly shown: ReadonlyArray<string | null>;
  readonly more: number;
}

/** The table a node's steps start from. Mirrors `SourceView` in crates/dageist-tauri/src/node.rs. */
export interface SourceView {
  readonly name: string;
  readonly from: OriginView;
  readonly columns: readonly FieldView[];
}

/** One input of a node, by the name a step document uses for it. Mirrors `InputView` in crates/dageist-tauri/src/node.rs. */
export type InputView =
  | {
      readonly kind: 'table';
      readonly name: string;
      readonly from: OriginView;
      readonly columns: readonly FieldView[];
    }
  /** A single value, as the engine writes it, or `null` where it is null. */
  | {
      readonly kind: 'value';
      readonly name: string;
      readonly from: OriginView;
      readonly dtype: string;
      readonly value: string | null;
    }
  | {
      readonly kind: 'list';
      readonly name: string;
      readonly from: OriginView;
      readonly dtype: string;
      readonly values: ValuesView;
    };

/** What a node of the graph is. Mirrors `KindView` in crates/dageist-tauri/src/graph.rs. */
export type NodeKind = 'source' | 'transform' | 'output' | 'parameters';

/** What an input takes. Mirrors `TakesView` in crates/dageist-tauri/src/graph.rs. */
export type Takes = 'table' | 'value' | 'list';

/** A place on a node a reference fills. Mirrors `SlotView` in crates/dageist-tauri/src/graph.rs. */
export type SlotView =
  /** The table a transform's steps start from. */
  | { readonly kind: 'start' }
  /** A named input of a transform. */
  | { readonly kind: 'input'; readonly name: string }
  /** The table an output writes. */
  | { readonly kind: 'written' };

/** What a node reads. Mirrors `ReferenceView` in crates/dageist-tauri/src/graph.rs. */
export type ReferenceView =
  /** The table a node gives. */
  | { readonly kind: 'table'; readonly node: string }
  /** The values of one column of the table a node gives. */
  | { readonly kind: 'column'; readonly node: string; readonly column: string }
  /** One value of a Parameters node. */
  | { readonly kind: 'parameter'; readonly node: string; readonly name: string };

/**
 * An output a slot can be filled from: a node's table, whole or by one of its columns, or one
 * value. Mirrors `FitView` in crates/dageist-tauri/src/graph.rs.
 */
export type GraphFitView =
  | { readonly kind: 'table'; readonly node: string }
  | { readonly kind: 'parameter'; readonly node: string; readonly name: string };

/** One slot of a node. Mirrors `SlotStateView` in crates/dageist-tauri/src/graph.rs. */
export interface SlotStateView {
  readonly slot: SlotView;
  readonly takes: Takes;
  /** What fills it, or `null` while it is not connected. */
  readonly from: ReferenceView | null;
  /** Every output that could fill it without closing a cycle. */
  readonly fits: readonly GraphFitView[];
}

/** A file a source reads or an output writes. Mirrors `FileView` in crates/dageist-tauri/src/graph.rs. */
export interface FileView {
  readonly path: string;
  readonly format: 'parquet' | 'csv';
}

/** One value of a Parameters node. Mirrors `ParameterView` in crates/dageist-tauri/src/graph.rs. */
export interface GraphParameterView {
  readonly name: string;
  readonly dtype: string;
  readonly text: string;
}

/**
 * Where a node's box stands and, once resized, its size, in canvas units. Mirrors `FrameView` in
 * crates/dageist-tauri/src/graph.rs.
 */
export interface FrameView {
  readonly x: number;
  readonly y: number;
  readonly width: number | null;
  readonly height: number | null;
}

/** One node as the canvas draws it. Mirrors `NodeBoxView` in crates/dageist-tauri/src/graph.rs. */
export interface NodeBoxView {
  readonly id: string;
  readonly name: string;
  readonly kind: NodeKind;
  /** What the node is set to, in a few words. */
  readonly summary: string;
  readonly slots: readonly SlotStateView[];
  /** For a transform, what may feed a new input; `null` for other kinds. */
  readonly newInput: readonly GraphFitView[] | null;
  /** The columns of the table the node gives or would write, or `null` while they are not known. */
  readonly columns: readonly FieldView[] | null;
  /** A Parameters node's values; empty for other kinds. */
  readonly parameters: readonly GraphParameterView[];
  readonly file: FileView | null;
  /** What keeps the node from being worked out, in words. */
  readonly problems: readonly string[];
  /** Where its box stands, or `null` while it has not been placed. */
  readonly frame: FrameView | null;
}

/** The graph as the canvas draws it. Mirrors `GraphView` in crates/dageist-tauri/src/graph.rs. */
export interface GraphView {
  readonly revision: number;
  readonly layoutRevision: number;
  /** Every node, each after the nodes it reads. */
  readonly nodes: readonly NodeBoxView[];
  /** The dtypes a parameter may have. */
  readonly parameterDtypes: readonly string[];
}

/** One edit of the graph. Mirrors `GraphEditRequest` in crates/dageist-tauri/src/graph.rs. */
export type GraphEdit =
  | {
      readonly edit: 'add';
      readonly kind: NodeKind;
      readonly follows: string | null;
      readonly name: string | null;
    }
  | { readonly edit: 'remove'; readonly node: string }
  | { readonly edit: 'rename'; readonly node: string; readonly name: string }
  | { readonly edit: 'setFile'; readonly node: string; readonly file: FileView | null }
  | {
      readonly edit: 'connect';
      readonly node: string;
      readonly slot: SlotView;
      readonly from: ReferenceView;
    }
  | { readonly edit: 'connectNew'; readonly node: string; readonly from: ReferenceView }
  | { readonly edit: 'disconnect'; readonly node: string; readonly slot: SlotView }
  | {
      readonly edit: 'addInput';
      readonly node: string;
      readonly name: string;
      readonly takes: Takes;
    }
  | { readonly edit: 'removeInput'; readonly node: string; readonly name: string }
  | {
      readonly edit: 'setParameter';
      readonly node: string;
      readonly name: string;
      readonly dtype: string;
      readonly text: string;
    }
  | { readonly edit: 'removeParameter'; readonly node: string; readonly name: string };

/** What an edit of the graph came to. Mirrors `GraphEditAnswerView` in crates/dageist-tauri/src/graph.rs. */
export type GraphEditAnswerView =
  /** The edit was made: the graph at its new revision, and the node it added, if any. */
  | { readonly kind: 'edited'; readonly graph: GraphView; readonly added: string | null }
  /** The graph refused the edit and is still at `revision`. */
  | { readonly kind: 'refused'; readonly revision: number; readonly message: string };

/** The revision the layout is at after a box was placed. Mirrors `PlacedView` in crates/dageist-tauri/src/graph.rs. */
export interface PlacedView {
  readonly layoutRevision: number;
}

/** What a file is chosen for. Mirrors `Purpose` in crates/dageist-tauri/src/dialog.rs. */
export type FilePurpose = 'open' | 'save';

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

/**
 * A place an edit is asked at: the step by its position and its identifier, which the host checks
 * against each other, then the way down through its arguments. An edit works inside a step, so
 * neither is absent. Mirrors `EditPlace` in crates/dageist-tauri/src/report.rs.
 */
export interface EditPlace {
  /** Where the step is in the document, counted from zero. */
  readonly stepIndex: number;
  readonly stepId: string;
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

/**
 * How many values a chain gives, mirroring `Shape` in the engine: `one` as an aggregate does, `many` as
 * the table has rows, or `counted`, another number that the data or the arguments decide.
 */
export const VALUE_SHAPES = ['one', 'many', 'counted'] as const;

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
  'duration-is-valid',
  'element-dtype-is-implemented',
  'element-dtype-is-measured',
  'element-only-in-pivot-aggregate',
  'filter-predicate-is-boolean',
  'folded-function-is-no-join-key',
  'join-key-names-are-not-repeated',
  'key-is-elementwise',
  'moments-share-a-zone',
  'parameter-takes-one-value',
  'pattern-is-valid',
  'range-is-valid',
  'replacement-values-match-in-number',
  'time-zone-is-valid',
  'values-meet-in-count',
  'window-input-is-not-counted',
  'window-input-is-not-widened',
  'arrays-meet-at-one-width',
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

/** How a step changed a column of the table it was given, mirroring `Change` in the engine. */
export const CHANGES = ['added', 'replaced'] as const;

/** How a step changed a column: `added` where the table before it lacked the column, `replaced` where it had it and the step wrote it or gave it another dtype. */
export type ChangeView = (typeof CHANGES)[number];

/** A column a step added or replaced. */
export interface ColumnChangeView {
  readonly name: string;
  readonly change: ChangeView;
}

/** The first values of one column a step changed. */
export interface ColumnValuesView {
  readonly name: string;
  readonly values: ValuesView;
}

/** What running a step on the rows of the working window came to. */
export type CountView =
  /** The step was not run on rows: the run read none, or a step before it was refused or incomplete, or it was itself. */
  | { readonly kind: 'notCounted' }
  /** Polars accepts the step, and it was not run on the rows because the step `failed` before it could not be. */
  | { readonly kind: 'notReached'; readonly failed: string }
  /** The step was given `received` rows and kept `kept`, with the first values of each column it changed. */
  | {
      readonly kind: 'counted';
      readonly received: number;
      readonly kept: number;
      readonly first: readonly ColumnValuesView[];
    }
  /** Polars accepted the step and could not run it on the rows, for this reason. */
  | { readonly kind: 'failed'; readonly reason: string };

/** What running one step came to. */
export type OutcomeView =
  /** Polars accepts the step, and its table has these columns, of which the step added or replaced `changes`. */
  | {
      readonly status: 'valid';
      readonly columns: readonly FieldView[];
      readonly changes: readonly ColumnChangeView[];
    }
  /** The step was refused, here, for this reason. */
  | { readonly status: 'refused'; readonly place: PlaceView; readonly cause: CauseView }
  /**
   * The step holds something still to be picked, at this place, which is the
   * start of the expression that is a hole. It was walked up to it and not
   * built.
   */
  | { readonly status: 'incomplete'; readonly place: PlaceView }
  /** A step before it was refused or incomplete, so it was not run. */
  | { readonly status: 'notRun'; readonly blockedBy: string };

/** What running one step reported. Mirrors `StepReportView` in crates/dageist-tauri/src/report.rs. */
export interface StepReportView {
  readonly id: string;
  /** What the step works on, in a few words written by the engine, such as `by dcn, timestamp`. */
  readonly summary: string;
  /** Whether the step stays in a lazy plan; a step that does not collected the data it needs when it was built. */
  readonly lazy: boolean;
  readonly outcome: OutcomeView;
  /** How many rows the step kept of those it received, and the first values of what it changed, where the engine ran the rows. */
  readonly count: CountView;
  /** The dtypes of every expression in the step's arguments, as far as they could be built. */
  readonly expressions: readonly ExpressionDtypesView[];
  readonly walk: WalkView;
  /**
   * The places of the step's arguments that are what their parameter holds when it is not given:
   * its own, and those of the calls inside its expressions. The engine decides which, and says
   * so whether or not the step ran.
   */
  readonly atDefault: readonly PlaceView[];
}

/**
 * What running a document came to: its steps reported one by one. Mirrors `RunView` in
 * crates/dageist-tauri/src/report.rs.
 */
export interface RunView {
  /** The columns of the table the steps start from. */
  readonly source: readonly FieldView[];
  /** One report for each step, in the document's order. */
  readonly steps: readonly StepReportView[];
}

/** One call of the document: the entry of the catalogue it uses, and the arguments it was given. */
export interface CallView {
  /** The identifier the catalogue gives the entry, such as `Expr.shift`. */
  readonly entry: string;
  /** The arguments, each by Python's name for its parameter, in the order they were given. */
  readonly arguments: readonly GivenView[];
}

/** The argument given for one parameter of a call. */
export interface GivenView {
  readonly parameter: string;
  readonly value: ArgumentView;
}

/** Where an expression starts. */
export type StartView =
  /** A call of a function that makes an expression, or `polars.when`. */
  | { readonly kind: 'call'; readonly call: CallView }
  /** A connected value or list, by name. */
  | { readonly kind: 'input'; readonly name: string }
  /** Nothing picked yet. */
  | { readonly kind: 'hole' };

/** One entry of pairs. */
export interface PairsEntryView {
  readonly key: ArgumentView;
  readonly value: ArgumentView;
}

/**
 * What an argument is, as the document holds it. A number is not sent as a number but as the
 * words the engine shows it as, so that the view draws it and formats nothing. Mirrors
 * `ArgumentView` in crates/dageist-tauri/src/document.rs.
 */
export type ArgumentView =
  /** A chain: where it starts, then the links after it. */
  | {
      readonly kind: 'expression';
      readonly start: StartView;
      readonly links: readonly CallView[];
    }
  | { readonly kind: 'columns'; readonly names: readonly string[] }
  | { readonly kind: 'integer'; readonly shown: string }
  | { readonly kind: 'float'; readonly shown: string }
  | { readonly kind: 'string'; readonly value: string }
  /** True, false, or unset, which leaves the choice to Polars. */
  | { readonly kind: 'flag'; readonly value: boolean | null }
  | { readonly kind: 'choice'; readonly word: string }
  | { readonly kind: 'dtype'; readonly name: string }
  | { readonly kind: 'duration'; readonly text: string }
  | { readonly kind: 'pattern'; readonly text: string }
  | { readonly kind: 'timeZone'; readonly zone: string | null }
  | { readonly kind: 'table'; readonly name: string }
  | { readonly kind: 'path'; readonly text: string }
  | { readonly kind: 'pairs'; readonly entries: readonly PairsEntryView[] }
  | { readonly kind: 'many'; readonly items: readonly ArgumentView[] };

/** One step of the document: its name, and the call it makes. */
export interface DocumentStepView {
  readonly id: string;
  readonly call: CallView;
}

/** The steps of a document, in the order they run. */
export interface DocumentView {
  readonly steps: readonly DocumentStepView[];
}

/**
 * What a window needs to draw the node's steps: the document the host holds, the revision it is
 * at, and what running it reported. Mirrors `DocumentStateView` in
 * crates/dageist-tauri/src/document.rs.
 */
export interface DocumentStateView {
  /** Goes up by one with each edit, from the milliseconds since the Unix epoch when the host started. An edit carries the revision it was made against. */
  readonly revision: number;
  readonly document: DocumentView;
  readonly run: RunView;
}

/** What a value is, by the engine's word for it. Mirrors `ValueKind` in the engine. */
export const VALUE_KINDS = [
  'expression',
  'columns',
  'integer',
  'float',
  'string',
  'flag',
  'choice',
  'dtype',
  'duration',
  'pattern',
  'timeZone',
  'table',
  'path',
  'pairs',
  'list',
] as const;

/** What an argument is, in the words of a refusal. */
export type ValueKindView = (typeof VALUE_KINDS)[number];

/** The kinds of thing a place can name, other than an argument. Mirrors `Thing` in the engine. */
export const THING_KINDS = [
  'step',
  'call',
  'parameter',
  'item',
  'key',
  'entryValue',
  'start',
  'link',
  'expressionWithoutCall',
  'value',
] as const;

/** What a place names, for a refusal that says what was there. */
export type ThingView =
  | {
      readonly kind: Exclude<(typeof THING_KINDS)[number], 'value'>;
    }
  /** An argument of this kind. */
  | { readonly kind: 'value'; readonly of: ValueKindView };

/** What an edit needs the place to name. Mirrors `Wanted` in the engine. */
export const WANTED = [
  'call',
  'argument',
  'parameter',
  'item',
  'entryPart',
  'link',
  'expression',
  'list',
  'pairs',
  'partOfStep',
] as const;

/** What an edit needs the place it is made at to name. */
export type WantedView = (typeof WANTED)[number];

/** The kinds of reason the engine has for refusing an edit. Mirrors `EditProblem` in the engine. */
export const PROBLEM_KINDS = [
  'noStep',
  'stepChanged',
  'nothing',
  'doesNotFit',
  'wrongPlace',
  'notPerItem',
  'noDefault',
  'outOfRange',
  'unknownEntry',
  'noStart',
  'cannotAdd',
  'needsValue',
  'refused',
] as const;

/** Why the engine refused an edit. */
export type ProblemView =
  /** The place names a step the document does not have. */
  | { readonly kind: 'noStep'; readonly steps: number }
  /** The place names a step by an identifier the step at its number does not have. */
  | { readonly kind: 'stepChanged'; readonly found: string }
  /** Nothing is at the place. */
  | { readonly kind: 'nothing' }
  /** What is there has no such part. */
  | { readonly kind: 'doesNotFit'; readonly found: ThingView }
  /** The place names another kind of thing than the edit needs. */
  | { readonly kind: 'wrongPlace'; readonly needs: WantedView; readonly found: ThingView }
  /** The parameter holds no value for each item of another. */
  | { readonly kind: 'notPerItem' }
  /** The parameter has no default for the items it has no value for. */
  | { readonly kind: 'noDefault' }
  /** A position past the end. */
  | { readonly kind: 'outOfRange'; readonly index: number; readonly len: number }
  /** An entry the catalogue lacks. */
  | { readonly kind: 'unknownEntry'; readonly entry: string }
  /** An entry that starts no step. */
  | { readonly kind: 'noStart'; readonly entry: string }
  /** An option that Polars refuses every other word of, for this reason. */
  | { readonly kind: 'cannotAdd'; readonly reason: string }
  /** An option that starts with nothing a document holds: it needs its value. */
  | { readonly kind: 'needsValue'; readonly parameter: string }
  /** The model refuses the result, for this reason. */
  | { readonly kind: 'refused'; readonly reason: string };

/**
 * What an edit came to. Mirrors `EditAnswerView` in crates/dageist-tauri/src/document.rs.
 */
export type EditAnswerView =
  /** The edit was made: the document as it is now, at its new revision. */
  | ({ readonly status: 'edited' } & DocumentStateView)
  /** The engine refused the edit, here, for this reason. The document is as it was. */
  | {
      readonly status: 'refused';
      /** The revision the document stays at. */
      readonly revision: number;
      readonly place: PlaceView;
      readonly problem: ProblemView;
      /** The engine's words for the refusal. */
      readonly message: string;
    };

/** A number or text that an expression is made of, as an edit sends it. */
export type LiteralRequest =
  | { readonly kind: 'integer'; readonly value: number }
  | { readonly kind: 'float'; readonly value: number }
  | { readonly kind: 'text'; readonly value: string };

/** An expression of one word, which an edit puts where an expression stands. */
export type ExpressionRequest =
  | { readonly kind: 'hole' }
  | { readonly kind: 'field'; readonly name: string }
  | { readonly kind: 'input'; readonly name: string }
  | { readonly kind: 'literal'; readonly literal: LiteralRequest };

/** What an expression starts from, for an edit that changes only that. */
export type StartRequest =
  | { readonly kind: 'hole' }
  | { readonly kind: 'input'; readonly name: string }
  | { readonly kind: 'field'; readonly name: string }
  | { readonly kind: 'literal'; readonly literal: LiteralRequest };

/**
 * An argument an edit gives a parameter. Its kind is the kind of the parameter it is for, as the
 * catalogue gives it; the model refuses an argument of another kind.
 */
export type ValueRequest =
  | { readonly kind: 'expression'; readonly expression: ExpressionRequest }
  | { readonly kind: 'columns'; readonly names: readonly string[] }
  | { readonly kind: 'integer'; readonly value: number }
  | { readonly kind: 'float'; readonly value: number }
  | { readonly kind: 'string'; readonly value: string }
  | { readonly kind: 'flag'; readonly value: boolean | null }
  | { readonly kind: 'choice'; readonly word: string }
  | { readonly kind: 'dtype'; readonly name: string }
  | { readonly kind: 'duration'; readonly text: string }
  | { readonly kind: 'pattern'; readonly text: string }
  | { readonly kind: 'timeZone'; readonly zone: string | null }
  | { readonly kind: 'table'; readonly name: string }
  | { readonly kind: 'path'; readonly text: string }
  | {
      readonly kind: 'pairs';
      readonly entries: ReadonlyArray<{ readonly key: ValueRequest; readonly value: ValueRequest }>;
    }
  | { readonly kind: 'many'; readonly items: readonly ValueRequest[] };

/** The table a new step starts on, and the tables it can bring in. */
export interface FrameRequest {
  readonly columns: readonly string[];
  readonly tables: readonly string[];
}

/**
 * One edit of the document, which the engine makes. The places are those the report gives. Mirrors
 * `EditRequest` in crates/dageist-tauri/src/document.rs.
 */
export type EditRequest =
  | {
      readonly kind: 'addStep';
      readonly at: number;
      readonly entry: string;
      readonly frame: FrameRequest;
    }
  | {
      readonly kind: 'swapStep';
      readonly step: EditPlace;
      readonly entry: string;
      readonly frame: FrameRequest;
    }
  | { readonly kind: 'removeStep'; readonly step: EditPlace }
  | { readonly kind: 'moveStep'; readonly step: EditPlace; readonly to: number }
  | { readonly kind: 'setArgument'; readonly place: EditPlace; readonly value: ValueRequest }
  | { readonly kind: 'clearArgument'; readonly place: EditPlace }
  | { readonly kind: 'addOption'; readonly call: EditPlace; readonly parameter: string }
  | {
      readonly kind: 'setForItem';
      readonly call: EditPlace;
      readonly parameter: string;
      readonly item: number;
      readonly value: ValueRequest;
    }
  | {
      readonly kind: 'insertItem';
      readonly list: EditPlace;
      readonly at: number;
      readonly item: ValueRequest;
    }
  | { readonly kind: 'removeItem'; readonly item: EditPlace }
  | { readonly kind: 'insertTest'; readonly list: EditPlace; readonly at: number }
  | {
      readonly kind: 'insertEntry';
      readonly pairs: EditPlace;
      readonly at: number;
      readonly key: ValueRequest;
      readonly value: ValueRequest;
    }
  | { readonly kind: 'removeEntry'; readonly entry: EditPlace }
  | { readonly kind: 'setStart'; readonly expression: EditPlace; readonly start: StartRequest }
  | { readonly kind: 'startWith'; readonly expression: EditPlace; readonly entry: string }
  | {
      readonly kind: 'insertLink';
      readonly expression: EditPlace;
      readonly at: number;
      readonly entry: string;
      readonly standing: PlaceRequest | null;
    }
  | {
      readonly kind: 'swapLink';
      readonly link: EditPlace;
      readonly entry: string;
      readonly standing: PlaceRequest | null;
    }
  | { readonly kind: 'removeLink'; readonly link: EditPlace }
  | { readonly kind: 'moveLink'; readonly link: EditPlace; readonly to: number };

/**
 * Where a call would stand, as a request names it: the positions that hold
 * the expression, outermost first, and what it has so far. The engine works
 * out the context from the positions. A walk's `OfferPlaceView` is the same
 * place, whose context is its outer position and the nested one if it has one.
 */
export interface PlaceRequest {
  readonly positions: readonly PositionView[];
  readonly after: AfterView;
}

/**
 * A number or text the engine reads from what is typed, as a step document writes it, with the
 * words the engine shows it as: a whole number as its digits, a number with a fraction as Python
 * writes it, and text as it is, which the view draws between quotation marks of its own. Mirrors
 * `LiteralView` in crates/dageist-tauri/src/offers.rs.
 */
export type LiteralView =
  | { readonly kind: 'integer'; readonly value: number; readonly shown: string }
  | { readonly kind: 'float'; readonly value: number; readonly shown: string }
  | { readonly kind: 'text'; readonly value: string; readonly shown: string };

/**
 * A value a list holds, which may be picked for a slot: the short name of the dtype of a field or
 * an input. What is typed is read by the engine instead, which `readTyped` asks.
 */
export type CandidateRequest =
  | { readonly kind: 'field'; readonly dtype: string }
  | { readonly kind: 'input'; readonly dtype: string; readonly list: boolean };

/** The argument a candidate would be given: a parameter of a method that stands at a place. */
export interface ArgumentRequest {
  /** The method, by the identifier the catalogue gives it. */
  readonly entry: string;
  /** The parameter, by Python's name. */
  readonly parameter: string;
  /** Where the method stands. */
  readonly place: PlaceRequest;
}

/** Which candidates are asked about, and where they would be given. */
export interface FitsRequest {
  /** The positions that hold the expression a candidate would start, or `null` when it is only an argument. */
  readonly start: readonly PositionView[] | null;
  /** The argument a candidate would be given, or `null` when it only starts an expression. */
  readonly argument: ArgumentRequest | null;
  readonly candidates: readonly CandidateRequest[];
}

/**
 * What a text is typed into, as the catalogue writes the kind of a parameter that takes a typed
 * value: an expression, a literal of a type, a duration, a pattern, a time zone or a path. The engine reads the text as
 * that slot takes it.
 */
export type TypedKind =
  | { readonly kind: 'expression' }
  | { readonly kind: 'value'; readonly type: ValueTypeView }
  | { readonly kind: 'duration' }
  | { readonly kind: 'pattern' }
  | { readonly kind: 'timeZone' }
  | { readonly kind: 'path' };

/**
 * A text typed into a slot, to be read by the engine. `start` and `argument` name the slot an
 * expression stands in, as for the fits of candidates, and the engine says what the tables
 * allow of each reading there; where neither is given, the text is only read.
 */
export interface TypedRequest {
  readonly text: string;
  readonly typed: TypedKind;
  readonly start: readonly PositionView[] | null;
  readonly argument: ArgumentRequest | null;
}

/** An option of an entry that is already chosen. */
export interface ChosenOption {
  /** The parameter, by Python's name. */
  readonly parameter: string;
  /** The word as the option table writes it: `true`, `false`, `unset`, or the word of a choice. */
  readonly word: string;
}

/** What a method is given for its first parameter when it is added, where the tables allow Polars' default there. */
export interface DefaultArgumentView {
  readonly parameter: string;
  readonly literal: LiteralView;
}

/** An entry and whether it may stand at a place. Mirrors `EntryOfferView` in crates/dageist-tauri/src/offers.rs. */
export interface EntryOfferView {
  /** The identifier the catalogue gives the entry. */
  readonly entry: string;
  readonly verdict: VerdictView;
  /** What a method that may stand is given when it is added, or `null`. */
  readonly argument: DefaultArgumentView | null;
  /**
   * The first parameter a method that may stand starts without giving, which then holds what
   * Polars gives it when it is not given, or `null`. The method is complete as it starts.
   */
  readonly leftOut: string | null;
  /**
   * The parameters a method that may stand starts with as slots to fill, besides its argument:
   * those a step must give and Polars gives no default, such as a pattern. They are options the
   * method starts with already set to what is still to be given.
   */
  readonly slots: readonly string[];
}

/** Every call that stands in an expression, offered or refused at a place. Mirrors `OffersView` in crates/dageist-tauri/src/offers.rs. */
export interface OffersView {
  /** The place that was asked about, as the engine took it. */
  readonly place: OfferPlaceView;
  /** One for each entry that stands in an expression, ordered by identifier. */
  readonly entries: readonly EntryOfferView[];
}

/** What the tables say of a candidate in a slot. */
export type FitView =
  /** The candidate has this class of dtype, and the tables judged it. */
  | { readonly kind: 'judged'; readonly dtype: string; readonly verdict: VerdictView }
  /** The candidate has no class of dtype the tables cover. */
  | { readonly kind: 'unjudged'; readonly reason: string };

/** What the tables say of each candidate, in the order they were asked. Mirrors `FitsView` in crates/dageist-tauri/src/offers.rs. */
export interface FitsView {
  readonly fits: readonly FitView[];
}

/** One way a typed text reads. */
export type ReadingView =
  /**
   * The text reads as `literal`, which has the dtype `dtype` where Polars gives it one. `fit` is
   * what the tables say of it in the slot that was asked about, or `null` where none was. The
   * slot judges the literal the document would hold, which is text where the document holds no
   * date, so a date has the `dtype` `Date` and is judged as the `String` that carries it.
   */
  | {
      readonly kind: 'read';
      readonly literal: LiteralView;
      readonly dtype: string | null;
      readonly fit: FitView | null;
    }
  /** The text is not what it was typed into, for this reason. */
  | { readonly kind: 'unread'; readonly reason: string };

/**
 * How a typed text reads, each way in the order the slot offers it; none for a text that was not
 * typed. Mirrors `ReadingsView` in crates/dageist-tauri/src/offers.rs.
 */
export interface ReadingsView {
  readonly readings: readonly ReadingView[];
}

/** One word an option may be set to, and why Polars refuses it, if it does. */
export interface OptionWordView {
  readonly word: string;
  readonly refusal: RefusalView | null;
}

/** An option of an entry: its default, and the words it may be set to. */
export interface OptionView {
  readonly parameter: string;
  /** The word the option has when it is not set. */
  readonly default: string | null;
  /**
   * The word the option starts with when it is added: the first that is not the default and that
   * Polars allows beside the options chosen, or `null` where there is none.
   */
  readonly starter: string | null;
  /** Why the option cannot be added, where it has no starter: the refusal that stops it. */
  readonly stuck: RefusalView | null;
  readonly words: readonly OptionWordView[];
}

/** The flags and choices of an entry. Mirrors `OptionsView` in crates/dageist-tauri/src/offers.rs. */
export interface OptionsView {
  readonly options: readonly OptionView[];
}

/** The table a new step works on, and the tables connected to the node. */
export interface StepStartsRequest {
  /** The names of the columns of the table, in its order. */
  readonly columns: readonly string[];
  /** The names of the connected tables, in the order the first is taken from. */
  readonly tables: readonly string[];
}

/** An entry of the list that starts a step. Mirrors `StepStartView` in crates/dageist-tauri/src/offers.rs. */
export interface StepStartView {
  /** The identifier the catalogue gives the entry. */
  readonly entry: string;
}

/** Which steps of the list can be added to a table. Mirrors `StepStartsView` in crates/dageist-tauri/src/offers.rs. */
export interface StepStartsView {
  /** One for each step that can, ordered by identifier. */
  readonly starts: readonly StepStartView[];
}

/**
 * What a failed command ended in, mirroring `ErrorKind` in
 * crates/dageist-tauri/src/sample_table.rs. The view decides on the kind and
 * never on the message, which is for a person to read.
 */
export const ERROR_KINDS = [
  /** The document has no step of the name asked for. */
  'noSuchStep',
  /** A step before the one asked for was refused, so it was not run. */
  'notRun',
  /** The step asked for was refused. */
  'refused',
  /** More was asked for than the window of a command holds. */
  'limit',
  /** The document is at another revision than the one the request was made for. */
  'stale',
  /** The request names an entry or a parameter the catalogue lacks, or a way down to an expression that starts at no position of a step. */
  'unknownName',
  /** Anything else. */
  'failed',
] as const;

/** What a failed command ended in. */
export type ErrorKind = (typeof ERROR_KINDS)[number];

/** Mirrors `ViewWindow` in crates/dageist-tauri/src/windows.rs. */
export type WindowName = 'steps';

/** Every command the view can send to the host. */
export interface Host {
  /** Asks the core who it is. */
  readonly coreIdentity: () => Promise<IdentityView>;
  /** Fetches the catalogue of operations the step editor offers, with its English words. */
  readonly operationCatalogue: () => Promise<CatalogueView>;
  /** Reports the graph, worked out: every node with its slots, columns and problems. */
  readonly graph: () => Promise<GraphView>;
  /**
   * Makes `edit` on the graph, which must be at `revision`, and reports the edited graph, or why
   * the graph refused the edit. It rejects with `stale` when the graph is at another revision.
   */
  readonly editGraph: (revision: number, edit: GraphEdit) => Promise<GraphEditAnswerView>;
  /**
   * Places the box of `node` on the layout, which must be at `revision`. It rejects with `stale`
   * when the layout is at another revision.
   */
  readonly placeNode: (revision: number, node: string, frame: FrameView) => Promise<PlacedView>;
  /** Opens the system's file dialog and reports the file chosen, or `null`. */
  readonly chooseFile: (purpose: FilePurpose) => Promise<FileView | null>;
  /** Opens the system's folder dialog and reports the folder chosen, or `null`. */
  readonly chooseFolder: () => Promise<string | null>;
  /**
   * Opens the workspace in `folder` in place of the one the host holds, and reports its graph.
   * Every view of the workspace let go is stale after it.
   */
  readonly openWorkspace: (folder: string) => Promise<GraphView>;
  /**
   * Reports what the transform `node` is connected to: the table its steps start from and every
   * other input.
   */
  readonly nodeInputs: (node: string) => Promise<NodeView>;
  /**
   * Reports the document of the transform `node`, the revision it is at, and what running it
   * reported.
   */
  readonly nodeDocument: (node: string) => Promise<DocumentStateView>;
  /**
   * Makes `edit` on the document of the transform `node`, which must be at `revision`, and
   * reports the edited document at its new revision, or why the engine refused the edit. It
   * rejects with `stale` when the document is at another revision.
   */
  readonly editDocument: (
    node: string,
    revision: number,
    edit: EditRequest,
  ) => Promise<EditAnswerView>;
  /**
   * Gives the rows after the step `step` of the document of the transform `node` at `revision`,
   * at most `limit` of them, as text. It rejects with `stale` when the document is at another
   * revision, when the step is not in it or was refused, or when `limit` is over what a preview
   * may hold.
   */
  readonly rowsAfterStep: (
    node: string,
    revision: number,
    step: string,
    limit: number,
  ) => Promise<RowsView>;
  /**
   * Asks which calls may stand at `place`: every entry that stands in an
   * expression, offered or refused, with whether the chain may end there.
   * It rejects with `unknownName` for a place the engine does not know.
   */
  readonly offersAt: (place: PlaceRequest) => Promise<OffersView>;
  /**
   * Asks which of the candidates fit a slot: as the start of an expression,
   * as the argument of a method, or both. It rejects with `unknownName` for an
   * entry, a parameter or a place the engine does not know.
   */
  readonly candidateFits: (request: FitsRequest) => Promise<FitsView>;
  /**
   * Asks how the engine reads a text typed into a slot: as a number, a date, a duration or text,
   * as the slot takes it, with the literal and the dtype each reading has or why the text is not
   * that. It rejects with `unknownName` for an entry, a parameter or a place the engine does not
   * know.
   */
  readonly readTyped: (request: TypedRequest) => Promise<ReadingsView>;
  /**
   * Asks what the flags and choices of the entry `entry` may be set to,
   * beside the options already `chosen`. It rejects with `unknownName` for an
   * entry or a parameter the catalogue lacks.
   */
  readonly optionOffers: (entry: string, chosen: readonly ChosenOption[]) => Promise<OptionsView>;
  /**
   * Asks what each step of the list starts with when it is added to a table with the columns
   * and the connected tables of `request`: the arguments the engine gives it.
   */
  readonly stepStarts: (request: StepStartsRequest) => Promise<StepStartsView>;
  /** Opens a working window of `keys` whole keys of the sample table, held in memory. */
  readonly openSampleWindow: (keys: number) => Promise<TableView>;
  /** Opens the whole sample table where it is stored, without loading it. */
  readonly openSampleResult: () => Promise<TableView>;
  /** Fetches a block of the open table, its cells written as text. */
  readonly fetchTableBlock: (request: BlockRequest) => Promise<BlockView>;
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

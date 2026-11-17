/**
 * @file The view's door to the host. Import the contract types from here and
 * obtain a host with `tauriHost`. A stand-in host seals and delivers the
 * messages between windows with the same envelope the Tauri host uses.
 */

export type {
  AfterView,
  BlockRequest,
  CatalogueView,
  CauseView,
  ChoiceView,
  ColumnView,
  ConditionStateView,
  ContextView,
  DefaultView,
  EndingView,
  EntryView,
  ExpressionDtypesView,
  FieldView,
  Host,
  IdentityView,
  InputView,
  KindView,
  NestedPosition,
  NodeView,
  OfferPlaceView,
  OuterPosition,
  OutcomeView,
  ParameterView,
  PlaceView,
  ReadProblemView,
  RefusalSourceView,
  RefusalView,
  ResolutionView,
  RowsView,
  RuleName,
  RunView,
  SegmentView,
  ShapeView,
  SourceView,
  StageView,
  StandingView,
  StepReportView,
  TableView,
  ValueShapeView,
  VerdictView,
  WalkRefusalView,
  WalkView,
  WalkedExpressionView,
  WindowName,
  WordsView,
} from './contract';
export { DecodeError } from './decode';
export { decodeCatalogue } from './decode-catalogue';
export { decodeNode, decodeRows } from './decode-node';
export { decodeRun } from './decode-run';
export { deliverMessage, sealMessage } from './envelope';
export { tauriHost } from './tauri';

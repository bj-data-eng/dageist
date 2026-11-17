/**
 * @file The view's door to the host. Import the contract types from here and
 * obtain a host with `tauriHost`, or with `bridgeHost` in a plain browser.
 * Both seal and deliver the messages between windows with the same envelope.
 */

export type {
  AfterView,
  AppliesView,
  BlockRequest,
  CatalogueView,
  CauseView,
  ChoiceView,
  ColumnView,
  ConditionStateView,
  ContextView,
  DefaultView,
  EndingView,
  ErrorKind,
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
  PositionView,
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
  ValueTypeView,
  VerdictView,
  WalkRefusalView,
  WalkView,
  WalkedExpressionView,
  WindowName,
  WordsView,
} from './contract';
export { BRIDGE_START_COMMAND, BridgeUnavailableError, bridgeHost } from './bridge';
export { ERROR_KINDS } from './contract';
export { DecodeError } from './decode';
export { decodeCatalogue } from './decode-catalogue';
export { decodeNode, decodeRows } from './decode-node';
export { decodeRun } from './decode-run';
export { decodeBytes, decodeIdentity, decodeTable } from './decode-sample';
export { deliverMessage, sealMessage } from './envelope';
export { HostError, decodeFailure } from './failure';
export { tauriHost } from './tauri';

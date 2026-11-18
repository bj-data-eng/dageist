/**
 * @file The view's door to the host. Import the contract types from here and
 * obtain a host with `tauriHost`, or with `bridgeHost` in a plain browser.
 * Both seal and deliver the messages between windows with the same envelope.
 */

export type {
  AfterView,
  AppliesView,
  ArgumentRequest,
  CandidateRequest,
  ChosenOption,
  BlockRequest,
  CatalogueView,
  CauseView,
  ChangeView,
  ChoiceView,
  ColumnChangeView,
  ColumnValuesView,
  ColumnView,
  ConditionStateView,
  ContextView,
  CountView,
  DefaultArgumentView,
  DefaultView,
  EndingView,
  EntryOfferView,
  ErrorKind,
  EntryView,
  ExpressionDtypesView,
  FieldView,
  FitView,
  FitsRequest,
  FitsView,
  Host,
  IdentityView,
  InputView,
  KindView,
  LiteralView,
  NestedPosition,
  NodeView,
  OfferPlaceView,
  OffersView,
  OptionView,
  OptionWordView,
  OptionsView,
  OriginView,
  OuterPosition,
  OutcomeView,
  ParameterView,
  PlaceRequest,
  PlaceView,
  PositionView,
  ReadProblemView,
  ReadingView,
  ReadingsView,
  RefusalSourceView,
  RefusalView,
  ResolutionView,
  RowsView,
  RuleName,
  RunView,
  SegmentView,
  ShapeView,
  SourceLineView,
  SourceView,
  StageView,
  StandingView,
  StepReportView,
  TableView,
  TypedKind,
  TypedRequest,
  ValueShapeView,
  ValueTypeView,
  ValuesView,
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
export { decodeFits, decodeOffers, decodeOptions, decodeReadings } from './decode-offers';
export { decodeRun } from './decode-run';
export { decodeBytes, decodeIdentity, decodeTable } from './decode-sample';
export { deliverMessage, sealMessage } from './envelope';
export { HostError, decodeFailure } from './failure';
export { tauriHost } from './tauri';

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
  BlockView,
  CatalogueView,
  CauseView,
  ChangeView,
  ChoiceView,
  ColumnChangeView,
  ColumnValuesView,
  ColumnView,
  ComparisonView,
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
  FiguresView,
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
  PairView,
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
  SettingView,
  ShapeView,
  SourceView,
  StageView,
  StandingView,
  StartArgumentView,
  StepReportView,
  StepStartView,
  StepStartsRequest,
  StepStartsView,
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
export { ERROR_KINDS, SETTING_KINDS } from './contract';
export { DecodeError } from './decode';
export { decodeCatalogue } from './decode-catalogue';
export { decodeNode, decodeRows } from './decode-node';
export {
  decodeFits,
  decodeOffers,
  decodeOptions,
  decodeReadings,
  decodeStepStarts,
} from './decode-offers';
export { decodeRun } from './decode-run';
export { decodeBlock } from './decode-block';
export { decodeIdentity, decodeTable } from './decode-sample';
export { deliverMessage, sealMessage } from './envelope';
export { HostError, decodeFailure } from './failure';
export { tauriHost } from './tauri';

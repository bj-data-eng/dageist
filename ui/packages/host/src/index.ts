/**
 * @file The view's door to the host. Import the contract types from here and
 * obtain a host with `tauriHost`. A stand-in host seals and delivers the
 * messages between windows with the same envelope the Tauri host uses.
 */

export type {
  BlockRequest,
  ColumnView,
  Host,
  IdentityView,
  TableView,
  WindowName,
} from './contract';
export { deliverMessage, sealMessage } from './envelope';
export { tauriHost } from './tauri';

/**
 * @file The view's door to the host. Import the contract types from here and
 * obtain a host with `tauriHost`.
 */

export type {
  BlockRequest,
  ColumnView,
  Host,
  IdentityView,
  TableView,
  WindowName,
} from './contract';
export { tauriHost } from './tauri';

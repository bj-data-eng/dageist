// The Tauri host injects this global because `app.withGlobalTauri` is enabled.
// It is absent when the view is opened in a plain browser. Only the members
// the view uses are declared.

interface TauriCore {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}

/** An event one window sends and every window can listen to. */
interface TauriEvent<T> {
  readonly payload: T;
}

interface TauriEvents {
  emit(event: string, payload?: unknown): Promise<void>;
  listen<T>(event: string, handler: (event: TauriEvent<T>) => void): Promise<() => void>;
}

interface Window {
  readonly __TAURI__?: { readonly core: TauriCore; readonly event: TauriEvents };
}

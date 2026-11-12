// The Tauri host injects this global because `app.withGlobalTauri` is enabled.
// It is absent when the view is opened in a plain browser. Only the members
// the view uses are declared.

interface TauriCore {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}

interface Window {
  readonly __TAURI__?: { readonly core: TauriCore };
}

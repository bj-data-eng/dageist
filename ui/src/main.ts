// Renders the dageist shell. The Rust core owns behavior; this view displays
// what the host reports and forwards user intent through IPC commands.

/** Mirrors `IdentityView` in crates/dageist-tauri/src/main.rs. */
interface IdentityView {
  readonly name: string;
  readonly version: string;
}

async function showIdentity(target: HTMLElement): Promise<void> {
  try {
    const identity = await window.__TAURI__.core.invoke<IdentityView>('core_identity');
    target.textContent = `${identity.name} ${identity.version}`;
    target.dataset.state = 'ready';
  } catch (error) {
    target.textContent = `Core unavailable: ${String(error)}`;
    target.dataset.state = 'error';
  }
}

const identity = document.getElementById('core-identity');
if (identity !== null) {
  void showIdentity(identity);
}

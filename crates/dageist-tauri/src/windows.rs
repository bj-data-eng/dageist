//! The windows the host opens besides the main one.
//!
//! The view asks for a window by name. Each name is one window: asking for it
//! again brings the open one forward instead of opening a second.

use std::sync::{Mutex, PoisonError};

use serde::Deserialize;
use tauri::{
    AppHandle, Manager, Runtime, Theme, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window,
    WindowEvent,
};

use crate::sample_table::ErrorView;

/// The label of the window the application opens on, as `tauri.conf.json` names it.
const MAIN_WINDOW: &str = "main";

/// Held from looking for a window until it is built. A window cannot be
/// found before it is built, so without this two requests that arrive
/// together would both find none and build two under one label.
static OPENING: Mutex<()> = Mutex::new(());

/// A window of the application besides the main one, by what it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewWindow {
    /// The steps of a transform, drawn as the blocks they are written with.
    Steps,
}

impl ViewWindow {
    /// The label Tauri knows the window by. The capability that lets the
    /// window reach the host names the same label.
    const fn label(self) -> &'static str {
        match self {
            Self::Steps => "steps",
        }
    }

    /// What the window's title bar says.
    const fn title(self) -> &'static str {
        match self {
            Self::Steps => "dageist · steps",
        }
    }

    /// The page the window loads: the view's own page, told which window it is.
    const fn page(self) -> &'static str {
        match self {
            Self::Steps => "index.html?window=steps",
        }
    }
}

/// Brings `window` forward: back from the dock when it is minimised, shown
/// when it is hidden, and in front of the others.
fn bring_forward<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), ErrorView> {
    window
        .unminimize()
        .and_then(|()| window.show())
        .and_then(|()| window.set_focus())
        .map_err(|error| ErrorView::new(error.to_string()))
}

/// Opens the window called `name`, or brings it forward when it is open.
#[tauri::command(async)]
pub fn open_view_window<R: Runtime>(app: AppHandle<R>, name: ViewWindow) -> Result<(), ErrorView> {
    open_once(&app, name, || {
        WebviewWindowBuilder::new(&app, name.label(), WebviewUrl::App(name.page().into()))
            .title(name.title())
            .inner_size(1180.0, 820.0)
            .min_inner_size(820.0, 520.0)
            .theme(Some(Theme::Dark))
            .build()
    })
}

/// Brings the window called `name` forward when it is open, and otherwise
/// opens it with `build`. One request at a time gets this far, so the second
/// of two finds the window the first built.
fn open_once<R: Runtime>(
    app: &AppHandle<R>,
    name: ViewWindow,
    build: impl FnOnce() -> tauri::Result<WebviewWindow<R>>,
) -> Result<(), ErrorView> {
    // The lock guards no value, so one a panic left poisoned is as good as any.
    let _opening = OPENING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(open) = app.get_webview_window(name.label()) {
        return bring_forward(&open);
    }
    build()
        .map(|_window| ())
        .map_err(|error| ErrorView::new(error.to_string()))
}

/// Closes every other window when the main one is closed, so that no window
/// is left open with nothing to edit. Without this the application would
/// stay open for as long as the steps window does.
pub fn close_with_main<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != MAIN_WINDOW || !matches!(event, WindowEvent::Destroyed) {
        return;
    }
    for (label, other) in window.app_handle().webview_windows() {
        if label != MAIN_WINDOW {
            // A window that will not close is one the user can still close by hand.
            let _ = other.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

    use super::{ViewWindow, close_with_main, open_once, open_view_window};

    /// How long the stand-in for a native window takes to build. A real one
    /// takes longer; this is long enough for a second request to arrive.
    const BUILD_TIME: Duration = Duration::from_millis(50);

    #[test]
    fn asking_twice_for_the_steps_window_opens_it_once() {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("the mock application builds");

        open_view_window(app.handle().clone(), ViewWindow::Steps).expect("the window opens");
        open_view_window(app.handle().clone(), ViewWindow::Steps)
            .expect("the open window comes forward");

        assert_eq!(app.webview_windows().len(), 1);
        assert!(app.get_webview_window("steps").is_some());
    }

    #[test]
    fn requests_that_arrive_together_build_one_steps_window() {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("the mock application builds");
        let handle = app.handle().clone();
        let built = AtomicUsize::new(0);
        let together = Barrier::new(2);

        thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| {
                    together.wait();
                    open_once(&handle, ViewWindow::Steps, || {
                        built.fetch_add(1, Ordering::SeqCst);
                        // The time under test: the window is not there to be found until
                        // it is built, and the other request arrives in the meantime.
                        thread::sleep(BUILD_TIME);
                        WebviewWindowBuilder::new(&handle, "steps", WebviewUrl::default()).build()
                    })
                    .expect("the window opens or comes forward");
                });
            }
        });

        assert_eq!(built.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn closing_another_window_leaves_the_steps_window_open() {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("the mock application builds");
        open_view_window(app.handle().clone(), ViewWindow::Steps).expect("the window opens");
        let steps = app.get_webview_window("steps").expect("the steps window");

        close_with_main(&steps.as_ref().window(), &WindowEvent::Destroyed);

        assert!(app.get_webview_window("steps").is_some());
    }

    #[test]
    fn reads_a_window_name_as_the_view_writes_it() {
        let name: ViewWindow = serde_json::from_str("\"steps\"").expect("a window name");

        assert_eq!(name, ViewWindow::Steps);
        assert!(serde_json::from_str::<ViewWindow>("\"canvas\"").is_err());
    }

    #[test]
    fn the_capability_reaches_the_steps_window() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json"))
                .expect("the capability is JSON");

        let windows = capability["windows"].as_array().expect("a list of windows");
        assert!(windows.contains(&serde_json::json!(ViewWindow::Steps.label())));
    }
}

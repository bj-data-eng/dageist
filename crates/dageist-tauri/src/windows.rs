//! The windows the host opens besides the main one.
//!
//! The view asks for a window by name. Each name is one window: asking for it
//! again brings the open one forward instead of opening a second.

use serde::Deserialize;
use tauri::{
    AppHandle, Manager, Runtime, Theme, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window,
    WindowEvent,
};

use crate::sample_table::ErrorView;

/// The label of the window the application opens on, as `tauri.conf.json` names it.
const MAIN_WINDOW: &str = "main";

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
/// opens it with `build`.
fn open_once<R: Runtime>(
    app: &AppHandle<R>,
    name: ViewWindow,
    build: impl FnOnce() -> tauri::Result<WebviewWindow<R>>,
) -> Result<(), ErrorView> {
    if let Some(open) = app.get_webview_window(name.label()) {
        return bring_forward(&open);
    }
    match build() {
        Ok(_window) => Ok(()),
        // Two requests that arrive together both find no window, and the second cannot build
        // one under a label that is now taken. The window it wanted is there all the same.
        Err(error) => match app.get_webview_window(name.label()) {
            Some(open) => bring_forward(&open),
            None => Err(ErrorView::new(error.to_string())),
        },
    }
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
    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::{Manager, WindowEvent};

    use super::{ViewWindow, close_with_main, open_view_window};

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

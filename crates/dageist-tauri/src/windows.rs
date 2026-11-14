//! The windows the host opens besides the main one.
//!
//! The view asks for a window by name. Each name is one window: asking for it
//! again brings the open one forward instead of opening a second.

use serde::Deserialize;
use tauri::{AppHandle, Manager, Runtime, Theme, WebviewUrl, WebviewWindowBuilder};

use crate::sample_table::ErrorView;

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

/// Opens the window called `name`, or brings it forward when it is open.
#[tauri::command(async)]
pub fn open_view_window<R: Runtime>(app: AppHandle<R>, name: ViewWindow) -> Result<(), ErrorView> {
    if let Some(open) = app.get_webview_window(name.label()) {
        return open
            .set_focus()
            .map_err(|error| ErrorView::new(error.to_string()));
    }
    WebviewWindowBuilder::new(&app, name.label(), WebviewUrl::App(name.page().into()))
        .title(name.title())
        .inner_size(1180.0, 820.0)
        .min_inner_size(820.0, 520.0)
        .theme(Some(Theme::Dark))
        .build()
        .map(|_window| ())
        .map_err(|error| ErrorView::new(error.to_string()))
}

#[cfg(test)]
mod tests {
    use tauri::Manager;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    use super::{ViewWindow, open_view_window};

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

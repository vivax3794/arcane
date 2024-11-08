//! Shows logs in app

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::Style;

use crate::prelude::*;

/// Logging plugin
pub(super) struct LogPlugin;

/// Open the log window
#[derive(Clone)]
struct OpenLogs;

impl Plugin for LogPlugin {
    fn on_load(&mut self, events: &mut EventManager) -> Result<()> {
        events.ensure_event::<OpenLogs>();
        events.dispatch(SetKeybind::chord(
            [
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('p'),
                },
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('l'),
                },
            ],
            OpenLogs,
        ));
        Ok(())
    }

    fn update(&mut self, events: &mut EventManager, _plugins: &PluginStore) -> Result<()> {
        let (reader, mut writer) = events.split();
        for _ in reader.read::<OpenLogs>() {
            writer.dispatch(WindowEvent::CreateWindow(Box::new(LogWindow)));
        }

        Ok(())
    }
}

/// Window to display logs
#[derive(Clone, Copy)]
struct LogWindow;

impl Window for LogWindow {
    fn name(&self) -> String {
        String::from("Logs")
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        _plugins: &crate::plugin_manager::PluginStore,
    ) {
        let log_widget = tui_logger::TuiLoggerWidget::default()
            .style_error(Style::new().red())
            .style_warn(Style::new().yellow())
            .style_info(Style::new().green())
            .style_debug(Style::new().light_green())
            .style_trace(Style::new().dark_gray());
        frame.render_widget(log_widget, area);
    }
}

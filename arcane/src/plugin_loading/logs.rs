//! Shows logs in app

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::Style;

use crate::prelude::*;

/// Logging plugin
pub(super) struct LogPlugin;

impl Plugin for LogPlugin {
    fn on_load(&mut self, events: &EventManager) -> Result<()> {
        Ok(())
    }

    fn update(&mut self, events: &EventManager, plugins: &PluginStore) -> Result<()> {
        let Some(keybinds) = plugins.get::<KeybindPlugin>() else {
            return Ok(());
        };

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
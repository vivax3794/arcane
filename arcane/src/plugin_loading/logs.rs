//! Shows logs in app

use std::sync::Arc;

use ansi_to_tui::IntoText;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use serde::{Deserialize, Serialize};

use super::keybindings::{BindResult, MenuEvent};
use crate::logging::Logger;
use crate::prelude::*;

/// Logging plugin
pub(super) struct LogPlugin {
    /// The log output
    logs: Logger,
}

impl LogPlugin {
    /// Create a new log plugin with the specified logs
    pub(super) const fn new(logs: Logger) -> Self {
        Self { logs }
    }
}

/// Open the log window
#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenLogs;

#[typetag::serde]
impl BindResult for OpenLogs {}

impl Plugin for LogPlugin {
    fn on_load(&mut self, events: &mut EventManager) -> Result<()> {
        events.ensure_event::<OpenLogs>();
        events.dispatch(RegisterKeybind::chord(
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
            writer.dispatch(WindowEvent::CreateWindow(Box::new(LogWindow {
                logs: Arc::clone(&self.logs),
                scroll: None,
            })));
        }

        Ok(())
    }
}

/// Window to display logs
#[derive(Clone)]
struct LogWindow {
    /// A reference to the logger
    logs: Logger,
    /// The status of the scroll
    scroll: Option<u16>,
}

impl Window for LogWindow {
    fn name(&self) -> String {
        String::from("Logs")
    }

    fn update(
        &mut self,
        events: &mut EventManager,
        _plugins: &PluginStore,
        focused: bool,
        _id: super::windows::WindowID,
    ) -> Result<()> {
        if !focused {
            return Ok(());
        }

        for event in events.read::<MenuEvent>() {
            match event {
                MenuEvent::Up => {
                    if let Some(value) = self.scroll.as_mut() {
                        *value = value.saturating_sub(1);
                    } else {
                        let Ok(logs) = self.logs.lock() else {
                            continue;
                        };
                        self.scroll = Some(
                            u16::try_from(bytecount::count(&logs, b'\n'))
                                .unwrap_or_default()
                                .saturating_sub(1),
                        );
                    }
                }
                MenuEvent::Down => {
                    if let Some(value) = self.scroll.as_mut() {
                        *value = value.saturating_add(1);
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        _plugins: &crate::plugin_manager::PluginStore,
    ) {
        let Ok(content) = self.logs.lock() else {
            event!(Level::ERROR, "Failed to lock memory logs");
            frame.render_widget("Could not get log lock".red(), area);

            return;
        };

        let Ok(text) = content.to_text() else {
            event!(Level::ERROR, "Failed to render logs");
            frame.render_widget("Could not render logs".red(), area);

            return;
        };

        let scroll_amount = match self.scroll {
            Some(value) => value,
            None => u16::try_from(text.lines.len())
                .unwrap_or_default()
                .saturating_sub(area.height),
        };

        let scroll_bar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        let mut scroll_bar_state =
            ScrollbarState::new(text.lines.len().saturating_sub(area.height as usize))
                .position(scroll_amount as usize);
        let paragraph = Paragraph::new(text).scroll((scroll_amount, 0));

        frame.render_widget(paragraph, area);
        frame.render_stateful_widget(scroll_bar, area, &mut scroll_bar_state);
    }
}

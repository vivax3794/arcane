//! Handles drawing the core Windows

use std::collections::HashMap;
use std::mem;

use crossterm::event::{KeyCode, KeyModifiers};
use dyn_clone::DynClone;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Color;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::settings::{
    get_settings,
    PluginSettings,
    RegisterSettings,
    SettingsValue,
    SettingsValueCommon,
};
use crate::plugin_manager::Plugin;
use crate::prelude::*;

/// Id ofs a window
pub type WindowID = u8;

/// Trait implementing all values needed for a window
pub trait Window: DynClone {
    /// The horizontal constarint for this window.
    ///
    /// Defaults to `Fill(1)`, i.e take up the same spaces as all other "normal windows"
    fn horizontal_constraints(&self) -> Constraint {
        Constraint::Fill(1)
    }

    /// The name for the window
    fn name(&self) -> String;

    /// Draw the window contents
    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        plugins: &crate::plugin_manager::PluginStore,
    );

    /// Update call for the window
    fn update(
        &mut self,
        _events: &EventManager,
        _plugins: &PluginStore,
        _focused: bool,
        _id: WindowID,
    ) -> Result<()> {
        Ok(())
    }

    /// Called when the window is deleted
    fn on_remove(&mut self, _events: &EventManager, _plugins: &PluginStore) -> Result<()> {
        Ok(())
    }
}

/// Events for stuff to do with windows
pub enum WindowEvent {
    /// Create a new window
    CreateWindow(Box<dyn Window>),
    /// Close a window
    CloseWindow(WindowID),
}

/// Settings for displaying windows
#[derive(Clone, Copy)]
struct WindowSettings {
    /// The kind of border
    focus_border: &'static str,
    /// Should focus border only be on the sides or all around
    focus_all_around: bool,
}

impl PluginSettings for WindowSettings {
    fn name(&self) -> &'static str {
        "Windows"
    }

    fn values(&mut self) -> Box<[SettingsValueCommon]> {
        Box::new([
            SettingsValueCommon {
                name: "focus_border_type",
                value: SettingsValue::DropDown(
                    &mut self.focus_border,
                    &["double", "rounded", "plain"],
                ),
            },
            SettingsValueCommon {
                name: "focus_box_all_sides",
                value: SettingsValue::Toogle(&mut self.focus_all_around),
            },
        ])
    }
}

impl WindowSettings {
    /// Convert the string setting to a enum
    fn border_type(&self) -> BorderType {
        match self.focus_border {
            "double" => BorderType::Double,
            "rounded" => BorderType::Rounded,
            _ => BorderType::Plain,
        }
    }
}

/// The plugin
pub(crate) struct WindowPlugin {
    /// The windows in view
    windows: HashMap<WindowID, Box<dyn Window>>,
    /// the next free ID
    next_free: WindowID,
    /// The order of windows
    window_order: Vec<WindowID>,
    /// The currently focused window
    focused: usize,
}

impl WindowPlugin {
    /// Create a empty instance
    pub(super) fn new() -> Self {
        WindowPlugin {
            windows: HashMap::new(),
            next_free: 0,
            window_order: Vec::new(),
            focused: 0,
        }
    }

    /// Re assign all window ids
    fn fill_gaps(&mut self) -> Result<()> {
        let windows = mem::take(&mut self.windows);
        let windows = windows.into_values();
        for (index, window) in windows.enumerate() {
            let index = index.try_into()?;
            self.windows.insert(index, window);
        }
        self.next_free = self.windows.len().try_into()?;

        Ok(())
    }
}

impl Plugin for WindowPlugin {
    fn on_load(&mut self, events: &EventManager) -> Result<()> {
        events.dispatch(RegisterSettings(Box::new(WindowSettings {
            focus_border: "double",
            focus_all_around: false,
        })));
        events.dispatch(SetKeybind {
            name: "window_focus_left",
            bind: KeyBind {
                modifers: KeyModifiers::CONTROL,
                key: KeyCode::Char('h'),
            },
        });
        events.dispatch(SetKeybind {
            name: "window_focus_rigth",
            bind: KeyBind {
                modifers: KeyModifiers::CONTROL,
                key: KeyCode::Char('l'),
            },
        });
        events.dispatch(SetKeybind {
            name: "close_window",
            bind: KeyBind {
                modifers: KeyModifiers::CONTROL,
                key: KeyCode::Char('w'),
            },
        });

        Ok(())
    }

    fn update(
        &mut self,
        events: &crate::plugin_manager::EventManager,
        plugins: &crate::plugin_manager::PluginStore,
    ) -> color_eyre::eyre::Result<()> {
        let focused_window_id = self
            .window_order
            .get(self.focused)
            .copied()
            .unwrap_or_default();
        for (window_id, window) in &mut self.windows {
            window.update(events, plugins, *window_id == focused_window_id, *window_id)?;
        }

        let Some(keybinds) = plugins.get::<KeybindPlugin>() else {
            return Ok(());
        };
        for event in events.read::<KeydownEvent>() {
            if keybinds.matches("window_focus_left", event) {
                self.focused = self.focused.saturating_sub(1);
            }
            if keybinds.matches("window_focus_rigth", event) {
                self.focused = self.focused.saturating_add(1);
            }
            if keybinds.matches("close_window", event) {
                if let Some(window_id) = self.window_order.get(self.focused) {
                    events.dispatch(WindowEvent::CloseWindow(*window_id));
                }
            }
        }

        for event in events.read::<WindowEvent>() {
            match event {
                WindowEvent::CreateWindow(window) => {
                    let id = self.next_free;
                    event!(Level::INFO, "Created window {id}");
                    if let Some(new_id) = self.next_free.checked_add(1) {
                        self.next_free = new_id;
                    } else {
                        event!(Level::WARN, "Ran out of window IDS, Filling gaps");
                        self.fill_gaps()?;
                    }
                    self.windows.insert(id, dyn_clone::clone_box(&**window));
                    self.window_order.push(id);
                    self.focused = self.window_order.len().saturating_sub(1);
                }
                WindowEvent::CloseWindow(id) => {
                    event!(Level::INFO, "Deleting window {id}");
                    if let Some(mut removed_window) = self.windows.remove(id) {
                        removed_window.on_remove(events, plugins)?;
                    }
                    self.window_order.retain(|window_id| window_id != id);
                }
            }
        }
        self.focused = self.focused.min(self.window_order.len().saturating_sub(1));

        Ok(())
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        plugins: &crate::plugin_manager::PluginStore,
    ) {
        if self.windows.is_empty() {
            let text = Paragraph::new("No Windows Open!").red();
            frame.render_widget(text, area);
            return;
        }

        let Some(settings_ref) = get_settings::<WindowSettings>(plugins) else {
            return;
        };
        let settings = *settings_ref;
        drop(settings_ref);

        let windows = self
            .window_order
            .iter()
            .filter_map(|id| self.windows.get(id))
            .collect::<Vec<_>>();

        let layout =
            Layout::horizontal(windows.iter().map(|window| window.horizontal_constraints()))
                .split(area);

        for (position, window) in windows.into_iter().enumerate() {
            let focused = position == self.focused;

            let borders = if self.windows.len() == 1 {
                Borders::NONE
            } else if focused {
                if settings.focus_all_around {
                    Borders::ALL
                } else {
                    Borders::LEFT | Borders::RIGHT
                }
            } else if position != 0 && position.saturating_sub(1) != self.focused {
                Borders::LEFT
            } else {
                Borders::NONE
            };
            let (color, border_type) = if focused {
                (Color::LightYellow, settings.border_type())
            } else {
                (Color::LightGreen, BorderType::Plain)
            };

            let block = Block::default()
                .borders(borders)
                .fg(color)
                .border_type(border_type);

            let Some(area) = layout.get(position) else {
                continue;
            };
            let inner_area = block.inner(*area);
            frame.render_widget(Clear, *area);
            frame.render_widget(block, *area);
            frame.render_widget(Clear, inner_area);
            window.draw(frame, inner_area, plugins);
        }
    }
}

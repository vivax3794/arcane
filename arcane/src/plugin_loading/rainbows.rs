//! Displays a rainbow! Yay!

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::Color;
use ratatui::widgets::canvas::{Canvas, Rectangle};

use super::keybindings::{KeyBind, KeybindPlugin, SetKeybind};
use super::windows::{Window, WindowEvent};
use crate::editor::{DeltaTimeEvent, KeydownEvent};
use crate::plugin_manager::{EventManager, Plugin, PluginStore};
use crate::prelude::*;

/// Display a rainbow in a window
pub struct RainbowPlugin {
    /// The sizes and colors of the rectangles
    sizes: Vec<(f64, Color)>,
}

impl RainbowPlugin {
    /// Create new instance of plugin
    pub(super) fn new() -> Self {
        Self {
            sizes: vec![
                (100.0, Color::Red),
                (90.0, Color::Green),
                (80.0, Color::Blue),
                (70.0, Color::DarkGray),
                (60.0, Color::Magenta),
                (50.0, Color::Yellow),
                (40.0, Color::Cyan),
            ],
        }
    }
}

impl Plugin for RainbowPlugin {
    fn on_load(&mut self, events: &EventManager) -> Result<()> {
        Ok(())
    }

    fn update(&mut self, events: &EventManager, plugins: &PluginStore) -> Result<()> {
        let Some(keybinds) = plugins.get::<KeybindPlugin>() else {
            return Ok(());
        };

        for update in events.read::<DeltaTimeEvent>() {
            for (size, _) in &mut self.sizes {
                *size -= update.0.as_secs_f64() * 20.0;
                if *size <= 0.0 {
                    *size = 100.0;
                }
            }
        }
        Ok(())
    }
}

/// A window showing the rainbow animation
#[derive(Clone, Copy)]
struct RainbowWindow {
    /// Is this window currently focused?
    focused: bool,
}

impl Window for RainbowWindow {
    fn name(&self) -> String {
        String::from("Rainbow")
    }

    fn update(
        &mut self,
        _events: &EventManager,
        _plugins: &PluginStore,
        focused: bool,
        _id: super::windows::WindowID,
    ) -> Result<()> {
        self.focused = focused;
        Ok(())
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        plugins: &PluginStore,
    ) {
        let Some(plugin) = plugins.get::<RainbowPlugin>() else {
            return;
        };

        let canvas = Canvas::default()
            .x_bounds([-100.0, 100.0])
            .y_bounds([-100.0, 100.0])
            .paint(|ctx| {
                for (size, color) in &plugin.sizes {
                    ctx.draw(&Rectangle {
                        x: -size,
                        y: -size,
                        width: size * 2.0,
                        height: size * 2.0,
                        color: if self.focused { *color } else { Color::Gray },
                    });
                }
            });
        frame.render_widget(canvas, area);
    }
}

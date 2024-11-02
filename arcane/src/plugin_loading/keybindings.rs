//! Handles abstracting actions into keybindings

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Constraint;
use ratatui::widgets::{Row, Table};

use crate::editor::KeydownEvent;
use crate::plugin_manager::Plugin;
use crate::prelude::*;

/// A keybind that can be matched against
#[derive(Clone, Copy)]
pub struct KeyBind {
    /// Modifiers that need to match exactly
    pub modifers: KeyModifiers,
    /// Key that needs to be pressed
    pub key: KeyCode,
}

impl KeyBind {
    /// Get a string version of the keybind
    fn render(&self) -> String {
        format!("{}+{}", self.modifers, self.key)
    }
}

/// Set the keybind
pub struct SetKeybind {
    /// The name of the keybind, doesnt have to be globally unique, but is only info shown to user
    /// so should be clear
    pub name: &'static str,
    /// The actual keybind
    pub bind: KeyBind,
}

/// Handles keybindings
pub struct KeybindPlugin {
    /// Holds the bindings, the key is a combimation of the owning plugin and the name
    bindings: HashMap<&'static str, KeyBind>,
}

impl KeybindPlugin {
    /// Create a empty plugin
    pub(super) fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Does the keydown event match the given keybind?
    pub fn matches(&self, name: &'static str, event: &KeydownEvent) -> bool {
        self.bindings
            .get(name)
            .is_some_and(|bind| event.0.code == bind.key && event.0.modifiers == bind.modifers)
    }
}

impl Plugin for KeybindPlugin {
    fn on_load(&mut self, _events: &mut EventManager) -> Result<()> {
        self.bindings.insert(
            "menu_left",
            KeyBind {
                modifers: KeyModifiers::NONE,
                key: KeyCode::Char('h'),
            },
        );
        self.bindings.insert(
            "menu_right",
            KeyBind {
                modifers: KeyModifiers::NONE,
                key: KeyCode::Char('l'),
            },
        );
        self.bindings.insert(
            "menu_up",
            KeyBind {
                modifers: KeyModifiers::NONE,
                key: KeyCode::Char('k'),
            },
        );
        self.bindings.insert(
            "menu_down",
            KeyBind {
                modifers: KeyModifiers::NONE,
                key: KeyCode::Char('j'),
            },
        );
        self.bindings.insert(
            "menu_select",
            KeyBind {
                modifers: KeyModifiers::NONE,
                key: KeyCode::Enter,
            },
        );
        self.bindings.insert(
            "open_keybinds",
            KeyBind {
                modifers: KeyModifiers::CONTROL,
                key: KeyCode::Char('k'),
            },
        );
        Ok(())
    }

    fn update(
        &mut self,
        events: &mut crate::plugin_manager::EventManager,
        _plugins: &crate::plugin_manager::PluginStore,
    ) -> color_eyre::eyre::Result<()> {
        for event in events.read::<SetKeybind>() {
            event!(
                Level::DEBUG,
                "set keybind {} to {}",
                event.name,
                event.bind.render()
            );
            self.bindings.insert(event.name, event.bind);
        }

        let (reader, mut writer) = events.split();
        for event in reader.read::<KeydownEvent>() {
            if self.matches("open_keybinds", event) {
                writer.dispatch(WindowEvent::CreateWindow(Box::new(KeybindWindow)));
            }
        }

        Ok(())
    }
}

/// A window to see and configure keybindings
#[derive(Clone, Copy)]
struct KeybindWindow;

impl Window for KeybindWindow {
    fn name(&self) -> String {
        String::from("Keybinds")
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        plugins: &crate::plugin_manager::PluginStore,
    ) {
        let Some(keybinds) = plugins.get::<KeybindPlugin>() else {
            return;
        };

        let rows = keybinds
            .bindings
            .iter()
            .map(|(key, value)| Row::new([(*key).to_owned(), value.render()]));
        let table = Table::new(rows, [Constraint::Fill(1), Constraint::Fill(1)]);
        frame.render_widget(table, area);
    }
}

//! Manages app settings.

use std::any::Any;
use std::cell::Ref;

use crossterm::event::{KeyCode, KeyModifiers};
use dyn_clone::DynClone;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Gauge, Paragraph, Row, Tabs};

use crate::anymap::{AnyMap, Downcast, IntoBoxed};
use crate::prelude::*;

/// The value in enum
pub enum SettingsValue<'v> {
    /// A integer value
    Integer {
        value: &'v mut i128,
        min: i128,
        max: i128,
    },
    /// Multiple Kinds of Values
    DropDown(&'v mut &'static str, &'static [&'static str]),
    /// A toogle
    Toogle(&'v mut bool),
}

/// Common metadata for settings
pub struct SettingsValueCommon<'v> {
    /// The name of the settings
    pub name: &'static str,
    /// A mutable reference to the data that needs changing
    pub value: SettingsValue<'v>,
}

/// Settings for a specific plugin
pub trait PluginSettings: Any + DynClone {
    /// Used for serializing and loading settings.
    fn name(&self) -> &'static str;
    /// The values in the settings menu
    fn values(&mut self) -> Box<[SettingsValueCommon]>;
}

impl Downcast for dyn PluginSettings {
    fn downcast<T>(this: &Self) -> Option<&T>
    where
        T: 'static,
    {
        (this as &dyn Any).downcast_ref()
    }
    fn downcast_mut<T>(this: &mut Self) -> Option<&mut T>
    where
        T: 'static,
    {
        (this as &mut dyn Any).downcast_mut()
    }
}

impl<P: PluginSettings> IntoBoxed<dyn PluginSettings> for P {
    fn into(self) -> Box<dyn PluginSettings> {
        Box::new(self)
    }
}

/// Register a new settings, thing
pub struct RegisterSettings(pub Box<dyn PluginSettings>);

/// The settings management plugin
pub struct SettingsPlugin {
    /// The settings for each plugin
    settings: AnyMap<dyn PluginSettings>,
}

impl SettingsPlugin {
    /// Create the plugin with default settings
    pub(super) fn new() -> Self {
        Self {
            settings: AnyMap::new(),
        }
    }

    /// Get a settings object from the plugin
    pub fn get<S: PluginSettings>(&self) -> Option<&S> {
        self.settings.get::<S>()
    }

    /// Get a sorted version of the settings list
    fn sorted_settings(&mut self) -> Vec<&mut Box<dyn PluginSettings>> {
        let mut plugins = self.settings.iter_mut().collect::<Vec<_>>();
        plugins.sort_by_key(|plugin| plugin.name());
        plugins
    }
}

/// Convnient method that retrives the plugin from the store, then your settings from the plugin
pub fn get_settings<S: PluginSettings>(store: &PluginStore) -> Option<Ref<S>> {
    let plugin = store.get::<SettingsPlugin>()?;
    let settings = Ref::filter_map(plugin, SettingsPlugin::get);
    settings.ok()
}

impl Plugin for SettingsPlugin {
    fn on_load(&mut self, events: &mut EventManager) -> Result<()> {
        events.dispatch(SetKeybind {
            name: "open_settings",
            bind: KeyBind {
                modifers: KeyModifiers::CONTROL,
                key: KeyCode::Char('p'),
            },
        });
        Ok(())
    }

    fn update(&mut self, events: &mut EventManager, plugins: &PluginStore) -> Result<()> {
        for event in events.read::<RegisterSettings>() {
            let settings = dyn_clone::clone_box(&*event.0);
            self.settings.insert_raw(settings);
        }

        let Some(keybinds) = plugins.get::<KeybindPlugin>() else {
            return Ok(());
        };

        let (reader, mut writer) = events.split();
        for event in reader.read::<KeydownEvent>() {
            if keybinds.matches("open_settings", event) {
                writer.dispatch(WindowEvent::CreateWindow(Box::new(SettingsWindow::new())));
            }
        }

        Ok(())
    }
}

/// The settings window
#[derive(Clone, Copy)]
struct SettingsWindow {
    /// The selected tab
    selected_tab: usize,
    /// Selected row
    selected_row: usize,
}

impl SettingsWindow {
    /// Create default bs
    const fn new() -> Self {
        Self {
            selected_tab: 0,
            selected_row: 0,
        }
    }
}

impl SettingsWindow {}

impl Window for SettingsWindow {
    fn name(&self) -> String {
        String::from("Settings")
    }

    fn update(
        &mut self,
        events: &mut EventManager,
        plugins: &PluginStore,
        focused: bool,
        _id: super::windows::WindowID,
    ) -> Result<()> {
        if !focused {
            return Ok(());
        }

        let Some(mut settings) = plugins.get_mut::<SettingsPlugin>() else {
            return Ok(());
        };

        let Some(keybinds) = plugins.get::<KeybindPlugin>() else {
            return Ok(());
        };
        for event in events.read::<KeydownEvent>() {
            if keybinds.matches("menu_left", event) {
                self.selected_tab = self.selected_tab.saturating_sub(1);
                self.selected_row = 0;
            } else if keybinds.matches("menu_right", event) {
                self.selected_tab = self
                    .selected_tab
                    .saturating_add(1)
                    .min(settings.settings.len().saturating_sub(1));
                self.selected_row = 0;
            } else if keybinds.matches("menu_up", event) {
                self.selected_row = self.selected_row.saturating_sub(1);
            } else if keybinds.matches("menu_down", event) {
                self.selected_row = self.selected_row.saturating_add(1);
            } else if keybinds.matches("menu_select", event) {
                let settings = settings.sorted_settings();
                let Some(select_setting) = settings.into_iter().nth(self.selected_tab) else {
                    return Ok(());
                };

                let values = select_setting.values();
                let Some(value) = IntoIterator::into_iter(values).nth(self.selected_row) else {
                    return Ok(());
                };

                match value.value {
                    SettingsValue::Toogle(value) => {
                        *value = !*value;
                    }
                    SettingsValue::DropDown(value, possible) => {
                        let selected = possible
                            .iter()
                            .enumerate()
                            .find_map(|(index, p)| (p == value).then_some(index))
                            .unwrap_or_default();

                        let selected = selected
                            .saturating_add(1)
                            .checked_rem(possible.len())
                            .unwrap_or_default();
                        if let Some(new) = possible.get(selected) {
                            *value = new;
                        }
                    }
                    _ => todo!("This setting kind isnt implemented yet"),
                }
            }
        }

        Ok(())
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        plugins: &crate::plugin_manager::PluginStore,
    ) {
        let Some(mut settings) = plugins.get_mut::<SettingsPlugin>() else {
            return;
        };
        let plugins = settings.sorted_settings();
        let names = plugins.iter().map(|plugin| plugin.name());

        let layout = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)])
            .spacing(1)
            .areas::<2>(area);

        let tabs = Tabs::new(names)
            .select(self.selected_tab)
            .divider("|".blue())
            .highlight_style(Style::default().yellow())
            .on_black();
        frame.render_widget(tabs, layout[0]);

        let Some(selected) = plugins.into_iter().nth(self.selected_tab) else {
            return;
        };
        let values = selected.values();

        let mut constraints = vec![Constraint::Length(1); values.len()];
        constraints.push(Constraint::Fill(1));
        let layout = Layout::vertical(constraints).spacing(1).split(layout[1]);

        for (index, (value, area)) in IntoIterator::into_iter(values)
            .zip(layout.iter())
            .enumerate()
        {
            let layout = Layout::horizontal(vec![Constraint::Length(30), Constraint::Fill(1)])
                .areas::<2>(*area);

            if index == self.selected_row {
                frame.render_widget(Paragraph::new("").bg(Color::Rgb(20, 20, 40)), *area);
            }

            frame.render_widget(value.name, layout[0]);
            match value.value {
                SettingsValue::Toogle(value) => {
                    let text = "◖█████████◗";
                    let text = if *value { text.green() } else { text.red() };
                    frame.render_widget(text, layout[1]);
                }
                SettingsValue::DropDown(selected, possible) => {
                    let selected = possible
                        .iter()
                        .enumerate()
                        .find_map(|(index, p)| (p == selected).then_some(index))
                        .unwrap_or_default();

                    let list = Tabs::new(possible.to_owned()).select(selected);
                    frame.render_widget(list, layout[1]);
                }
                _ => {
                    frame.render_widget("TODO".on_red(), layout[1]);
                }
            }
        }
    }
}

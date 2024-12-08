//! Manages app settings via window.
use arcane_core::Result;
use arcane_keybindings::{KeyBind, KeyCode, KeyModifiers, MenuEvent, RegisterKeybind};
use arcane_windows::{Window, WindowEvent};
use error_mancer::errors;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::widgets::{Gauge, Paragraph, Tabs};
use serde::{Deserialize, Serialize};

/// Open the settings menu
#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenSettings;

#[typetag::serde]
impl arcane_keybindings::BindResult for OpenSettings {}

pub struct SettingsWindowPlugin;

#[errors]
impl arcane_core::Plugin for SettingsWindowPlugin {
    fn new() -> Self {
        Self
    }

    #[errors]
    fn on_load(&mut self, events: &mut arcane_core::EventManager) -> Result<()> {
        events.ensure_event::<OpenSettings>();
        events.dispatch(RegisterKeybind::chord(
            [
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('p'),
                },
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('p'),
                },
            ],
            OpenSettings,
        ));

        Ok(())
    }

    #[errors()]
    fn update(
        &mut self,
        events: &mut arcane_core::EventManager,
        _plugins: &arcane_core::PluginStore,
    ) -> Result<()> {
        let (reader, mut writer) = events.split();
        for _ in reader.read::<OpenSettings>() {
            writer.dispatch(WindowEvent::CreateWindow(Box::new(SettingsWindow::new())));
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

arcane_core::register_plugin!(SettingsWindowPlugin);

impl Window for SettingsWindow {
    fn name(&self) -> String {
        String::from("Settings")
    }

    #[errors()]
    fn update(
        &mut self,
        events: &mut arcane_core::EventManager,
        plugins: &arcane_core::PluginStore,
        focused: bool,
        _id: arcane_windows::WindowID,
    ) -> Result<()> {
        if !focused {
            return Ok(());
        }

        let Some(mut settings) = plugins.get_mut::<arcane_settings::SettingsPlugin>() else {
            return Ok(());
        };

        let mut modified_settings = false;
        for event in events.read::<MenuEvent>() {
            match event {
                MenuEvent::Left => {
                    self.selected_tab = self.selected_tab.saturating_sub(1);
                    self.selected_row = 0;
                }
                MenuEvent::Right => {
                    self.selected_tab = self
                        .selected_tab
                        .saturating_add(1)
                        .min(settings.settings.len().saturating_sub(1));
                    self.selected_row = 0;
                }
                MenuEvent::Up => {
                    self.selected_row = self.selected_row.saturating_sub(1);
                }
                MenuEvent::Down => {
                    self.selected_row = self.selected_row.saturating_add(1);
                }
                MenuEvent::Select | MenuEvent::AltSelect => {
                    let settings = settings.sorted_settings();
                    let Some(select_setting) = settings.into_iter().nth(self.selected_tab) else {
                        return Ok(());
                    };

                    let values = select_setting.values();
                    let Some(value) = IntoIterator::into_iter(values).nth(self.selected_row) else {
                        return Ok(());
                    };
                    value.handle_settings_update(event == &MenuEvent::AltSelect);
                    modified_settings = true;
                }
                _ => (),
            }
        }
        if modified_settings {
            events.dispatch(arcane_settings::SaveSettings);
        }

        Ok(())
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        plugins: &arcane_core::PluginStore,
    ) {
        let Some(mut settings) = plugins.get_mut::<arcane_settings::SettingsPlugin>() else {
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
                arcane_settings::SettingsValue::Toogle(value) => {
                    let text = "◖█████████◗";
                    let text = if *value { text.green() } else { text.red() };
                    frame.render_widget(text, layout[1]);
                }
                arcane_settings::SettingsValue::Selection(selected, possible) => {
                    let selected = possible
                        .iter()
                        .enumerate()
                        .find_map(|(index, p)| (p == selected).then_some(index))
                        .unwrap_or_default();

                    let list = Tabs::new(possible.to_owned()).select(selected);
                    frame.render_widget(list, layout[1]);
                }
                arcane_settings::SettingsValue::Integer {
                    value, min, max, ..
                } => {
                    let norm_value: f64 = value.saturating_sub(min).into();
                    let norm_max: f64 = max.saturating_sub(min).into();

                    let bar = Gauge::default()
                        .ratio(norm_value / norm_max)
                        .label(value.to_string());
                    frame.render_widget(bar, layout[1]);
                }
            }
        }
    }
}

//! Handles drawing the core Windows
#![feature(used_with_arg)]

use std::collections::HashMap;
use std::mem;
use std::str::FromStr;

use arcane_anymap::dyn_clone;
use arcane_core::{event, Level, Result};
use arcane_keybindings::{KeyBind, KeyCode, KeyModifiers, RegisterKeybind};
use derive_more::derive::Debug;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Tabs};
use serde::{Deserialize, Serialize};

/// Id ofs a window
pub type WindowID = u8;

/// Trait implementing all values needed for a window
pub trait Window: dyn_clone::DynClone {
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
        plugins: &arcane_core::PluginStore,
    );

    /// Update call for the window
    fn update(
        &mut self,
        _events: &mut arcane_core::EventManager,
        _plugins: &arcane_core::PluginStore,
        _focused: bool,
        _id: WindowID,
    ) -> Result<()> {
        Ok(())
    }

    /// Called when the window is deleted
    fn on_remove(
        &mut self,
        _events: &arcane_core::EventManager,
        _plugins: &arcane_core::PluginStore,
    ) -> Result<()> {
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
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct WindowSettings {
    /// The kind of border
    focus_border_type: String,
    /// The border type for unfocused windows
    other_border_type: String,
    /// Should focus border only be on the sides or all around
    focus_full_border: bool,
    /// Should other windows have full borders
    all_full_border: bool,
    /// Always show the tab bar
    always_show_tab_bar: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            focus_border_type: String::from("Double"),
            other_border_type: String::from("Rounded"),
            focus_full_border: true,
            all_full_border: true,
            always_show_tab_bar: false,
        }
    }
}

#[typetag::serde]
impl arcane_settings::PluginSettings for WindowSettings {
    fn name(&self) -> &'static str {
        "Windows"
    }

    fn values(&mut self) -> Box<[arcane_settings::SettingsValueCommon]> {
        let all_full_border = self.all_full_border;
        let mut options = vec![
            arcane_settings::SettingsValueCommon {
                name: "focus_border_type",
                value: arcane_settings::SettingsValue::Selection(
                    &mut self.focus_border_type,
                    &["Double", "Rounded", "Plain"],
                ),
            },
            arcane_settings::SettingsValueCommon {
                name: "other_border_type",
                value: arcane_settings::SettingsValue::Selection(
                    &mut self.other_border_type,
                    &["Double", "Rounded", "Plain"],
                ),
            },
            arcane_settings::SettingsValueCommon {
                name: "all_full_border",
                value: arcane_settings::SettingsValue::Toogle(&mut self.all_full_border),
            },
            arcane_settings::SettingsValueCommon {
                name: "always_show_tab_bar",
                value: arcane_settings::SettingsValue::Toogle(&mut self.always_show_tab_bar),
            },
        ];
        if !all_full_border {
            options.push(arcane_settings::SettingsValueCommon {
                name: "focus_full_border",
                value: arcane_settings::SettingsValue::Toogle(&mut self.focus_full_border),
            });
        }
        options.into_boxed_slice()
    }
}

/// The plugin
pub struct WindowPlugin {
    /// The windows in view
    windows: HashMap<WindowID, Box<dyn Window>>,
    /// the next free ID
    next_free: WindowID,
    /// The list of tabs
    tabs: Vec<Vec<WindowID>>,
    /// The currently focused window
    focused_window: usize,
    /// The currently focused tab
    focused_tab: usize,
}

arcane_core::register_plugin!(WindowPlugin);

impl WindowPlugin {
    /// Re assign all window ids
    ///
    /// Warning: This messes up references from the location datas, which I am honestly fine with
    /// because this function is super rare to trigger, and you are asking for strange stuff
    /// honestly.
    ///
    /// Considering this function is only useful when you hit >255 opened windows, but have also
    /// closed some.
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

/// Ui Events for windows
#[derive(Clone, Debug, Serialize, Deserialize)]
enum WindowUiEvent {
    /// Move focus to the left
    #[debug("Window::FocusLeft")]
    FocusLeft,
    /// Move focus to the right
    #[debug("Window::FocusRight")]
    FocusRight,
    /// Delete the window that is in focus
    #[debug("Window::Close")]
    DeleteFocus,
    /// Move the window to the left
    #[debug("Window::MoveLeft")]
    MoveLeft,
    /// Move the window to the right
    #[debug("Window::MoveRight")]
    MoveRight,
    /// Create new tab
    #[debug("Window::NewTab")]
    NewTab,
    /// Move to the next tab
    #[debug("Window::NextTab")]
    NextTab,
    /// Move to the previous tab
    #[debug("Window::PreviousTab")]
    PreviousTab,
    /// Close a tab
    #[debug("Window::CloseTab")]
    CloseTab,
}

#[typetag::serde]
impl arcane_keybindings::BindResult for WindowUiEvent {}

impl arcane_core::Plugin for WindowPlugin {
    fn new() -> Self {
        WindowPlugin {
            windows: HashMap::new(),
            next_free: 0,
            tabs: vec![vec![]],
            focused_tab: 0,
            focused_window: 0,
        }
    }
    fn on_load(&mut self, events: &mut arcane_core::EventManager) -> Result<()> {
        events.dispatch(arcane_settings::RegisterSettings(Box::new(
            WindowSettings::default(),
        )));

        events.ensure_event::<WindowUiEvent>();
        events.dispatch(RegisterKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::CONTROL,
                key: KeyCode::Char('h'),
            },
            WindowUiEvent::FocusLeft,
        ));
        events.dispatch(RegisterKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::CONTROL,
                key: KeyCode::Char('l'),
            },
            WindowUiEvent::FocusRight,
        ));
        events.dispatch(RegisterKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::CONTROL,
                key: KeyCode::Char('w'),
            },
            WindowUiEvent::DeleteFocus,
        ));
        events.dispatch(RegisterKeybind::chord(
            [
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('m'),
                },
                KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Char('h'),
                },
            ],
            WindowUiEvent::MoveLeft,
        ));
        events.dispatch(RegisterKeybind::chord(
            [
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('m'),
                },
                KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Char('l'),
                },
            ],
            WindowUiEvent::MoveRight,
        ));
        events.dispatch(RegisterKeybind::chord(
            [
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('t'),
                },
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('t'),
                },
            ],
            WindowUiEvent::NewTab,
        ));
        events.dispatch(RegisterKeybind::chord(
            [
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('t'),
                },
                KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Char('n'),
                },
            ],
            WindowUiEvent::NextTab,
        ));
        events.dispatch(RegisterKeybind::chord(
            [
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('t'),
                },
                KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Char('p'),
                },
            ],
            WindowUiEvent::PreviousTab,
        ));
        events.dispatch(RegisterKeybind::chord(
            [
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('t'),
                },
                KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Char('c'),
                },
            ],
            WindowUiEvent::CloseTab,
        ));

        Ok(())
    }

    fn update(
        &mut self,
        events: &mut arcane_core::EventManager,
        plugins: &arcane_core::PluginStore,
    ) -> Result<()> {
        let focused_window_id = self
            .tabs
            .get(self.focused_tab)
            .and_then(|tab| tab.get(self.focused_window))
            .copied()
            .unwrap_or_default();
        for (window_id, window) in &mut self.windows {
            window.update(events, plugins, *window_id == focused_window_id, *window_id)?;
        }

        let (reader, mut writer) = events.split();
        for event in reader.read::<WindowUiEvent>() {
            match event {
                WindowUiEvent::FocusLeft => {
                    self.focused_window = self.focused_window.saturating_sub(1);
                }
                WindowUiEvent::FocusRight => {
                    self.focused_window = self.focused_window.saturating_add(1);
                }
                WindowUiEvent::DeleteFocus => {
                    writer.dispatch(WindowEvent::CloseWindow(focused_window_id));
                }
                WindowUiEvent::MoveLeft => {
                    let target = self.focused_window.saturating_sub(1);
                    if let Some(current_tab) = self.tabs.get_mut(self.focused_tab) {
                        if target < current_tab.len() {
                            current_tab.swap(self.focused_window, target);
                            self.focused_window = target;
                        }
                    }
                }
                WindowUiEvent::MoveRight => {
                    let target = self.focused_window.saturating_add(1);
                    if let Some(current_tab) = self.tabs.get_mut(self.focused_tab) {
                        if target < current_tab.len() {
                            current_tab.swap(self.focused_window, target);
                            self.focused_window = target;
                        }
                    }
                }
                WindowUiEvent::NewTab => {
                    self.tabs.push(vec![]);
                    self.focused_tab = self.tabs.len().saturating_sub(1);
                }
                WindowUiEvent::NextTab => {
                    self.focused_tab = self.focused_tab.saturating_add(1);
                }
                WindowUiEvent::PreviousTab => {
                    self.focused_tab = self.focused_tab.saturating_sub(1);
                }
                WindowUiEvent::CloseTab => {
                    if self.focused_tab < self.tabs.len() {
                        let removed_tab = self.tabs.remove(self.focused_tab);
                        for window_id in removed_tab {
                            writer.dispatch(WindowEvent::CloseWindow(window_id));
                        }
                        if self.tabs.is_empty() {
                            self.tabs.push(vec![]);
                        }
                    }
                }
            }
        }

        for event in events.read::<WindowEvent>() {
            match event {
                WindowEvent::CreateWindow(window) => {
                    let id = self.next_free;
                    event!(Level::DEBUG, "Created window {id}");
                    if let Some(new_id) = self.next_free.checked_add(1) {
                        self.next_free = new_id;
                    } else {
                        event!(Level::WARN, "Ran out of window IDS, Filling gaps");
                        self.fill_gaps()?;
                    }
                    self.windows.insert(id, dyn_clone::clone_box(&**window));
                    if let Some(current_tab) = self.tabs.get_mut(self.focused_tab) {
                        current_tab.push(id);
                        self.focused_window = current_tab.len().saturating_sub(1);
                    }
                }
                WindowEvent::CloseWindow(id) => {
                    event!(Level::DEBUG, "Deleting window {id}");
                    if let Some(mut removed_window) = self.windows.remove(id) {
                        removed_window.on_remove(events, plugins)?;
                    }
                    for tab in &mut self.tabs {
                        tab.retain(|window_id| window_id != id);
                    }
                }
            }
        }

        self.focused_tab = self.focused_tab.min(self.tabs.len().saturating_sub(1));
        if let Some(current_tab) = self.tabs.get(self.focused_tab) {
            self.focused_window = self.focused_window.min(current_tab.len().saturating_sub(1));
        }

        Ok(())
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        mut area: ratatui::prelude::Rect,
        plugins: &arcane_core::PluginStore,
    ) {
        if self.windows.is_empty() {
            let text = Paragraph::new("No Windows Open!").red();
            frame.render_widget(text, area);
            return;
        }

        let Some(settings_ref) = arcane_settings::get_settings::<WindowSettings>(plugins) else {
            return;
        };
        let settings = settings_ref.clone();
        drop(settings_ref);

        let Some(current_tab) = self.tabs.get(self.focused_tab) else {
            return;
        };
        let windows = current_tab
            .iter()
            .filter_map(|id| self.windows.get(id))
            .collect::<Vec<_>>();

        if self.tabs.len() > 1 || settings.always_show_tab_bar {
            let [tab_bar_area, new_area] =
                Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas::<2>(area);
            area = new_area;

            let tab_bar = Tabs::new(self.tabs.iter().enumerate().map(|(index, tab)| {
                format!(
                    "{}{}",
                    if index == self.focused_tab { "> " } else { "" },
                    tab.len()
                )
            }))
            .select(self.focused_tab)
            .divider(" | ")
            .highlight_style(Style::default().yellow())
            .on_black();
            frame.render_widget(tab_bar, tab_bar_area);
        }

        let layout =
            Layout::horizontal(windows.iter().map(|window| window.horizontal_constraints()))
                .split(area);

        for (position, window) in windows.into_iter().enumerate() {
            let focused = position == self.focused_window;

            let borders = if current_tab.len() == 1 {
                Borders::NONE
            } else if settings.all_full_border {
                Borders::ALL
            } else if focused {
                if settings.focus_full_border {
                    Borders::ALL
                } else {
                    Borders::LEFT | Borders::RIGHT
                }
            } else if position != 0 && position.saturating_sub(1) != self.focused_window {
                Borders::LEFT
            } else {
                Borders::NONE
            };
            let (color, border_type) = if focused {
                (
                    Color::LightYellow,
                    BorderType::from_str(&settings.focus_border_type).unwrap_or_default(),
                )
            } else {
                (
                    Color::LightGreen,
                    BorderType::from_str(&settings.other_border_type).unwrap_or_default(),
                )
            };

            let block = Block::default()
                .borders(borders)
                .fg(color)
                .border_type(border_type);

            let block = if borders.contains(Borders::TOP) {
                block.title_top(window.name())
            } else {
                block
            };

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

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::disallowed_methods)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use arcane_core::{Plugin, StateManager};

    use super::*;

    #[derive(Clone)]
    struct TestWindow {
        update_calls: Rc<RefCell<u8>>,
    }
    impl Window for TestWindow {
        fn name(&self) -> String {
            String::from("Test")
        }
        fn update(
            &mut self,
            _events: &mut arcane_core::EventManager,
            _plugins: &arcane_core::PluginStore,
            _focused: bool,
            _id: super::WindowID,
        ) -> Result<()> {
            *self.update_calls.borrow_mut() += 1;
            Ok(())
        }
        fn draw(
            &self,
            _frame: &mut ratatui::Frame,
            _area: ratatui::prelude::Rect,
            _plugins: &arcane_core::PluginStore,
        ) {
            *self.update_calls.borrow_mut() += 1;
        }
    }

    #[test]
    fn delete_on_empty() {
        let mut states = StateManager::new();
        states.plugins.insert(WindowPlugin::new());
        states.on_load().unwrap();

        states.events.dispatch(WindowEvent::CloseWindow(0));
        states.events.swap_buffers();
        states.update().unwrap();
    }

    #[test]
    fn create_window() {
        let mut states = StateManager::new();
        states.plugins.insert(WindowPlugin::new());
        states.on_load().unwrap();

        let update_calls = Rc::new(RefCell::new(0));

        states
            .events
            .dispatch(WindowEvent::CreateWindow(Box::new(TestWindow {
                update_calls: Rc::clone(&update_calls),
            })));
        states.events.swap_buffers();
        states.update().unwrap();
        states.events.swap_buffers();
        states.update().unwrap();

        assert_eq!(*update_calls.borrow(), 1);
    }

    #[test]
    fn destroy_window() {
        let mut states = StateManager::new();
        states.plugins.insert(WindowPlugin::new());
        states.on_load().unwrap();

        let update_calls = Rc::new(RefCell::new(0));

        states
            .events
            .dispatch(WindowEvent::CreateWindow(Box::new(TestWindow {
                update_calls: Rc::clone(&update_calls),
            })));
        states.events.swap_buffers();
        states.update().unwrap();
        states.events.swap_buffers();
        states.update().unwrap();

        states.events.dispatch(WindowEvent::CloseWindow(0));
        states.events.swap_buffers();
        states.update().unwrap();
        states.events.swap_buffers();
        states.update().unwrap();

        assert_eq!(*update_calls.borrow(), 2);
    }

    #[test]
    fn inserting_over_cap() {
        let mut states = StateManager::new();
        states.plugins.insert(WindowPlugin::new());
        states.on_load().unwrap();

        for _ in 0..=256 {
            states
                .events
                .dispatch(WindowEvent::CreateWindow(Box::new(TestWindow {
                    update_calls: Rc::default(),
                })));
        }
        states.events.swap_buffers();
        assert!(states.update().is_err());
    }

    #[test]
    fn overflow_id_but_with_gaps() {
        let mut states = StateManager::new();
        states.plugins.insert(WindowPlugin::new());
        states.on_load().unwrap();

        for _ in 0..200 {
            states
                .events
                .dispatch(WindowEvent::CreateWindow(Box::new(TestWindow {
                    update_calls: Rc::default(),
                })));
        }
        states.events.swap_buffers();
        states.update().unwrap();

        for i in 0..100 {
            states.events.dispatch(WindowEvent::CloseWindow(i));
        }
        states.events.swap_buffers();
        states.update().unwrap();

        for _ in 0..100 {
            states
                .events
                .dispatch(WindowEvent::CreateWindow(Box::new(TestWindow {
                    update_calls: Rc::default(),
                })));
        }
        states.events.swap_buffers();
        assert!(states.update().is_ok());
    }
}

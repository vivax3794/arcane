//! Handles abstracting actions into keybindings
#![feature(iter_intersperse)]

use arcane_core::Result;
use arcane_keybindings::{
    Chord,
    KeyBind,
    KeyCode,
    KeyModifiers,
    KeybindPlugin,
    LockKeybindings,
    MenuEvent,
    RebindKeybind,
    RegisterKeybind,
};
use arcane_windows::{Window, WindowEvent};
use error_mancer::errors;
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Matcher, Utf32Str};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Row, Table};
use serde::{Deserialize, Serialize};

/// The key sequence to mark the end of a key chord
const CHORD_END: KeyBind = KeyBind {
    modifiers: KeyModifiers::CONTROL,
    key: KeyCode::Esc,
};

/// Open the keybinding menu
#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenKeybindings;

#[typetag::serde]
impl arcane_keybindings::BindResult for OpenKeybindings {}

pub struct KeybindingWindowPlugin;

arcane_core::register_plugin!(KeybindingWindowPlugin);

impl arcane_core::Plugin for KeybindingWindowPlugin {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self
    }

    fn on_load(&mut self, events: &mut arcane_core::EventManager) -> Result<()> {
        events.ensure_event::<OpenKeybindings>();
        events.dispatch(RegisterKeybind::chord(
            [
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('p'),
                },
                KeyBind {
                    modifiers: KeyModifiers::CONTROL,
                    key: KeyCode::Char('k'),
                },
            ],
            OpenKeybindings,
        ));

        Ok(())
    }

    fn update(
        &mut self,
        events: &mut arcane_core::EventManager,
        _plugins: &arcane_core::PluginStore,
    ) -> Result<()> {
        let (reader, mut writer) = events.split();
        for _ in reader.read::<OpenKeybindings>() {
            writer.dispatch(WindowEvent::CreateWindow(
                Box::new(KeybindWindow::default()),
            ));
        }

        Ok(())
    }
}

/// A window to see and configure keybindings
#[derive(Clone, Default)]
struct KeybindWindow {
    /// The keys visible in the window
    visible_keys: Vec<(String, String)>,
    /// The fuzzy matcher
    fuzzy_matcher: Matcher,
    /// The search bar input
    search: String,
    /// The focused element
    focused_element: usize,
    /// Element is selected
    element_selected: bool,
    /// The timer for when to blink the cursor
    cursor_blink: f32,
    /// The currently being recorded keybind
    recording: Vec<KeyBind>,
}

impl Window for KeybindWindow {
    fn name(&self) -> String {
        String::from("Keybinds")
    }

    #[errors()]
    fn update(
        &mut self,
        events: &mut arcane_core::EventManager,
        plugins: &arcane_core::PluginStore,
        focused: bool,
        _id: arcane_windows::WindowID,
    ) -> Result<()> {
        let Some(keybinds) = plugins.get::<KeybindPlugin>() else {
            return Ok(());
        };

        if focused {
            let (reader, mut writer) = events.split();
            for event in reader.read::<MenuEvent>() {
                match event {
                    MenuEvent::Select => {
                        if self.focused_element == 0 {
                            self.element_selected = !self.element_selected;
                        } else if !self.element_selected {
                            self.element_selected = true;
                            writer.dispatch(arcane_keybindings::LockKeybindings(true));
                        }
                    }
                    MenuEvent::Down if !self.element_selected => {
                        self.focused_element = self.focused_element.saturating_add(1);
                    }
                    MenuEvent::Up if !self.element_selected => {
                        self.focused_element = self.focused_element.saturating_sub(1);
                    }
                    _ => (),
                }
            }

            if self.focused_element != 0 && self.element_selected {
                let (reader, mut writer) = events.split();
                for event in reader.read::<arcane_core::KeydownEvent>() {
                    let keybind = KeyBind {
                        modifiers: event.0.modifiers,
                        key: event.0.code,
                    };
                    if keybind.is_only_modifiers() {
                        continue;
                    }
                    if keybind == CHORD_END {
                        let chord = Chord {
                            keys: std::mem::take(&mut self.recording).into_boxed_slice(),
                        };

                        if let Some((_, action)) = self
                            .visible_keys
                            .get(self.focused_element.saturating_sub(1))
                        {
                            writer.dispatch(RebindKeybind {
                                bind: chord,
                                event: action.clone(),
                            });
                        }

                        self.element_selected = false;
                        writer.dispatch(LockKeybindings(false));
                    } else {
                        self.recording.push(keybind);
                    }
                }
            }

            if self.focused_element == 0 && self.element_selected {
                for event in events.read::<arcane_core::KeydownEvent>() {
                    match event.0.code {
                        KeyCode::Char(c) => {
                            self.search.push(c);
                        }
                        KeyCode::Backspace => {
                            self.search.pop();
                        }
                        _ => {}
                    }
                }

                for event in events.read::<arcane_core::DeltaTimeEvent>() {
                    self.cursor_blink += event.0.as_secs_f32();
                    self.cursor_blink %= 1.0;
                }
            }
        }

        let mut rows = keybinds
            .raw_bindings
            .iter()
            .flat_map(|(key, action)| action.iter().map(move |action| (key, action)))
            .map(|(key, action)| (key.render(), format!("{action:?}")))
            .collect::<Vec<_>>();

        if self.search.is_empty() {
            rows.sort_by(|(_, a1), (_, a2)| a1.cmp(a2));
        } else {
            let pattern = nucleo_matcher::pattern::Pattern::new(
                &self.search,
                CaseMatching::Smart,
                Normalization::Smart,
                AtomKind::Fuzzy,
            );
            let mut what_is_this_for = Vec::new();
            rows.sort_by_key(|(_, value)| {
                pattern.score(
                    Utf32Str::new(value, &mut what_is_this_for),
                    &mut self.fuzzy_matcher,
                )
            });
            rows.reverse();
        }
        self.visible_keys = rows;

        Ok(())
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        _plugins: &arcane_core::PluginStore,
    ) {
        let area = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas::<2>(area);

        let cursor =
            if self.focused_element == 0 && self.element_selected && self.cursor_blink > 0.5 {
                Color::White
            } else {
                Color::Black
            };
        let background = if self.focused_element == 0 && self.element_selected {
            Color::DarkGray
        } else if self.focused_element == 0 {
            Color::Black
        } else {
            Color::Rgb(20, 20, 40)
        };
        let text = Line::from(vec![self.search.clone().into(), "_".fg(cursor)]).bg(background);
        frame.render_widget(text, area[0]);

        let rows = self
            .visible_keys
            .iter()
            .enumerate()
            .map(|(i, (key, action))| {
                let background =
                    if i.saturating_add(1) == self.focused_element && self.element_selected {
                        Color::DarkGray
                    } else if i.saturating_add(1) == self.focused_element {
                        Color::Black
                    } else {
                        Color::Reset
                    };
                let key = if self.element_selected && i.saturating_add(1) == self.focused_element {
                    self.recording
                        .iter()
                        .map(KeyBind::render)
                        .intersperse(" ".into())
                        .collect::<String>()
                } else {
                    key.clone()
                };
                Row::new([key, action.clone()]).bg(background)
            });

        let table = Table::new(rows, [Constraint::Fill(1), Constraint::Fill(1)]);
        frame.render_widget(table, area[1]);
    }
}

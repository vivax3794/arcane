//! Handles abstracting actions into keybindings

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::create_dir_all;

use crossterm::event::{KeyCode, KeyModifiers, ModifierKeyCode};
use derive_more::derive::Debug;
use dyn_clone::DynClone;
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Matcher, Utf32Str};
use ouroboros::self_referencing;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Row, Table};
use serde::{Deserialize, Serialize};
use trie_rs::inc_search::{Answer, IncSearch};
use trie_rs::map::{Trie, TrieBuilder};

use crate::editor::{DeltaTimeEvent, KeydownEvent};
use crate::plugin_manager::{Plugin, RawEvent};
use crate::prelude::*;
use crate::project_dirs;

/// A keybind that can be matched against
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct KeyBind {
    /// Modifiers that need to match exactly
    pub modifiers: KeyModifiers,
    /// Key that needs to be pressed
    pub key: KeyCode,
}

impl KeyBind {
    /// Checks if the keybind is only a modifier key being pressed
    fn is_only_modifiers(&self) -> bool {
        match self.modifiers {
            KeyModifiers::CONTROL => {
                self.key == KeyCode::Modifier(ModifierKeyCode::LeftControl)
                    || self.key == KeyCode::Modifier(ModifierKeyCode::RightControl)
            }
            KeyModifiers::ALT => {
                self.key == KeyCode::Modifier(ModifierKeyCode::LeftAlt)
                    || self.key == KeyCode::Modifier(ModifierKeyCode::RightAlt)
            }
            KeyModifiers::SHIFT => {
                self.key == KeyCode::Modifier(ModifierKeyCode::LeftShift)
                    || self.key == KeyCode::Modifier(ModifierKeyCode::RightShift)
            }
            KeyModifiers::META => {
                self.key == KeyCode::Modifier(ModifierKeyCode::LeftMeta)
                    || self.key == KeyCode::Modifier(ModifierKeyCode::RightMeta)
            }
            KeyModifiers::SUPER => {
                self.key == KeyCode::Modifier(ModifierKeyCode::LeftSuper)
                    || self.key == KeyCode::Modifier(ModifierKeyCode::RightSuper)
            }
            KeyModifiers::HYPER => {
                self.key == KeyCode::Modifier(ModifierKeyCode::LeftHyper)
                    || self.key == KeyCode::Modifier(ModifierKeyCode::RightHyper)
            }
            _ => false,
        }
    }
}

impl PartialOrd for KeyBind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for KeyBind {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if let Some(order) = self.modifiers.partial_cmp(&other.modifiers) {
            if order != Ordering::Equal {
                return order;
            }
        }
        self.key.to_string().cmp(&other.key.to_string())
    }
}

impl KeyBind {
    /// Get a string version of the keybind
    fn render(&self) -> String {
        if self.modifiers.is_empty() {
            format!("{}", self.key)
        } else {
            format!("{}+{}", self.modifiers, self.key)
        }
    }
}

/// Stores a list of keys
#[derive(PartialEq, Eq, Hash, Clone, PartialOrd, Ord, Serialize, Deserialize, Debug)]
pub struct Chord {
    /// The order of keys that need to be hit
    pub keys: Box<[KeyBind]>,
}

impl Chord {
    /// Render a human readable version of the binding
    fn render(&self) -> String {
        self.keys
            .iter()
            .map(KeyBind::render)
            .intersperse(String::from(" "))
            .collect::<String>()
    }
}

/// Trait that implements everything a event needs to be dispatched by the keybinding system
#[typetag::serde(tag = "event", content = "data")]
pub trait BindResult: RawEvent + DynClone + std::fmt::Debug {}

/// How is a keybinding event stored
type KeyBindEvent = Box<dyn BindResult>;

/// Set the keybind
pub struct RegisterKeybind {
    /// The actual keybind
    pub bind: Chord,
    /// The event to dispatch on this event
    pub event: KeyBindEvent,
}

/// Delete a keybind
struct DeleteKeybind(Chord);

impl RegisterKeybind {
    /// Shortcut for a keybiding with a single key
    pub fn single_key<E>(key: KeyBind, event: E) -> Self
    where
        E: BindResult + 'static,
    {
        Self {
            bind: Chord {
                keys: Box::new([key]),
            },
            event: Box::new(event),
        }
    }

    /// Shortcut for a keybiding with a chord key
    pub fn chord<E>(keys: impl IntoIterator<Item = KeyBind>, event: E) -> Self
    where
        E: BindResult + 'static,
    {
        Self {
            bind: Chord {
                keys: keys.into_iter().collect::<Vec<_>>().into_boxed_slice(),
            },
            event: Box::new(event),
        }
    }
}

/// This holds a immutable trie tree, and a mutable incremental search of it
#[self_referencing]
struct TrieHolder {
    bindings_tree: Trie<KeyBind, KeyBindEvent>,
    #[borrows(bindings_tree)]
    #[covariant]
    search: IncSearch<'this, KeyBind, KeyBindEvent>,
}

impl TrieHolder {
    /// Create the trie tree from the hashmap
    fn from_raw(raw: &HashMap<Chord, KeyBindEvent>) -> Self {
        let mut builder = TrieBuilder::new();
        for (chord, event) in raw {
            builder.push(chord.keys.clone(), dyn_clone::clone_box(&**event));
        }

        TrieHolderBuilder {
            bindings_tree: builder.build(),
            search_builder: |tree| tree.inc_search(),
        }
        .build()
    }

    ///Search for possible keybinds
    fn search(&mut self, key: &KeyBind) -> Option<Answer> {
        self.with_search_mut(|search| search.query(key))
    }

    /// Get the current match when there is some
    fn get_match(&mut self) -> Option<&KeyBindEvent> {
        self.with_search_mut(|search| search.value())
    }

    /// Clear the search
    fn clear(&mut self) {
        // The suggestion causes a compiler error
        #[allow(clippy::redundant_closure_for_method_calls)]
        self.with_search_mut(|search| search.reset());
    }
}

/// Handles keybindings
pub struct KeybindPlugin {
    /// Holds the bindings, the key is a combimation of the owning plugin and the name
    raw_bindings: HashMap<Chord, KeyBindEvent>,
    /// The trie tree
    trie: TrieHolder,
    /// Should keybindings be emmitted
    enabled: bool,
}

/// Generic events to move around a menu
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MenuEvent {
    /// Move to the left
    #[debug("Menu::Left")]
    Left,
    /// Move to the right
    #[debug("Menu::Right")]
    Right,
    /// Move to the up
    #[debug("Menu::Up")]
    Up,
    /// Move to the down
    #[debug("Menu::Down")]
    Down,
    /// select something
    #[debug("Menu::Select")]
    Select,
    /// alt select something
    #[debug("Menu::Alt Select")]
    AltSelect,
}

#[typetag::serde]
impl BindResult for MenuEvent {}

/// Open the keybinding menu
#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenKeybindings;

#[typetag::serde]
impl BindResult for OpenKeybindings {}

/// Disable or Enable keybindings
#[derive(Clone, Debug, Serialize, Deserialize)]
struct LockKeybindings(bool);

impl KeybindPlugin {
    /// Create a empty plugin
    pub(super) fn new() -> Self {
        Self {
            raw_bindings: HashMap::new(),
            trie: TrieHolder::from_raw(&HashMap::new()),
            enabled: true,
        }
    }
}

impl Plugin for KeybindPlugin {
    #[errors(serde_json::Error)]
    fn on_load(&mut self, events: &mut EventManager) -> Result<()> {
        events.ensure_event::<MenuEvent>();
        events.dispatch(RegisterKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Char('h'),
            },
            MenuEvent::Left,
        ));
        events.dispatch(RegisterKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Char('l'),
            },
            MenuEvent::Right,
        ));
        events.dispatch(RegisterKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Char('j'),
            },
            MenuEvent::Down,
        ));
        events.dispatch(RegisterKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Char('k'),
            },
            MenuEvent::Up,
        ));
        events.dispatch(RegisterKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Enter,
            },
            MenuEvent::Select,
        ));
        events.dispatch(RegisterKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::SHIFT,
                key: KeyCode::Enter,
            },
            MenuEvent::AltSelect,
        ));

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

        if let Some(project_directory) = project_dirs() {
            let config_path = project_directory.config_dir().join("keybinds.json");
            if let Ok(file) = std::fs::File::open(&config_path) {
                let data: Vec<(Chord, KeyBindEvent)> = serde_json::from_reader(file)?;
                event!(Level::DEBUG, "Loaded {} keybinds", data.len());
                event!(Level::DEBUG, "Data: {:?}", data);
                self.raw_bindings = data.into_iter().collect();
                self.trie = TrieHolder::from_raw(&self.raw_bindings);
            }
        }

        Ok(())
    }

    #[errors(std::io::Error, serde_json::Error)]
    fn update(
        &mut self,
        events: &mut crate::plugin_manager::EventManager,
        _plugins: &crate::plugin_manager::PluginStore,
    ) -> color_eyre::eyre::Result<()> {
        let mut bindings_modified = false;
        // Very important to remove first
        for event in events.read::<DeleteKeybind>() {
            self.raw_bindings.remove(&event.0);
            bindings_modified = true;
        }
        for event in events.read::<RegisterKeybind>() {
            if self
                .raw_bindings
                .iter()
                .any(|(_, action)| format!("{action:?}") == format!("{:?}", event.event))
            {
                event!(Level::TRACE, "Keybind {:?} already exists", event.event);
            } else {
                event!(Level::DEBUG, "Registering keybind: {}", event.bind.render());
                self.raw_bindings
                    .insert(event.bind.clone(), dyn_clone::clone_box(&*event.event));
                bindings_modified = true;
            }
        }

        if bindings_modified {
            self.trie = TrieHolder::from_raw(&self.raw_bindings);

            if let Some(project_directory) = project_dirs() {
                let config_dir = project_directory.config_dir();
                create_dir_all(config_dir)?;

                let keybinds_path = config_dir.join("keybinds.json");
                event!(Level::INFO, "Saving keybinds to {keybinds_path:?}");
                let file = std::fs::File::create(keybinds_path)?;
                let bindings = self.raw_bindings.iter().collect::<Vec<_>>();
                serde_json::ser::to_writer_pretty(file, &bindings)?;
            }
        }

        for event in events.read::<LockKeybindings>() {
            self.enabled = !event.0;
        }

        let (reader, mut writer) = events.split();
        if self.enabled {
            for event in reader.read::<KeydownEvent>() {
                let keybind = KeyBind {
                    modifiers: event.0.modifiers,
                    key: event.0.code,
                };
                if keybind.is_only_modifiers() {
                    continue;
                }
                match self.trie.search(&keybind) {
                    None => {
                        event!(Level::TRACE, "No match for {keybind:?}");
                        if let Some(event) = self.trie.get_match() {
                            event!(Level::DEBUG, "Emitting {event:?}");
                            let event = dyn_clone::clone_box(&**event);
                            writer.dispatch_raw(event as Box<dyn RawEvent>);
                        }
                        self.trie.clear();
                    }
                    Some(Answer::Match) => {
                        event!(Level::DEBUG, "Match for {keybind:?}");
                        if let Some(event) = self.trie.get_match() {
                            event!(Level::DEBUG, "Emitting {event:?}");
                            let event = dyn_clone::clone_box(&**event);
                            writer.dispatch_raw(event as Box<dyn RawEvent>);
                        }
                        self.trie.clear();
                    }
                    Some(Answer::Prefix | Answer::PrefixAndMatch) => {
                        event!(Level::DEBUG, "Prefix match for {keybind:?}");
                    }
                }
            }
        }
        for _ in reader.read::<OpenKeybindings>() {
            writer.dispatch(WindowEvent::CreateWindow(
                Box::new(KeybindWindow::default()),
            ));
        }

        Ok(())
    }
}

/// The key sequence to mark the end of a key chord
const CHORD_END: KeyBind = KeyBind {
    modifiers: KeyModifiers::CONTROL,
    key: KeyCode::Esc,
};

/// A window to see and configure keybindings
#[derive(Clone, Default)]
struct KeybindWindow {
    /// The keys visible in the window
    visible_keys: Vec<(String, String, usize)>,
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
        events: &mut EventManager,
        plugins: &PluginStore,
        focused: bool,
        _id: super::windows::WindowID,
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
                            writer.dispatch(LockKeybindings(true));
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
                for event in reader.read::<KeydownEvent>() {
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

                        let keybind_index = self
                            .visible_keys
                            .get(self.focused_element.saturating_sub(1))
                            .map(|(_, _, i)| *i)
                            .unwrap_or_default();
                        if let Some((current_chord, action)) =
                            keybinds.raw_bindings.iter().nth(keybind_index)
                        {
                            writer.dispatch(DeleteKeybind(current_chord.clone()));
                            writer.dispatch(RegisterKeybind {
                                bind: chord,
                                event: dyn_clone::clone_box(&**action),
                            });
                            writer.dispatch(LockKeybindings(false));
                            self.element_selected = false;
                        };
                    } else {
                        self.recording.push(keybind);
                    }
                }
            }

            if self.focused_element == 0 && self.element_selected {
                for event in events.read::<KeydownEvent>() {
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

                for event in events.read::<DeltaTimeEvent>() {
                    self.cursor_blink += event.0.as_secs_f32();
                    self.cursor_blink %= 1.0;
                }
            }
        }

        let mut rows = keybinds
            .raw_bindings
            .iter()
            .enumerate()
            .map(|(i, (key, action))| (key.render(), format!("{action:?}"), i))
            .collect::<Vec<_>>();

        if self.search.is_empty() {
            rows.sort_by(|(_, a1, _), (_, a2, _)| a1.cmp(a2));
        } else {
            let pattern = nucleo_matcher::pattern::Pattern::new(
                &self.search,
                CaseMatching::Smart,
                Normalization::Smart,
                AtomKind::Fuzzy,
            );
            let mut what_is_this_for = Vec::new();
            rows.sort_by_key(|(_, value, _)| {
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
        _plugins: &crate::plugin_manager::PluginStore,
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
            .map(|(i, (key, action, _))| {
                let background =
                    if i.saturating_add(1) == self.focused_element && self.element_selected {
                        Color::DarkGray
                    } else if i.saturating_add(1) == self.focused_element {
                        Color::Black
                    } else {
                        Color::Reset
                    };
                Row::new([key.clone(), action.clone()]).bg(background)
            });

        let table = Table::new(rows, [Constraint::Fill(1), Constraint::Fill(1)]);
        frame.render_widget(table, area[1]);
    }
}

#[coverage(off)]
#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::{BindResult, Deserialize, KeybindPlugin, RegisterKeybind, Serialize};
    use crate::plugin_loading::keybindings::{Chord, DeleteKeybind};
    use crate::plugin_manager::StateManager;
    use crate::KeydownEvent;

    #[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
    enum TestEvent {
        Bar,
        Foo,
    }

    #[typetag::serde]
    impl BindResult for TestEvent {}

    #[test]
    fn single_key() {
        let mut state = StateManager::new();
        state.plugins.insert(KeybindPlugin::new());
        state.events.ensure_event::<TestEvent>();
        state.events.dispatch(RegisterKeybind::single_key(
            super::KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Up,
            },
            TestEvent::Foo,
        ));

        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Up,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[TestEvent::Foo]);
    }

    #[test]
    fn single_key_miss_first() {
        let mut state = StateManager::new();
        state.plugins.insert(KeybindPlugin::new());
        state.events.ensure_event::<TestEvent>();
        state.events.dispatch(RegisterKeybind::single_key(
            super::KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Up,
            },
            TestEvent::Foo,
        ));

        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Down,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();
        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[]);

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Up,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[TestEvent::Foo]);
    }

    #[test]
    fn chord() {
        let mut state = StateManager::new();
        state.plugins.insert(KeybindPlugin::new());
        state.events.ensure_event::<TestEvent>();
        state.events.dispatch(RegisterKeybind::chord(
            [
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Up,
                },
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Down,
                },
            ],
            TestEvent::Foo,
        ));

        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Up,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Down,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[TestEvent::Foo]);
    }

    #[test]
    fn chord_missed() {
        let mut state = StateManager::new();
        state.plugins.insert(KeybindPlugin::new());
        state.events.ensure_event::<TestEvent>();
        state.events.dispatch(RegisterKeybind::chord(
            [
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Up,
                },
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Down,
                },
            ],
            TestEvent::Foo,
        ));

        state.events.swap_buffers();
        state.update().unwrap();
        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[]);

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Down,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Up,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Down,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[TestEvent::Foo]);
    }

    #[test]
    fn chord_with_prefix_last() {
        let mut state = StateManager::new();
        state.plugins.insert(KeybindPlugin::new());
        state.events.ensure_event::<TestEvent>();
        state.events.dispatch(RegisterKeybind::chord(
            [
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Up,
                },
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Down,
                },
            ],
            TestEvent::Bar,
        ));
        state.events.dispatch(RegisterKeybind::chord(
            [super::KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Up,
            }],
            TestEvent::Foo,
        ));

        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Up,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();
        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[]);

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Down,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[TestEvent::Bar]);
    }

    #[test]
    fn chord_with_prefix_prefix() {
        let mut state = StateManager::new();
        state.plugins.insert(KeybindPlugin::new());
        state.events.ensure_event::<TestEvent>();
        state.events.dispatch(RegisterKeybind::chord(
            [
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Up,
                },
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Down,
                },
            ],
            TestEvent::Bar,
        ));
        state.events.dispatch(RegisterKeybind::chord(
            [super::KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Up,
            }],
            TestEvent::Foo,
        ));

        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Up,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();
        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[]);

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Left,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[TestEvent::Foo]);
    }

    #[test]
    fn activeate_chord_then_non_bind() {
        let mut state = StateManager::new();
        state.plugins.insert(KeybindPlugin::new());
        state.events.ensure_event::<TestEvent>();
        state.events.dispatch(RegisterKeybind::chord(
            [
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Up,
                },
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Down,
                },
            ],
            TestEvent::Bar,
        ));

        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Up,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();
        state.events.swap_buffers();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Down,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Left,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();
        state.events.swap_buffers();
        assert_ne!(state.events.read::<TestEvent>(), &[TestEvent::Bar]);
    }

    #[test]
    fn delete_key() {
        let mut state = StateManager::new();
        state.plugins.insert(KeybindPlugin::new());
        state.events.ensure_event::<TestEvent>();
        state.events.dispatch(RegisterKeybind::single_key(
            super::KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Up,
            },
            TestEvent::Foo,
        ));

        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(DeleteKeybind(Chord {
            keys: Box::new([super::KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Up,
            }]),
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Up,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();

        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[]);
    }
}

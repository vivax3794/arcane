//! Handles abstracting actions into keybindings

use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};
use dyn_clone::DynClone;
use ouroboros::self_referencing;
use ratatui::layout::Constraint;
use ratatui::widgets::{Row, Table};
use trie_rs::inc_search::{Answer, IncSearch};
use trie_rs::map::{Trie, TrieBuilder};

use crate::editor::KeydownEvent;
use crate::plugin_manager::{Plugin, RawEvent};
use crate::prelude::*;

/// A keybind that can be matched against
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBind {
    /// Modifiers that need to match exactly
    pub modifiers: KeyModifiers,
    /// Key that needs to be pressed
    pub key: KeyCode,
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

#[derive(PartialEq, Eq, Hash, Clone, PartialOrd, Ord)]
pub struct Chord {
    pub keys: Box<[KeyBind]>,
}

impl Chord {
    fn render(&self) -> String {
        self.keys
            .iter()
            .map(KeyBind::render)
            .intersperse(String::from(" "))
            .collect::<String>()
    }
}

trait BindResult: RawEvent + DynClone {}
impl<T: RawEvent + Clone> BindResult for T {}

type KeyBindEvent = Box<dyn BindResult>;

/// Set the keybind
pub struct SetKeybind {
    /// The actual keybind
    pub bind: Chord,
    /// The event to dispatch on this event
    pub event: KeyBindEvent,
}

impl SetKeybind {
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

#[self_referencing]
struct TrieHolder {
    bindings_tree: Trie<KeyBind, KeyBindEvent>,
    #[borrows(bindings_tree)]
    #[covariant]
    search: IncSearch<'this, KeyBind, KeyBindEvent>,
}

impl TrieHolder {
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

    fn search(&mut self, key: &KeyBind) -> Option<Answer> {
        self.with_search_mut(|search| search.query(key))
    }

    fn get_match(&mut self) -> Option<&KeyBindEvent> {
        self.with_search_mut(|search| search.value())
    }

    fn clear(&mut self) {
        self.with_search_mut(|search| search.reset());
    }
}

/// Handles keybindings
pub struct KeybindPlugin {
    /// Holds the bindings, the key is a combimation of the owning plugin and the name
    raw_bindings: HashMap<Chord, KeyBindEvent>,
    trie: TrieHolder,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MenuEvent {
    Left,
    Right,
    Up,
    Down,
    Select,
    AltSelect,
}

impl KeybindPlugin {
    /// Create a empty plugin
    pub(super) fn new() -> Self {
        Self {
            raw_bindings: HashMap::new(),
            trie: TrieHolder::from_raw(&HashMap::new()),
        }
    }
}

impl Plugin for KeybindPlugin {
    fn on_load(&mut self, events: &mut EventManager) -> Result<()> {
        events.ensure_event::<MenuEvent>();
        events.dispatch(SetKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Char('h'),
            },
            MenuEvent::Left,
        ));
        events.dispatch(SetKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Left,
            },
            MenuEvent::Left,
        ));
        events.dispatch(SetKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Char('l'),
            },
            MenuEvent::Right,
        ));
        events.dispatch(SetKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Right,
            },
            MenuEvent::Right,
        ));
        events.dispatch(SetKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Char('j'),
            },
            MenuEvent::Down,
        ));
        events.dispatch(SetKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Down,
            },
            MenuEvent::Down,
        ));
        events.dispatch(SetKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Char('k'),
            },
            MenuEvent::Up,
        ));
        events.dispatch(SetKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Up,
            },
            MenuEvent::Up,
        ));
        events.dispatch(SetKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Enter,
            },
            MenuEvent::Select,
        ));
        events.dispatch(SetKeybind::single_key(
            KeyBind {
                modifiers: KeyModifiers::SHIFT,
                key: KeyCode::Enter,
            },
            MenuEvent::AltSelect,
        ));
        Ok(())
    }

    fn update(
        &mut self,
        events: &mut crate::plugin_manager::EventManager,
        _plugins: &crate::plugin_manager::PluginStore,
    ) -> color_eyre::eyre::Result<()> {
        let mut bindings_modified = false;
        for event in events.read::<SetKeybind>() {
            self.raw_bindings
                .insert(event.bind.clone(), dyn_clone::clone_box(&*event.event));
            bindings_modified = true;
        }
        if bindings_modified {
            self.trie = TrieHolder::from_raw(&self.raw_bindings);
        }

        let (reader, mut writer) = events.split();
        for event in reader.read::<KeydownEvent>() {
            let keybind = KeyBind {
                modifiers: event.0.modifiers,
                key: event.0.code,
            };
            match self.trie.search(&keybind) {
                None => {
                    if let Some(event) = self.trie.get_match() {
                        let event = dyn_clone::clone_box(&**event);
                        writer.dispatch_raw(event as Box<dyn RawEvent>);
                    }
                    self.trie.clear();
                }
                Some(Answer::Match) => {
                    if let Some(event) = self.trie.get_match() {
                        let event = dyn_clone::clone_box(&**event);
                        writer.dispatch_raw(event as Box<dyn RawEvent>);
                    }
                    self.trie.clear();
                }
                Some(Answer::Prefix | Answer::PrefixAndMatch) => {}
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
            .raw_bindings
            .keys()
            .map(|key| Row::new([key.render()]));
        let table = Table::new(rows, [Constraint::Fill(1), Constraint::Fill(1)]);
        frame.render_widget(table, area);
    }
}

#[coverage(off)]
#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::{KeybindPlugin, SetKeybind};
    use crate::plugin_manager::StateManager;
    use crate::KeydownEvent;

    #[derive(Clone, PartialEq, Eq, Debug)]
    enum TestEvent {
        Bar,
        Foo,
    }

    #[test]
    fn single_key() {
        let mut state = StateManager::new();
        state.plugins.insert(KeybindPlugin::new());
        state.events.ensure_event::<TestEvent>();
        state.events.dispatch(SetKeybind::single_key(
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
        state.events.dispatch(SetKeybind::single_key(
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
        state.events.dispatch(SetKeybind::chord(
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
        state.events.dispatch(SetKeybind::chord(
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
        state.events.dispatch(SetKeybind::chord(
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
        state.events.dispatch(SetKeybind::chord(
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
        state.events.dispatch(SetKeybind::chord(
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
        state.events.dispatch(SetKeybind::chord(
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
        state.events.dispatch(SetKeybind::chord(
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
}

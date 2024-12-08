//! Handles abstracting actions into keybindings
#![feature(iter_intersperse)]
#![feature(trait_upcasting)]

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::create_dir_all;

use arcane_anymap::dyn_clone;
use arcane_core::{event, Level, Result};
pub use crossterm::event::{KeyCode, KeyModifiers, ModifierKeyCode};
use derive_more::derive::Debug;
use error_mancer::errors;
use ouroboros::self_referencing;
use serde::{Deserialize, Serialize};
use trie_rs::inc_search::{Answer, IncSearch};
use trie_rs::map::{Trie, TrieBuilder};

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
    pub const fn is_only_modifiers(&self) -> bool {
        matches!(
            (self.modifiers, &self.key),
            (
                KeyModifiers::CONTROL,
                KeyCode::Modifier(ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl)
            ) | (
                KeyModifiers::ALT,
                KeyCode::Modifier(ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt)
            ) | (
                KeyModifiers::SHIFT,
                KeyCode::Modifier(ModifierKeyCode::LeftShift | ModifierKeyCode::RightShift)
            ) | (
                KeyModifiers::META,
                KeyCode::Modifier(ModifierKeyCode::LeftMeta | ModifierKeyCode::RightMeta)
            ) | (
                KeyModifiers::SUPER,
                KeyCode::Modifier(ModifierKeyCode::LeftSuper | ModifierKeyCode::RightSuper)
            ) | (
                KeyModifiers::HYPER,
                KeyCode::Modifier(ModifierKeyCode::LeftHyper | ModifierKeyCode::RightHyper)
            )
        )
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
    pub fn render(&self) -> String {
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
    pub fn render(&self) -> String {
        self.keys
            .iter()
            .map(KeyBind::render)
            .intersperse(String::from(" "))
            .collect::<String>()
    }
}

/// Trait that implements everything a event needs to be dispatched by the keybinding system
#[typetag::serde(tag = "event", content = "data")]
pub trait BindResult: arcane_core::RawEvent + dyn_clone::DynClone + std::fmt::Debug {}

/// How is a keybinding event stored
pub type KeyBindEvent = Box<dyn BindResult>;

/// Set the keybind
pub struct RegisterKeybind {
    /// The actual keybind
    pub bind: Chord,
    /// The event to dispatch on this event
    pub event: KeyBindEvent,
}

/// Rebind a Keybind
pub struct RebindKeybind {
    /// The actual keybind
    pub bind: Chord,
    /// The event to dispatch on this event
    pub event: String,
}

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
    bindings_tree: Trie<KeyBind, Vec<KeyBindEvent>>,
    #[borrows(bindings_tree)]
    #[covariant]
    search: IncSearch<'this, KeyBind, Vec<KeyBindEvent>>,
}

impl TrieHolder {
    /// Create the trie tree from the hashmap
    fn from_raw(raw: &HashMap<Chord, Vec<KeyBindEvent>>) -> Self {
        let mut builder = TrieBuilder::new();
        for (chord, event) in raw {
            if chord.keys.is_empty() {
                continue;
            }
            builder.push(
                chord.keys.clone(),
                event.iter().map(|e| dyn_clone::clone_box(&**e)).collect(),
            );
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
    fn get_match(&mut self) -> Option<&Vec<KeyBindEvent>> {
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
    pub raw_bindings: HashMap<Chord, Vec<KeyBindEvent>>,
    /// The trie tree
    trie: TrieHolder,
    /// Should keybindings be emmitted
    pub enabled: bool,
}

arcane_core::register_plugin!(KeybindPlugin);

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

/// Disable or Enable keybindings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockKeybindings(pub bool);

impl arcane_core::Plugin for KeybindPlugin {
    fn new() -> Self {
        Self {
            raw_bindings: HashMap::new(),
            trie: TrieHolder::from_raw(&HashMap::new()),
            enabled: true,
        }
    }

    #[errors(serde_json::Error)]
    fn on_load(&mut self, events: &mut arcane_core::EventManager) -> Result<()> {
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

        if let Some(project_directory) = arcane_core::project_dirs() {
            let config_path = project_directory.config_dir().join("keybinds.json");
            if let Ok(file) = std::fs::File::open(&config_path) {
                let data: Vec<(Chord, serde_json::Value)> = serde_json::from_reader(file)?;
                event!(Level::DEBUG, "loading {} keybinds", data.len());
                let data = data.into_iter().filter_map(|(chord, action)| {
                    let action = serde_json::from_value(action).ok();
                    if action.is_none() {
                        event!(Level::ERROR, "Invalid action in keybindings file!");
                    }
                    action.map(|action| (chord, action))
                });
                self.raw_bindings = data.collect();
                event!(Level::DEBUG, "Loaded {} keybinds", self.raw_bindings.len());
                self.trie = TrieHolder::from_raw(&self.raw_bindings);
            }
        }

        Ok(())
    }

    #[errors(std::io::Error, serde_json::Error)]
    fn update(
        &mut self,
        events: &mut arcane_core::EventManager,
        _plugins: &arcane_core::PluginStore,
    ) -> Result<()> {
        let mut bindings_modified = false;
        // Very important to remove first
        for event in events.read::<RegisterKeybind>() {
            if self
                .raw_bindings
                .iter()
                .flat_map(|(_, actions)| actions.iter())
                .any(|action| format!("{action:?}") == format!("{:?}", event.event))
            {
                event!(Level::TRACE, "Keybind {:?} already exists", event.event);
            } else {
                event!(Level::DEBUG, "Registering keybind: {}", event.bind.render());
                let action = dyn_clone::clone_box(&*event.event);
                self.raw_bindings
                    .entry(event.bind.clone())
                    .or_default()
                    .push(action);
                bindings_modified = true;
            }
        }
        for event in events.read::<RebindKeybind>() {
            event!(
                Level::DEBUG,
                "Rebinding keybind: {} to {}",
                event.event,
                event.bind.render()
            );

            let action = self.raw_bindings.values_mut().find_map(|actions| {
                let index = actions
                    .iter()
                    .position(|action| format!("{action:?}") == event.event)?;
                Some(actions.remove(index))
            });

            if let Some(action) = action {
                self.raw_bindings
                    .entry(event.bind.clone())
                    .or_default()
                    .push(action);
                bindings_modified = true;
            } else {
                event!(Level::ERROR, "Keybind not found");
            }
        }

        if bindings_modified {
            self.raw_bindings.retain(|_, actions| !actions.is_empty());
            self.trie = TrieHolder::from_raw(&self.raw_bindings);

            if let Some(project_directory) = arcane_core::project_dirs() {
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
            for event in reader.read::<arcane_core::KeydownEvent>() {
                let keybind = KeyBind {
                    modifiers: event.0.modifiers,
                    key: event.0.code,
                };
                if keybind.is_only_modifiers() {
                    continue;
                }

                loop {
                    event!(Level::TRACE, "Chekcing: {}", keybind.render());
                    let depth = self.trie.borrow_search().prefix_len();
                    event!(Level::TRACE, "Current Search depth: {}", depth);
                    match self.trie.search(&keybind) {
                        None => {
                            event!(Level::TRACE, "No match for {}", keybind.render());
                            if let Some(events) = self.trie.get_match() {
                                for event in events {
                                    event!(Level::DEBUG, "Emitting {event:?}");
                                    let event = dyn_clone::clone_box(&**event);
                                    writer.dispatch_raw(event as Box<dyn arcane_core::RawEvent>);
                                }
                            }

                            event!(Level::TRACE, "Clearing search");
                            self.trie.clear();

                            if depth > 0 {
                                event!(Level::TRACE, "non-root mismatch, retrying at root");
                                continue;
                            } else {
                                break;
                            }
                        }
                        Some(Answer::Match) => {
                            event!(Level::TRACE, "Match for {}", keybind.render());
                            if let Some(events) = self.trie.get_match() {
                                for event in events {
                                    event!(Level::DEBUG, "Emitting {event:?}");
                                    let event = dyn_clone::clone_box(&**event);
                                    writer.dispatch_raw(event as Box<dyn arcane_core::RawEvent>);
                                }
                            }

                            event!(Level::TRACE, "Clearing search");
                            self.trie.clear();
                            break;
                        }
                        Some(Answer::Prefix | Answer::PrefixAndMatch) => {
                            event!(Level::TRACE, "Prefix match for {}", keybind.render());
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use arcane_core::{KeydownEvent, Plugin, StateManager};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::{BindResult, Deserialize, KeybindPlugin, RegisterKeybind, Serialize};

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
    fn chord_interrupted_by_single_match() {
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
        state.events.dispatch(RegisterKeybind::single_key(
            super::KeyBind {
                modifiers: KeyModifiers::NONE,
                key: KeyCode::Left,
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
    fn chord_interrupted_by_new_chord_match() {
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
            [
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Left,
                },
                super::KeyBind {
                    modifiers: KeyModifiers::NONE,
                    key: KeyCode::Right,
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
        state.events.swap_buffers();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Left,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();
        state.events.swap_buffers();

        state.events.dispatch(KeydownEvent(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Right,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();
        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[TestEvent::Foo]);
    }

    #[test]
    fn chord_duplicate_keys() {
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
                    key: KeyCode::Up,
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
            code: KeyCode::Up,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        state.events.swap_buffers();
        state.update().unwrap();
        state.events.swap_buffers();
        assert_eq!(state.events.read::<TestEvent>(), &[TestEvent::Bar]);
    }
}

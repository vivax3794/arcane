//! Plugin management and creation types

#![feature(trait_upcasting)]

use std::any::{Any, TypeId};
use std::cell::{Ref, RefCell, RefMut};

use derive_more::Debug;
use directories::ProjectDirs;
pub use inventory;
pub use tracing::{event, Level};

/// Dispatched every frame hodling the delta from the last frame.
#[derive(Clone, Copy, Debug)]
pub struct DeltaTimeEvent(pub std::time::Duration);

/// A key was pressed
#[derive(Clone, Copy, Debug)]
pub struct KeydownEvent(pub crossterm::event::KeyEvent);

pub type Result<T> = color_eyre::Result<T>;
///
/// Get a struct that can be used to get the project directories to use
///
/// # Errors
/// If missing envs
pub fn project_dirs() -> Option<ProjectDirs> {
    let result = ProjectDirs::from("dev", "viv", "arcane");
    if result.is_none() {
        event!(
            Level::ERROR,
            "Project Directories not found, config and similar will not be saved."
        );
    }
    result
}

/// Plugin trait
pub trait Plugin: Any {
    fn new() -> Self
    where
        Self: Sized;
    /// Called on editor startup
    ///
    /// # Errors
    /// For whatever reason the plugin wants
    fn on_load(&mut self, _events: &mut EventManager) -> Result<()> {
        Ok(())
    }

    /// Read the events you care about from the event manager
    /// and access readonly state thru the plugins store
    ///
    /// # Errors
    /// Can error I guess :P
    fn update(&mut self, _events: &mut EventManager, _plugins: &PluginStore) -> Result<()> {
        Ok(())
    }

    /// Draw to the terminal, the plugin order is undefined.
    ///
    /// Prefer drawing to Windows
    fn draw(
        &self,
        _frame: &mut ratatui::Frame,
        _area: ratatui::prelude::Rect,
        _plugins: &PluginStore,
    ) {
    }

    /// The z-index of the draw calls
    ///
    /// Defaults to 0
    fn z_index(&self) -> u32 {
        0
    }
}

/// A wrapper trait around vectors (or anything really) that can be used as a queue.
/// Used to allow to type safely clear out all queues in the event manager without clearing the
/// hashmap itself.
trait DynVec: Any {
    /// Clear the container
    fn clear(&mut self);
    /// Push a element onto the vector, if types dont match ignore it
    fn push(&mut self, element: Box<dyn Any>);
}

impl<E> DynVec for Vec<E>
where
    E: 'static,
{
    fn clear(&mut self) {
        Vec::clear(self);
    }

    fn push(&mut self, element: Box<dyn Any>) {
        if let Ok(element) = element.downcast() {
            self.push(*element);
        }
    }
}

impl arcane_anymap::Downcast for dyn DynVec {
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

impl<T: 'static> arcane_anymap::IntoBoxed<dyn DynVec> for Vec<T> {
    fn into(self) -> Box<dyn DynVec> {
        Box::new(self)
    }
}

/// A raw event is the trait needed to be implemented for types used in `dispatch_raw`
///
/// This is implemented for all types
pub trait RawEvent: Any {
    /// Get the type id of a `Vec` containing this type
    fn vec_type_id(&self) -> TypeId;
}
impl<T: Any> RawEvent for T {
    fn vec_type_id(&self) -> TypeId {
        TypeId::of::<Vec<Self>>()
    }
}

/// Holds a reference to all event queues
#[derive(Debug)]
pub struct EventManager {
    /// The buffer events are currently being read from
    #[debug(skip)]
    read_buffer: arcane_anymap::AnyMap<dyn DynVec>,
    /// The buffer new events will be written to
    #[debug(skip)]
    write_buffer: arcane_anymap::AnyMap<dyn DynVec>,
}

/// A seperated out reader for events
#[derive(Debug)]
pub struct EventReader<'e>(#[debug(skip)] &'e arcane_anymap::AnyMap<dyn DynVec>);
impl EventReader<'_> {
    /// Same as read method on `EventManager`
    #[must_use]
    pub fn read<E>(&self) -> &[E]
    where
        E: 'static,
    {
        match self.0.get::<Vec<E>>() {
            Some(events) => events.as_slice(),
            None => &[],
        }
    }
}

/// A seperated out writer for events
#[derive(Debug)]
pub struct EventWriter<'e>(#[debug(skip)] &'e mut arcane_anymap::AnyMap<dyn DynVec>);
impl EventWriter<'_> {
    /// Same as write method on `EventManager`
    pub fn dispatch<E>(&mut self, event: E)
    where
        E: 'static,
    {
        self.0.entry::<Vec<E>>().or_default().push(event);
    }

    /// Insert a `dyn Any` into its corresponding queue.
    /// `ensure_event` should have been called beforehand to ensure there is a queue to push into.
    pub fn dispatch_raw(&mut self, event: Box<dyn RawEvent>) {
        if let Some(events) = self.0.get_mut_raw(&(*event).vec_type_id()) {
            events.push(event);
        } else {
            event!(Level::WARN, "Tried to insert raw to unknown event type (can not create default queue for unknown types)");
        }
    }
}

impl Default for EventManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EventManager {
    /// Create a empty queue
    pub fn new() -> Self {
        Self {
            read_buffer: arcane_anymap::AnyMap::new(),
            write_buffer: arcane_anymap::AnyMap::new(),
        }
    }

    /// Add the event to the correct queue, creates queue if missing.
    pub fn dispatch<E>(&mut self, event: E)
    where
        E: 'static,
    {
        self.write_buffer.entry::<Vec<E>>().or_default().push(event);
    }

    /// Returns clones of all events in the queue
    #[must_use]
    pub fn read<E>(&self) -> &[E]
    where
        E: 'static,
    {
        match self.read_buffer.get::<Vec<E>>() {
            Some(events) => events.as_slice(),
            None => &[],
        }
    }

    #[must_use]
    /// Split the event manager into a reader and writer to allow writing events based on read
    /// events easialy
    pub fn split(&mut self) -> (EventReader, EventWriter) {
        let reader = EventReader(&self.read_buffer);
        let writer = EventWriter(&mut self.write_buffer);
        (reader, writer)
    }

    /// Make sure a queue exsists for the specified type
    pub fn ensure_event<E>(&mut self)
    where
        E: 'static,
    {
        self.write_buffer.entry::<Vec<E>>().or_default();
        self.read_buffer.entry::<Vec<E>>().or_default();
    }

    /// Insert a `dyn Any` into its corresponding queue.
    /// `ensure_event` should have been called beforehand to ensure there is a queue to push into.
    pub fn dispatch_raw(&mut self, event: Box<dyn RawEvent>) {
        if let Some(events) = self.write_buffer.get_mut_raw(&(*event).vec_type_id()) {
            events.push(event);
        } else {
            event!(Level::WARN, "Tried to insert raw to unknown event type (can not create default queue for unknown types)");
        }
    }

    /// Clear the current read buffer, then swap the buffers;
    pub fn swap_buffers(&mut self) {
        for queue in self.read_buffer.iter_mut() {
            queue.clear();
        }
        std::mem::swap(&mut self.read_buffer, &mut self.write_buffer);
    }
}

/// Represents a `RefCell<P>` where P is a plugin
///
/// Used to enforce stronger trait guaranties on plugin store
trait PluginWrapper: Any {
    /// Borrow immutable
    fn borrow(&self) -> Option<Ref<dyn Plugin>>;
    /// Borrow mut
    fn borrow_mut(&self) -> Option<RefMut<dyn Plugin>>;
}

impl<P: Plugin> PluginWrapper for RefCell<P> {
    fn borrow(&self) -> Option<Ref<dyn Plugin>> {
        RefCell::try_borrow(self)
            .ok()
            .map(|p| Ref::map(p, |p| p as &dyn Plugin))
    }
    fn borrow_mut(&self) -> Option<RefMut<dyn Plugin>> {
        RefCell::try_borrow_mut(self)
            .ok()
            .map(|p| RefMut::map(p, |p| p as &mut dyn Plugin))
    }
}

impl arcane_anymap::Downcast for dyn PluginWrapper {
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

impl<P: Plugin> arcane_anymap::IntoBoxed<dyn PluginWrapper> for RefCell<P> {
    fn into(self) -> Box<dyn PluginWrapper> {
        Box::new(self)
    }
}

pub struct RegisterPlugin {
    constructor: fn() -> Box<dyn PluginWrapper>,
}

inventory::collect!(RegisterPlugin);

impl RegisterPlugin {
    pub const fn new<P: Plugin>() -> Self {
        Self {
            constructor: move || Box::new(RefCell::new(P::new())),
        }
    }
}

#[macro_export]
macro_rules! register_plugin {
    ($plugin:ident) => {
        ::arcane_core::inventory::submit!(::arcane_core::RegisterPlugin::new::<$plugin>());
    };
}

/// Stores all plugins in the application
#[derive(Debug)]
pub struct PluginStore {
    /// The plugins are stored as `RefCell`s in this map
    #[debug(skip)]
    plugins: arcane_anymap::AnyMap<dyn PluginWrapper>,
}

impl Default for PluginStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginStore {
    /// Create a new empty store
    pub fn new() -> Self {
        let mut store = Self {
            plugins: arcane_anymap::AnyMap::new(),
        };
        for plugin in inventory::iter::<RegisterPlugin>() {
            store.register(plugin);
        }
        store
    }

    /// Get a readonly reference to a plugin.
    ///
    /// # Panics
    /// Same reason as `RefCell`
    #[must_use]
    pub fn get<P: Plugin + 'static>(&self) -> Option<Ref<P>> {
        self.plugins
            .get::<RefCell<P>>()
            .and_then(|plugin| plugin.try_borrow().ok())
    }

    /// Get a mut reference to a plugin.
    ///
    /// # Panics
    /// Same reason as `RefCell`
    #[must_use]
    pub fn get_mut<P: Plugin + 'static>(&self) -> Option<RefMut<P>> {
        self.plugins
            .get::<RefCell<P>>()
            .and_then(|plugin| plugin.try_borrow_mut().ok())
    }

    /// Insert a plugin into the map
    pub fn insert<P: Plugin>(&mut self, value: P) {
        self.plugins.insert(RefCell::new(value));
    }

    pub fn register(&mut self, plugin: &RegisterPlugin) {
        self.plugins.insert_raw((plugin.constructor)());
    }

    /// Iterate over immutable references to the plugins
    pub fn iter(&self) -> impl Iterator<Item = Ref<dyn Plugin>> {
        self.plugins.iter().filter_map(|plugin| plugin.borrow())
    }
}

/// The plugin manager
#[derive(Debug)]
pub struct StateManager {
    /// Holds all the plugins
    pub plugins: PluginStore,
    /// Holds all the events
    pub events: EventManager,
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StateManager {
    /// Create a new plugin manager
    pub fn new() -> Self {
        Self {
            plugins: PluginStore::new(),
            events: EventManager::new(),
        }
    }

    /// Call the handle event method of every plugin
    pub fn update(&mut self) -> Result<()> {
        for plugin in self.plugins.plugins.iter() {
            if let Some(mut plugin) = plugin.borrow_mut() {
                plugin.update(&mut self.events, &self.plugins)?;
            }
        }
        Ok(())
    }

    /// Call the draw method of every plugin
    pub fn draw(&self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let mut plugins = self.plugins.plugins.iter().collect::<Vec<_>>();
        plugins.sort_by_key(|plugin| plugin.borrow().map(|p| p.z_index()).unwrap_or_default());
        for plugin in plugins {
            if let Some(plugin) = plugin.borrow() {
                plugin.draw(frame, area, &self.plugins);
            }
        }
    }

    /// Run on load of all plugins
    pub fn on_load(&mut self) -> Result<()> {
        event!(Level::INFO, "Running on loads");
        for plugin in self.plugins.plugins.iter() {
            if let Some(mut plugin) = plugin.borrow_mut() {
                plugin.on_load(&mut self.events)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use color_eyre::eyre::eyre;

    use super::{Plugin, StateManager};
    use crate::PluginStore;

    mod events {
        use crate::{EventManager, RawEvent};

        #[test]
        fn read_empty() {
            let events = EventManager::new();
            assert!(events.read::<i32>().is_empty());
        }

        #[test]
        fn simple() {
            let mut events = EventManager::new();
            events.dispatch(10_i32);
            events.dispatch(20_i32);
            events.swap_buffers();

            assert_eq!(events.read::<i32>(), [10, 20]);
        }

        #[test]
        fn does_not_consume() {
            let mut events = EventManager::new();
            events.dispatch(10_i32);
            events.dispatch(20_i32);
            events.swap_buffers();

            assert_eq!(events.read::<i32>(), [10, 20]);
            assert_eq!(events.read::<i32>(), [10, 20]);
            assert_eq!(events.read::<i32>(), [10, 20]);
            assert_eq!(events.read::<i32>(), [10, 20]);
        }

        #[test]
        fn multiple_types() {
            let mut events = EventManager::new();
            events.dispatch(10_i32);
            events.dispatch(20_i8);
            events.swap_buffers();

            assert_eq!(events.read::<i32>(), [10]);
            assert_eq!(events.read::<i8>(), [20]);
        }

        #[test]
        fn need_to_swap() {
            let mut events = EventManager::new();
            events.dispatch(10_i32);
            events.dispatch(20_i32);

            assert!(events.read::<i32>().is_empty());
        }

        #[test]
        fn swap_clears() {
            let mut events = EventManager::new();
            events.dispatch(10_i32);
            events.dispatch(20_i32);
            events.swap_buffers();
            events.dispatch(30_i32);
            events.swap_buffers();

            assert_eq!(events.read::<i32>(), [30]);
        }

        #[test]
        fn dispatch_goes_to_write_buffer() {
            let mut events = EventManager::new();
            events.dispatch(10_i32);
            events.dispatch(20_i32);
            events.swap_buffers();
            events.dispatch(30_i32);

            assert_eq!(events.read::<i32>(), [10, 20]);
        }

        #[test]
        fn dispatch_raw() {
            let mut events = EventManager::new();
            events.ensure_event::<i32>();

            let event: Box<dyn RawEvent> = Box::new(10_i32);
            events.dispatch_raw(event);
            events.swap_buffers();

            assert_eq!(events.read::<i32>(), &[10]);
        }

        #[test]
        fn split() {
            let mut events = EventManager::new();
            events.dispatch(10_i32);
            events.dispatch(10_i32);
            events.swap_buffers();

            let (reader, mut writer) = events.split();
            for _ in reader.read::<i32>() {
                writer.dispatch(20_i8);
            }

            events.swap_buffers();
            assert_eq!(events.read::<i8>(), &[20, 20]);
        }

        #[test]
        fn split_dispatch_raw() {
            let mut events = EventManager::new();
            events.ensure_event::<i32>();

            let (_reader, mut writer) = events.split();
            let event: Box<dyn RawEvent> = Box::new(10_i32);
            writer.dispatch_raw(event);

            events.swap_buffers();
            assert_eq!(events.read::<i32>(), &[10]);
        }
    }

    #[derive(PartialEq, Eq, Debug, Clone, Copy)]
    struct TestPlugin(u8);

    impl Plugin for TestPlugin {
        fn new() -> Self {
            TestPlugin(1)
        }
        fn update(
            &mut self,
            _events: &mut super::EventManager,
            _plugins: &PluginStore,
        ) -> color_eyre::eyre::Result<()> {
            self.0 *= 10;
            Ok(())
        }
    }

    #[derive(PartialEq, Eq, Debug, Clone, Copy)]
    struct ErrPlugin;

    impl Plugin for ErrPlugin {
        fn new() -> Self
        where
            Self: Sized,
        {
            ErrPlugin
        }
        fn update(
            &mut self,
            _events: &mut super::EventManager,
            _plugins: &PluginStore,
        ) -> color_eyre::eyre::Result<()> {
            Err(eyre!("OH NO!"))
        }
    }

    #[test]
    fn update() {
        let mut state = StateManager::new();
        state.plugins.insert(TestPlugin(10));
        state.update().unwrap();

        assert_eq!(
            state.plugins.get::<TestPlugin>().map(|x| *x),
            Some(TestPlugin(100))
        );
    }

    #[test]
    fn update_error() {
        let mut state = StateManager::new();
        state.plugins.insert(TestPlugin(10));
        state.plugins.insert(ErrPlugin);

        assert!(state.update().is_err());
    }

    #[test]
    fn get_ref() {
        let mut plugins = PluginStore::new();
        plugins.insert(TestPlugin(10));

        assert_eq!(
            plugins.get::<TestPlugin>().map(|x| *x),
            Some(TestPlugin(10))
        );
    }

    #[test]
    fn get_mut() {
        let mut plugins = PluginStore::new();
        plugins.insert(TestPlugin(10));

        if let Some(mut value) = plugins.get_mut::<TestPlugin>() {
            value.0 += 10;
        }

        assert_eq!(
            plugins.get::<TestPlugin>().map(|x| *x),
            Some(TestPlugin(20))
        );
    }

    #[test]
    fn break_borrow_rules_mut() {
        let mut plugins = PluginStore::new();
        plugins.insert(TestPlugin(10));

        let plugin = plugins.get::<TestPlugin>();
        assert!(plugins.get_mut::<TestPlugin>().is_none());
        drop(plugin);
    }

    #[test]
    fn break_borrow_rules_ref() {
        let mut plugins = PluginStore::new();
        plugins.insert(TestPlugin(10));

        let plugin = plugins.get_mut::<TestPlugin>();
        assert!(plugins.get::<TestPlugin>().is_none());
        drop(plugin);
    }
}

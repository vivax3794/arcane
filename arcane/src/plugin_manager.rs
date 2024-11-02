//! Plugin management and creation types

use std::any::Any;
use std::cell::{Ref, RefCell, RefMut};

use derive_more::Debug;

use crate::anymap::{AnyMap, Downcast, IntoBoxed};
use crate::prelude::*;

/// Plugin trait
pub trait Plugin: Any {
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

/// A wrapper trait around vectors (or anything really) that can be cleared.
/// Also contains wrapper methods to allow downcasting.
/// Used to allow to type safely clear out all queues in the event manager without clearing the
/// hashmap itself.
trait Clearable: Any {
    /// Clear the container
    fn clear(&mut self);
}

impl<E> Clearable for Vec<E>
where
    E: 'static,
{
    fn clear(&mut self) {
        Vec::clear(self);
    }
}

impl Downcast for dyn Clearable {
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

impl<T: Clearable> IntoBoxed<dyn Clearable> for T {
    fn into(self) -> Box<dyn Clearable> {
        Box::new(self)
    }
}

/// Holds a reference to all event queues
#[derive(Debug)]
pub struct EventManager {
    /// The buffer events are currently being read from
    #[debug(skip)]
    read_buffer: AnyMap<dyn Clearable>,
    /// The buffer new events will be written to
    #[debug(skip)]
    write_buffer: AnyMap<dyn Clearable>,
}

/// A seperated out reader for events
#[derive(Debug)]
pub struct EventReader<'e>(#[debug(skip)] &'e AnyMap<dyn Clearable>);
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
pub struct EventWriter<'e>(#[debug(skip)] &'e mut AnyMap<dyn Clearable>);
impl EventWriter<'_> {
    /// Same as write method on `EventManager`
    pub fn dispatch<E>(&mut self, event: E)
    where
        E: 'static,
    {
        self.0.entry::<Vec<E>>().or_default().push(event);
    }
}

impl EventManager {
    /// Create a empty queue
    fn new() -> Self {
        Self {
            read_buffer: AnyMap::new(),
            write_buffer: AnyMap::new(),
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

    /// Clear the current read buffer, then swap the buffers;
    pub(crate) fn swap_buffers(&mut self) {
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
    fn borrow(&self) -> Ref<dyn Plugin>;
    /// Borrow mut
    fn borrow_mut(&self) -> RefMut<dyn Plugin>;
}

impl<P: Plugin> PluginWrapper for RefCell<P> {
    fn borrow(&self) -> Ref<dyn Plugin> {
        RefCell::borrow(self)
    }
    fn borrow_mut(&self) -> RefMut<dyn Plugin> {
        RefCell::borrow_mut(self)
    }
}

impl Downcast for dyn PluginWrapper {
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

impl<P: Plugin> IntoBoxed<dyn PluginWrapper> for RefCell<P> {
    fn into(self) -> Box<dyn PluginWrapper> {
        Box::new(self)
    }
}

/// Stores all plugins in the application
#[derive(Debug)]
pub struct PluginStore {
    /// The plugins are stored as `RefCell`s in this map
    #[debug(skip)]
    plugins: AnyMap<dyn PluginWrapper>,
}

impl PluginStore {
    /// Create a new empty store
    fn new() -> Self {
        Self {
            plugins: AnyMap::new(),
        }
    }

    /// Get a readonly reference to a plugin.
    ///
    /// # Panics
    /// Same reason as `RefCell`
    #[must_use]
    pub fn get<P: Plugin + 'static>(&self) -> Option<Ref<P>> {
        self.plugins
            .get::<RefCell<P>>()
            .map(|plugin| plugin.borrow())
    }

    /// Get a mutable reference to a plugin.
    ///
    /// # Panics
    /// Same reason as `RefCell`
    #[must_use]
    pub fn get_mut<P: Plugin + 'static>(&self) -> Option<RefMut<P>> {
        self.plugins
            .get::<RefCell<P>>()
            .map(|plugin| plugin.borrow_mut())
    }

    /// Insert a plugin into the map
    pub(crate) fn insert<P: Plugin>(&mut self, value: P) {
        self.plugins.insert(RefCell::new(value));
    }

    /// Iterate over immutable references to the plugins
    pub fn iter(&self) -> impl Iterator<Item = Ref<dyn Plugin>> {
        self.plugins.iter().map(|plugin| plugin.borrow())
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

impl StateManager {
    /// Create a new plugin manager
    pub(crate) fn new() -> Self {
        Self {
            plugins: PluginStore::new(),
            events: EventManager::new(),
        }
    }

    /// Call the handle event method of every plugin
    pub(crate) fn update(&mut self) -> Result<()> {
        for plugin in self.plugins.plugins.iter() {
            plugin
                .borrow_mut()
                .update(&mut self.events, &self.plugins)?;
        }
        Ok(())
    }

    /// Call the draw method of every plugin
    pub(crate) fn draw(&self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let mut plugins = self.plugins.plugins.iter().collect::<Vec<_>>();
        plugins.sort_by_key(|plugin| plugin.borrow().z_index());
        for plugin in plugins {
            plugin.borrow().draw(frame, area, &self.plugins);
        }
    }

    /// Run on load of all plugins
    pub(crate) fn on_load(&mut self) -> Result<()> {
        event!(Level::INFO, "Running on loads");
        for plugin in self.plugins.plugins.iter() {
            plugin.borrow_mut().on_load(&mut self.events)?;
        }
        Ok(())
    }
}

#[coverage(off)]
#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use color_eyre::eyre::eyre;

    use super::{Plugin, StateManager};
    use crate::PluginStore;

    mod events {
        use crate::EventManager;

        #[test]
        fn read_empty() {
            let events = EventManager::new();
            assert_eq!(events.read::<i32>(), []);
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

            assert_eq!(events.read::<i32>(), []);
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
    }

    #[derive(PartialEq, Eq, Debug, Clone, Copy)]
    struct TestPlugin(u8);

    impl Plugin for TestPlugin {
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
}

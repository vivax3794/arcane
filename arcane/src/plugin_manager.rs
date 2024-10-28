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
    fn on_load(&mut self, _events: &EventManager) -> Result<()> {
        Ok(())
    }

    /// Read the events you care about from the event manager
    /// and access readonly state thru the plugins store
    ///
    /// # Errors
    /// Can error I guess :P
    fn update(&mut self, _events: &EventManager, _plugins: &PluginStore) -> Result<()> {
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
    fn _downcast<T>(this: &Self) -> Option<&T>
    where
        T: 'static,
    {
        (this as &dyn Any).downcast_ref()
    }
    fn _downcast_mut<T>(this: &mut Self) -> Option<&mut T>
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
    write_buffer: RefCell<AnyMap<dyn Clearable>>,
}

impl EventManager {
    /// Create a empty queue
    fn new() -> Self {
        Self {
            read_buffer: AnyMap::new(),
            write_buffer: RefCell::new(AnyMap::new()),
        }
    }

    /// Add the event to the correct queue, creates queue if missing.
    pub fn dispatch<E>(&self, event: E)
    where
        E: 'static,
    {
        let mut events = self.write_buffer.borrow_mut();
        events.entry::<Vec<E>>().or_default().push(event);
    }

    /// Returns clones of all events in the queue
    pub fn read<E>(&self) -> &[E]
    where
        E: 'static,
    {
        match self.read_buffer.get::<Vec<E>>() {
            Some(events) => events.as_slice(),
            None => &[],
        }
    }

    /// Clear the current read buffer, then swap the buffers;
    pub(crate) fn swap_buffers(&mut self) {
        for queue in self.read_buffer.iter_mut() {
            queue.clear();
        }
        std::mem::swap(&mut self.read_buffer, &mut self.write_buffer.borrow_mut());
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
    fn _downcast<T>(this: &Self) -> Option<&T>
    where
        T: 'static,
    {
        (this as &dyn Any).downcast_ref()
    }
    fn _downcast_mut<T>(this: &mut Self) -> Option<&mut T>
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
            plugin.borrow_mut().update(&self.events, &self.plugins)?;
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
    pub(crate) fn on_load(&self) -> Result<()> {
        event!(Level::INFO, "Running on loads");
        for plugin in self.plugins.plugins.iter() {
            plugin.borrow_mut().on_load(&self.events)?;
        }
        Ok(())
    }
}

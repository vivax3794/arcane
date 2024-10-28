//! Custom safe implementaion of `AnyMap` crate
//!
//! NOTE: this is not near a complete implementation and only implements the methods needed by this
//! application. As such I will only add methods that I need.

use std::any::{Any, TypeId};
use std::collections::{hash_map, HashMap};
use std::marker::PhantomData;

use crate::prelude::*;

/// Helper trait to convert a value into a Boxed version.
///
/// In this context it should be implemented for all values implementing a trait into a boxed dyn
/// of that trait. like the builtin:
///
/// ```rs
/// impl<T: Any> IntoBoxed<dyn Any> for T { ... }
/// ```
pub(crate) trait IntoBoxed<A: ?Sized> {
    /// Convert the value into a boxed version
    fn into(self) -> Box<A>;
}

impl<T: Any> IntoBoxed<dyn Any> for T {
    fn into(self) -> Box<dyn Any> {
        Box::new(self)
    }
}

/// Downcast the value to the specific type
pub(crate) trait Downcast {
    /// Downcast a immutable reference
    fn _downcast<T>(this: &Self) -> Option<&T>
    where
        T: 'static;
    /// Downcast a mutable reference
    fn _downcast_mut<T>(this: &mut Self) -> Option<&mut T>
    where
        T: 'static;
}

impl Downcast for dyn Any {
    fn _downcast<T>(this: &Self) -> Option<&T>
    where
        T: 'static,
    {
        this.downcast_ref()
    }
    fn _downcast_mut<T>(this: &mut Self) -> Option<&mut T>
    where
        T: 'static,
    {
        this.downcast_mut()
    }
}

/// A map that can hold any type and also can take custom trait bounds.
#[derive(Debug)]
pub(crate) struct AnyMap<A: Downcast + ?Sized = dyn Any>(HashMap<TypeId, Box<A>>);

impl<A: Any + Downcast + ?Sized> AnyMap<A>
where
    A: 'static,
{
    /// Create a new empty map
    pub(crate) fn new() -> Self {
        Self(HashMap::new())
    }

    /// How long is the map
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// Insert a value into the map
    pub(crate) fn insert<T: IntoBoxed<A> + 'static>(&mut self, value: T) {
        self.0.insert(TypeId::of::<T>(), value.into());
    }

    /// Get a value from the map
    pub(crate) fn get<T: IntoBoxed<A> + 'static>(&self) -> Option<&T> {
        let any = self.0.get(&TypeId::of::<T>())?;
        if let Some(value) = A::_downcast(any) {
            Some(value)
        } else {
            event!(Level::ERROR, "Missmatch type in anymap");
            None
        }
    }

    /// Get a entry from the map for in place operations
    pub(crate) fn entry<T: IntoBoxed<A> + 'static>(&mut self) -> Entry<A, T> {
        let entry = self.0.entry(TypeId::of::<T>());
        Entry {
            entry,
            _phantom: PhantomData,
        }
    }

    /// Iterate over the mutable references to the values
    pub(crate) fn iter_mut(&mut self) -> hash_map::ValuesMut<TypeId, Box<A>> {
        self.0.values_mut()
    }

    /// Iterate over immutable references to the values
    pub(crate) fn iter(&self) -> hash_map::Values<TypeId, Box<A>> {
        self.0.values()
    }

    /// Insert Raw
    pub(crate) fn insert_raw(&mut self, value: Box<A>)
    where
        A: Any,
    {
        let key = (*value).type_id();
        self.0.insert(key, value);
    }
}

/// A entry in the map
pub(crate) struct Entry<'h, A: Downcast + ?Sized, T> {
    /// The actual hashmap entry
    entry: hash_map::Entry<'h, TypeId, Box<A>>,
    /// The type the data can be cast to
    _phantom: PhantomData<T>,
}

impl<'h, A: Downcast + ?Sized, T> Entry<'h, A, T> {
    /// Insert the default value if empty and return a mutable reference
    #[allow(clippy::expect_used)] // The type system should ensure this is always valid
    pub(crate) fn or_default(self) -> &'h mut T
    where
        T: Default + IntoBoxed<A> + 'static,
    {
        let value = self.entry.or_insert_with(|| T::default().into());
        A::_downcast_mut(value).expect("Mismatch between actual type and expected type")
    }
}

#[cfg(test)]
mod tests {
    use super::AnyMap;

    #[test]
    fn basic() {
        let mut map = AnyMap::new();
        map.insert(10_i32);
        map.insert(20_i8);

        assert_eq!(map.get::<i32>(), Some(&10));
        assert_eq!(map.get::<i8>(), Some(&20));
        assert_eq!(map.get::<bool>(), None);
    }

    #[test]
    fn entry() {
        let mut map = AnyMap::new();
        map.insert(10_i32);

        assert_eq!(map.entry::<i32>().or_default(), &10);
        assert_eq!(map.entry::<i8>().or_default(), &0);

        *map.entry::<i32>().or_default() += 10;
        *map.entry::<i8>().or_default() += 10;

        assert_eq!(map.entry::<i32>().or_default(), &20);
        assert_eq!(map.entry::<i8>().or_default(), &10);
    }

    #[test]
    fn iter() {
        let mut map = AnyMap::new();
        map.insert(10_i32);
        map.insert(20_i8);

        let mut len = 0;
        for _ in map.iter_mut() {
            len += 1;
        }
        assert_eq!(len, 2);
    }
}

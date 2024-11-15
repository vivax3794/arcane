//! Custom safe implementaion of `AnyMap` crate
//!
//! NOTE: this is not near a complete implementation and only implements the methods needed by this
//! application. As such I will only add methods that I need.

use std::any::{Any, TypeId};
use std::collections::{hash_map, HashMap};
use std::marker::PhantomData;

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
    fn downcast<T>(this: &Self) -> Option<&T>
    where
        T: 'static;
    /// Downcast a mutable reference
    fn downcast_mut<T>(this: &mut Self) -> Option<&mut T>
    where
        T: 'static;
}

impl Downcast for dyn Any {
    fn downcast<T>(this: &Self) -> Option<&T>
    where
        T: 'static,
    {
        this.downcast_ref()
    }
    fn downcast_mut<T>(this: &mut Self) -> Option<&mut T>
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
    #[allow(clippy::expect_used)]
    pub(crate) fn get<T: IntoBoxed<A> + 'static>(&self) -> Option<&T> {
        let any = self.0.get(&TypeId::of::<T>())?;
        Some(A::downcast(any).expect("AnyMap corrupted"))
    }

    /// Get a mutable reference to a entry based on a type id
    ///
    /// # Safety
    /// - **Do not** use this mutable reference to change the concrete type of the value.
    /// - **Only** modify attributes or call methods on the inner type `A`.
    /// - Changing the concrete type will lead to panics and undefined behavior.
    pub(crate) fn get_mut_raw(&mut self, id: &TypeId) -> Option<&mut Box<A>> {
        self.0.get_mut(id)
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

    /// Insert Raw, but only insert if the key doesnt exsist
    pub(crate) fn insert_raw_if_missing(&mut self, value: Box<A>)
    where
        A: Any,
    {
        let key = (*value).type_id();
        let _ = self.0.try_insert(key, value);
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
    #[allow(clippy::expect_used)]
    pub(crate) fn or_default(self) -> &'h mut T
    where
        T: Default + IntoBoxed<A> + 'static,
    {
        let value = self.entry.or_insert_with(|| T::default().into());
        A::downcast_mut(value).expect("AnyMap corrupted")
    }
}

#[coverage(off)]
#[cfg(test)]
mod tests {
    use std::any::{Any, TypeId};

    use proptest::proptest;

    use super::AnyMap;

    #[test]
    fn insert_get() {
        let mut map = AnyMap::new();
        map.insert(10_i32);
        map.insert(20_i8);

        assert_eq!(map.get::<i32>(), Some(&10));
        assert_eq!(map.get::<i8>(), Some(&20));
        assert_eq!(map.get::<bool>(), None);
    }

    #[test]
    fn overwrite() {
        let mut map = AnyMap::new();
        map.insert(10_i32);
        map.insert(20_i32);

        assert_eq!(map.get::<i32>(), Some(&20));
    }

    #[test]
    fn entry_or_default_present() {
        let mut map = AnyMap::new();
        map.insert(10_i32);

        assert_eq!(map.entry::<i32>().or_default(), &10);
        *map.entry::<i32>().or_default() += 10;
        assert_eq!(map.entry::<i32>().or_default(), &20);
    }

    #[test]
    fn entry_or_default_missing() {
        let mut map = AnyMap::new();

        assert_eq!(map.entry::<i32>().or_default(), &0);
        *map.entry::<i32>().or_default() += 10;
        assert_eq!(map.entry::<i32>().or_default(), &10);
    }

    #[test]
    fn iter() {
        let mut map = AnyMap::new();
        map.insert(10_i32);
        map.insert(20_i8);

        for value in map.iter() {
            if let Some(value) = value.downcast_ref::<i32>() {
                assert_eq!(value, &10);
            } else if let Some(value) = value.downcast_ref::<i8>() {
                assert_eq!(value, &20);
            }
        }
    }

    #[test]
    fn iter_mut() {
        let mut map = AnyMap::new();
        map.insert(10_i32);
        map.insert(20_i8);

        for value in map.iter_mut() {
            if let Some(value) = value.downcast_mut::<i32>() {
                *value += 5;
            } else if let Some(value) = value.downcast_mut::<i8>() {
                *value *= 2;
            }
        }

        assert_eq!(map.get::<i32>(), Some(&15));
        assert_eq!(map.get::<i8>(), Some(&40));
    }

    #[test]
    fn len() {
        let mut map = AnyMap::new();
        map.insert(10_i32);
        map.insert(10_i8);
        map.insert(20_i32);
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn insert_raw() {
        let mut map = AnyMap::new();
        let value: Box<dyn Any> = Box::new(20_i32);
        map.insert_raw(value);

        assert_eq!(map.get::<i32>(), Some(&20));
    }

    #[should_panic(expected = "AnyMap corrupted")]
    #[test]
    fn corruped_map_get() {
        let mut map = AnyMap::<dyn Any>::new();
        // This is not a public api, we are corrupting the data on purpose
        map.0.insert(TypeId::of::<bool>(), Box::new(8_i8));
        map.get::<bool>();
    }

    #[should_panic(expected = "AnyMap corrupted")]
    #[test]
    fn corrupted_entry_or_default() {
        let mut map = AnyMap::<dyn Any>::new();
        // This is not a public api, we are corrupting the data on purpose
        map.0.insert(TypeId::of::<bool>(), Box::new(8_i8));
        map.entry::<bool>().or_default();
    }

    proptest! {
        #[test]
        fn fuzzy_simple(data: i32) {
            let mut map = AnyMap::<dyn Any>::new();
            map.insert(data);
            assert_eq!(map.get::<i32>(), Some(&data));
        }

        #[test]
        fn fuzzy_many_types(a: i32, b: String, c: Vec<u8>) {
            let mut map = AnyMap::<dyn Any>::new();
            map.insert(a);
            map.insert(b.clone());
            map.insert(c.clone());

            assert_eq!(map.get(), Some(&a));
            assert_eq!(map.get(), Some(&b));
            assert_eq!(map.get(), Some(&c));
        }
    }

    mod non_any {
        use std::any::{Any, TypeId};

        use crate::anymap::{AnyMap, Downcast, IntoBoxed};

        trait TestTrait: Any {
            fn ref_method(&self) -> i32;
            fn mut_method(&mut self);
        }

        #[derive(PartialEq, Eq, Debug)]
        struct TestStruct1(i32);
        #[derive(PartialEq, Eq, Debug)]
        struct TestStruct2(i32);

        impl TestTrait for TestStruct1 {
            fn ref_method(&self) -> i32 {
                self.0
            }
            #[allow(clippy::arithmetic_side_effects)]
            fn mut_method(&mut self) {
                self.0 *= 2;
            }
        }
        impl TestTrait for TestStruct2 {
            fn ref_method(&self) -> i32 {
                self.0
            }
            #[allow(clippy::arithmetic_side_effects)]
            fn mut_method(&mut self) {
                self.0 += 5;
            }
        }

        impl Downcast for dyn TestTrait {
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

        impl<T: TestTrait> IntoBoxed<dyn TestTrait> for T {
            fn into(self) -> Box<dyn TestTrait> {
                Box::new(self)
            }
        }

        #[test]
        fn immutable() {
            let mut map = AnyMap::<dyn TestTrait>::new();
            map.insert(TestStruct1(10));
            map.insert(TestStruct2(10));

            for value in map.iter() {
                assert_eq!(value.ref_method(), 10);
            }
        }

        #[test]
        fn mutable() {
            let mut map = AnyMap::<dyn TestTrait>::new();
            map.insert(TestStruct1(10));
            map.insert(TestStruct2(10));

            for value in map.iter_mut() {
                value.mut_method();
            }

            assert_eq!(map.get(), Some(&TestStruct1(20)));
            assert_eq!(map.get(), Some(&TestStruct2(15)));
        }

        #[test]
        fn get_raw() {
            let mut map = AnyMap::<dyn TestTrait>::new();
            map.insert(TestStruct1(10));
            map.insert(TestStruct2(10));

            map.get_mut_raw(&TypeId::of::<TestStruct1>())
                .unwrap()
                .mut_method();

            assert_eq!(map.get(), Some(&TestStruct1(20)));
            assert_eq!(map.get(), Some(&TestStruct2(10)));
        }
    }
}

use fxhash::FxHashMap;
use std::hash::Hash;
use std::mem;

pub(crate) struct Map<K, V> {
    inner: MapInner<K, V>,
}

enum MapInner<K, V> {
    Empty,
    One(V),
    Map(Box<FxHashMap<K, V>>),
}

impl<K, V> Default for Map<K, V> {
    fn default() -> Self {
        Map {
            inner: MapInner::Empty,
        }
    }
}

impl<K, V> Map<K, V>
where
    K: Eq + Hash,
{
    pub(crate) fn get(&self, key: &K, key_from_value: impl FnOnce(&V) -> K) -> Option<&V> {
        match &self.inner {
            MapInner::One(one) if *key == key_from_value(one) => Some(one),
            MapInner::Map(map) => map.get(key),
            MapInner::Empty | MapInner::One(_) => None,
        }
    }

    pub(crate) fn get_or_insert_with(
        &mut self,
        key: K,
        key_from_value: impl FnOnce(&V) -> K,
        new_value: impl FnOnce() -> V,
    ) -> &mut V {
        match self.inner {
            MapInner::Empty => {
                self.inner = MapInner::One(new_value());
                match &mut self.inner {
                    MapInner::One(one) => one,
                    _ => unreachable!(),
                }
            }
            MapInner::One(_) => {
                let one = match mem::replace(&mut self.inner, MapInner::Empty) {
                    MapInner::One(one) => one,
                    _ => unreachable!(),
                };
                // If this panics, the child `one` will be lost.
                let one_key = key_from_value(&one);
                // Same for the equality test.
                if key == one_key {
                    self.inner = MapInner::One(one);
                    match &mut self.inner {
                        MapInner::One(one) => return one,
                        _ => unreachable!(),
                    }
                }
                self.inner = MapInner::Map(Default::default());
                let map = match &mut self.inner {
                    MapInner::Map(map) => map,
                    _ => unreachable!(),
                };
                map.insert(one_key, one).unwrap();
                // But it doesn't matter if f panics, by this point
                // the map is as before but represented as a map instead
                // of a single value.
                map.entry(key).or_insert_with(new_value)
            }
            MapInner::Map(ref mut map) => map.entry(key).or_insert_with(new_value),
        }
    }

    pub(crate) fn remove(&mut self, key: &K, key_from_value: impl FnOnce(&V) -> K) -> Option<V> {
        match &mut self.inner {
            MapInner::One(one) if *key == key_from_value(one) => {
                match mem::replace(&mut self.inner, MapInner::Empty) {
                    MapInner::One(one) => Some(one),
                    _ => unreachable!(),
                }
            }
            MapInner::Map(map) => map.remove(key),
            MapInner::Empty | MapInner::One(_) => None,
        }
    }
}

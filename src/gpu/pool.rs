//! Generational object pools (the browser backends' GPU objects, the renderer's props): ids are
//! `generation << 20 | index`, so a destroyed object's id is reported as stale instead of
//! reaching a recycled one.

type Result<T> = std::result::Result<T, String>;

const INDEX_BITS: u32 = 20;
const INDEX_MASK: u32 = (1 << INDEX_BITS) - 1;
const GENERATIONS: u32 = 1 << (32 - INDEX_BITS);

/// Generational slots: ids are `generation << 20 | index`.
pub(crate) struct Pool<T> {
    pub(crate) slots: Vec<(u32, Option<T>)>,
    pub(crate) free: Vec<u32>,
}
impl<T> Default for Pool<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }
}
impl<T> Pool<T> {
    pub(crate) fn insert(&mut self, value: T) -> Result<u32> {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.1 = Some(value);
            return Ok(slot.0 << INDEX_BITS | index);
        }
        let index = self.slots.len() as u32;
        if index > INDEX_MASK {
            return Err("too many live GPU objects of one kind".into());
        }
        self.slots.push((0, Some(value)));
        Ok(index)
    }
    pub(crate) fn get(&self, raw: u32) -> Option<&T> {
        let slot = self.slots.get((raw & INDEX_MASK) as usize)?;
        (slot.0 == raw >> INDEX_BITS).then_some(slot.1.as_ref()?)
    }
    pub(crate) fn get_mut(&mut self, raw: u32) -> Option<&mut T> {
        let slot = self.slots.get_mut((raw & INDEX_MASK) as usize)?;
        (slot.0 == raw >> INDEX_BITS).then_some(slot.1.as_mut()?)
    }
    pub(crate) fn remove(&mut self, raw: u32) -> Option<T> {
        let index = raw & INDEX_MASK;
        let slot = self.slots.get_mut(index as usize)?;
        if slot.0 != raw >> INDEX_BITS {
            return None;
        }
        let value = slot.1.take()?;
        slot.0 = (slot.0 + 1) % GENERATIONS;
        self.free.push(index);
        Some(value)
    }
}

impl<T> Pool<T> {
    /// Live values with their ids.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (u32, &T)> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, (generation, v))| {
                v.as_ref().map(|v| (generation << INDEX_BITS | i as u32, v))
            })
    }
    /// Removes every value. Ids handed out before stay stale.
    pub(crate) fn drain(&mut self) -> Vec<T> {
        let mut out = Vec::new();
        self.free.clear();
        for (i, slot) in self.slots.iter_mut().enumerate().rev() {
            if let Some(v) = slot.1.take() {
                slot.0 = (slot.0 + 1) % GENERATIONS;
                out.push(v);
            }
            self.free.push(i as u32);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::Pool;

    #[test]
    fn stale_ids_miss_after_remove_and_drain() {
        let mut p = Pool::default();
        let a = p.insert("a").unwrap();
        let b = p.insert("b").unwrap();
        assert_eq!(p.remove(a), Some("a"));
        assert_eq!(p.get(a), None);
        let c = p.insert("c").unwrap();
        assert_ne!(a, c, "a recycled slot gets a new generation");
        assert_eq!(p.iter().map(|(_, v)| *v).collect::<Vec<_>>(), ["c", "b"]);
        let mut all = p.drain();
        all.sort_unstable();
        assert_eq!(all, ["b", "c"]);
        assert_eq!((p.get(b), p.get(c)), (None, None));
        let d = p.insert("d").unwrap();
        assert!(d != b && d != c && p.get(d) == Some(&"d"));
        assert_eq!(p.iter().count(), 1);
    }
}

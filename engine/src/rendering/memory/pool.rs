/// A pool allocator for fixed-size objects.
pub struct Pool {
    free_indices: Vec<u64>,
    next_free_index: u64,
    capacity: u64,
    object_size: u64,
}

impl Pool {
    pub fn new(capacity: u64, object_size: u64) -> Self {
        Pool {
            free_indices: Vec::new(),
            next_free_index: 0,
            capacity,
            object_size,
        }
    }

    pub fn allocate(&mut self) -> Option<u64> {
        // If there are any free indices, reuse one
        if let Some(index) = self.free_indices.pop() {
            return Some(index);
        }

        // Otherwise, allocate a new index if we have capacity
        if self.next_free_index < self.capacity {
            let index = self.next_free_index;
            self.next_free_index += 1;
            Some(index)
        } else {
            None
        }
    }

    pub fn free(&mut self, index: u64) {
        assert!(
            index < self.next_free_index,
            "Invalid index to free: {}",
            index
        );
        self.free_indices.push(index);
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }
}

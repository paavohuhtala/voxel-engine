#[derive(Debug, Clone, Copy)]
struct Block {
    offset: u64,
    order: u8,
    is_free: bool,
}

impl Block {
    fn size(&self) -> u64 {
        1u64 << self.order
    }

    fn get_buddy_offset(&self) -> u64 {
        self.offset ^ self.size()
    }
}

#[derive(Debug, Clone)]
pub struct AllocatorConfig {
    pub total_size: u64,
    pub alignment: u64,
    pub min_order: u8,
}

/// A simple Buddy allocator implementation to be used with external memory, primarily GPU buffers.
pub struct BuddyAllocator {
    config: AllocatorConfig,
    blocks: Vec<Block>,
}

impl BuddyAllocator {
    pub fn new(config: AllocatorConfig) -> Self {
        assert!(
            config.total_size.is_power_of_two(),
            "total_size must be a power of two"
        );

        // Initially there's just a single large free block
        let initial_block = Block {
            offset: 0,
            order: (config.total_size as f64).log2() as u8,
            is_free: true,
        };
        BuddyAllocator {
            config,
            blocks: vec![initial_block],
        }
    }

    pub fn allocate(&mut self, size: u64) -> Option<u64> {
        let alloc_order = self.get_order_for_size(size);

        // Find a suitable free block
        let free_block_index = self
            .blocks
            .iter()
            .position(|block| block.is_free && block.order >= alloc_order)?;

        // Split blocks until we reach the desired order
        let block = self.split_until_order(free_block_index, alloc_order);
        block.is_free = false;
        Some(block.offset)
    }

    fn free_block(&mut self, index: usize) {
        // Mark the block as free
        self.blocks[index].is_free = true;
        // Try to merge with buddies
        self.merge_buddies(index);
    }

    pub fn free(&mut self, offset: u64) {
        // Find the block index using binary search (O(log N))
        let index = self
            .find_block_index_by_offset(offset)
            .expect("Attempted to free a non-allocated block");

        if self.blocks[index].is_free {
            panic!(
                "Attempted to free an already free block at offset {}",
                offset
            );
        }

        self.free_block(index);
    }

    pub fn reallocate(&mut self, offset: u64, new_size: u64) -> Option<u64> {
        // Fast path: current block might already fit the new size
        let current_block_index = self.find_block_index_by_offset(offset)?;

        let current_block = self.blocks[current_block_index];
        let alloc_order = self.get_order_for_size(new_size);
        if alloc_order == current_block.order {
            // Current block is exactly the right size (order)
            return Some(offset);
        }

        // The block is either too small or too large
        // Free the current block and allocate a new one
        self.free_block(current_block_index);
        self.allocate(new_size)
    }

    pub fn size(&self) -> u64 {
        self.config.total_size
    }

    fn merge_buddies(&mut self, mut index: usize) {
        // Try to merge with buddies
        loop {
            let block = self.blocks[index];
            let buddy_offset = block.get_buddy_offset();

            let mut merged = false;

            if buddy_offset > block.offset {
                // Buddy should be the next block (right buddy)
                if index + 1 < self.blocks.len() {
                    let next_block = self.blocks[index + 1];
                    if next_block.offset == buddy_offset
                        && next_block.is_free
                        && next_block.order == block.order
                    {
                        // Merge with next
                        self.blocks.remove(index + 1);
                        self.blocks[index].order += 1;
                        merged = true;
                    }
                }
            } else if index > 0 {
                // Buddy should be the previous block (left buddy)
                let prev_block = self.blocks[index - 1];
                if prev_block.offset == buddy_offset
                    && prev_block.is_free
                    && prev_block.order == block.order
                {
                    // Merge with prev
                    self.blocks.remove(index);
                    index -= 1;
                    self.blocks[index].order += 1;
                    merged = true;
                }
            }

            if !merged {
                break;
            }
        }
    }

    fn split_until_order(&mut self, index: usize, target_order: u8) -> &mut Block {
        let current_order = self.blocks[index].order;
        if current_order <= target_order {
            return &mut self.blocks[index];
        }

        let mut buddies = Vec::with_capacity((current_order - target_order) as usize);
        let current_offset = self.blocks[index].offset;

        // We are splitting down.
        // Original: Order K.
        // Becomes: Order Target.
        // Buddies created: Order Target, Order Target+1, ..., Order K-1.
        // And they are inserted in that order after the block.
        for order in target_order..current_order {
            let size = 1u64 << order;
            let buddy = Block {
                offset: current_offset + size,
                order,
                is_free: true,
            };
            buddies.push(buddy);
        }

        // Update the block at index
        self.blocks[index].order = target_order;

        // Insert buddies
        self.blocks.splice(index + 1..index + 1, buddies);

        &mut self.blocks[index]
    }

    fn get_order_for_size(&self, size: u64) -> u8 {
        // Compute the smallest order that fits the requested size and alignment
        let size_order = if size == 0 {
            0
        } else {
            size.next_power_of_two().trailing_zeros() as u8
        };
        let align_order = self.config.alignment.trailing_zeros() as u8;
        let required_order = size_order.max(align_order);
        required_order.max(self.config.min_order)
    }

    fn find_block_index_by_offset(&self, offset: u64) -> Option<usize> {
        self.blocks
            .binary_search_by_key(&offset, |block| block.offset)
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_layout(allocator: &BuddyAllocator, expected: &[(u64, u64, bool)]) {
        assert_eq!(
            allocator.blocks.len(),
            expected.len(),
            "Block count mismatch"
        );
        for (i, (block, &(exp_offset, exp_size, exp_free))) in
            allocator.blocks.iter().zip(expected.iter()).enumerate()
        {
            assert_eq!(block.offset, exp_offset, "Block {} offset mismatch", i);
            assert_eq!(block.size(), exp_size, "Block {} size mismatch", i);
            assert_eq!(block.is_free, exp_free, "Block {} free status mismatch", i);
        }
    }

    #[test]
    fn test_allocator_basic() {
        let config = AllocatorConfig {
            total_size: 1024,
            alignment: 1,
            min_order: 0,
        };
        let mut allocator = BuddyAllocator::new(config);

        assert_layout(&allocator, &[(0, 1024, true)]);

        let offset1 = allocator.allocate(128).unwrap();
        let offset2 = allocator.allocate(128).unwrap();

        assert_layout(
            &allocator,
            &[
                (0, 128, false),
                (128, 128, false),
                (256, 256, true),
                (512, 512, true),
            ],
        );

        allocator.free(offset1);
        allocator.free(offset2);

        // Should be merged back to a single block, so next alloc starts at 0
        allocator.allocate(1024).unwrap();
        assert_layout(&allocator, &[(0, 1024, false)]);
    }

    #[test]
    fn test_allocator_fragmentation() {
        let config = AllocatorConfig {
            total_size: 256,
            alignment: 1,
            min_order: 0,
        };
        let mut allocator = BuddyAllocator::new(config);

        let a = allocator.allocate(64).unwrap();
        let b = allocator.allocate(64).unwrap();
        let c = allocator.allocate(64).unwrap();
        let _d = allocator.allocate(64).unwrap();

        assert_layout(
            &allocator,
            &[
                (0, 64, false),   // A
                (64, 64, false),  // B
                (128, 64, false), // C
                (192, 64, false), // D
            ],
        );

        allocator.free(b); // Free 64
        allocator.free(c); // Free 128

        // B and C aren't merged, because they are not buddies
        assert_layout(
            &allocator,
            &[
                (0, 64, false),   // A
                (64, 64, true),   // Free
                (128, 64, true),  // Free
                (192, 64, false), // D
            ],
        );

        let e = allocator.allocate(128);
        assert!(e.is_none()); // Can't fit 128 contiguous

        allocator.free(a); // Free 0. Buddy of 0 is 64. Both free. Merge -> 0-127 (size 128).

        assert_layout(
            &allocator,
            &[
                (0, 128, true),   // Free (merged A+B)
                (128, 64, true),  // Free (C)
                (192, 64, false), // D
            ],
        );

        let f = allocator.allocate(128).unwrap();
        assert_eq!(f, 0);

        assert_layout(
            &allocator,
            &[
                (0, 128, false),  // F
                (128, 64, true),  // Free (C)
                (192, 64, false), // D
            ],
        );
    }

    #[test]
    fn test_alignment() {
        let config = AllocatorConfig {
            total_size: 1024,
            alignment: 64,
            min_order: 0,
        };
        let mut allocator = BuddyAllocator::new(config);

        // Request 1 byte. Should be aligned to 64, so block size at least 64.
        let offset = allocator.allocate(1).unwrap();
        assert_eq!(offset % 64, 0);

        // Check if it actually consumed 64 bytes (order 6)
        // If we allocate another 1 byte, it should be at 64
        let offset2 = allocator.allocate(1).unwrap();
        assert_eq!(offset2, 64);
    }

    #[test]
    fn test_reallocate() {
        let config = AllocatorConfig {
            total_size: 1024,
            alignment: 1,
            min_order: 0,
        };
        let mut allocator = BuddyAllocator::new(config);

        // 1. Allocate 128
        let mut ptr = allocator.allocate(128).unwrap();
        assert_eq!(ptr, 0);

        // 2. Reallocate to 256 (Grow)
        // Should free 0 (128), merge with buddy 128 (if free), alloc 256.
        ptr = allocator.reallocate(ptr, 256).unwrap();
        assert_eq!(ptr, 0);

        // 3. Reallocate to 64 (Shrink)
        // Should free 0 (256), split to 128, 64.
        ptr = allocator.reallocate(ptr, 64).unwrap();
        assert_eq!(ptr, 0);

        // 4. Fragment and Grow
        let ptr2 = allocator.allocate(64).unwrap(); // Takes the buddy at 64
        assert_eq!(ptr2, 64);

        // Try to grow ptr to 128.
        // Free(ptr=0) -> [Free(64) | Alloc(64, ptr2) | Free(128) ...]
        // Alloc(128) -> Can't take 0 (merged 128) because 64 is taken.
        // Takes 128.
        ptr = allocator.reallocate(ptr, 128).unwrap();
        assert_eq!(ptr, 128);

        // 5. Same size (No-op)
        let old_ptr = ptr;
        ptr = allocator.reallocate(ptr, 100).unwrap(); // 100 fits in 128 (order 7)
        assert_eq!(ptr, old_ptr);
    }
}

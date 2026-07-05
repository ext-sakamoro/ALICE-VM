//! Simple heap with bump allocation and free-list coalescing.

use crate::constants::MAX_HEAP;
use crate::error::VmError;

/// Simple heap with bump allocation and free-list.
#[derive(Debug, Clone)]
pub struct Heap {
    memory: Vec<i64>,
    free_list: Vec<(usize, usize)>,
    allocations: Vec<(usize, usize)>,
}

impl Heap {
    pub fn new() -> Self {
        Self {
            memory: vec![0; MAX_HEAP],
            free_list: vec![(0, MAX_HEAP)],
            allocations: Vec::new(),
        }
    }

    pub fn alloc(&mut self, size: usize) -> Result<usize, VmError> {
        if size == 0 {
            return Err(VmError::InvalidHeapSize);
        }
        for i in 0..self.free_list.len() {
            let (start, free_size) = self.free_list[i];
            if free_size >= size {
                let addr = start;
                if free_size == size {
                    self.free_list.remove(i);
                } else {
                    self.free_list[i] = (start + size, free_size - size);
                }
                self.allocations.push((addr, size));
                return Ok(addr);
            }
        }
        Err(VmError::HeapOutOfMemory)
    }

    pub fn free(&mut self, addr: usize) -> Result<(), VmError> {
        let pos = self
            .allocations
            .iter()
            .position(|&(a, _)| a == addr)
            .ok_or(VmError::HeapDoubleFree(addr))?;
        let (start, size) = self.allocations.remove(pos);
        self.free_list.push((start, size));
        self.free_list.sort_unstable_by_key(|&(s, _)| s);
        self.merge_free_list();
        Ok(())
    }

    fn merge_free_list(&mut self) {
        let mut i = 0;
        while i + 1 < self.free_list.len() {
            let (s1, sz1) = self.free_list[i];
            let (s2, sz2) = self.free_list[i + 1];
            if s1 + sz1 == s2 {
                self.free_list[i] = (s1, sz1 + sz2);
                self.free_list.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }

    pub fn load(&self, addr: usize) -> Result<i64, VmError> {
        self.memory
            .get(addr)
            .copied()
            .ok_or(VmError::HeapInvalidAddress(addr))
    }

    pub fn store(&mut self, addr: usize, value: i64) -> Result<(), VmError> {
        let slot = self
            .memory
            .get_mut(addr)
            .ok_or(VmError::HeapInvalidAddress(addr))?;
        *slot = value;
        Ok(())
    }
}

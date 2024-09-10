use anyhow::{bail, Result};

use memflow::prelude::v1::*;

#[repr(C)]
pub struct UtlMemory<T> {
    pub mem: Pointer64<[T]>, // 0x0000
    pub alloc_count: i32,    // 0x0008
    pub grow_size: i32,      // 0x000C
}

impl<T: Pod> UtlMemory<T> {
    #[inline]
    pub fn count(&self) -> i32 {
        self.alloc_count
    }

    pub fn element(&self, process: &mut IntoProcessInstanceArcBox<'_>, idx: usize) -> Result<T> {
        if idx >= self.count() as _ {
            bail!("index out of bounds");
        }

        process
            .read_ptr(self.mem.at(idx as _))
            .data_part()
            .map_err(Into::into)
    }

    #[inline]
    pub fn is_externally_allocated(&self) -> bool {
        self.grow_size < 0
    }
}

unsafe impl<T: 'static> Pod for UtlMemory<T> {}

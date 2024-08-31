use alloc::sync::Arc;
use core::ops::Deref;
use moic_driver::{TaskMeta, TaskId, Moic};
use core::cell::UnsafeCell;

use crate::BaseScheduler;

/// A task wrapper for the [`MOICScheduler`].
///
/// It add a task metadata to use in Moic scheduler.
pub struct MOICTask<T> {
    inner: T,
    meta: UnsafeCell<TaskMeta>,
}

impl<T> MOICTask<T> {
    /// Creates a new [`MOICTask`] from the inner task struct.
    pub const fn new(inner: T) -> Self {
        Self {
            inner,
            meta: UnsafeCell::new(TaskMeta::init()),
        }
    }

    /// Returns a reference to the inner task struct.
    pub const fn inner(&self) -> &T {
        &self.inner
    }

    /// Init the task
    pub fn init(&self, inner: usize) {
        self.get_mut_meta().inner = inner;
    }

    /// Init the task
    pub fn init_arc(self: &Arc<Self>) {
        let inner = Arc::into_raw(self.clone()) as usize;
        self.init(inner);
    }

    /// Get the task meta
    pub(crate) fn get_meta(&self) -> &TaskMeta {
        unsafe { &*self.meta.get() }
    }

    /// Get the mut task meta
    pub(crate) fn get_mut_meta(&self) -> &mut TaskMeta {
        unsafe { &mut *self.meta.get() }
    }
}

impl<T> Deref for MOICTask<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

unsafe impl<T> Sync for MOICTask<T> {}
unsafe impl<T> Send for MOICTask<T> {}

const PHYSICAL_OFFSET: usize = 0xffff_ffc0_0000_0000;
const MOIC_MMIO_ADDR: usize = 0x100_0000 + PHYSICAL_OFFSET;
static mut COUNT: usize = 0;

/// A Moic scheduler.
pub struct MOICScheduler<T> {
    inner: Moic,
    _phantom: core::marker::PhantomData<T>,
}

impl<T> MOICScheduler<T> {
    /// Creates a new empty [`MOICScheduler`].
    pub const fn new() -> Self {
        Self {
            inner: Moic::new(MOIC_MMIO_ADDR),
            _phantom: core::marker::PhantomData,
        }
    }
    /// get the name of scheduler
    pub fn scheduler_name() -> &'static str {
        "Moic"
    }
}


impl<T> BaseScheduler for MOICScheduler<T> {
    type SchedItem = Arc<MOICTask<T>>;

    fn init(&mut self) {

        self.inner = Moic::new(MOIC_MMIO_ADDR + unsafe { COUNT } * 0x1000);
        let tid = TaskMeta::new(0, false);
        let phy_tid = tid.value() - PHYSICAL_OFFSET;
        self.inner.switch_os(Some(unsafe { TaskId::virt(phy_tid) }));
        unsafe { COUNT += 1 };
    }

    fn add_task(&mut self, task: Self::SchedItem) {
        task.init_arc();
        let raw_meta = task.get_meta() as *const _ as usize - PHYSICAL_OFFSET;
        self.inner.add(unsafe { TaskId::virt(raw_meta) });
    }

    fn remove_task(&mut self, _task: &Self::SchedItem) -> Option<Self::SchedItem> {
        unimplemented!()
    }

    fn pick_next_task(&mut self) -> Option<Self::SchedItem> {
        if let Ok(tid) = self.inner.fetch() {
            let v = tid.value() + PHYSICAL_OFFSET;
            let meta: &mut TaskMeta = unsafe { TaskId::virt(v).into() };
            let raw_ptr = meta.inner as *const MOICTask<T>;
            return Some(unsafe { Arc::from_raw(raw_ptr) });
        }
        None
    }

    fn put_prev_task(&mut self, prev: Self::SchedItem, preempt: bool) {
        prev.init_arc();
        prev.get_mut_meta().is_preempt = preempt;
        let raw_meta = prev.get_meta() as *const _ as usize - PHYSICAL_OFFSET;
        self.inner.add(unsafe { TaskId::virt(raw_meta) })
    }

    fn task_tick(&mut self, _current: &Self::SchedItem) -> bool {
        false // no reschedule
    }

    fn set_priority(&mut self, _task: &Self::SchedItem, _prio: isize) -> bool {
        false
    }
}

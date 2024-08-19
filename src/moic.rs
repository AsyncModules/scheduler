use alloc::sync::Arc;
use core::ops::Deref;
use moic_driver::{TaskMeta, TaskId, Moic};
use lazy_init::LazyInit;
use core::cell::UnsafeCell;

use crate::BaseScheduler;

/// A task wrapper for the [`MOICScheduler`].
///
/// It add a time slice counter to use in round-robin scheduling.
pub struct MOICTask<T> {
    inner: T,
    meta: UnsafeCell<TaskMeta>,
}

impl<T> MOICTask<T> {
    /// Creates a new [`RRTask`] from the inner task struct.
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

const MOIC_MMIO_ADDR: usize = 0x100_0000 + 0xffff_ffc0_0000_0000;
static mut COUNT: usize = 0;

/// A simple [Round-Robin] (RR) preemptive scheduler.
///
/// It's very similar to the [`FifoScheduler`], but every task has a time slice
/// counter that is decremented each time a timer tick occurs. When the current
/// task's time slice counter reaches zero, the task is preempted and needs to
/// be rescheduled.
///
/// Unlike [`FifoScheduler`], it uses [`VecDeque`] as the ready queue. So it may
/// take O(n) time to remove a task from the ready queue.
///
/// [Round-Robin]: https://en.wikipedia.org/wiki/Round-robin_scheduling
/// [`FifoScheduler`]: crate::FifoScheduler
pub struct MOICScheduler<T> {
    inner: Moic,
    _phantom: core::marker::PhantomData<T>,
}

impl<T> MOICScheduler<T> {
    /// Creates a new empty [`RRScheduler`].
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

static OS_TID: LazyInit<TaskId> = LazyInit::new();

impl<T> BaseScheduler for MOICScheduler<T> {
    type SchedItem = Arc<MOICTask<T>>;

    fn init(&mut self) {
        if !OS_TID.is_init() {
            OS_TID.init_by(TaskMeta::new(0, false));
        }
        self.inner = Moic::new(MOIC_MMIO_ADDR + unsafe { COUNT } * 0x1000);
        self.inner.switch_os(Some(*OS_TID));
        unsafe { COUNT += 1 };
    }

    fn add_task(&mut self, task: Self::SchedItem) {
        task.init_arc();
        let raw_meta = task.get_meta() as *const _ as usize;
        self.inner.add(unsafe { TaskId::virt(raw_meta) });
    }

    fn remove_task(&mut self, _task: &Self::SchedItem) -> Option<Self::SchedItem> {
        unimplemented!()
    }

    fn pick_next_task(&mut self) -> Option<Self::SchedItem> {
        if let Ok(tid) = self.inner.fetch() {
            let meta: &mut TaskMeta = tid.into();
            let raw_ptr = meta.inner as *const MOICTask<T>;
            return Some(unsafe { Arc::from_raw(raw_ptr) });
        }
        None
    }

    fn put_prev_task(&mut self, prev: Self::SchedItem, preempt: bool) {
        prev.init_arc();
        prev.get_mut_meta().is_preempt = preempt;
        let raw_meta = prev.get_meta() as *const _ as usize;
        self.inner.add(unsafe { TaskId::virt(raw_meta) })
    }

    fn task_tick(&mut self, _current: &Self::SchedItem) -> bool {
        false // no reschedule
    }

    fn set_priority(&mut self, _task: &Self::SchedItem, _prio: isize) -> bool {
        false
    }
}

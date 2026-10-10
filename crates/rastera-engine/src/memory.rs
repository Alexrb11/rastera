use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use rastera_core::{CancellationToken, ErrorKind, RasteraError, Result};

#[derive(Clone)]
pub struct MemoryBudget(Arc<Budget>);

struct Budget {
    limit: u64,
    used: AtomicU64,
    peak: AtomicU64,
    foreground_loading: AtomicBool,
}

impl MemoryBudget {
    pub fn new(limit: u64) -> Self {
        Self(Arc::new(Budget {
            limit,
            used: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            foreground_loading: AtomicBool::new(false),
        }))
    }

    pub fn used(&self) -> u64 {
        self.0.used.load(Ordering::Acquire)
    }
    pub fn peak(&self) -> u64 {
        self.0.peak.load(Ordering::Acquire)
    }
    pub fn limit(&self) -> u64 {
        self.0.limit
    }

    pub fn set_foreground_loading(&self, loading: bool) {
        self.0.foreground_loading.store(loading, Ordering::Release);
    }

    pub fn reserve_wait(
        &self,
        bytes: u64,
        cancel: &CancellationToken,
        background: bool,
    ) -> Result<MemoryLease> {
        if bytes > self.0.limit {
            return self.reserve(bytes);
        }
        loop {
            cancel.check()?;
            if (!background || !self.0.foreground_loading.load(Ordering::Acquire))
                && let Ok(lease) = self.reserve(bytes)
            {
                return Ok(lease);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    pub fn reserve(&self, bytes: u64) -> Result<MemoryLease> {
        let mut used = self.used();
        loop {
            let next = used.checked_add(bytes).filter(|next| *next <= self.0.limit)
                .ok_or_else(|| RasteraError::new(
                    ErrorKind::TooLarge,
                    "No hay presupuesto de memoria para cargar esta imagen; prueba con una más pequeña",
                ))?;
            match self
                .0
                .used
                .compare_exchange_weak(used, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break,
                Err(current) => used = current,
            }
        }
        self.0.peak.fetch_max(used + bytes, Ordering::AcqRel);
        Ok(MemoryLease {
            budget: self.clone(),
            bytes,
        })
    }
}

pub struct MemoryLease {
    budget: MemoryBudget,
    bytes: u64,
}

impl MemoryLease {
    pub fn shrink_to(&mut self, bytes: u64) {
        if bytes < self.bytes {
            self.budget
                .0
                .used
                .fetch_sub(self.bytes - bytes, Ordering::AcqRel);
            self.bytes = bytes;
        }
    }
}

impl Drop for MemoryLease {
    fn drop(&mut self) {
        self.budget.0.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_results_keep_memory_reserved_until_drop() {
        let budget = MemoryBudget::new(100);
        let mut lease = budget.reserve(80).unwrap();
        assert!(budget.reserve(21).is_err());
        lease.shrink_to(30);
        let other = budget.reserve(70).unwrap();
        assert_eq!(budget.used(), 100);
        assert_eq!(budget.peak(), 100);
        drop(lease);
        drop(other);
        assert_eq!(budget.used(), 0);
        assert_eq!(budget.peak(), 100);
        assert_eq!(budget.limit(), 100);
    }

    #[test]
    fn concurrent_reservations_never_exceed_budget() {
        let budget = MemoryBudget::new(10);
        std::thread::scope(|scope| {
            for _ in 0..16 {
                let budget = budget.clone();
                scope.spawn(move || {
                    for _ in 0..100 {
                        if let Ok(lease) = budget.reserve(3) {
                            assert!(budget.used() <= 10);
                            std::thread::yield_now();
                            drop(lease);
                        }
                    }
                });
            }
        });
        assert_eq!(budget.used(), 0);
    }

    #[test]
    fn waiting_for_memory_or_foreground_is_cancellable() {
        let budget = MemoryBudget::new(10);
        let lease = budget.reserve(10).unwrap();
        let cancel = CancellationToken::default();
        cancel.cancel();
        assert_eq!(
            budget.reserve_wait(1, &cancel, false).err().unwrap().kind,
            ErrorKind::Cancelled
        );
        drop(lease);
        budget.set_foreground_loading(true);
        assert_eq!(
            budget.reserve_wait(1, &cancel, true).err().unwrap().kind,
            ErrorKind::Cancelled
        );
        assert_eq!(
            budget
                .reserve_wait(11, &CancellationToken::default(), false)
                .err()
                .unwrap()
                .kind,
            ErrorKind::TooLarge
        );
        budget.set_foreground_loading(false);
        drop(
            budget
                .reserve_wait(10, &CancellationToken::default(), true)
                .unwrap(),
        );
        assert_eq!(budget.used(), 0);
    }
}

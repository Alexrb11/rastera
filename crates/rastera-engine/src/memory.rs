use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use rastera_core::{ErrorKind, RasteraError, Result};

#[derive(Clone)]
pub struct MemoryBudget(Arc<Budget>);

struct Budget {
    limit: u64,
    used: AtomicU64,
}

impl MemoryBudget {
    pub fn new(limit: u64) -> Self {
        Self(Arc::new(Budget {
            limit,
            used: AtomicU64::new(0),
        }))
    }

    pub fn used(&self) -> u64 {
        self.0.used.load(Ordering::Acquire)
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
        drop(lease);
        drop(other);
        assert_eq!(budget.used(), 0);
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
}

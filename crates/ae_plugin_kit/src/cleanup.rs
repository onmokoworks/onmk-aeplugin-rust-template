//! SmartRender cleanup that runs on every exit path.
//!
//! Every successful `checkout_layer_pixels` must be balanced by
//! `checkin_layer_pixels`, including when parameter reads fail, a world fails
//! validation, or render code panics. (The output world from
//! `checkout_output` has no check-in call in the AE SDK.)
//!
//! Panics are converted into an error *after* cleanup instead of resuming the
//! unwind: `EffectMain` is `extern "C"`, and in release builds the pinned
//! `after-effects` crate only catches panics when the `catch_panics` cfg is set,
//! so an unwind reaching the host would abort After Effects.

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Run `work`, then always run `cleanup` exactly once.
///
/// Result priority: the first error wins.
/// - `work` ok, `cleanup` ok: `Ok(value)`
/// - `work` ok, `cleanup` err: cleanup error
/// - `work` err: work error (cleanup still runs; its error is dropped)
/// - `work` panics: cleanup runs, then `Err(on_panic)`
///
/// A panic inside `cleanup` is also contained and reported as `on_panic`
/// unless an earlier error already won.
pub fn with_render_cleanup<T, E>(
    work: impl FnOnce() -> Result<T, E>,
    cleanup: impl FnOnce() -> Result<(), E>,
    on_panic: E,
) -> Result<T, E> {
    let result = catch_unwind(AssertUnwindSafe(work));
    let released = catch_unwind(AssertUnwindSafe(cleanup));
    match result {
        Ok(Ok(value)) => match released {
            Ok(Ok(())) => Ok(value),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(on_panic),
        },
        Ok(Err(error)) => Err(error),
        Err(_) => Err(on_panic),
    }
}

/// Minimal interface of the SmartRender checkout callbacks so the tracking
/// logic can be unit tested without After Effects.
pub trait CheckinHost {
    type Layer;
    type Error: Copy;
    fn checkout(&self, id: u32) -> Result<Option<Self::Layer>, Self::Error>;
    fn checkin(&self, id: u32) -> Result<(), Self::Error>;
}

/// Records which checkout ids were acquired so they can all be released.
pub struct CheckoutTracker<'a, H: CheckinHost> {
    host: &'a H,
    acquired: RefCell<Vec<u32>>,
}

impl<'a, H: CheckinHost> CheckoutTracker<'a, H> {
    pub fn new(host: &'a H) -> Self {
        Self {
            host,
            acquired: RefCell::new(Vec::new()),
        }
    }

    /// Check out pixels for `id`. Only successful, non-NULL checkouts are
    /// recorded for check-in.
    pub fn checkout(&self, id: u32) -> Result<Option<H::Layer>, H::Error> {
        let layer = self.host.checkout(id)?;
        if layer.is_some() {
            self.acquired.borrow_mut().push(id);
        }
        Ok(layer)
    }

    /// Like [`CheckoutTracker::checkout`] but treats "no pixels" as `missing`.
    pub fn checkout_required(&self, id: u32, missing: H::Error) -> Result<H::Layer, H::Error> {
        self.checkout(id)?.ok_or(missing)
    }

    pub fn acquired(&self) -> Vec<u32> {
        self.acquired.borrow().clone()
    }

    /// Check in every acquired id in reverse order. All ids are attempted; the
    /// first error is returned. Calling it twice is harmless.
    pub fn checkin_all(&self) -> Result<(), H::Error> {
        let ids = std::mem::take(&mut *self.acquired.borrow_mut());
        let mut first = Ok(());
        for id in ids.into_iter().rev() {
            if let Err(error) = self.host.checkin(id) {
                if first.is_ok() {
                    first = Err(error);
                }
            }
        }
        first
    }
}

/// SmartRender scope: everything `work` checks out through the tracker is
/// checked in afterwards on success, error and panic.
pub fn with_checkouts<H: CheckinHost, T>(
    host: &H,
    on_panic: H::Error,
    work: impl FnOnce(&CheckoutTracker<'_, H>) -> Result<T, H::Error>,
) -> Result<T, H::Error> {
    let tracker = CheckoutTracker::new(host);
    with_render_cleanup(|| work(&tracker), || tracker.checkin_all(), on_panic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum E {
        Work,
        Cleanup,
        Panic,
        Missing,
        Checkout,
    }

    #[test]
    fn cleanup_runs_once_and_first_error_wins() {
        for work_fails in [false, true] {
            for cleanup_fails in [false, true] {
                let calls = Cell::new(0);
                let work = if work_fails { Err(E::Work) } else { Ok(7) };
                let cleanup = if cleanup_fails {
                    Err(E::Cleanup)
                } else {
                    Ok(())
                };
                let result = with_render_cleanup(
                    || work,
                    || {
                        calls.set(calls.get() + 1);
                        cleanup
                    },
                    E::Panic,
                );
                assert_eq!(result, work.and_then(|v| cleanup.map(|()| v)));
                assert_eq!(calls.get(), 1);
            }
        }
    }

    #[test]
    fn panic_runs_cleanup_and_becomes_error() {
        let called = Cell::new(false);
        let result: Result<(), E> = with_render_cleanup(
            || panic!("render"),
            || {
                called.set(true);
                Ok(())
            },
            E::Panic,
        );
        assert_eq!(result, Err(E::Panic));
        assert!(called.get());
    }

    #[test]
    fn panicking_cleanup_is_contained() {
        let result: Result<(), E> = with_render_cleanup(|| Ok(()), || panic!("checkin"), E::Panic);
        assert_eq!(result, Err(E::Panic));
        let result: Result<(), E> =
            with_render_cleanup(|| Err(E::Work), || panic!("checkin"), E::Panic);
        assert_eq!(result, Err(E::Work));
    }

    struct FakeHost {
        present: Vec<u32>,
        fail_checkout: Option<u32>,
        fail_checkin: Option<u32>,
        log: RefCell<Vec<String>>,
    }

    impl FakeHost {
        fn new(present: &[u32]) -> Self {
            Self {
                present: present.to_vec(),
                fail_checkout: None,
                fail_checkin: None,
                log: RefCell::new(Vec::new()),
            }
        }
    }

    impl CheckinHost for FakeHost {
        type Layer = u32;
        type Error = E;
        fn checkout(&self, id: u32) -> Result<Option<u32>, E> {
            if self.fail_checkout == Some(id) {
                return Err(E::Checkout);
            }
            self.log.borrow_mut().push(format!("out {id}"));
            Ok(self.present.contains(&id).then_some(id))
        }
        fn checkin(&self, id: u32) -> Result<(), E> {
            self.log.borrow_mut().push(format!("in {id}"));
            if self.fail_checkin == Some(id) {
                Err(E::Cleanup)
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn every_acquired_layer_is_checked_in_on_error_and_panic() {
        for mode in ["ok", "error", "panic"] {
            let host = FakeHost::new(&[0, 1]);
            let result = with_checkouts(&host, E::Panic, |t| {
                t.checkout_required(0, E::Missing)?;
                t.checkout(1)?;
                assert_eq!(t.checkout(2)?, None); // not present: no check-in
                match mode {
                    "error" => Err(E::Work),
                    "panic" => panic!("render"),
                    _ => Ok(()),
                }
            });
            let expected = match mode {
                "error" => Err(E::Work),
                "panic" => Err(E::Panic),
                _ => Ok(()),
            };
            assert_eq!(result, expected, "{mode}");
            assert_eq!(
                *host.log.borrow(),
                ["out 0", "out 1", "out 2", "in 1", "in 0"],
                "{mode}"
            );
        }
    }

    #[test]
    fn failed_checkout_releases_earlier_ones_and_checkin_errors_surface() {
        let mut host = FakeHost::new(&[0, 1]);
        host.fail_checkout = Some(1);
        let result = with_checkouts(&host, E::Panic, |t| {
            t.checkout(0)?;
            t.checkout(1)?;
            Ok(())
        });
        assert_eq!(result, Err(E::Checkout));
        assert_eq!(*host.log.borrow(), ["out 0", "in 0"]);

        let mut host = FakeHost::new(&[0, 1]);
        host.fail_checkin = Some(1);
        let result = with_checkouts(&host, E::Panic, |t| {
            t.checkout(0)?;
            t.checkout(1)?;
            Ok(())
        });
        // Check-in of 0 is still attempted after 1 failed.
        assert_eq!(result, Err(E::Cleanup));
        assert_eq!(*host.log.borrow(), ["out 0", "out 1", "in 1", "in 0"]);
    }

    #[test]
    fn missing_required_input_is_an_error_without_checkin() {
        let host = FakeHost::new(&[]);
        let result = with_checkouts(&host, E::Panic, |t| t.checkout_required(0, E::Missing));
        assert_eq!(result, Err(E::Missing));
        assert_eq!(*host.log.borrow(), ["out 0"]);
    }
}

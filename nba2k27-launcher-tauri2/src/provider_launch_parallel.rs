//! Scoped parallel lanes for independent pre-Steam preparation (QA 0.53).
//! Every lane is joined before `run` returns, so no owned receipt, link or
//! journal can outlive the caller's exact-owned cleanup. A hard failure (or
//! panic) in one lane raises `stop`; the others observe it at their next
//! safety checkpoint and return a `SiblingStop` error instead of mutating.
//! The flag only ever ADDS a failure: it never skips or replaces the fresh
//! game/ownership check each lane performs before its own mutations.
use anyhow::{Error, Result};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicBool, Ordering},
    thread,
};

#[derive(Debug)]
pub(super) struct SiblingStop;
impl std::fmt::Display for SiblingStop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Launch preparation stopped because another preparation step failed")
    }
}
impl std::error::Error for SiblingStop {}

pub(super) fn is_sibling_stop(error: &Error) -> bool {
    error.root_cause().downcast_ref::<SiblingStop>().is_some()
}

pub(super) struct Sibling<'a>(&'a AtomicBool);
impl Sibling<'_> {
    pub(super) fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Acquire) {
            return Err(SiblingStop.into());
        }
        Ok(())
    }
}

pub(super) struct Lanes<M, A, B> {
    pub main: Result<M>,
    pub second: Option<Result<A>>,
    pub third: Option<Result<B>>,
}
impl<M, A, B> Lanes<M, A, B> {
    fn errors(&self) -> [Option<&Error>; 3] {
        [
            self.main.as_ref().err(),
            self.second.as_ref().and_then(|r| r.as_ref().err()),
            self.third.as_ref().and_then(|r| r.as_ref().err()),
        ]
    }
    /// The root cause in the original sequential order (main, second, third):
    /// a lane stopped only by its sibling never hides the real failure.
    pub(super) fn first_error(&self) -> Option<&Error> {
        let errors = self.errors();
        errors
            .iter()
            .flatten()
            .find(|e| !is_sibling_stop(e))
            .or_else(|| errors.iter().flatten().next())
            .copied()
    }
    /// Move every successful value out (so the caller can own it for cleanup)
    /// together with the reported failure, if any.
    pub(super) fn into_parts(self) -> (Option<M>, Option<A>, Option<B>, Option<Error>) {
        let index = {
            let errors = self.errors();
            errors
                .iter()
                .position(|e| e.is_some_and(|e| !is_sibling_stop(e)))
                .or_else(|| errors.iter().position(Option::is_some))
        };
        let mut errors: [Option<Error>; 3] = [None, None, None];
        let main = match self.main {
            Ok(v) => Some(v),
            Err(e) => {
                errors[0] = Some(e);
                None
            }
        };
        fn split<T>(slot: &mut Option<Error>, r: Option<Result<T>>) -> Option<T> {
            match r {
                Some(Ok(v)) => Some(v),
                Some(Err(e)) => {
                    *slot = Some(e);
                    None
                }
                None => None,
            }
        }
        let [_, two, three] = &mut errors;
        let second = split(two, self.second);
        let third = split(three, self.third);
        let error = index.and_then(|i| errors[i].take());
        (main, second, third, error)
    }
}

fn guarded<T>(stop: &AtomicBool, lane: impl FnOnce() -> Result<T>) -> Result<T> {
    let result = catch_unwind(AssertUnwindSafe(lane)).unwrap_or_else(|_| {
        Err(anyhow::anyhow!(
            "A launch preparation worker stopped unexpectedly; owned files are restored by cleanup"
        ))
    });
    if result.is_err() {
        stop.store(true, Ordering::Release);
    }
    result
}

/// Run `main` on this thread and the optional side lanes on scoped workers.
pub(super) fn run<M, A: Send, B: Send>(
    stop: &AtomicBool,
    main: impl FnOnce(&Sibling) -> Result<M>,
    second: Option<impl FnOnce(&Sibling) -> Result<A> + Send>,
    third: Option<impl FnOnce(&Sibling) -> Result<B> + Send>,
) -> Lanes<M, A, B> {
    let sibling = Sibling(stop);
    thread::scope(|scope| {
        let sibling = &sibling;
        let second = second.map(|lane| scope.spawn(move || guarded(stop, || lane(sibling))));
        let third = third.map(|lane| scope.spawn(move || guarded(stop, || lane(sibling))));
        let main = guarded(stop, || main(sibling));
        fn join<T>(
            stop: &AtomicBool,
            handle: thread::ScopedJoinHandle<'_, Result<T>>,
        ) -> Result<T> {
            handle.join().unwrap_or_else(|_| {
                stop.store(true, Ordering::Release);
                Err(anyhow::anyhow!(
                    "A launch preparation worker could not be joined"
                ))
            })
        }
        Lanes {
            main,
            second: second.map(|h| join(stop, h)),
            third: third.map(|h| join(stop, h)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn real_cause_wins_over_sibling_stops_in_sequential_order() {
        let stop = AtomicBool::new(false);
        let lanes = run(
            &stop,
            |s| -> Result<()> {
                loop {
                    s.check()?;
                    thread::sleep(Duration::from_millis(1));
                }
            },
            Some(|_: &Sibling| -> Result<()> { anyhow::bail!("second failed") }),
            Some(|s: &Sibling| -> Result<()> {
                loop {
                    s.check()?;
                    thread::sleep(Duration::from_millis(1));
                }
            }),
        );
        assert!(is_sibling_stop(lanes.main.as_ref().unwrap_err()));
        assert_eq!(lanes.first_error().unwrap().to_string(), "second failed");
        let (main, second, third, error) = lanes.into_parts();
        assert!(main.is_none() && second.is_none() && third.is_none());
        assert_eq!(error.unwrap().to_string(), "second failed");
    }
    #[test]
    fn two_real_failures_report_the_earlier_sequential_step() {
        let stop = AtomicBool::new(false);
        let lanes = run(
            &stop,
            |_| -> Result<()> {
                thread::sleep(Duration::from_millis(20));
                anyhow::bail!("players failed")
            },
            Some(|_: &Sibling| -> Result<()> { anyhow::bail!("jukebox failed") }),
            None::<fn(&Sibling) -> Result<()>>,
        );
        assert_eq!(lanes.first_error().unwrap().to_string(), "players failed");
    }
    #[test]
    fn successes_are_returned_for_ownership_even_when_another_lane_failed() {
        let stop = AtomicBool::new(false);
        let (main, second, third, error) = run(
            &stop,
            |_| -> Result<&str> { Ok("players-store") },
            Some(|_: &Sibling| -> Result<&str> { Ok("jukebox-session") }),
            Some(|_: &Sibling| -> Result<()> { anyhow::bail!("stock decode failed") }),
        )
        .into_parts();
        assert_eq!(
            (main, second),
            (Some("players-store"), Some("jukebox-session"))
        );
        assert!(third.is_none());
        assert_eq!(error.unwrap().to_string(), "stock decode failed");
        assert!(stop.load(Ordering::Acquire));
    }
    #[test]
    fn all_success_keeps_stop_clear_and_values_intact() {
        let stop = AtomicBool::new(false);
        let (main, second, third, error) = run(
            &stop,
            |s| -> Result<u8> {
                s.check()?;
                Ok(1)
            },
            Some(|s: &Sibling| -> Result<u8> {
                s.check()?;
                Ok(2)
            }),
            Some(|s: &Sibling| -> Result<u8> {
                s.check()?;
                Ok(3)
            }),
        )
        .into_parts();
        assert_eq!((main, second, third), (Some(1), Some(2), Some(3)));
        assert!(error.is_none() && !stop.load(Ordering::Acquire));
    }
}

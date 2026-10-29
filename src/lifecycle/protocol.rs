//! Testable named-object handshake. Handles release ownership on their owner
//! thread when dropped. No backend/taskbar API is reachable from this module.
pub(super) trait Kernel {
    type Mutex;
    type Event;
    fn now_ms(&self) -> u64;
    fn create_mutex(&self, key: &str) -> Result<Self::Mutex, String>;
    fn open_mutex(&self, key: &str) -> Result<Option<Self::Mutex>, String>;
    /// true means acquired (including abandoned); false means timed out.
    fn acquire(&self, mutex: &mut Self::Mutex, timeout_ms: u32) -> Result<bool, String>;
    fn create_event(&self) -> Result<Self::Event, String>;
    fn open_event(&self) -> Result<Self::Event, String>;
    fn reset_event(&self, event: &Self::Event) -> Result<(), String>;
    fn signal_event(&self, event: &Self::Event) -> Result<(), String>;
}

pub(super) struct Claim<K: Kernel> {
    // Field drop order matters on notification/session startup failure: a busy
    // runtime mutex must never be observable without a live stop event.
    pub running: K::Mutex,
    pub stop: K::Event,
}

fn gate<K: Kernel>(kernel: &K, timeout: u32) -> Result<K::Mutex, String> {
    let mut gate = kernel.create_mutex("gate")?;
    if !kernel.acquire(&mut gate, timeout)? {
        return Err("timed out waiting for runtime startup/stop coordination".into());
    }
    Ok(gate)
}

pub(super) fn claim<K: Kernel>(kernel: &K) -> Result<Option<Claim<K>>, String> {
    let _gate = gate(kernel, 5000)?;
    let mut running = kernel.create_mutex("running")?;
    if !kernel.acquire(&mut running, 0)? {
        return Ok(None);
    }
    let stop = kernel.create_event()?;
    // Clearing a stale signal is legal ONLY after owning the runtime mutex and
    // while the gate excludes stoppers. A duplicate returns above without reset.
    kernel.reset_event(&stop)?;
    Ok(Some(Claim { stop, running }))
}

pub(super) fn stop<K: Kernel>(kernel: &K, timeout: u32) -> Result<(), String> {
    let started = kernel.now_ms();
    let remaining =
        || u64::from(timeout).saturating_sub(kernel.now_ms().saturating_sub(started)) as u32;
    // Keep the gate through restoration/exit: no successor can steal the mutex
    // before this waiter observes completion, or consume this stop request.
    let _gate = gate(kernel, remaining())?;
    let Some(mut running) = kernel.open_mutex("running")? else {
        return Ok(());
    };
    if kernel.acquire(&mut running, 0)? {
        return Ok(());
    } // unowned/abandoned stale object
    let event = match kernel.open_event() {
        Ok(event) => event,
        Err(error) => {
            // The owner may have exited between the probe and opening the event.
            if kernel.acquire(&mut running, 0)? {
                return Ok(());
            }
            return Err(error);
        }
    };
    kernel.signal_event(&event)?;
    if !kernel.acquire(&mut running, remaining())? {
        return Err(
            "timed out waiting for runtime restoration/exit; the stop request remains pending"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};
    #[derive(Default)]
    struct Data {
        events: Vec<String>,
        existing: bool,
        missing_event: bool,
        signaled: bool,
        busy_gate: bool,
        // First acquisition is always the gate; later responses model runtime ownership.
        running_waits: VecDeque<bool>,
        now: u64,
    }
    #[derive(Default)]
    struct Fake(Rc<RefCell<Data>>);
    struct Mutex {
        key: String,
        owner: bool,
        log: Rc<RefCell<Data>>,
    }
    struct Event(Rc<RefCell<Data>>);
    impl Drop for Event {
        fn drop(&mut self) {
            self.0.borrow_mut().events.push("close:event".into());
        }
    }
    impl Drop for Mutex {
        fn drop(&mut self) {
            if self.owner {
                self.log
                    .borrow_mut()
                    .events
                    .push(format!("release:{}", self.key));
            }
        }
    }
    impl Kernel for Fake {
        type Mutex = Mutex;
        type Event = Event;
        fn now_ms(&self) -> u64 {
            self.0.borrow().now
        }
        fn create_mutex(&self, key: &str) -> Result<Mutex, String> {
            self.0.borrow_mut().events.push(format!("create:{key}"));
            Ok(Mutex {
                key: key.into(),
                owner: false,
                log: self.0.clone(),
            })
        }
        fn open_mutex(&self, key: &str) -> Result<Option<Mutex>, String> {
            self.0.borrow_mut().events.push(format!("open:{key}"));
            Ok(self.0.borrow().existing.then(|| Mutex {
                key: key.into(),
                owner: false,
                log: self.0.clone(),
            }))
        }
        fn acquire(&self, mutex: &mut Mutex, timeout: u32) -> Result<bool, String> {
            let mut data = self.0.borrow_mut();
            data.events.push(format!("wait:{}:{timeout}", mutex.key));
            let acquired = if mutex.key == "gate" {
                !data.busy_gate
            } else {
                data.running_waits
                    .pop_front()
                    .expect("unexpected runtime wait")
            };
            mutex.owner = acquired;
            if mutex.key == "gate" {
                data.now += 10;
            }
            Ok(acquired)
        }
        fn create_event(&self) -> Result<Event, String> {
            self.0.borrow_mut().events.push("create:event".into());
            Ok(Event(self.0.clone()))
        }
        fn open_event(&self) -> Result<Event, String> {
            self.0.borrow_mut().events.push("open:event".into());
            if self.0.borrow().missing_event {
                Err("missing event".into())
            } else {
                Ok(Event(self.0.clone()))
            }
        }
        fn reset_event(&self, _: &Event) -> Result<(), String> {
            let mut data = self.0.borrow_mut();
            data.events.push("reset:event".into());
            data.signaled = false;
            Ok(())
        }
        fn signal_event(&self, _: &Event) -> Result<(), String> {
            let mut data = self.0.borrow_mut();
            data.events.push("signal:event".into());
            data.signaled = true;
            Ok(())
        }
    }
    fn fake(waits: impl IntoIterator<Item = bool>) -> Fake {
        let f = Fake::default();
        f.0.borrow_mut().running_waits.extend(waits);
        f
    }
    #[test]
    fn duplicate_does_not_touch_stop_event() {
        let f = fake([false]);
        f.0.borrow_mut().signaled = true;
        assert!(claim(&f).unwrap().is_none());
        assert!(f.0.borrow().signaled);
        assert!(!f.0.borrow().events.iter().any(|e| e.contains("event")));
    }
    #[test]
    fn stale_event_is_reset_only_while_gate_and_runtime_are_owned() {
        let f = fake([true]);
        f.0.borrow_mut().signaled = true;
        let lease = claim(&f).unwrap().unwrap();
        assert!(!f.0.borrow().signaled);
        assert_eq!(
            f.0.borrow().events,
            [
                "create:gate",
                "wait:gate:5000",
                "create:running",
                "wait:running:0",
                "create:event",
                "reset:event",
                "release:gate"
            ]
        );
        drop(lease);
        assert_eq!(
            &f.0.borrow().events[7..],
            ["release:running", "close:event"]
        );
    }
    #[test]
    fn absent_stop_only_opens_runtime_and_never_creates_or_signals_event() {
        let f = fake([]);
        stop(&f, 100).unwrap();
        assert_eq!(
            f.0.borrow().events,
            [
                "create:gate",
                "wait:gate:100",
                "open:running",
                "release:gate"
            ]
        );
    }
    #[test]
    fn unowned_or_abandoned_stale_mutex_is_not_an_instance() {
        let f = fake([true]);
        f.0.borrow_mut().existing = true;
        stop(&f, 100).unwrap();
        assert!(!f.0.borrow().events.iter().any(|e| e.contains("event")));
        assert_eq!(
            &f.0.borrow().events[4..],
            ["release:running", "release:gate"]
        );
    }
    #[test]
    fn startup_stop_handshake_is_serialized_and_timeout_is_total() {
        let f = fake([false, true]);
        f.0.borrow_mut().existing = true;
        stop(&f, 100).unwrap();
        assert!(f.0.borrow().signaled);
        assert_eq!(
            &f.0.borrow().events[4..],
            [
                "open:event",
                "signal:event",
                "wait:running:90",
                "close:event",
                "release:running",
                "release:gate"
            ]
        );
    }
    #[test]
    fn stop_timeout_leaves_request_pending_without_reset_or_termination() {
        let f = fake([false, false]);
        f.0.borrow_mut().existing = true;
        assert!(stop(&f, 100).unwrap_err().contains("timed out"));
        assert!(f.0.borrow().signaled);
        assert_eq!(f.0.borrow().events.last().unwrap(), "release:gate");
    }
    #[test]
    fn owner_exiting_before_event_open_is_a_successful_stop() {
        let f = fake([false, true]);
        f.0.borrow_mut().existing = true;
        f.0.borrow_mut().missing_event = true;
        stop(&f, 100).unwrap();
        assert!(!f.0.borrow().signaled);
    }

    #[test]
    fn startup_gate_timeout_never_probes_or_signals_half_created_instance() {
        let f = fake([]);
        f.0.borrow_mut().busy_gate = true;
        assert!(stop(&f, 100).unwrap_err().contains("timed out"));
        assert_eq!(f.0.borrow().events, ["create:gate", "wait:gate:100"]);
    }
}

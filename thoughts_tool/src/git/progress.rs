use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use gix::progress::{Count, NestedProgress, Progress, Unit};

/// A simple inline progress reporter for gitoxide operations.
/// Shows progress on a single line with carriage return updates.
#[derive(Clone)]
pub struct InlineProgress {
    name: String,
    state: Arc<State>,
}

struct State {
    last_draw: std::sync::Mutex<Option<Instant>>,
    current: AtomicUsize,
    max: AtomicUsize,
    has_max: AtomicBool,
    finished: AtomicBool,
}

impl InlineProgress {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: Arc::new(State {
                last_draw: std::sync::Mutex::new(None),
                current: AtomicUsize::new(0),
                max: AtomicUsize::new(0),
                has_max: AtomicBool::new(false),
                finished: AtomicBool::new(false),
            }),
        }
    }

    fn draw(&self) {
        let now = Instant::now();

        // Throttle updates
        {
            let mut last = self.state.last_draw.lock().unwrap();
            if let Some(last_time) = *last
                && now.duration_since(last_time) < Duration::from_millis(50)
                && self.state.has_max.load(Ordering::Relaxed)
            {
                return;
            }
            *last = Some(now);
        }

        let current = self.state.current.load(Ordering::Relaxed);
        let has_max = self.state.has_max.load(Ordering::Relaxed);
        let max = self.state.max.load(Ordering::Relaxed);

        let mut line = String::new();
        line.push_str("  ");
        line.push_str(&self.name);
        line.push_str(": ");

        if has_max && max > 0 {
            let pct = (current as f32 / max as f32) * 100.0;
            line.push_str(&format!("{}/{} ({:.1}%)", current, max, pct));
        } else {
            line.push_str(&format!("{}", current));
        }

        print!("\r{}", line);
        io::stdout().flush().ok();
    }
}

impl Count for InlineProgress {
    fn set(&self, step: usize) {
        self.state.current.store(step, Ordering::Relaxed);
        self.draw();
    }

    fn step(&self) -> usize {
        self.state.current.load(Ordering::Relaxed)
    }

    fn inc_by(&self, step: usize) {
        self.state.current.fetch_add(step, Ordering::Relaxed);
        self.draw();
    }

    fn counter(&self) -> gix::progress::StepShared {
        // Return a shared counter backed by our atomic
        Arc::new(self.state.current.load(Ordering::Relaxed).into())
    }
}

impl Progress for InlineProgress {
    fn init(&mut self, max: Option<usize>, _unit: Option<Unit>) {
        if let Some(m) = max {
            self.state.max.store(m, Ordering::Relaxed);
            self.state.has_max.store(true, Ordering::Relaxed);
        } else {
            self.state.has_max.store(false, Ordering::Relaxed);
        }
        self.state.current.store(0, Ordering::Relaxed);
        self.state.finished.store(false, Ordering::Relaxed);
        self.draw();
    }

    fn set_name(&mut self, _name: String) {
        // We keep our own name, ignore updates
    }

    fn name(&self) -> Option<String> {
        Some(self.name.clone())
    }

    fn id(&self) -> gix::progress::Id {
        [0u8; 4]
    }

    fn message(&self, _level: gix::progress::MessageLevel, _message: String) {
        // Ignore messages for now
    }
}

impl NestedProgress for InlineProgress {
    type SubProgress = InlineProgress;

    fn add_child(&mut self, name: impl Into<String>) -> Self::SubProgress {
        // Finish current line before starting child
        if !self.state.finished.load(Ordering::Relaxed) {
            println!();
        }
        InlineProgress::new(name)
    }

    fn add_child_with_id(
        &mut self,
        name: impl Into<String>,
        _id: gix::progress::Id,
    ) -> Self::SubProgress {
        self.add_child(name)
    }
}

impl Drop for InlineProgress {
    fn drop(&mut self) {
        // Ensure we print a newline when done
        if !self.state.finished.swap(true, Ordering::Relaxed) {
            // Only print newline if we actually drew something
            if self.state.last_draw.lock().unwrap().is_some() {
                println!();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_and_inc() {
        let mut p = InlineProgress::new("test");
        p.init(Some(100), None);
        p.inc_by(1);
        p.inc_by(9);
        p.set(25);
    }

    #[test]
    fn nested_children() {
        let mut p = InlineProgress::new("root");
        let mut c1 = p.add_child("child-1");
        c1.init(Some(10), None);
        c1.inc_by(3);
    }

    #[test]
    fn no_max_progress() {
        let mut p = InlineProgress::new("bytes");
        p.init(None, None);
        p.inc_by(100);
        p.inc_by(200);
    }
}

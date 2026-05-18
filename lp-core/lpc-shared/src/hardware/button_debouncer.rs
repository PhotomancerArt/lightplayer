use super::{ButtonEvent, ButtonEventKind, HardwareAddress};

#[derive(Debug, Clone)]
pub struct ButtonDebouncer {
    source: HardwareAddress,
    stable_state_pressed: bool,
    candidate_state_pressed: bool,
    candidate_since_ms: u64,
    stable_ms: u64,
    sequence: u32,
}

impl ButtonDebouncer {
    pub const DEFAULT_STABLE_MS: u64 = 30;

    pub fn new(source: HardwareAddress, stable_ms: u64) -> Self {
        Self {
            source,
            stable_state_pressed: false,
            candidate_state_pressed: false,
            candidate_since_ms: 0,
            stable_ms,
            sequence: 0,
        }
    }

    pub fn sample(&mut self, now_ms: u64, pressed: bool) -> Option<ButtonEvent> {
        if pressed != self.candidate_state_pressed {
            self.candidate_state_pressed = pressed;
            self.candidate_since_ms = now_ms;
            return None;
        }

        if self.candidate_state_pressed == self.stable_state_pressed {
            return None;
        }

        if now_ms.saturating_sub(self.candidate_since_ms) < self.stable_ms {
            return None;
        }

        self.stable_state_pressed = self.candidate_state_pressed;
        self.sequence = self.sequence.wrapping_add(1);
        Some(ButtonEvent::new(
            self.source.clone(),
            self.sequence,
            if self.stable_state_pressed {
                ButtonEventKind::Pressed
            } else {
                ButtonEventKind::Released
            },
        ))
    }

    pub fn stable_state_pressed(&self) -> bool {
        self.stable_state_pressed
    }
}

impl Default for ButtonDebouncer {
    fn default() -> Self {
        Self::new(HardwareAddress::gpio(0), Self::DEFAULT_STABLE_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_after_pressed_state_is_stable() {
        let mut debouncer = ButtonDebouncer::new(HardwareAddress::gpio(4), 30);

        assert_eq!(debouncer.sample(0, true), None);
        assert_eq!(debouncer.sample(20, true), None);

        let event = debouncer.sample(30, true).expect("pressed event");
        assert_eq!(event.source(), &HardwareAddress::gpio(4));
        assert_eq!(event.sequence(), 1);
        assert_eq!(event.kind(), ButtonEventKind::Pressed);
        assert!(debouncer.stable_state_pressed());
    }

    #[test]
    fn ignores_bounces_before_stable_interval() {
        let mut debouncer = ButtonDebouncer::new(HardwareAddress::gpio(4), 30);

        assert_eq!(debouncer.sample(0, true), None);
        assert_eq!(debouncer.sample(10, false), None);
        assert_eq!(debouncer.sample(20, true), None);
        assert_eq!(debouncer.sample(40, true), None);

        let event = debouncer.sample(50, true).expect("pressed event");
        assert_eq!(event.kind(), ButtonEventKind::Pressed);
    }

    #[test]
    fn emits_release_after_pressed() {
        let mut debouncer = ButtonDebouncer::new(HardwareAddress::gpio(4), 30);

        assert!(debouncer.sample(0, true).is_none());
        assert!(debouncer.sample(30, true).is_some());
        assert!(debouncer.sample(40, false).is_none());

        let event = debouncer.sample(70, false).expect("released event");
        assert_eq!(event.sequence(), 2);
        assert_eq!(event.kind(), ButtonEventKind::Released);
        assert!(!debouncer.stable_state_pressed());
    }
}

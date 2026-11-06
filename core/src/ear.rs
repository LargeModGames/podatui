//! Ear-detection auto-pause: pause on a single bud removal while playing, resume only what we paused.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EarAction {
    Pause,
    Resume,
}

#[derive(Default)]
pub struct EarPause {
    in_ear: [bool; 2],
    we_paused: bool,
}

impl EarPause {
    /// `None` for a bud holds its previous state (a transitional, in-motion reading).
    pub fn update(
        &mut self,
        ear: [Option<bool>; 2],
        playing: impl FnOnce() -> bool,
    ) -> Option<EarAction> {
        let new = [
            ear[0].unwrap_or(self.in_ear[0]),
            ear[1].unwrap_or(self.in_ear[1]),
        ];
        if new == self.in_ear {
            return None;
        }
        let was_wearing = self.in_ear[0] || self.in_ear[1];
        self.in_ear = new;
        if new[0] && new[1] {
            if self.we_paused {
                self.we_paused = false;
                return Some(EarAction::Resume);
            }
        } else if was_wearing && playing() {
            self.we_paused = true;
            return Some(EarAction::Pause);
        }
        None
    }

    pub fn in_ear(&self) -> [bool; 2] {
        self.in_ear
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IN: [Option<bool>; 2] = [Some(true), Some(true)];
    const LEFT_OUT: [Option<bool>; 2] = [Some(false), Some(true)];

    fn worn() -> EarPause {
        let mut ear = EarPause::default();
        assert_eq!(ear.update(IN, || panic!("not asked when seeding")), None);
        ear
    }

    #[test]
    fn seed_gives_no_action() {
        assert_eq!(worn().in_ear(), [true, true]);
    }

    #[test]
    fn single_removal_while_playing_pauses() {
        let mut ear = worn();
        assert_eq!(ear.update(LEFT_OUT, || true), Some(EarAction::Pause));
        assert_eq!(ear.in_ear(), [false, true]);
    }

    #[test]
    fn removal_while_not_playing_does_nothing() {
        let mut ear = worn();
        assert_eq!(ear.update(LEFT_OUT, || false), None);
        assert_eq!(ear.update(IN, || true), None);
    }

    #[test]
    fn both_back_in_after_we_paused_resumes_once() {
        let mut ear = worn();
        assert_eq!(ear.update(LEFT_OUT, || true), Some(EarAction::Pause));
        assert_eq!(ear.update([Some(false), Some(false)], || false), None);
        assert_eq!(ear.update(IN, || false), Some(EarAction::Resume));
        assert_eq!(ear.update(LEFT_OUT, || false), None);
        assert_eq!(ear.update(IN, || false), None);
    }

    #[test]
    fn insertion_without_our_pause_does_nothing() {
        let mut ear = EarPause::default();
        assert_eq!(ear.update(LEFT_OUT, || true), None);
        assert_eq!(ear.update(IN, || true), None);
    }

    #[test]
    fn none_holds_the_previous_value() {
        let mut ear = worn();
        assert_eq!(ear.update([None, None], || true), None);
        assert_eq!(ear.update([None, Some(true)], || true), None);
        assert_eq!(ear.in_ear(), [true, true]);
        assert_eq!(
            ear.update([None, Some(false)], || true),
            Some(EarAction::Pause)
        );
        assert_eq!(ear.in_ear(), [true, false]);
    }
}

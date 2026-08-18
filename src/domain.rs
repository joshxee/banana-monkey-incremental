use bevy::prelude::Resource;

pub const BANANAS_PER_HARVEST: f64 = 1.0;
pub const MAX_SAFE_BANANAS_COUNT: u64 = 9_007_199_254_740_991;
pub const MAX_SAFE_BANANAS: f64 = 9_007_199_254_740_991.0;

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Treasury {
    bananas: f64,
}

impl Default for Treasury {
    fn default() -> Self {
        Self { bananas: 0.0 }
    }
}

impl Treasury {
    pub fn from_saved(bananas: f64) -> Option<Self> {
        is_valid_banana_count(bananas).then_some(Self { bananas })
    }

    pub fn display_count(self) -> u64 {
        self.bananas as u64
    }

    pub fn commit_harvest(&mut self) -> bool {
        if self.bananas > MAX_SAFE_BANANAS - BANANAS_PER_HARVEST {
            return false;
        }

        self.bananas += BANANAS_PER_HARVEST;
        true
    }

    pub fn restart(&mut self) {
        self.bananas = 0.0;
    }
}

pub fn is_valid_banana_count(value: f64) -> bool {
    value.is_finite() && (0.0..=MAX_SAFE_BANANAS).contains(&value) && value.fract() == 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harvest_commits_exactly_one_banana() {
        let mut treasury = Treasury::default();

        assert!(treasury.commit_harvest());
        assert_eq!(treasury.bananas, 1.0);
        assert_eq!(treasury.display_count(), 1);
    }

    #[test]
    fn harvest_does_not_exceed_max_safe_integer() {
        let mut treasury = Treasury::from_saved(MAX_SAFE_BANANAS).unwrap();

        assert!(!treasury.commit_harvest());
        assert_eq!(treasury.bananas, MAX_SAFE_BANANAS);
    }

    #[test]
    fn saved_count_must_be_nonnegative_finite_whole_and_safe() {
        assert!(Treasury::from_saved(42.0).is_some());
        assert!(Treasury::from_saved(-1.0).is_none());
        assert!(Treasury::from_saved(1.5).is_none());
        assert!(Treasury::from_saved(f64::INFINITY).is_none());
        assert!(Treasury::from_saved(f64::NAN).is_none());
        assert!(Treasury::from_saved(MAX_SAFE_BANANAS + 1.0).is_none());
    }

    #[test]
    fn restart_clears_treasury() {
        let mut treasury = Treasury::from_saved(12.0).unwrap();

        treasury.restart();

        assert_eq!(treasury, Treasury::default());
    }
}

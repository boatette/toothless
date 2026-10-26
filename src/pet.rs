use std::time::Duration;

use crate::art::{Face, Pose, SNORE_STEPS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Asleep,
    Awake,
    Excited,
    Charging,
}

/// CPU percentages at which he wakes up, gets excited and starts charging a blast.
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    pub awake: f32,
    pub excited: f32,
    pub charging: f32,
}

impl Thresholds {
    pub fn mood(&self, cpu: f32) -> Mood {
        if cpu >= self.charging {
            Mood::Charging
        } else if cpu >= self.excited {
            Mood::Excited
        } else if cpu >= self.awake {
            Mood::Awake
        } else {
            Mood::Asleep
        }
    }
}

const SLEEP_FRAME: Duration = Duration::from_millis(600);
const AWAKE_FRAME: Duration = Duration::from_millis(200);
const BLINK_FRAME: Duration = Duration::from_millis(150);
const CHARGE_FRAME: Duration = Duration::from_millis(120);
const TRICK_FRAME: Duration = Duration::from_millis(160);
const TRICK_FRAMES: u32 = 14;
// excited flapping speeds up from the slow end at the excited threshold to the
// fast end at the charging one
const FLAP_SLOW_MS: f32 = 280.0;
const FLAP_FAST_MS: f32 = 110.0;

pub struct Pet {
    thresholds: Thresholds,
    cpu: f32,
    mood: Mood,
    step: u32,
    trick: u32,
    blink_in: u32,
    seed: u32,
}

impl Pet {
    pub fn new(thresholds: Thresholds, seed: u32) -> Self {
        let mut pet = Self {
            thresholds,
            cpu: 0.0,
            mood: Mood::Asleep,
            step: 0,
            trick: 0,
            blink_in: 0,
            seed: seed | 1,
        };
        pet.blink_in = pet.blink_gap();
        pet
    }

    pub fn mood(&self) -> Mood {
        self.mood
    }

    pub fn set_cpu(&mut self, cpu: f32) {
        self.cpu = cpu;
        let mood = self.thresholds.mood(cpu);
        if mood != self.mood {
            self.mood = mood;
            self.step = 0;
        }
    }

    /// A click: he hops and grins for a couple of seconds, whatever his mood.
    pub fn poke(&mut self) {
        self.trick = TRICK_FRAMES;
    }

    /// The next frame and how long to show it.
    pub fn advance(&mut self) -> (Pose, Duration) {
        let step = self.step;
        self.step = self.step.wrapping_add(1);

        if self.trick > 0 {
            self.trick -= 1;
            let hop = if self.trick > TRICK_FRAMES / 2 {
                (step % 2) as i32 * 2
            } else {
                0
            };
            return (
                Pose::Sit {
                    face: Face::Happy,
                    wing: false,
                    hop,
                },
                TRICK_FRAME,
            );
        }

        match self.mood {
            Mood::Asleep => (
                Pose::Sleep {
                    breath: step / 2 % 2 == 1,
                    snore: (step % SNORE_STEPS as u32) as u8,
                },
                SLEEP_FRAME,
            ),
            Mood::Awake => {
                if self.blink_in == 0 {
                    self.blink_in = self.blink_gap();
                    (
                        Pose::Sit {
                            face: Face::Blink,
                            wing: false,
                            hop: 0,
                        },
                        BLINK_FRAME,
                    )
                } else {
                    self.blink_in -= 1;
                    (
                        Pose::Sit {
                            face: Face::Plain,
                            wing: false,
                            hop: 0,
                        },
                        AWAKE_FRAME,
                    )
                }
            }
            Mood::Excited => {
                let span = (self.thresholds.charging - self.thresholds.excited).max(1.0);
                let t = ((self.cpu - self.thresholds.excited) / span).clamp(0.0, 1.0);
                let ms = FLAP_SLOW_MS - t * (FLAP_SLOW_MS - FLAP_FAST_MS);
                let up = step.is_multiple_of(2);
                (
                    Pose::Sit {
                        face: Face::Plain,
                        wing: up,
                        hop: if up { 0 } else { 2 },
                    },
                    Duration::from_millis(ms as u64),
                )
            }
            Mood::Charging => {
                const PULSE: [i32; 4] = [1, 2, 3, 2];
                (
                    Pose::Charge {
                        glow: PULSE[step as usize % PULSE.len()],
                    },
                    CHARGE_FRAME,
                )
            }
        }
    }

    // awake frames until the next blink, 3 to 6 seconds
    fn blink_gap(&mut self) -> u32 {
        // xorshift32
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        15 + self.seed % 16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Thresholds = Thresholds {
        awake: 10.0,
        excited: 50.0,
        charging: 85.0,
    };

    #[test]
    fn mood_follows_the_thresholds() {
        assert_eq!(Mood::Asleep, T.mood(0.0));
        assert_eq!(Mood::Asleep, T.mood(9.9));
        assert_eq!(Mood::Awake, T.mood(10.0));
        assert_eq!(Mood::Excited, T.mood(60.0));
        assert_eq!(Mood::Charging, T.mood(85.0));
        assert_eq!(Mood::Charging, T.mood(100.0));
    }

    #[test]
    fn sleeps_until_the_cpu_wakes_him() {
        let mut pet = Pet::new(T, 7);
        assert!(matches!(pet.advance().0, Pose::Sleep { .. }));
        pet.set_cpu(30.0);
        assert!(matches!(pet.advance().0, Pose::Sit { .. }));
        pet.set_cpu(95.0);
        assert!(matches!(pet.advance().0, Pose::Charge { .. }));
    }

    #[test]
    fn blinks_now_and_then() {
        let mut pet = Pet::new(T, 7);
        pet.set_cpu(30.0);
        let blinks = (0..200)
            .filter(|_| {
                matches!(
                    pet.advance().0,
                    Pose::Sit {
                        face: Face::Blink,
                        ..
                    }
                )
            })
            .count();
        assert!((6..=14).contains(&blinks), "{blinks} blinks");
    }

    #[test]
    fn flaps_faster_under_more_load() {
        let mut pet = Pet::new(T, 7);
        pet.set_cpu(51.0);
        let slow = pet.advance().1;
        pet.set_cpu(84.0);
        let fast = pet.advance().1;
        assert!(fast < slow);
    }

    #[test]
    fn a_poke_overrides_the_mood_then_wears_off() {
        let mut pet = Pet::new(T, 7);
        pet.poke();
        for _ in 0..TRICK_FRAMES {
            assert!(matches!(
                pet.advance().0,
                Pose::Sit {
                    face: Face::Happy,
                    ..
                }
            ));
        }
        assert!(matches!(pet.advance().0, Pose::Sleep { .. }));
    }
}

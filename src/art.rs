use anyhow::Result;

use crate::sprite::{Canvas, Grid};

/// The frame every pose is drawn into, with room above for hops and snores and
/// room in front of his mouth for a plasma blast.
pub const WIDTH: usize = 40;
pub const HEIGHT: usize = 28;

// where each sprite sits on the frame
const SIT: (i32, i32) = (1, 4);
const SLEEP: (i32, i32) = (1, 15);

// where each patch sits on the sitting sprite
const EYE: (i32, i32) = (22, 5);
const HAPPY: (i32, i32) = (22, 5);
const MOUTH: (i32, i32) = (22, 9);
const WING: (i32, i32) = (0, 1);
const PLASMA: (i32, i32) = (34, 10);

// the part of the sleeping sprite that rises as he breathes in: his back and folded wing
const BACK_COLUMNS: std::ops::Range<usize> = 2..18;
const BACK_ROWS: std::ops::Range<usize> = 0..6;

const SNORE: &str = "ZZZZZ\n___Z_\n__Z__\n_Z___\nZZZZZ";
const SMALL_SNORE: &str = "ZZZZ\n__Z_\n_Z__\nZZZZ";
pub const SNORE_STEPS: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Plain,
    Blink,
    Happy,
    Roar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pose {
    Sleep {
        breath: bool,
        snore: u8,
    },
    Sit {
        face: Face,
        wing: bool,
        hop: i32,
    },
    /// Sitting with his mouth open on a plasma blast of the given size, 1 to 3.
    Charge {
        glow: i32,
    },
}

pub struct Art {
    sit: Grid,
    sleep: Grid,
    wing: Grid,
    blink: Grid,
    happy: Grid,
    roar: Grid,
    snore: Grid,
    small_snore: Grid,
}

impl Art {
    pub fn load() -> Result<Self> {
        Ok(Self {
            sit: Grid::parse("sit", include_str!("../sprites/sit.txt"))?,
            sleep: Grid::parse("sleep", include_str!("../sprites/sleep.txt"))?,
            wing: Grid::parse("wing_up", include_str!("../sprites/wing_up.txt"))?,
            blink: Grid::parse("blink", include_str!("../sprites/blink.txt"))?,
            happy: Grid::parse("happy", include_str!("../sprites/happy.txt"))?,
            roar: Grid::parse("roar", include_str!("../sprites/roar.txt"))?,
            snore: Grid::parse("snore", SNORE)?,
            small_snore: Grid::parse("small snore", SMALL_SNORE)?,
        })
    }

    pub fn render(&self, pose: Pose) -> Canvas {
        let mut canvas = Canvas::new(WIDTH, HEIGHT);
        match pose {
            Pose::Sleep { breath, snore } => self.sleep(&mut canvas, breath, snore),
            Pose::Sit { face, wing, hop } => self.sit(&mut canvas, face, wing, hop),
            Pose::Charge { glow } => {
                self.sit(&mut canvas, Face::Roar, false, 0);
                let (x, y) = (SIT.0 + PLASMA.0, SIT.1 + PLASMA.1);
                canvas.disc(x, y, glow + 1, b'v');
                canvas.disc(x, y, glow, b'V');
                canvas.disc(x, y, glow - 1, b'L');
            }
        }
        canvas
    }

    fn sit(&self, canvas: &mut Canvas, face: Face, wing: bool, hop: i32) {
        let (x, y) = (SIT.0, SIT.1 - hop);
        canvas.draw(&self.sit, x, y);
        if wing {
            canvas.draw(&self.wing, x + WING.0, y + WING.1);
        }
        match face {
            Face::Plain => {}
            Face::Blink => canvas.draw(&self.blink, x + EYE.0, y + EYE.1),
            Face::Happy => canvas.draw(&self.happy, x + HAPPY.0, y + HAPPY.1),
            Face::Roar => canvas.draw(&self.roar, x + MOUTH.0, y + MOUTH.1),
        }
    }

    fn sleep(&self, canvas: &mut Canvas, breath: bool, snore: u8) {
        let (x, y) = SLEEP;
        canvas.draw(&self.sleep, x, y);
        if breath {
            canvas.draw_part(&self.sleep, BACK_COLUMNS, BACK_ROWS, x, y - 1);
        }

        // two snores drift up and away from his head, half a cycle apart
        let rise = |step: u8| {
            let step = (step % SNORE_STEPS) as i32;
            (x + 28 + step / 2, y - 4 - step * 2)
        };
        let (sx, sy) = rise(snore);
        canvas.draw(&self.small_snore, sx, sy);
        let (sx, sy) = rise(snore + SNORE_STEPS / 2);
        canvas.draw(&self.snore, sx, sy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sprite_parses() {
        Art::load().unwrap();
    }

    #[test]
    fn patches_fit_on_the_sitting_sprite() {
        let art = Art::load().unwrap();
        for (grid, at) in [
            (&art.wing, WING),
            (&art.blink, EYE),
            (&art.happy, HAPPY),
            (&art.roar, MOUTH),
        ] {
            assert!(at.0 as usize + grid.width <= art.sit.width);
            assert!(at.1 as usize + grid.height <= art.sit.height);
        }
    }

    #[test]
    fn sprites_fit_the_frame() {
        let art = Art::load().unwrap();
        assert!(SIT.0 as usize + art.sit.width <= WIDTH);
        assert_eq!(HEIGHT, SIT.1 as usize + art.sit.height);
        assert_eq!(HEIGHT, SLEEP.1 as usize + art.sleep.height);
    }

    #[test]
    fn poses_differ() {
        let art = Art::load().unwrap();
        let plain = art.render(Pose::Sit {
            face: Face::Plain,
            wing: false,
            hop: 0,
        });
        for pose in [
            Pose::Sit {
                face: Face::Blink,
                wing: false,
                hop: 0,
            },
            Pose::Sit {
                face: Face::Happy,
                wing: false,
                hop: 0,
            },
            Pose::Sit {
                face: Face::Plain,
                wing: true,
                hop: 0,
            },
            Pose::Charge { glow: 2 },
        ] {
            assert_ne!(plain, art.render(pose), "{pose:?}");
        }
        assert_ne!(
            art.render(Pose::Sleep {
                breath: false,
                snore: 0
            }),
            art.render(Pose::Sleep {
                breath: true,
                snore: 0
            })
        );
    }
}

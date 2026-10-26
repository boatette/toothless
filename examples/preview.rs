//! Writes every pose side by side to a PPM sheet, for looking at the sprites
//! without a compositor: `cargo run --example preview -- sheet.ppm`

use std::io::Write;

use toothless::art::{Art, Face, HEIGHT, Pose, WIDTH};
use toothless::sprite::BYTES_PER_PIXEL;

const SCALE: usize = 6;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sheet.ppm".to_owned());
    let art = Art::load()?;
    let poses = [
        Pose::Sleep {
            breath: false,
            snore: 0,
        },
        Pose::Sleep {
            breath: true,
            snore: 3,
        },
        Pose::Sit {
            face: Face::Plain,
            wing: false,
            hop: 0,
        },
        Pose::Sit {
            face: Face::Blink,
            wing: false,
            hop: 0,
        },
        Pose::Sit {
            face: Face::Plain,
            wing: true,
            hop: 2,
        },
        Pose::Sit {
            face: Face::Happy,
            wing: false,
            hop: 0,
        },
        Pose::Charge { glow: 3 },
    ];

    let (w, h) = (WIDTH * SCALE, HEIGHT * SCALE);
    let mut sheet = vec![0u8; poses.len() * w * h * 3];
    let mut frame = vec![0u8; w * h * BYTES_PER_PIXEL];
    for (i, pose) in poses.into_iter().enumerate() {
        art.render(pose).blit(&mut frame, SCALE, false);
        for y in 0..h {
            for x in 0..w {
                let p = &frame[(y * w + x) * BYTES_PER_PIXEL..][..BYTES_PER_PIXEL];
                // over a mid-grey wallpaper stand-in, checkered per cell
                let bg = if ((x / SCALE) + (y / SCALE)).is_multiple_of(2) {
                    0x3a
                } else {
                    0x44
                };
                let over = |c: u8| c as u32 + bg * (255 - p[3] as u32) / 255;
                let at = (y * w * poses.len() + i * w + x) * 3;
                sheet[at..at + 3].copy_from_slice(&[
                    over(p[2]) as u8,
                    over(p[1]) as u8,
                    over(p[0]) as u8,
                ]);
            }
        }
    }

    let mut out = std::fs::File::create(path)?;
    write!(out, "P6\n{} {}\n255\n", w * poses.len(), h)?;
    out.write_all(&sheet)?;
    Ok(())
}

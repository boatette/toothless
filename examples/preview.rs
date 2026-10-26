//! Writes every pose side by side to a PPM sheet, for looking at the sprites
//! without a compositor: `cargo run --example preview -- sheet.ppm`

use std::io::Write;

use toothless::art::{Art, Face, HEIGHT, Pose, WIDTH};
use toothless::bubble::{self, Bubble};
use toothless::sprite::BYTES_PER_PIXEL;

const SCALE: usize = 6;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sheet.ppm".to_owned());
    let art = Art::load()?;
    let bubble = Bubble::load()?;
    // each pose, and the CPU reading he's asked about over some of them
    let poses = [
        (
            Pose::Sleep {
                breath: false,
                snore: 0,
            },
            None,
        ),
        (
            Pose::Sleep {
                breath: true,
                snore: 3,
            },
            Some(4.0),
        ),
        (
            Pose::Sit {
                face: Face::Plain,
                wing: false,
                hop: 0,
            },
            None,
        ),
        (
            Pose::Sit {
                face: Face::Blink,
                wing: false,
                hop: 0,
            },
            Some(27.0),
        ),
        (
            Pose::Sit {
                face: Face::Plain,
                wing: true,
                hop: 2,
            },
            Some(68.0),
        ),
        (
            Pose::Sit {
                face: Face::Happy,
                wing: false,
                hop: 0,
            },
            None,
        ),
        (Pose::Charge { glow: 3 }, Some(100.0)),
    ];

    let (w, h) = (WIDTH * SCALE, (bubble::HEIGHT + HEIGHT) * SCALE);
    let mut sheet = vec![0u8; poses.len() * w * h * 3];
    let mut frame = vec![0u8; w * h * BYTES_PER_PIXEL];
    for (i, (pose, usage)) in poses.into_iter().enumerate() {
        let (strip, below) = frame.split_at_mut(bubble::HEIGHT * SCALE * w * BYTES_PER_PIXEL);
        match usage {
            Some(usage) => bubble
                .render(&bubble::cpu(Some(usage)), false, false)
                .blit(strip, SCALE, false),
            None => strip.fill(0),
        }
        art.render(pose).blit(below, SCALE, false);
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

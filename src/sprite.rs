use anyhow::{Result, bail};

pub const BYTES_PER_PIXEL: usize = 4;

/// A cell that draws nothing. `.` in a grid is empty space, `_` in a patch keeps
/// whatever is underneath; both leave the canvas alone.
const EMPTY: u8 = 0;

/// Premultiplied ARGB8888 in memory order (b, g, r, a) for each palette character.
pub fn color(cell: u8) -> [u8; 4] {
    let (rgb, alpha) = match cell {
        b'o' => (0x56608a, 0xff), // rim light, so he reads on a dark wallpaper
        b'B' => (0x181a25, 0xff), // body
        b'S' => (0x23283b, 0xff), // wing membrane
        b'K' => (0x07080c, 0xff), // lines: brow, mouth, creases, pupils
        b'G' => (0x9cf06a, 0xff), // eye
        b'g' => (0x4fa83a, 0xff), // eye shadow
        b'W' => (0xf1ffe8, 0xff), // eye glint
        b'R' => (0xd23a32, 0xff), // tail fin
        b'r' => (0x8c2220, 0xff), // tail fin shadow
        b'M' => (0xf08ba6, 0xff), // gums
        b'V' => (0x9b8cff, 0xff), // plasma
        b'v' => (0x9b8cff, 0x70), // plasma glow
        b'L' => (0xe8e3ff, 0xff), // plasma core
        b'Z' => (0xb8c4ec, 0xff), // snores
        _ => return [0; 4],
    };
    let premultiply = |shift: u32| (((rgb >> shift) & 0xff) * alpha / 255) as u8;
    [premultiply(0), premultiply(8), premultiply(16), alpha as u8]
}

fn known(cell: u8) -> bool {
    matches!(cell, b'.' | b'_') || color(cell)[3] != 0
}

/// A hand-drawn sprite or patch, one character per pixel.
#[derive(Debug, Clone)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
    cells: Vec<u8>,
}

impl Grid {
    pub fn parse(name: &str, text: &str) -> Result<Self> {
        let rows: Vec<&[u8]> = text.lines().map(str::as_bytes).collect();
        let Some(width) = rows.first().map(|row| row.len()) else {
            bail!("sprite {name} is empty");
        };

        let mut cells = Vec::with_capacity(width * rows.len());
        for (y, row) in rows.iter().enumerate() {
            if row.len() != width {
                bail!(
                    "sprite {name} row {y} is {} wide, expected {width}",
                    row.len()
                );
            }
            if let Some(x) = row.iter().position(|&cell| !known(cell)) {
                bail!(
                    "sprite {name} has an unknown colour `{}` at {x},{y}",
                    row[x] as char
                );
            }
            cells.extend(row.iter().map(|&cell| match cell {
                b'.' | b'_' => EMPTY,
                cell => cell,
            }));
        }

        Ok(Self {
            width,
            height: rows.len(),
            cells,
        })
    }

    fn get(&self, x: usize, y: usize) -> u8 {
        self.cells[y * self.width + x]
    }
}

/// Palette cells composed into one frame, before scaling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    cells: Vec<u8>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![EMPTY; width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.cells[y * self.width + x]
    }

    pub fn set(&mut self, x: i32, y: i32, cell: u8) {
        if (0..self.width as i32).contains(&x) && (0..self.height as i32).contains(&y) {
            self.cells[y as usize * self.width + x as usize] = cell;
        }
    }

    pub fn draw(&mut self, grid: &Grid, x: i32, y: i32) {
        self.draw_part(grid, 0..grid.width, 0..grid.height, x, y);
    }

    /// Draws the given columns and rows of `grid` with its origin at `x`, `y`.
    pub fn draw_part(
        &mut self,
        grid: &Grid,
        columns: std::ops::Range<usize>,
        rows: std::ops::Range<usize>,
        x: i32,
        y: i32,
    ) {
        for gy in rows {
            for gx in columns.clone() {
                let cell = grid.get(gx, gy);
                if cell != EMPTY {
                    self.set(x + gx as i32, y + gy as i32, cell);
                }
            }
        }
    }

    pub fn disc(&mut self, cx: i32, cy: i32, radius: i32, cell: u8) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                // r² + r rounds the edge out, which reads rounder at pixel sizes
                if dx * dx + dy * dy <= radius * radius + radius {
                    self.set(cx + dx, cy + dy, cell);
                }
            }
        }
    }

    /// Writes the canvas into an ARGB8888 buffer, each cell as a `scale`-pixel square.
    pub fn blit(&self, dst: &mut [u8], scale: usize, mirror: bool) {
        let stride = self.width * scale * BYTES_PER_PIXEL;
        for y in 0..self.height {
            let row = &mut dst[y * scale * stride..][..stride];
            for x in 0..self.width {
                let source = if mirror { self.width - 1 - x } else { x };
                let pixel = color(self.get(source, y));
                for chunk in row[x * scale * BYTES_PER_PIXEL..][..scale * BYTES_PER_PIXEL]
                    .chunks_exact_mut(BYTES_PER_PIXEL)
                {
                    chunk.copy_from_slice(&pixel);
                }
            }
            let (first, rest) = dst[y * scale * stride..][..scale * stride].split_at_mut(stride);
            for copy in rest.chunks_exact_mut(stride) {
                copy.copy_from_slice(first);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_ragged_rows() {
        assert!(Grid::parse("t", "ab\nabc").is_err());
    }

    #[test]
    fn rejects_unknown_colours() {
        assert!(Grid::parse("t", "o?").is_err());
    }

    #[test]
    fn patches_keep_what_is_underneath() {
        let base = Grid::parse("base", "BB\nBB").unwrap();
        let patch = Grid::parse("patch", "_K\n._").unwrap();
        let mut canvas = Canvas::new(2, 2);
        canvas.draw(&base, 0, 0);
        canvas.draw(&patch, 0, 0);
        assert_eq!(
            [b'B', b'K', b'B', b'B'],
            [
                canvas.get(0, 0),
                canvas.get(1, 0),
                canvas.get(0, 1),
                canvas.get(1, 1)
            ]
        );
    }

    #[test]
    fn blit_scales_and_mirrors() {
        let mut canvas = Canvas::new(2, 1);
        canvas.set(0, 0, b'G');
        let mut out = vec![0; 2 * 2 * 2 * BYTES_PER_PIXEL];
        canvas.blit(&mut out, 2, true);
        let pixel = |x: usize, y: usize| &out[(y * 4 + x) * BYTES_PER_PIXEL..][..BYTES_PER_PIXEL];
        assert_eq!(&[0; 4], pixel(0, 1));
        assert_eq!(&color(b'G'), pixel(3, 1));
    }
}

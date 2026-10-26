use anyhow::Result;

use crate::sprite::{Canvas, Grid};

/// The strip his speech bubble is drawn in: above his frame, or below it when he
/// sits in a top corner.
pub const WIDTH: usize = crate::art::WIDTH;
pub const HEIGHT: usize = 14;

// where the bubble sits on the strip, so its tail points at his head
const BUBBLE_X: i32 = 5;
// the top row of text on the bubble
const TEXT_Y: i32 = 3;
const GLYPH_HEIGHT: usize = 5;

// just enough of a 3x5 font to say how busy the CPU is
const GLYPHS: &[(char, &str)] = &[
    ('0', "KKK\nK.K\nK.K\nK.K\nKKK"),
    ('1', "KK.\n.K.\n.K.\n.K.\nKKK"),
    ('2', "KKK\n..K\nKKK\nK..\nKKK"),
    ('3', "KKK\n..K\n.KK\n..K\nKKK"),
    ('4', "K.K\nK.K\nKKK\n..K\n..K"),
    ('5', "KKK\nK..\nKKK\n..K\nKKK"),
    ('6', "KKK\nK..\nKKK\nK.K\nKKK"),
    ('7', "KKK\n..K\n..K\n.K.\n.K."),
    ('8', "KKK\nK.K\nKKK\nK.K\nKKK"),
    ('9', "KKK\nK.K\nKKK\n..K\nKKK"),
    ('%', "K.K\n..K\n.K.\nK..\nK.K"),
    ('C', ".KK\nK..\nK..\nK..\n.KK"),
    ('P', "KK.\nK.K\nKK.\nK..\nK.."),
    ('U', "K.K\nK.K\nK.K\nK.K\nKKK"),
    ('.', ".\n.\n.\n.\nK"),
    (' ', ".\n.\n.\n.\n."),
];

/// What he says when asked how busy the CPU is, given its last reading.
pub fn cpu(usage: Option<f32>) -> String {
    match usage {
        // iowait can run backwards, which can nudge a reading past 100
        Some(usage) => format!("CPU {:.0}%", usage.clamp(0.0, 100.0)),
        None => "CPU ...".to_owned(),
    }
}

pub struct Bubble {
    shape: Grid,
    glyphs: Vec<(char, Grid)>,
}

impl Bubble {
    pub fn load() -> Result<Self> {
        let glyphs = GLYPHS
            .iter()
            .map(|&(c, text)| Ok((c, Grid::parse(&format!("glyph {c}"), text)?)))
            .collect::<Result<_>>()?;
        Ok(Self {
            shape: Grid::parse("bubble", include_str!("../sprites/bubble.txt"))?,
            glyphs,
        })
    }

    /// The bubble saying `text`, over his head with its tail pointing down at him,
    /// or under him pointing up when `below`. `mirror` when he faces left.
    pub fn render(&self, text: &str, below: bool, mirror: bool) -> Canvas {
        let mut canvas = Canvas::new(WIDTH, HEIGHT);
        canvas.draw(&self.shape, BUBBLE_X, 0);
        if mirror {
            canvas.mirror();
        }
        if below {
            canvas.flip();
        }

        // the text goes on after the bubble is turned around, so it still reads forwards
        let shape_width = self.shape.width as i32;
        let left = if mirror {
            WIDTH as i32 - BUBBLE_X - shape_width
        } else {
            BUBBLE_X
        };
        let mut x = left + (shape_width - self.width(text)) / 2;
        let y = if below {
            (HEIGHT - GLYPH_HEIGHT) as i32 - TEXT_Y
        } else {
            TEXT_Y
        };
        for glyph in self.glyphs(text) {
            canvas.draw(glyph, x, y);
            x += glyph.width as i32 + 1;
        }
        canvas
    }

    /// The glyphs for `text`, skipping any characters the font doesn't have.
    fn glyphs(&self, text: &str) -> impl Iterator<Item = &Grid> {
        text.chars().filter_map(|c| {
            self.glyphs
                .iter()
                .find(|(glyph, _)| *glyph == c)
                .map(|(_, grid)| grid)
        })
    }

    /// Cells across `text`, with one between each pair of letters.
    fn width(&self, text: &str) -> i32 {
        self.glyphs(text).map(|g| g.width as i32 + 1).sum::<i32>() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_the_reading_as_a_whole_percentage() {
        assert_eq!("CPU 42%", cpu(Some(42.4)));
        assert_eq!("CPU 100%", cpu(Some(100.3)));
        assert_eq!("CPU ...", cpu(None));
    }

    #[test]
    fn bubble_fits_the_strip() {
        let bubble = Bubble::load().unwrap();
        assert_eq!(HEIGHT, bubble.shape.height);
        assert!(BUBBLE_X as usize + bubble.shape.width <= WIDTH);
        assert!(bubble.glyphs.iter().all(|(_, g)| g.height == GLYPH_HEIGHT));
    }

    #[test]
    fn has_a_glyph_for_everything_he_says() {
        let bubble = Bubble::load().unwrap();
        for text in [cpu(None), cpu(Some(100.0)), "0123456789".to_owned()] {
            assert_eq!(text.chars().count(), bubble.glyphs(&text).count(), "{text}");
        }
    }

    #[test]
    fn the_longest_reading_fits_with_room_around_it() {
        let bubble = Bubble::load().unwrap();
        // an edge and two cells of padding on each side
        assert!(bubble.width(&cpu(Some(100.0))) <= bubble.shape.width as i32 - 6);
    }

    #[test]
    fn text_lands_inside_the_bubble_and_reads_forwards() {
        let bubble = Bubble::load().unwrap();
        for below in [false, true] {
            for mirror in [false, true] {
                let blank = bubble.render("", below, mirror);
                let said = bubble.render("C", below, mirror);
                let mut ink = Vec::new();
                for y in 0..HEIGHT {
                    for x in 0..WIDTH {
                        if blank.get(x, y) != said.get(x, y) {
                            assert_eq!(b'w', blank.get(x, y), "{x},{y} off the bubble");
                            ink.push((x, y));
                        }
                    }
                }
                // a C's left side is three cells tall, its right side two dots
                let left = ink.iter().map(|&(x, _)| x).min().unwrap();
                let tall = ink.iter().filter(|&&(x, _)| x == left).count();
                assert_eq!(3, tall, "below {below}, mirror {mirror}");
            }
        }
    }
}

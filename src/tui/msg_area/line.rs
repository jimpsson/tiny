use std::io;
use std::io::Write;

use rustbox::{RustBox};
use termbox_sys::tb_change_cell;
use termbox_sys;

use tui::style;

/// A single line added to the widget. May be rendered as multiple lines on the
/// screen.
#[derive(Debug)]
pub struct Line {
    /// Note that this String may not be directly renderable - TODO: explain.
    str       : String,

    /// Number of _visible_ (i.e. excludes color encodings) characters in the
    /// line.
    len_chars : i32,

    /// Visible char indexes (not counting color encodings) of split positions
    /// of the string - when the line doesn't fit into the screen we split it
    /// into multiple lines using these.
    ///
    /// It's important that these are really indices ignoring invisible chars,
    /// as we use difference between two indices in this vector as length of
    /// substrings.
    splits    : Vec<i32>,
}

impl Line {
    pub fn new() -> Line {
        Line {
            str: String::new(),
            len_chars: 0,
            splits: Vec::new(),
        }
    }

    pub fn add_text(&mut self, str : &str) {
        self.str.reserve(str.len());

        let mut iter = str.chars();
        while let Some(mut char) = iter.next() {
            if char == style::COLOR_PREFIX {
                self.str.push(char);
                // read fg
                self.str.push(iter.next().unwrap());
                self.str.push(iter.next().unwrap());
                if let Some(mb_comma) = iter.next() {
                    if mb_comma == ',' {
                        self.str.push(mb_comma);
                        // read bg
                        self.str.push(iter.next().unwrap());
                        self.str.push(iter.next().unwrap());
                        continue;
                    } else {
                        char = mb_comma;
                    }
                } else {
                    break;
                }
            }

            if char == style::TERMBOX_COLOR_PREFIX {
                self.str.push(char);
                // read fg
                self.str.push(iter.next().unwrap());
                // read bg
                self.str.push(iter.next().unwrap());
            }

            else if char == style::RESET_PREFIX || char == style::BOLD_PREFIX {
                self.str.push(char);
            }

            // Ignore some chars that break the rendering. These are used by the
            // protocol.
            else if char > '\x07' {
                self.str.push(char);
                if char.is_whitespace() {
                    self.splits.push(self.len_chars);
                }
                self.len_chars += 1;
            }
        }
    }

    pub fn add_char(&mut self, char : char) {
        assert!(char != style::COLOR_PREFIX);
        if char.is_whitespace() {
            self.splits.push(self.len_chars);
        }
        self.str.push(char);
        self.len_chars += 1;
    }

    pub fn len_chars(&self) -> i32 {
        self.len_chars
    }

    /// How many lines does this take when rendered? O(n) where n = number of
    /// split positions in the lines (i.e.  whitespaces).
    pub fn rendered_height(&self, width : i32) -> i32 {
        let mut lines : i32 = 1;
        let mut line_start : i32 = 0;

        for split_idx in 0 .. self.splits.len() {
            let char_idx = *unsafe { self.splits.get_unchecked(split_idx) };
            // writeln!(io::stderr(), "rendered_height: char_idx: {}", char_idx);
            let col = char_idx - line_start;

            // How many more chars can we render in this line?
            let slots_in_line : i32 = width - (col + 1);

            // How many chars do we need to render if until the next split
            // point?
            let chars_until_next_split : i32 =
                // -1 because we don't need to render the space or EOL.
                *self.splits.get(split_idx + 1).unwrap_or(&self.len_chars) - 1 - char_idx;

            // writeln!(io::stderr(),
            //          "rendered_height: slots_in_line: {}, chars_until_next_split: {}",
            //          slots_in_line, chars_until_next_split);

            if (chars_until_next_split as i32) > slots_in_line {
                // writeln!(io::stderr(), "splitting at {}", char_idx);
                lines += 1;
                line_start = char_idx + 1;
            }
        }

        lines
    }

    #[inline]
    pub fn draw(&self, rustbox : &RustBox, pos_x : i32, pos_y : i32, width : i32) {
        self.draw_from(rustbox, pos_x, pos_y, 0, width);
    }

    pub fn draw_from(&self, _ : &RustBox, pos_x : i32, pos_y : i32, first_line : i32, width : i32) {
        writeln!(io::stderr(), "drawing {:?}", self.str).unwrap();

        let mut col = pos_x;
        let mut line = 0;

        let mut next_split_idx : usize = 0;

        let mut char_idx : i32 = 0;

        let mut fg : u16 = 0;
        let mut bg : u16 = 0;

        let mut iter = self.str.chars();
        while let Some(mut char) = iter.next() {
            if char == style::COLOR_PREFIX {
                let fg_1 = to_dec(iter.next().unwrap()) as u16;
                let fg_2 = to_dec(iter.next().unwrap()) as u16;
                // We 'or' here as 'fg' can have 'bold' value
                fg |= irc_color_to_termbox(fg_1 * 10 + fg_2);

                if let Some(char_) = iter.next() {
                    if char_ == ',' {
                        let bg_1 = to_dec(iter.next().unwrap()) as u16;
                        let bg_2 = to_dec(iter.next().unwrap()) as u16;
                        bg = irc_color_to_termbox(bg_1 * 10 + bg_2);
                        continue;
                    } else {
                        bg = 0;
                        char = char_;
                    }
                } else {
                    break;
                }
            }

            if char == style::TERMBOX_COLOR_PREFIX {
                fg = iter.next().unwrap() as u16;
                bg = iter.next().unwrap() as u16;
                continue;
            } else if char == style::BOLD_PREFIX {
                fg |= termbox_sys::TB_BOLD;
                continue;
            } else if char == style::RESET_PREFIX {
                fg = 0;
                bg = 0;
                continue;
            }

            if char.is_whitespace() {
                // We may want to move to the next line
                next_split_idx += 1;
                let next_split = self.splits.get(next_split_idx).unwrap_or(&self.len_chars);

                // How many more chars can we render in this line?
                let slots_in_line = width - (col - pos_x);

                // How many chars do we need to render if until the next
                // split point?
                assert!(*next_split > char_idx);
                let chars_until_next_split : i32 = *next_split - char_idx;

                // writeln!(io::stderr(), "chars_until_next_split: {}, slots_in_line: {}",
                //          chars_until_next_split, slots_in_line);

                if (chars_until_next_split as i32) <= slots_in_line {
                    // keep rendering chars
                    if line >= first_line {
                        unsafe { tb_change_cell(col, pos_y + line, char as u32, fg, bg); }
                    }
                    col += 1;
                } else {
                    // need to split here. ignore whitespace char.
                    line += 1;
                    col = pos_x;
                }

                char_idx += 1;
            }

            else {
                // Not possible to split. Need to make sure we don't render out
                // of bounds.
                if col - pos_x < width {
                    if line >= first_line {
                        unsafe { tb_change_cell(col, pos_y + line, char as u32, fg, bg); }
                    }
                    col += 1;
                }

                char_idx += 1;
            }
        }
    }
}

#[inline]
pub fn to_dec(ch : char) -> i8 {
    ((ch as u32) - ('0' as u32)) as i8
}

// IRC colors: http://en.wikichip.org/wiki/irc/colors
// Termbox colors: http://www.calmar.ws/vim/256-xterm-24bit-rgb-color-chart.html
fn irc_color_to_termbox(irc_color : u16) -> u16 {
    match irc_color {
         0 => 15,  // white
         1 => 0,   // black
         2 => 17,  // navy
         3 => 2,   // green
         4 => 9,   // red
         5 => 88,  // maroon
         6 => 5,   // purple
         7 => 130, // olive
         8 => 11,  // yellow
         9 => 10,  // light green
        10 => 6,   // teal
        11 => 14,  // cyan
        12 => 12,  // awful blue
        13 => 13,  // magenta
        14 => 8,   // gray
        15 => 7,   // light gray

        // The rest is directly mapped to termbox colors.
        _  => irc_color,
    }
}

////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {

extern crate test;

use self::test::Bencher;
use std::fs::File;
use std::io::Read;
use super::*;

#[test]
fn height_test_1() {
    let mut line = Line::new();
    line.add_text("a b c d e");
    assert_eq!(line.rendered_height(1), 5);
    assert_eq!(line.rendered_height(2), 5);
    assert_eq!(line.rendered_height(3), 3);
    assert_eq!(line.rendered_height(4), 3);
    assert_eq!(line.rendered_height(5), 2);
    assert_eq!(line.rendered_height(6), 2);
    assert_eq!(line.rendered_height(7), 2);
    assert_eq!(line.rendered_height(8), 2);
    assert_eq!(line.rendered_height(9), 1);
}

#[test]
fn height_test_2() {
    let mut line = Line::new();
    line.add_text("ab c d e");
    assert_eq!(line.rendered_height(1), 4);
    assert_eq!(line.rendered_height(2), 4);
    assert_eq!(line.rendered_height(3), 3);
    assert_eq!(line.rendered_height(4), 2);
    assert_eq!(line.rendered_height(5), 2);
    assert_eq!(line.rendered_height(6), 2);
    assert_eq!(line.rendered_height(7), 2);
    assert_eq!(line.rendered_height(8), 1);
}

#[test]
fn height_test_3() {
    let mut line = Line::new();
    line.add_text("ab cd e");
    assert_eq!(line.rendered_height(1), 3);
    assert_eq!(line.rendered_height(2), 3);
    assert_eq!(line.rendered_height(3), 3);
    assert_eq!(line.rendered_height(4), 2);
    assert_eq!(line.rendered_height(5), 2);
    assert_eq!(line.rendered_height(6), 2);
    assert_eq!(line.rendered_height(7), 1);
}

#[test]
fn height_test_4() {
    let mut line = Line::new();
    line.add_text("ab cde");
    assert_eq!(line.rendered_height(1), 2);
    assert_eq!(line.rendered_height(2), 2);
    assert_eq!(line.rendered_height(3), 2);
    assert_eq!(line.rendered_height(4), 2);
    assert_eq!(line.rendered_height(5), 2);
    assert_eq!(line.rendered_height(6), 1);
}

#[test]
fn height_test_5() {
    let mut line = Line::new();
    line.add_text("abcde");
    for i in 0 .. 6 {
        assert_eq!(line.rendered_height(i), 1);
    }
}

#[bench]
fn bench_rendered_height(b : &mut Bencher) {

    // 1160 words, 2,237 ns/iter (+/- 150)

    let mut text = String::new();
    {
        let mut file = File::open("test/lipsum.txt").unwrap();
        file.read_to_string(&mut text).unwrap();
    }

    let mut line = Line::new();
    line.add_text(&text);
    b.iter(|| {
        line.rendered_height(1)
    });
}

} // mod tests

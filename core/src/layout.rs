//! Box-sizing a Card: choose line breaks and a font size that fill the frame
//! without ever overflowing it.
//!
//! Deliberately not a renderer. It measures real glyph advances from the font
//! file and returns strings plus a pixel size, so the webview only has to place
//! what it is given. That makes the hardest part of the typography testable.

use ttf_parser::Face;

/// The constraints of one Card on one viewport.
#[derive(Debug, Clone, Copy)]
pub struct WrapSpec {
    /// Fraction of the viewport width a line may occupy.
    pub max_width_fraction: f32,
    /// Largest the type may ever get, in px.
    pub max_px: f32,
    /// Smallest the type may shrink to before we accept overflow.
    pub min_px: f32,
    /// CSS tracking, in em. Negative tightens.
    pub tracking_em: f32,
}

impl WrapSpec {
    /// 1920×1080 defaults from docs/design.md.
    pub fn hd() -> Self {
        WrapSpec { max_width_fraction: 0.84, max_px: 280.0, min_px: 110.0, tracking_em: -0.02 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub lines: Vec<String>,
    pub size_px: f32,
}

/// A parsed font, kept as bytes so `Face` can be borrowed per call.
pub struct Font {
    data: Vec<u8>,
}

impl Font {
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Self, String> {
        Face::parse(&data, 0).map_err(|e| format!("{e:?}"))?;
        Ok(Font { data })
    }

    fn face(&self) -> Face<'_> {
        Face::parse(&self.data, 0).expect("font was validated on construction")
    }

    /// Width of `text` in px at `size_px`, including tracking.
    pub fn width(&self, text: &str, size_px: f32, tracking_em: f32) -> f32 {
        let face = self.face();
        let upem = face.units_per_em().max(1) as f32;
        let mut advance = 0f32;
        for ch in text.chars() {
            if let Some(glyph) = face.glyph_index(ch) {
                advance += face.glyph_hor_advance(glyph).unwrap_or(0) as f32;
            }
        }
        let glyphs = text.chars().count() as f32;
        advance / upem * size_px + tracking_em * size_px * glyphs
    }
}

/// Fit `text` to `viewport_width`.
///
/// Manual line breaks are honoured. Otherwise the line count comes from the word
/// count and the breaks are chosen to make the lines as equal as possible, so a
/// short word never sits alone on a line while its neighbour takes the rest.
pub fn wrap(font: &Font, text: &str, viewport_width: f32, spec: &WrapSpec) -> Layout {
    let max_width = viewport_width * spec.max_width_fraction;
    let manual: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();

    let lines: Vec<Vec<String>> = if manual.len() > 1 || text.contains('\n') {
        manual.iter().map(|l| l.split_whitespace().map(str::to_string).collect()).collect()
    } else {
        let words: Vec<String> = text.split_whitespace().map(str::to_string).collect();
        if words.is_empty() {
            return Layout { lines: vec![String::new()], size_px: spec.max_px };
        }
        let target = ((words.len() + 1) / 2).clamp(1, 3);
        break_lines(font, &words, target, spec.tracking_em)
    };

    let widest = lines
        .iter()
        .map(|line| font.width(&line.join(" "), 1.0, spec.tracking_em))
        .fold(0f32, f32::max)
        .max(f32::EPSILON);

    let fitted = max_width / widest;
    let size_px = fitted.clamp(spec.min_px.min(spec.max_px), spec.max_px);

    Layout { lines: lines.into_iter().map(|l| l.join(" ")).collect(), size_px }
}

fn break_lines(font: &Font, words: &[String], k: usize, tracking_em: f32) -> Vec<Vec<String>> {
    let n = words.len();
    if k >= n {
        return words.iter().map(|w| vec![w.clone()]).collect();
    }

    // all widths at a reference size of 1px, so scoring is size-independent
    let word_w: Vec<f32> = words.iter().map(|w| font.width(w, 1.0, tracking_em)).collect();
    let space_w = font.width(" ", 1.0, 0.0) - font.width("", 1.0, 0.0);

    let mut best: Option<(f32, Vec<usize>)> = None;
    for sizes in partitions(n, k) {
        let mut i = 0;
        let mut widths = Vec::with_capacity(k);
        for size in &sizes {
            let mut w = 0.0;
            for q in 0..*size {
                w += word_w[i + q] + if q > 0 { space_w } else { 0.0 };
            }
            i += *size;
            widths.push(w);
        }
        let mean = widths.iter().sum::<f32>() / k as f32;
        let mut score: f32 = widths.iter().map(|w| (w - mean).powi(2)).sum();
        // a line far narrower than the mean is a widow; discourage, never forbid
        let thin = widths.iter().filter(|w| **w < mean * 0.35).count();
        score += thin as f32 * mean * mean * 0.25;

        if best.as_ref().map(|(s, _)| score < *s).unwrap_or(true) {
            best = Some((score, sizes));
        }
    }

    let sizes = best.map(|(_, s)| s).unwrap_or_else(|| vec![n]);
    let mut out = Vec::with_capacity(k);
    let mut i = 0;
    for size in sizes {
        out.push(words[i..i + size].to_vec());
        i += size;
    }
    out
}

/// Every way to cut n items into k non-empty consecutive groups.
fn partitions(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut sizes = Vec::with_capacity(k);
    fn walk(start: usize, n: usize, k: usize, sizes: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if sizes.len() == k - 1 {
            let mut done = sizes.clone();
            done.push(n - start);
            out.push(done);
            return;
        }
        let remaining_lines = k - 1 - sizes.len();
        for size in 1..=(n - start - remaining_lines) {
            sizes.push(size);
            walk(start + size, n, k, sizes, out);
            sizes.pop();
        }
    }
    walk(0, n, k, &mut sizes, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        Font::from_bytes(include_bytes!("../assets/texgyreadventor-bold.otf").to_vec()).unwrap()
    }

    fn layout(text: &str) -> Layout {
        wrap(&font(), text, 1920.0, &WrapSpec::hd())
    }

    #[test]
    fn three_words_balance_instead_of_orphaning_the_first() {
        let l = layout("GO DRINK WATER");
        assert_eq!(l.lines, vec!["GO DRINK", "WATER"]);
    }

    #[test]
    fn short_text_is_one_line_at_max_size() {
        let l = layout("REVIEW MAIL");
        assert_eq!(l.lines.len(), 1);
        assert!(l.size_px <= 280.0 && l.size_px > 110.0);
    }

    #[test]
    fn a_single_word_fills_the_frame() {
        let l = layout("WAREHOUSE");
        assert_eq!(l.lines, vec!["WAREHOUSE"]);
        assert!((l.size_px - 280.0).abs() < 0.5, "expected the cap, got {}", l.size_px);
    }

    #[test]
    fn long_text_gets_more_lines_and_smaller_type() {
        let l = layout("CALL THE DENTIST BEFORE THEY CLOSE");
        assert!(l.lines.len() >= 2);
        assert!(l.size_px >= 110.0);
    }

    #[test]
    fn manual_breaks_always_win() {
        let l = layout("EAGLE LIMITED\nAWE");
        assert_eq!(l.lines, vec!["EAGLE LIMITED", "AWE"]);
    }

    #[test]
    fn nothing_ever_exceeds_the_frame() {
        let f = font();
        for text in [
            "GO DRINK WATER",
            "WAREHOUSE",
            "CALL THE DENTIST BEFORE THEY CLOSE",
            "STAND UP\nSTRETCH\nLOOK AWAY",
            "A",
        ] {
            let l = wrap(&f, text, 1920.0, &WrapSpec::hd());
            for line in &l.lines {
                let w = f.width(line, l.size_px, -0.02);
                assert!(w <= 1920.0 * 0.84 + 1.0, "{text:?} line {line:?} is {w}px wide");
            }
        }
    }

    #[test]
    fn a_narrow_viewport_shrinks_the_type() {
        let wide = wrap(&font(), "GO DRINK WATER", 1920.0, &WrapSpec::hd());
        let narrow = wrap(&font(), "GO DRINK WATER", 1280.0, &WrapSpec::hd());
        assert!(narrow.size_px < wide.size_px);
    }
}

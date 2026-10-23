//! Right-to-left text in text fields: the `LayoutJob::bidi` patch in the vendored `epaint`, and
//! the vendored `egui` turning it on for `TextEdit` (vendor/README.md). Typed Arabic and Hebrew
//! read right to left, right-to-left paragraphs align right, the cursor stands on the right
//! edge of a right-to-left character, and ← / → move it the way they point.
//!
//! The faces here are egui's defaults, which have no Arabic or Hebrew glyphs: those characters
//! draw as replacement boxes, which is enough to check order, positions and cursors.

use std::sync::Arc;

use egui::text::{CCursor, Galley, LayoutJob};
use egui::{Color32, FontId, Key, vec2};
use egui_kittest::Harness;

/// `text` laid out by egui's fonts, `width` wide, with or without bidi.
fn galley(text: &str, bidi: bool, width: f32) -> Arc<Galley> {
    let ctx = egui::Context::default();
    // The fonts exist once a pass has run. Its font-atlas upload is not wanted here.
    ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    let mut job = LayoutJob::simple(text.to_owned(), FontId::proportional(14.0), Color32::WHITE, width);
    job.bidi = bidi;
    job.keep_trailing_whitespace = true;
    let galley = ctx.fonts_mut(|f| f.layout_job(job));
    // Laying out added glyphs to the atlas: take that upload too, so the context drops clean.
    ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    galley
}

/// The x of each character's middle on `row`, in logical order, in galley coordinates.
fn middles(galley: &Galley, row: usize) -> Vec<f32> {
    let placed = &galley.rows[row];
    placed.row.glyphs.iter().map(|g| placed.pos.x + g.pos.x + g.advance_width / 2.0).collect()
}

fn decreasing(xs: &[f32]) -> bool {
    xs.windows(2).all(|w| w[0] > w[1])
}

#[test]
fn without_bidi_or_without_right_to_left_text_nothing_changes() {
    // Bidi off: right-to-left letters are placed left to right, as upstream egui does.
    let plain = galley("אבג דהו", false, f32::INFINITY);
    assert!(middles(&plain, 0).windows(2).all(|w| w[0] < w[1]));
    assert!(plain.rows.iter().all(|r| !r.row.is_bidi()));
    // Bidi on, but nothing right to left: the same galley, row for row.
    let a = galley("Hello, world 42 — wrapped text", false, 60.0);
    let b = galley("Hello, world 42 — wrapped text", true, 60.0);
    assert_eq!(a.rows.len(), b.rows.len());
    for (ra, rb) in a.rows.iter().zip(&b.rows) {
        assert_eq!(ra.pos, rb.pos);
        assert_eq!(ra.row.glyphs.iter().map(|g| g.pos).collect::<Vec<_>>(), rb.row.glyphs.iter().map(|g| g.pos).collect::<Vec<_>>());
    }
}

#[test]
fn right_to_left_text_reads_from_the_right_in_logical_glyph_order() {
    for text in ["אבג דהו", "مرحبا بكم"] {
        let g = galley(text, true, f32::INFINITY);
        assert_eq!(g.rows.len(), 1);
        // Glyph i is still character i: cursors index the text as before.
        assert_eq!(g.rows[0].row.text(), text);
        assert!(decreasing(&middles(&g, 0)), "{text}: {:?}", middles(&g, 0));
        // The cursor before the first character is on the right; at the end, on the left.
        let n = text.chars().count();
        let start = g.pos_from_cursor(CCursor::new(0)).min.x;
        let end = g.pos_from_cursor(CCursor::new(n)).min.x;
        assert!(start > end, "{text}: start {start}, end {end}");
        assert!((start - g.rect.right()).abs() < 1.0 && (end - g.rect.left()).abs() < 1.0, "{text}: {start} {end} {:?}", g.rect);
    }
}

#[test]
fn mixed_text_puts_each_run_in_its_own_direction() {
    // Left-to-right paragraph: the Hebrew words sit between "ab" and "cd", right to left.
    let g = galley("ab אב גד cd", true, f32::INFINITY);
    let x = middles(&g, 0);
    assert!(x[0] < x[1] && x[1] < x[7], "{x:?}");
    assert!(x[7] < x[6] && x[6] < x[4] && x[4] < x[3], "Hebrew right to left: {x:?}");
    assert!(x[3] < x[9] && x[9] < x[10], "{x:?}");
    // Right-to-left paragraph: the Latin word is on the left and still reads left to right.
    let g = galley("אב cd", true, f32::INFINITY);
    let x = middles(&g, 0);
    assert!(x[3] < x[4] && x[4] < x[1] && x[1] < x[0], "{x:?}");
    // A number inside Arabic keeps its digits left to right, between the words around it.
    let g = galley("سعر 120 ريال", true, f32::INFINITY);
    let x = middles(&g, 0);
    assert!(x[4] < x[5] && x[5] < x[6], "120 left to right: {x:?}");
    assert!(x[8] < x[4] && x[6] < x[0], "120 between the words: {x:?}");
}

#[test]
fn right_to_left_paragraphs_wrap_in_reading_order_and_align_right() {
    let text = "אבג דהו זחט יכל מנס עפצ קרש תאב";
    let width = 80.0;
    let g = galley(text, true, width);
    assert!(g.rows.len() > 1, "wraps");
    let mut seen = String::new();
    for (i, row) in g.rows.iter().enumerate() {
        let letters: Vec<f32> = row.row.glyphs.iter().zip(middles(&g, i)).filter(|(g, _)| !g.chr.is_whitespace()).map(|(_, x)| x).collect();
        assert!(decreasing(&letters), "row {i}: {letters:?}");
        assert!((row.pos.x + row.row.size.x - width).abs() < 1.0, "row {i} right-aligned: {:?} {}", row.pos, row.row.size.x);
        seen.push_str(&row.row.text());
    }
    // The first words are on the first row: rows break in logical order.
    assert_eq!(seen, text);
    // A second paragraph in its own direction: Latin after a newline is left-aligned.
    let g = galley("אבג\nabc", true, width);
    assert!(g.rows[0].pos.x > 0.0 && g.rows[1].pos.x.abs() < 1e-3, "{:?} {:?}", g.rows[0].pos, g.rows[1].pos);
}

#[test]
fn cursors_round_trip_through_positions_in_bidi_text() {
    for text in ["אבג דהו", "ab אב cd", "مرحبا 12 cd بكم"] {
        let g = galley(text, true, f32::INFINITY);
        for i in 0..=text.chars().count() {
            let rect = g.pos_from_cursor(CCursor::new(i));
            let back = g.cursor_from_pos(rect.center().to_vec2());
            let again = g.pos_from_cursor(back);
            assert!((rect.min.x - again.min.x).abs() < 1.0, "{text}: cursor {i} at {} came back as {} at {}", rect.min.x, back.index, again.min.x);
        }
    }
}

#[test]
fn bidi_layout_never_panics_on_odd_text() {
    for text in ["", "\n", "א\n", "\u{5B0}", "\u{2067}אב\u{2069} x", "a\u{200F}b", "אב\nab\nגד", "  אב  ", "\u{202E}abc", "ـــ", "لا"] {
        for width in [0.0, 5.0, 40.0, f32::INFINITY] {
            let g = galley(text, true, width);
            for i in 0..=text.chars().count() + 1 {
                let _ = g.pos_from_cursor(CCursor::new(i));
                let _ = g.is_rtl_at(&CCursor::new(i));
            }
            let _ = g.cursor_from_pos(vec2(-10.0, 0.0));
            let _ = g.cursor_from_pos(vec2(1e6, 1e6));
        }
    }
}

/// A single text field and what it showed last frame.
struct Field {
    text: String,
    multiline: bool,
    focus: bool,
    field: egui::Rect,
    galley: Option<Arc<Galley>>,
    galley_left: f32,
    cursor: Option<usize>,
}

impl Field {
    fn new(multiline: bool) -> Self {
        Self { text: String::new(), multiline, focus: true, field: egui::Rect::NOTHING, galley: None, galley_left: 0.0, cursor: None }
    }
}

fn field_harness(multiline: bool) -> Harness<'static, Field> {
    let mut h = Harness::builder().with_size(vec2(420.0, 160.0)).build_ui_state(
        |ui, f: &mut Field| {
            let edit = if f.multiline { egui::TextEdit::multiline(&mut f.text) } else { egui::TextEdit::singleline(&mut f.text) };
            let out = edit.desired_width(300.0).id_salt("field").show(ui);
            if f.focus {
                out.response.request_focus();
                f.focus = false;
            }
            f.field = out.response.rect;
            // The text is drawn from `galley_pos` (the painter takes the galley's own left off).
            f.galley_left = out.galley_pos.x;
            f.galley = Some(out.galley);
            f.cursor = out.cursor_range.map(|r| r.primary.index.0);
        },
        Field::new(multiline),
    );
    h.run_steps(2);
    h
}

#[test]
fn typing_arabic_into_a_field_reads_right_to_left_from_its_right_edge() {
    let mut h = field_harness(false);
    h.event(egui::Event::Text("مرحبا بكم".into()));
    h.run_steps(2);
    let f = h.state();
    assert_eq!(f.text, "مرحبا بكم", "stored in typing order");
    let g = f.galley.clone().expect("laid out");
    assert!(decreasing(&middles(&g, 0)), "{:?}", middles(&g, 0));
    // Aligned on the right of the field, as Latin text is on the left.
    let right = f.galley_left + g.rect.width();
    assert!(f.galley_left > f.field.center().x && f.field.right() - right < 12.0, "{:?} {} {right}", f.field, f.galley_left);
    let n = f.text.chars().count();
    assert_eq!(f.cursor, Some(n), "the cursor after the typed text");

    // Latin text in the same field stays on the left.
    let mut h = field_harness(false);
    h.event(egui::Event::Text("hello".into()));
    h.run_steps(2);
    assert!(h.state().galley_left < h.state().field.center().x);
}

#[test]
fn arrow_keys_move_the_cursor_the_way_they_point_in_arabic() {
    let mut h = field_harness(false);
    h.event(egui::Event::Text("مرحبا".into()));
    h.run_steps(2);
    assert_eq!(h.state().cursor, Some(5), "at the logical end, which is the left");
    // → goes back toward the start of the word (the right).
    h.key_press(Key::ArrowRight);
    h.run_steps(1);
    h.key_press(Key::ArrowRight);
    h.run_steps(1);
    assert_eq!(h.state().cursor, Some(3));
    // ← goes forward again (to the left).
    h.key_press(Key::ArrowLeft);
    h.run_steps(1);
    assert_eq!(h.state().cursor, Some(4));

    // Latin is unchanged: ← goes back.
    let mut h = field_harness(false);
    h.event(egui::Event::Text("hello".into()));
    h.run_steps(2);
    h.key_press(Key::ArrowLeft);
    h.run_steps(1);
    assert_eq!(h.state().cursor, Some(4));
}

#[test]
fn a_multiline_note_in_arabic_wraps_from_the_right() {
    let mut h = field_harness(true);
    let note = "هذه ملاحظة طويلة مكتوبة باللغة العربية لتجربة التفاف الأسطر داخل الحقل";
    h.event(egui::Event::Text(note.into()));
    h.run_steps(2);
    let f = h.state();
    assert_eq!(f.text, note);
    let g = f.galley.clone().expect("laid out");
    assert!(g.rows.len() > 1, "wraps");
    let rights: Vec<f32> = g.rows.iter().map(|r| r.pos.x + r.row.size.x).collect();
    assert!(rights.windows(2).all(|w| (w[0] - w[1]).abs() < 1.0), "every row ends at the right edge: {rights:?}");
    let first_row: String = g.rows[0].row.text();
    assert!(note.starts_with(first_row.trim_end()), "the first words are on the first row: {first_row:?}");
}

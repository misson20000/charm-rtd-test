const DIGIT_STRINGS: [&str; 16] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f"];

use crate::model::listing::token;
use crate::view::config;
use crate::view::helpers;
use crate::view::listing::facet::cursor::CursorView;

use gtk::gdk;
use gtk::graphene;
use gtk::gsk;
use gtk::pango;
use gtk::prelude::*;

#[derive(Clone, Copy, Debug)]
pub enum Entry {
    Punctuation(token::PunctuationClass),
    Digit(u8),
    PrintableAscii(u8),
    Dot,
    Colon,
    Space,
}

pub struct Cache {
    font: pango::Font,
    
    gs_space: pango::GlyphString, // " "
    gs_comma: pango::GlyphString, // ", "
    gs_open: pango::GlyphString, // "{"
    gs_close: pango::GlyphString, // "}"
    gs_digit: [pango::GlyphString; 16], // "0", "1", ..., "f"
    gs_ascii: [pango::GlyphString; 0x7f-0x20], // ' ', '!', '"', ..., 'y', 'z', '{', '|', '}', '~'
    gs_dot: pango::GlyphString, // "."
    gs_colon: pango::GlyphString, // ": "

    space_width: i32,
}

impl Cache {
    pub fn new(pg: &pango::Context, font: &pango::Font) -> Cache {
        pg.set_font_description(Some(&font.describe()));

        let gs_space = Self::shape(pg, " ");
        let space_width = gs_space.width();
        
        Cache {
            font: font.clone(),
            
            gs_space,
            gs_comma: Self::shape(pg, ", "),
            gs_open: Self::shape(pg, "{"),
            gs_close: Self::shape(pg, "}"),
            gs_digit: DIGIT_STRINGS.map(|d| Self::shape(pg, d)),
            gs_ascii: std::array::from_fn(|i| Self::shape(pg, std::str::from_utf8(&[0x20 + i as u8]).unwrap())),
            gs_dot: Self::shape(pg, "."),
            gs_colon: Self::shape(pg, ": "),

            space_width,
        }
    }

    pub fn space_width(&self) -> i32 {
        self.space_width
    }
    
    fn shape(pg: &pango::Context, text: &str) -> pango::GlyphString {
        let items = pango::itemize(pg, text, 0, text.len() as i32, &pango::AttrList::new(), None);
        if items.len() != 1 {
            panic!("itemized '{}' into multiple items?", text);
        }

        let mut gs = pango::GlyphString::new();
        pango::shape(text, items[0].analysis(), &mut gs);

        gs
    }

    pub fn get(&self, entry: Entry) -> Option<&pango::GlyphString> {
        match entry {
            Entry::Punctuation(punct) => match punct {
                token::PunctuationClass::Empty => None,
                token::PunctuationClass::Space => Some(&self.gs_space),
                token::PunctuationClass::Comma => Some(&self.gs_comma),
                token::PunctuationClass::OpenBracket => Some(&self.gs_open),
                token::PunctuationClass::CloseBracket => Some(&self.gs_close),
            },
            Entry::Digit(digit) => self.gs_digit.get(digit as usize),
            Entry::PrintableAscii(ord) if (0x20..0x7f).contains(&ord) => Some(&self.gs_ascii[ord as usize - 0x20]),
            Entry::PrintableAscii(_) => Some(&self.gs_dot),
            Entry::Dot => Some(&self.gs_dot),
            Entry::Colon => Some(&self.gs_colon),
            Entry::Space => Some(&self.gs_space),
        }
    }

    pub fn print(&self, snapshot: &gtk::Snapshot, entry: Entry, color: &gdk::RGBA, pos: &mut graphene::Point) {
        if let Some(gs) = self.get(entry) {
            if let Some(tn) = gsk::TextNode::new(
                &self.font,
                gs,
                color,
                pos) {
                snapshot.append_node(tn);
            }

            let advance = helpers::pango_unscale(gs.width());
            pos.set_x(pos.x() + advance);
        }
    }

    pub fn print_with_cursor(&self, snapshot: &gtk::Snapshot, entry: Entry, config: &config::Config, cursor: &CursorView, pos: &mut graphene::Point) {
        if let Some(gs) = self.get(entry) {
            let color = if cursor.has_focus && cursor.get_blink() {                
                let (_ink, logical) = gs.clone().extents(&self.font);
                snapshot.append_color(&config.cursor_bg_color, &graphene::Rect::new(
                    pos.x() + helpers::pango_unscale(logical.x()) + cursor.get_bonk(),
                    pos.y() + helpers::pango_unscale(logical.y()),
                    helpers::pango_unscale(logical.width()),
                    helpers::pango_unscale(logical.height())));
                
                &config.cursor_fg_color
            } else if !cursor.has_focus {
                let (_ink, logical) = gs.clone().extents(&self.font);
                snapshot.append_border(
                    &gsk::RoundedRect::new(
                        graphene::Rect::new(
                            pos.x() + helpers::pango_unscale(logical.x()) + cursor.get_bonk(),
                            pos.y() + helpers::pango_unscale(logical.y()),
                            helpers::pango_unscale(logical.width()),
                            helpers::pango_unscale(logical.height())),
                        graphene::Size::zero(),
                        graphene::Size::zero(),
                        graphene::Size::zero(),
                        graphene::Size::zero()),
                    &[1.0; 4],
                    &[config.cursor_bg_color; 4],
                );

                &config.text_color
            } else {
                &config.text_color
            };
            
            if let Some(tn) = gsk::TextNode::new(
                &self.font,
                gs,
                color,
                pos) {
                snapshot.append_node(tn);
            }

            let advance = helpers::pango_unscale(gs.width());
            pos.set_x(pos.x() + advance);
        }
    }
}

pub fn render_text(snapshot: &gtk::Snapshot, pg: &pango::Context, font: &pango::Font, color: &gdk::RGBA, text: &str, pos: &mut graphene::Point) {
    let items = pango::itemize(pg, text, 0, text.len() as i32, &pango::AttrList::new(), None);

    for item in items {
        let mut gs = pango::GlyphString::new();
        pango::shape(text, item.analysis(), &mut gs);
        snapshot.append_node(
            gsk::TextNode::new(
                font,
                &gs,
                color,
                pos)
                .unwrap());

        let advance = helpers::pango_unscale(gs.width());
        pos.set_x(pos.x() + advance);
    }
}

pub fn render_text_with_cursor(snapshot: &gtk::Snapshot, pg: &pango::Context, font: &pango::Font, config: &config::Config, cursor: &CursorView, text: &str, pos: &mut graphene::Point) {
    let items = pango::itemize(pg, text, 0, text.len() as i32, &pango::AttrList::new(), None);

    for item in items {
        let mut gs = pango::GlyphString::new();
        pango::shape(text, item.analysis(), &mut gs);

        let color = if cursor.has_focus && cursor.get_blink() {
            let (_ink, logical) = gs.clone().extents(font);
            snapshot.append_color(&config.cursor_bg_color, &graphene::Rect::new(
                pos.x() + helpers::pango_unscale(logical.x()) + cursor.get_bonk(),
                pos.y() + helpers::pango_unscale(logical.y()),
                helpers::pango_unscale(logical.width()),
                helpers::pango_unscale(logical.height())));

            &config.cursor_fg_color
        } else if !cursor.has_focus {
            let (_ink, logical) = gs.clone().extents(font);
            snapshot.append_border(
                &gsk::RoundedRect::new(
                    graphene::Rect::new(
                        pos.x() + helpers::pango_unscale(logical.x()) + cursor.get_bonk(),
                        pos.y() + helpers::pango_unscale(logical.y()),
                        helpers::pango_unscale(logical.width()),
                        helpers::pango_unscale(logical.height())),
                    graphene::Size::zero(),
                    graphene::Size::zero(),
                    graphene::Size::zero(),
                    graphene::Size::zero()),
                &[1.0; 4],
                &[config.cursor_bg_color; 4],
            );

            &config.text_color
        } else {
            &config.text_color
        };
        
        snapshot.append_node(
            gsk::TextNode::new(
                font,
                &gs,
                color,
                pos)
                .unwrap());

        let advance = helpers::pango_unscale(gs.width());
        pos.set_x(pos.x() + advance);
    }
}

pub fn render_text_align_right(snapshot: &gtk::Snapshot, pg: &pango::Context, font: &pango::Font, color: &gdk::RGBA, text: &str, pos: &mut graphene::Point) {
    let items = pango::itemize(pg, text, 0, text.len() as i32, &pango::AttrList::new(), None);

    for item in items {
        let mut gs = pango::GlyphString::new();
        pango::shape(text, item.analysis(), &mut gs);

        let advance = helpers::pango_unscale(gs.width());
        pos.set_x(pos.x() - advance);
        
        snapshot.append_node(
            gsk::TextNode::new(
                font,
                &gs,
                color,
                pos)
                .unwrap());
    }
}

use std::fmt;
use std::vec;

use charm::logic::tokenizer;
use charm::model::listing::layout;
use charm::model::listing::token;

pub struct TokenExampleFormat<'a>(&'a token::Token);

impl<'a> fmt::Display for TokenExampleFormat<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.class {
            token::TokenClass::Punctuation(punct) => write!(f, "{}", match punct {
                token::PunctuationClass::Empty => "",
                token::PunctuationClass::Space => " ",
                token::PunctuationClass::Comma => ", ",
                token::PunctuationClass::OpenBracket => "{",
                token::PunctuationClass::CloseBracket => "}",
            }),
            token::TokenClass::Title => write!(f, "{}: ", &self.0.node.name),
            token::TokenClass::SummaryLabel => write!(f, "{}: ", &self.0.node.name),
            token::TokenClass::Hexdump(extent) => {
                for i in 0..extent.length().bytes {
                    write!(f, "{:02x}", (extent.begin.byte + i) & 0xff)?;
                    if i + 1 < extent.length().bytes {
                        write!(f, " ")?;
                    }
                }
                Ok(())
            },
            token::TokenClass::Hexstring(extent) => {
                for i in 0..extent.length().bytes {
                    write!(f, "{:02x}", (extent.begin.byte + i) & 0xff)?
                }
                Ok(())
            }
        }
    }
}

struct Line {
    indent: usize,
    tokens: vec::Vec<token::Token>
}

impl layout::Line for Line {
    type TokenIterator = vec::IntoIter<token::Token>;
    
    fn from_tokens(tokens: vec::Vec<token::Token>) -> Self {
        Line {
            indent: tokens[0].depth,
            tokens: tokens.into(),
        }
    }

    fn to_tokens(self) -> Self::TokenIterator {
        self.tokens.into_iter()
    }
}

fn main() {
    let mut args = std::env::args_os();
    args.next().expect("expected argv[0]");
    
    let xml_path = args.next().expect("expected path to xml");
    
    let xml = std::fs::read_to_string(xml_path.clone()).unwrap_or_else(|e| panic!("could not read file {:?}: {}", xml_path, e));
    let document = match roxmltree::Document::parse(&xml) {
        Ok(document) => document,
        Err(e) => panic!("{}", e)
    };
    let tc = tokenizer::xml::Testcase::from_xml(&document);
    let mut window = layout::Window::<Line>::new(&tc.structure);

    window.resize(150);

    for line in window.lines {
        for _ in 0..line.indent {
            print!("  ");
        }
        for token in line.tokens {
            print!("{}", TokenExampleFormat(&token));
        }
        println!();
    }
}
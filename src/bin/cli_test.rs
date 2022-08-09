#![allow(dead_code)]

use std::sync;
use std::path;

use charm::model::document;
use charm::model::listing;
use charm::model::space;

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    
    let space = sync::Arc::new(
        space::file::FileAddressSpace::open(
            rt.handle().clone(),
            &path::PathBuf::from("sample/cat.elf"),
            "cat.elf")
            .unwrap());

    let mut document = document::Document::new(space);
    //let mut gen = listing::window::TokenGenerator::new_from_root(&document.root);
}
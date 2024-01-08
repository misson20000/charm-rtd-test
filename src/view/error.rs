use std::fmt;
use std::fmt::Write;
use std::sync;

use crate::model;
use crate::model::document;
use crate::model::selection::listing;
use crate::model::selection::tree;
use crate::view::window;

use gtk::prelude::*;

pub enum Action {
    /* Internal actions */
    TreeSelectionDocumentUpdate,
    ListingSelectionDocumentUpdate,

    /* User-facing actions */
    DeleteNode,
    DestructureNode,
    InsertNodeParseOffset,
    InsertNodeParseSize,
    InsertNode,
    Nest,

    ModifyTreeSelection,
    RubberBandSelection,
}

pub enum Trouble {
    None,
    DocumentUpdateFailure {
        error: document::change::ApplyError,
        attempted_version: sync::Arc<document::Document>,
    },
    ListingSelectionUpdateFailure {        
        error: listing::ApplyError,
        attempted_version: sync::Arc<listing::Selection>,
    },
    TreeSelectionUpdateFailure {
        error: tree::ApplyError,
        attempted_version: sync::Arc<tree::Selection>,
    },
    AddressParseFailed {
        error: model::addr::AddressParseError,
        address: String,
    },
}

pub enum Level {
    Informational,
    Warning,
    Error
}

pub struct Error {
    pub while_attempting: Action,
    pub trouble: Trouble,
    pub level: Level,
    pub is_bug: bool,
}

impl Error {
    fn message(&self) -> String {
        match self.while_attempting {
            Action::TreeSelectionDocumentUpdate => "Failed to update tree selection in response to a document update.",
            Action::ListingSelectionDocumentUpdate => "Failed to update listing selection in response to a document update.",

            Action::DeleteNode => "Failed to delete node.",
            Action::DestructureNode => "Failed to destructure node.",
            Action::InsertNodeParseOffset => "Failed to parse offset.",
            Action::InsertNodeParseSize => "Failed to parse size.",
            Action::InsertNode => "Failed to insert node.",
            Action::Nest => "Failed to nest nodes.",

            Action::ModifyTreeSelection => "Failed to modify tree selection.",
            Action::RubberBandSelection => "Failed to rubber-band select.",
        }.to_string()
    }

    fn detail(&self) -> String {
        let mut msg = String::new();
        if let Err(_) = self.write_detail(&mut msg) {
            msg+= "Failed to format details.\n";
        }
        msg
    }
    
    fn write_detail(&self, msg: &mut String) -> Result<(), fmt::Error> {
        match &self.trouble {
            Trouble::None => write!(msg, "No further details.")?,

            Trouble::DocumentUpdateFailure { error, attempted_version: document } => {
                write!(msg, "Failed to apply change to document.\n")?;
                
                match &error.ty {
                    document::change::ApplyErrorType::UpdateFailed { error: update_error, incompatible_change } => {
                        write!(msg, "Change was originated against an older version of the document and conflicts with a newer change.\n")?;
                        match update_error {
                            document::change::UpdateError::NoCommonAncestor => write!(msg, "Couldn't find common parent document.")?,
                            document::change::UpdateError::NotUpdatable => write!(msg, "This type of change must always be applied to the latest version of the document.")?,
                            document::change::UpdateError::NotYetImplemented => write!(msg, "This type of change can't automatically update itself to newer versions of the document yet because it hasn't been implemented.")?,
                            document::change::UpdateError::NodeDeleted => write!(msg, "A node referenced by this change was been deleted.")?,
                            document::change::UpdateError::RangeSplit => write!(msg, "The range of nodes this change was meant to affect got split up.")?,
                        };
                        write!(msg, "\n")?;
                        if let Some(incompatible_change) = &incompatible_change {
                            write!(msg, "Incompatible change: ")?;
                            write_document_change_detail(msg, &document, incompatible_change)?;
                        } else {
                            write!(msg, "No information recorded about the incompatible newer change.\n")?;
                        }
                    },
                    document::change::ApplyErrorType::InvalidRange(reason) => write!(msg, "Range was invalid: {}.\n", match reason {
                        document::structure::RangeInvalidity::IndexExceedsNumberOfChildren => "the start or end index exceeded the number of children in the node",
                        document::structure::RangeInvalidity::Inverted => "the end index was before the start index",
                    })?,
                    document::change::ApplyErrorType::InvalidParameters(message) => write!(msg, "Parameters were invalid: {}\n", message)?,
                };
                
                write!(msg, "\n")?;
                write!(msg, "Attempted change: ")?;
                write_document_change_detail(msg, &document, &error.change)?;
            },

            Trouble::ListingSelectionUpdateFailure { error, attempted_version: _ } => {
                match error {
                    listing::ApplyError::WrongMode => write!(msg, "Failed to make the requested change to the listing panel's selection because the selection was in the wrong mode.\n")?,
                }
            },

            Trouble::TreeSelectionUpdateFailure { error, attempted_version: _ } => {
                match error {
                    tree::ApplyError::NodeDeleted => write!(msg, "Failed to make the requested change to the tree panel's selection because a requested node was deleted.\n")?,
                }
            },

            Trouble::AddressParseFailed { error, address } => {
                write!(msg, "Failed to parse '{}' as an address because ", address)?;
                match error {
                    model::addr::AddressParseError::MissingBytes => write!(msg, "it was missing the bytes value.\n")?,
                    model::addr::AddressParseError::MalformedBytes(e) => write!(msg, "the bytes section was malformed ({}).\n", e)?,
                    model::addr::AddressParseError::MalformedBits(e) => write!(msg, "the bytes section was malformed ({}).\n", e)?,
                    model::addr::AddressParseError::TooManyBits => write!(msg, "a bit was specified outside of 0-7.\n")?,
                }
            },
        };

        Ok(())
    }

    pub fn create_dialog(&self, parent: &window::CharmWindow) -> gtk::ApplicationWindow {
        let builder = gtk::Builder::from_string(include_str!("error-dialog.ui"));

        let message_label: gtk::Label = builder.object("message").unwrap();
        let bug_label: gtk::Label = builder.object("bug_label").unwrap();
        let detail_buffer: gtk::TextBuffer = builder.object("detail_buffer").unwrap();
        let ok_button: gtk::Button = builder.object("ok_button").unwrap();
        
        let dialog = gtk::ApplicationWindow::builder()
            .application(&parent.application.application)
            .child(&builder.object::<gtk::Widget>("toplevel").unwrap())
            .resizable(true)
            .title("Charm Error")
            .transient_for(&parent.window)
            .destroy_with_parent(true)
            .default_widget(&ok_button)
            .build();

        message_label.set_text(&self.message());
        detail_buffer.set_text(&self.detail());
        bug_label.set_visible(self.is_bug);

        dialog
    }
}

struct SafePathDescription<'a> {
    document: &'a document::Document,
    path: document::structure::PathSlice<'a>,
}

impl<'a> SafePathDescription<'a> {
    fn new(document: &'a document::Document, path: document::structure::PathSlice<'a>) -> Self {
        Self {
            document, path
        }
    }
}

impl<'a> std::fmt::Display for SafePathDescription<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let mut node = Some(&self.document.root);
        let mut path = self.path;
        write!(f, "{}", self.document.root.props.name)?;

        while path.len() > 0 {
            write!(f, ".")?;

            node = node.and_then(|node| node.children.get(path[0])).map(|childhood| &childhood.node);

            match node {
                Some(node) => write!(f, "{}", node.props.name)?,
                None => write!(f, "<missing {}>", path[0])?,
            }

            path = &path[1..];
        }

        Ok(())
    }
}

fn write_document_change_detail(msg: &mut String, document: &document::Document, change: &document::change::Change) -> Result<(), fmt::Error> {
    match &change.ty {
        document::change::ChangeType::AlterNode { path, props } => {
            write!(msg, "Alter node at {}\n", SafePathDescription::new(document, &path))?;
            write!(msg, "New properties: {:?}\n", props)?;
        },
        document::change::ChangeType::InsertNode { parent, index, child } => {
            write!(msg, "Insert node under {}\n", SafePathDescription::new(document, &parent))?;
            write!(msg, "Index: {}\n", index)?;
            write!(msg, "Offset: {}\n", child.offset)?;
            write!(msg, "Properties: {:?}\n", child.node.props)?;
        },
        document::change::ChangeType::Nest { range, extent, props } => {
            write!(msg, "Nest children under {}\n", SafePathDescription::new(document, &range.parent))?;
            write!(msg, "Indices: {}-{} (inclusive)\n", range.first, range.last)?;
            write!(msg, "Extent: {:?}\n", extent)?;
            write!(msg, "Properties: {:?}\n", props)?;
        },
        document::change::ChangeType::Destructure { parent, child_index, num_grandchildren, offset } => {
            write!(msg, "Destructuring child under {}\n", SafePathDescription::new(document, &parent))?;
            write!(msg, "Child index: {}\n", child_index)?;
            write!(msg, "Num grandchildren: {}\n", num_grandchildren)?;
            write!(msg, "Offset: {}\n", offset)?;
        },
        document::change::ChangeType::DeleteRange { range } => {
            write!(msg, "Delete children under {}\n", SafePathDescription::new(document, &range.parent))?;
            write!(msg, "Indices: {}-{} (inclusive)\n", range.first, range.last)?;
        },
    };

    Ok(())
}

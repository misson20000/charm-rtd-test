use std::sync;
use std::vec;

use lazy_static::lazy_static;

use crate::model::addr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleDisplay {
    Inline,
    Major,
    Minor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildrenDisplay {
    None, //< fully collapsed
    Summary,
    //Recolor,
    Full
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentDisplay {
    None,
    Hexdump(addr::Size),
    Hexstring
}

pub type Path = vec::Vec<usize>;
pub type PathSlice<'a> = &'a [usize];
pub type PathIter<'a> = std::vec::IntoIter<usize>;

#[derive(Debug, Clone)]
pub struct Childhood {
    pub node: sync::Arc<Node>,
    pub offset: addr::Address,
}

impl Childhood {
    pub fn new(node: sync::Arc<Node>, offset: addr::Address) -> Childhood {
        Childhood { node, offset }
    }
    
    pub fn end(&self) -> addr::Address {
        self.offset + self.node.size
    }

    pub fn extent(&self) -> addr::Extent {
        addr::Extent::sized(self.offset, self.node.size)
    }
}

/// These are separated from the rest of the Node because they can be changed
/// without rearranging nodes, which makes it easier to port paths across
/// Changes if we can use a different ChangeType when only properties are
/// affected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Properties {
    pub name: String,
    pub title_display: TitleDisplay,
    pub children_display: ChildrenDisplay,
    pub content_display: ContentDisplay,
    pub locked: bool,    
}

#[derive(Debug, Clone)]
pub struct Node {
    /* reference to parent causes a lot of problems, so we don't have one and
       opt to refer to nodes by path when necessary. */
    pub props: Properties,
    pub size: addr::Size,
    pub children: vec::Vec<Childhood>
}

impl TitleDisplay {
    pub fn has_blanks(&self) -> bool {
        match self {
            TitleDisplay::Inline => false,
            TitleDisplay::Major => true,
            TitleDisplay::Minor => false,
        }
    }

    pub fn is_inline(&self) -> bool {
        match self {
            TitleDisplay::Inline => true,
            TitleDisplay::Major => false,
            TitleDisplay::Minor => false,
        }
    }
}

impl Default for TitleDisplay {
    fn default() -> TitleDisplay {
        TitleDisplay::Major
    }
}

impl Default for ChildrenDisplay {
    fn default() -> ChildrenDisplay {
        ChildrenDisplay::Full
    }
}

impl ContentDisplay {
    pub fn preferred_pitch(&self) -> Option<addr::Size> {
        match self {
            ContentDisplay::None => None,
            ContentDisplay::Hexdump(pitch) => Some(*pitch),
            ContentDisplay::Hexstring => None,
        }
    }
}

impl Default for ContentDisplay {
    fn default() -> ContentDisplay {
        ContentDisplay::Hexdump(addr::Size::from(16))
    }
}

impl Default for Properties {
    fn default() -> Properties {
        Properties {
            name: "default".to_string(),
            title_display: TitleDisplay::default(),
            children_display: ChildrenDisplay::default(),
            content_display: ContentDisplay::default(),
            locked: true,
        }
    }
}

impl Default for Node {
    fn default() -> Node {
        Node {
            props: Properties::default(),
            size: addr::unit::MAX,
            children: vec::Vec::new(),
        }
    }
}


impl Default for Childhood {
    fn default() -> Self {
        lazy_static! {
            static ref DEFAULT_NODE: sync::Arc<Node> = sync::Arc::new(Node::default());
        }
        
        Childhood {
            node: DEFAULT_NODE.clone(),
            offset: addr::unit::NULL,
        }
    }
}

impl Node {
    pub fn default_sized(size: addr::Size) -> Node {
        Node {
            props: Properties::default(),
            size,
            children: vec::Vec::new(),
        }
    }

    pub fn builder() -> builder::StructureBuilder {
        builder::StructureBuilder::default()
    }
}

/* This is mostly useful for testcases */
pub mod builder {
    use super::*;

    #[derive(Default)]
    pub struct StructureBuilder {
        node: Node
    }

    impl StructureBuilder {
        pub fn props(mut self, props: Properties) -> Self {
            self.node.props = props;
            self
        }

        pub fn name<S: ToString>(mut self, name: S) -> Self {
            self.node.props.name = name.to_string();
            self
        }

        pub fn title_display(mut self, value: TitleDisplay) -> Self {
            self.node.props.title_display = value;
            self
        }

        pub fn children_display(mut self, value: ChildrenDisplay) -> Self {
            self.node.props.children_display = value;
            self
        }

        pub fn content_display(mut self, value: ContentDisplay) -> Self {
            self.node.props.content_display = value;
            self
        }

        pub fn lock(mut self) -> Self {
            self.node.props.locked = true;
            self
        }

        pub fn unlock(mut self) -> Self {
            self.node.props.locked = false;
            self
        }

        pub fn size<S: Into<addr::Size>>(mut self, size: S) -> Self {
            self.node.size = size.into();
            self
        }

        pub fn child<A: Into<addr::Address>, F: FnOnce(StructureBuilder) -> StructureBuilder>(mut self, offset: A, builder: F) -> Self {
            self.node.children.push(Childhood {
                offset: offset.into(),
                node: builder(Self::default()).build()
            });
            self
        }

        pub fn build(&self) -> sync::Arc<Node> {
            sync::Arc::new(self.node.clone())
        }

        pub fn build_child(&self, offset: addr::Address) -> Childhood {
            Childhood::new(sync::Arc::new(self.node.clone()), offset)
        }
    }
}

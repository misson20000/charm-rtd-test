use std::cell;
use std::sync;
use std::vec;

use gtk::gio;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::model::addr;
use crate::model::document;
use crate::model::document::change;
use crate::model::document::structure;
use crate::view::helpers;

fn create_tree_list_model(document_host: sync::Arc<document::DocumentHost>, document: sync::Arc<document::Document>, autoexpand: bool) -> gtk::TreeListModel {
    let root_model = gio::ListStore::new(NodeItem::static_type());

    let root_item = NodeItem::new(NodeInfo {
        path: vec![],
        node: document.root.clone(),
        props: document.root.props.clone(),
        offset: addr::unit::NULL,
        address: addr::unit::NULL,
        document_host: document_host.clone(),
        document: document.clone(),
    });
    
    root_model.append(&root_item);
    
    let model = gtk::TreeListModel::new(&root_model, false, autoexpand, |obj| {
        Some(StructureListModel::from_node_info(
            &obj.downcast_ref::<NodeItem>().unwrap().imp().info.get().unwrap().borrow()
        ).upcast())
    });

    let subscriber = helpers::subscribe_to_document_updates(root_item.downgrade(), document_host.clone(), document, move |root_item, new_document| {
        root_item.stage(NodeInfo {
            path: vec![],
            node: new_document.root.clone(),
            props: new_document.root.props.clone(),
            offset: addr::unit::NULL,
            address: addr::unit::NULL,
            document: new_document.clone(),
            document_host: document_host.clone(),
        });
        root_item.update();
    });

    /* The root item lasts forever. */
    /* TODO: no it doesn't; you can close and reopen the window. */
    std::mem::forget(subscriber);
    
    model
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub node: sync::Arc<structure::Node>,
    
    /* when we update a gobject property, it needs to be reflected immediately,
     * before document update happens. */
    pub props: structure::Properties,
    
    pub path: structure::Path,
    pub offset: addr::Address,
    pub address: addr::Address,
    pub document_host: sync::Arc<document::DocumentHost>,
    pub document: sync::Arc<document::Document>,
}

#[derive(Debug, Clone)]
pub enum SelectionMode {
    Empty,
    Single(structure::Path),
    SiblingRange(structure::Path, usize, usize),
    All,
}

impl SelectionMode {
    fn new_range_between(begin: &Vec<usize>, end: &Vec<usize>) -> SelectionMode {
        if begin.is_empty() || end.is_empty() {
            return SelectionMode::All;
        }
        
        let shared_prefix: Vec<usize> = (&begin[0..begin.len()-1])
            .iter()
            .zip((&end[0..end.len()-1]).iter())
            .map_while(|(x, y)| if x == y { Some(*x) } else { None })
            .collect();
        
        let sp_len = shared_prefix.len();
        
        SelectionMode::SiblingRange(shared_prefix, begin[sp_len], end[sp_len])
    }
}

mod imp {
    use std::cell;
    use std::sync;
    use std::vec;
    
    use gtk::gio;
    use gtk::glib;
    use gtk::subclass::prelude::*;
    use gtk::prelude::*;
    
    use crate::model::addr;
    use crate::model::document;
    use crate::model::document::change;
    use crate::model::document::structure;
    use crate::view::helpers;

    use super::NodeInfo;
    use super::SelectionMode;

    #[derive(Debug)]
    pub struct StructureListModelInterior {
        pub path: structure::Path,
        pub children: vec::Vec<super::NodeItem>,
        pub address: addr::Address,
        pub document_host: sync::Arc<document::DocumentHost>,
        pub document: sync::Arc<document::Document>,
        pub subscriber: helpers::AsyncSubscriber,
    }

    #[derive(Default)]
    pub struct StructureListModel {
        pub interior: once_cell::unsync::OnceCell<cell::RefCell<StructureListModelInterior>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StructureListModel {
        const NAME: &'static str = "CharmStructureListModel";
        type Type = super::StructureListModel;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for StructureListModel {
    }

    impl ListModelImpl for StructureListModel {
        fn item_type(&self) -> glib::Type {
            super::NodeItem::static_type()
        }

        fn n_items(&self) -> u32 {
            self.interior.get().map_or(0, |i| i.borrow().children.len() as u32)
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.interior.get().and_then(|i| {
                let i = i.borrow();
                i.children.get(position as usize).map(|ch| ch.clone().upcast())
            })
        }
    }
    
    #[derive(Default)]
    pub struct NodeItem {
        pub info: once_cell::unsync::OnceCell<cell::RefCell<NodeInfo>>,
        pub staged_info: cell::RefCell<Option<NodeInfo>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NodeItem {
        const NAME: &'static str = "CharmNodeItem";
        type Type = super::NodeItem;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for NodeItem {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: once_cell::sync::Lazy<Vec<glib::ParamSpec>> =
                once_cell::sync::Lazy::new(|| vec![
                    glib::ParamSpecString::builder("name").build(),
                    glib::ParamSpecString::builder("addr").build(),
                    glib::ParamSpecString::builder("size").build(),
                    glib::ParamSpecString::builder("children-display").build(),
                ]);
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            /* we update our local copy of properties immediately. when the
               document update goes through, it will notify us, but we'll see
               that the new properties already match our local properties and we
               won't notify the properties as changed again. */

            println!("property {} is being set", pspec.name());
            
            let mut info = self.info.get().unwrap().borrow_mut();
            let old_info = info.clone();
            
            match pspec.name() {
                "name" => info.props.name = value.get().unwrap(),
                "children-display" => info.props.children_display = match value.get().unwrap() {
                    "n" => structure::ChildrenDisplay::None,
                    "s" => structure::ChildrenDisplay::Summary,
                    "f" => structure::ChildrenDisplay::Full,
                    _ => structure::ChildrenDisplay::Full,
                },
                _ => unimplemented!(),
            };

            if let Err(e) = info.document_host.alter_node(&info.document, info.path.clone(), info.props.clone()) {
                /* roll back */
                println!("failed to alter node: {:?}", e);
                std::mem::drop(info);
                self.obj().stage(old_info);
                self.obj().update();
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let info = self.info.get().unwrap().borrow();
            match pspec.name() {
                "name" => glib::ToValue::to_value(&info.props.name),
                "addr" => glib::ToValue::to_value(&format!("{}", info.address)),
                "size" => glib::ToValue::to_value(&format!("{}", info.node.size)),
                "children-display" => glib::ToValue::to_value(match info.props.children_display {
                    structure::ChildrenDisplay::None => "n",
                    structure::ChildrenDisplay::Summary => "s",
                    structure::ChildrenDisplay::Full => "f",
                }),
                _ => unimplemented!(),
            }
        }
    }
    
    pub struct StructureSelectionModelInterior {
        pub tree_model: gtk::TreeListModel,
        pub mode: SelectionMode,
        pub document: sync::Arc<document::Document>,
        pub document_host: sync::Arc<document::DocumentHost>,
        pub subscriber: helpers::AsyncSubscriber,
    }

    impl StructureSelectionModelInterior {
        fn item(&self, position: u32) -> Option<super::NodeItem> {
            self.tree_model.item(position).map(|item| item.downcast::<gtk::TreeListRow>().unwrap().item().unwrap().downcast().unwrap())
        }
    }
    
    #[derive(Default)]
    pub struct StructureSelectionModel {
        pub interior: cell::RefCell<Option<StructureSelectionModelInterior>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StructureSelectionModel {
        const NAME: &'static str = "CharmStructureSelectionModel";
        type Type = super::StructureSelectionModel;
        type Interfaces = (gio::ListModel, gtk::SelectionModel,);
    }

    impl ObjectImpl for StructureSelectionModel {
    }

    impl StructureSelectionModel {
        pub fn borrow_interior_mut(&self) -> Option<std::cell::RefMut<'_, StructureSelectionModelInterior>> {
            std::cell::RefMut::filter_map(self.interior.borrow_mut(), Option::as_mut).ok()
        }

        pub fn borrow_interior(&self) -> Option<std::cell::Ref<'_, StructureSelectionModelInterior>> {
            std::cell::Ref::filter_map(self.interior.borrow(), Option::as_ref).ok()
        }
        
        fn notify_all_changed(&self, interior: std::cell::RefMut<'_, StructureSelectionModelInterior>) {
            let n_items = interior.tree_model.n_items();
            drop(interior);
            self.obj().selection_changed(0, n_items);
        }

        fn port_doc<'a>(interior: cell::RefMut<'a, StructureSelectionModelInterior>, old_doc: &sync::Arc<document::Document>, new_doc: &sync::Arc<document::Document>) -> cell::RefMut<'a, StructureSelectionModelInterior> {
            if old_doc.is_outdated(new_doc) {
                match &new_doc.previous {
                    Some((prev_doc, change)) => {
                        let interior = Self::port_doc(interior, old_doc, prev_doc);
                        Self::port_change(interior, new_doc, change)
                    },
                    None => panic!("no common ancestor")
                }
            } else {
                interior
            }
        }

        fn port_change<'a>(mut interior: cell::RefMut<'a, StructureSelectionModelInterior>, new_doc: &sync::Arc<document::Document>, change: &change::Change) -> cell::RefMut<'a, StructureSelectionModelInterior> {
            interior.document = new_doc.clone();
            
            interior.mode = match std::mem::replace(&mut interior.mode, SelectionMode::Empty) {
                SelectionMode::Empty => SelectionMode::Empty,
                SelectionMode::Single(mut path) => { change::update_path(&mut path, change).unwrap(); SelectionMode::Single(path) },
                SelectionMode::SiblingRange(mut path, first_child, last_child) => {
                    change::update_path(&mut path, change).unwrap();

                    match &change.ty {
                        change::ChangeType::AlterNode(_, _) => SelectionMode::SiblingRange(path, first_child, last_child),
                        change::ChangeType::InsertNode(affected_path, insertion_index, _, _) if affected_path == &path && (first_child..=last_child).contains(insertion_index) => SelectionMode::SiblingRange(path, first_child, last_child+1),
                        change::ChangeType::InsertNode(_, _, _, _) => SelectionMode::SiblingRange(path, first_child, last_child),
                        change::ChangeType::Nest(affected_path, _nested_first, _nested_last, _props) if affected_path == &path => {
                            path.push(first_child);
                            SelectionMode::Single(path)
                        },
                        change::ChangeType::Nest(_, _, _, _) => SelectionMode::SiblingRange(path, first_child, last_child)
                    }
                },
                SelectionMode::All => SelectionMode::All,
            };
            
            interior
        }
        
        pub fn update(&self, new_doc: &sync::Arc<document::Document>) {
            if let Some(i) = self.borrow_interior_mut() {
                let old_doc = i.document.clone();

                let i = Self::port_doc(i, &old_doc, new_doc);
                self.notify_all_changed(i);
            }
        }
    }
    
    impl SelectionModelImpl for StructureSelectionModel {
        fn selection_in_range(&self, position: u32, n_items: u32) -> gtk::Bitset {
            self.borrow_interior().map_or_else(|| gtk::Bitset::new_empty(), |i| {
                match &i.mode {
                    SelectionMode::Empty => gtk::Bitset::new_empty(),
                    SelectionMode::Single(path) => {
                        let matched_position = (position..position+n_items).into_iter().find_map(|position| match i.item(position) {
                            None => Some(None),
                            Some(item) => {
                                if &item.imp().info.get().unwrap().borrow().path == path {
                                    Some(Some(position))
                                } else {
                                    None
                                }
                            }
                        });

                        match matched_position {
                            Some(None) | None => gtk::Bitset::new_empty(),
                            Some(Some(pos)) => gtk::Bitset::new_range(pos-position, 1)
                        }
                    },
                    SelectionMode::SiblingRange(_, _, _) => todo!(),
                    SelectionMode::All => gtk::Bitset::new_range(0, n_items),
                }
            })
        }

        fn is_selected(&self, position: u32) -> bool {
            self.borrow_interior().map_or(false, |i| match &i.mode {
                SelectionMode::Empty => false,
                SelectionMode::Single(path) => i.item(position).map_or(false, |item| {
                    &item.imp().info.get().unwrap().borrow().path == path
                }),
                SelectionMode::SiblingRange(path, first, last) => i.item(position).map_or(false, |item| {
                    let query_path = &item.imp().info.get().unwrap().borrow().path;
                    query_path[0..std::cmp::min(path.len(), query_path.len())] == path[..] && query_path.get(path.len()).map_or(false, |sibling| sibling >= first && sibling <= last)
                }),
                SelectionMode::All => true,
            })
        }

        fn select_all(&self) -> bool {
            if let Some(mut interior) = self.borrow_interior_mut() {
                interior.mode = SelectionMode::All;
                self.notify_all_changed(interior);
                true
            } else {
                true
            }
        }

        fn select_item(&self, position: u32, unselect_rest: bool) -> bool {
            if let Some(mut interior) = self.borrow_interior_mut() {
                let item = match interior.item(position) {
                    Some(item) => item,
                    None if unselect_rest => {
                        interior.mode = SelectionMode::Empty;
                        self.notify_all_changed(interior);
                        return true
                    },
                    None => return true
                };

                match (&interior.mode, unselect_rest) {
                    (SelectionMode::Empty, _) | (_, true) => {
                        interior.mode = SelectionMode::Single(item.imp().info.get().unwrap().borrow().path.clone());
                        self.notify_all_changed(interior);
                    },
                    (SelectionMode::Single(current_path), _) => {
                        let item_path = &item.imp().info.get().unwrap().borrow().path;
                        
                        let begin = std::cmp::min(item_path, current_path);
                        let end = std::cmp::max(item_path, current_path);
                        interior.mode = SelectionMode::new_range_between(begin, end);
                        self.notify_all_changed(interior);
                    }
                    (SelectionMode::SiblingRange(_, _, _), _) => todo!(),
                    (SelectionMode::All, _) => {},
                }
                
                true
            } else {
                true
            }
        }

        fn select_range(&self, position: u32, n_items: u32, unselect_rest: bool) -> bool {            
            if let Some(mut interior) = self.borrow_interior_mut() {
                if n_items == 0 {
                    if unselect_rest {
                        interior.mode = SelectionMode::Empty;
                        self.notify_all_changed(interior);
                    }
                    return true;
                }

                if n_items == 1 {
                    drop(interior);
                    return self.select_item(position, unselect_rest);
                }

                let first_item = match interior.item(position) {
                    Some(item) => item,
                    None => return true
                };

                let last_item = match interior.item(position + n_items - 1) {
                    Some(item) => item,
                    None => return true
                };

                let first_info = first_item.imp().info.get().unwrap().borrow();
                let last_info = last_item.imp().info.get().unwrap().borrow();

                let mut begin_path = first_info.path.clone();
                let mut end_path = last_info.path.clone();
                
                if !unselect_rest {
                    match &interior.mode {
                        SelectionMode::Empty => {},
                        SelectionMode::Single(path) => {
                            begin_path = std::cmp::min(begin_path, path.clone());
                            end_path = std::cmp::max(end_path, path.clone());
                        },
                        SelectionMode::SiblingRange(prefix, begin_child, end_child) => {
                            let mut current_begin = prefix.clone();
                            let mut current_end = prefix.clone();
                            current_begin.push(*begin_child);
                            current_end.push(*end_child);
                            
                            begin_path = std::cmp::min(begin_path, current_begin);
                            end_path = std::cmp::max(end_path, current_end);
                        },
                        SelectionMode::All => return true,
                    }
                }
                
                interior.mode = SelectionMode::new_range_between(&begin_path, &end_path);
                self.notify_all_changed(interior);
                
                true
            } else {
                true
            }
        }

        fn set_selection(&self, _selected: &gtk::Bitset, _mask: &gtk::Bitset) -> bool {
            false
        }

        fn unselect_all(&self) -> bool {
            if let Some(mut interior) = self.borrow_interior_mut() {
                interior.mode = SelectionMode::Empty;
                self.notify_all_changed(interior);
                true
            } else {
                true
            }
        }

        fn unselect_item(&self, _position: u32) -> bool {
            false
        }

        fn unselect_range(&self, _position: u32, _n_items: u32) -> bool {
            false
        }
    }

    impl ListModelImpl for StructureSelectionModel {
        fn item_type(&self) -> glib::Type {
            gtk::TreeListRow::static_type()
        }

        fn n_items(&self) -> u32 {
            self.borrow_interior().map_or(0, |i| i.tree_model.n_items())
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.borrow_interior().and_then(|i| {
                i.tree_model.item(position)
            })
        }
    }
}

glib::wrapper! {
    pub struct StructureListModel(ObjectSubclass<imp::StructureListModel>)
        @implements gio::ListModel;
}

glib::wrapper! {
    pub struct NodeItem(ObjectSubclass<imp::NodeItem>)
        ;
}

glib::wrapper! {
    pub struct StructureSelectionModel(ObjectSubclass<imp::StructureSelectionModel>)
        @implements gio::ListModel, gtk::SelectionModel;
}

impl StructureListModel {
    fn from_node_info(info: &NodeInfo) -> Self {
        let model: Self = glib::Object::builder().build();

        let subscriber = helpers::subscribe_to_document_updates(model.downgrade(), info.document_host.clone(), info.document.clone(), move |model, new_document| {
            model.update(new_document);
        });

        model.imp().interior.set(cell::RefCell::new(imp::StructureListModelInterior {
            path: info.path.clone(),
            children: info.node.children[..].iter().enumerate().map(|tuple| {
                let (i, ch) = tuple;
                
                let mut path = info.path.clone();
                path.push(i);

                NodeItem::new(NodeInfo {
                    path,
                    node: ch.node.clone(),
                    props: ch.node.props.clone(),
                    offset: ch.offset,
                    address: info.address + ch.offset.to_size(),
                    document_host: info.document_host.clone(),
                    document: info.document.clone()
                })                
            }).collect(),
            address: info.address,
            document_host: info.document_host.clone(),
            document: info.document.clone(),
            subscriber,
        })).unwrap();
        
        model
    }

    fn port_doc(&self, old_doc: &sync::Arc<document::Document>, new_doc: &sync::Arc<document::Document>) {
        if old_doc.is_outdated(new_doc) {
            match &new_doc.previous {
                Some((prev_doc, change)) => {
                    self.port_doc(old_doc, prev_doc);
                    self.port_change(new_doc, change);
                },
                None => panic!("no common ancestor")
            }
        }
    }

    fn port_change(&self, new_doc: &sync::Arc<document::Document>, change: &change::Change) {
        let mut i = self.imp().interior.get().unwrap().borrow_mut();
        document::change::update_path(&mut i.path, change).unwrap();
        i.document = new_doc.clone();
        
        let (new_node, addr) = new_doc.lookup_node(&i.path);

        i.address = addr;
        
        let items_changed = match &change.ty {
            /* Was one of our children altered? */
            change::ChangeType::AlterNode(path, new_props) if i.path.len() + 1 == path.len() && path[0..i.path.len()] == i.path[..]=> {
                let index = path[i.path.len()];
                let child_item = i.children[index].clone();
                let childhood = &new_node.children[index];
                let document_host = i.document_host.clone();

                child_item.stage(NodeInfo {
                    path: path.clone(),
                    node: childhood.node.clone(),
                    props: new_props.clone(),
                    offset: childhood.offset,
                    address: addr + childhood.offset.to_size(),
                    document: new_doc.clone(),
                    document_host,
                });

                None
            },
            change::ChangeType::AlterNode(_, _) => None,

            /* Did we get a new child? */
            change::ChangeType::InsertNode(affected_path, affected_index, _new_node_offset, _new_node) if affected_path[..] == i.path[..] => {
                let childhood = &new_node.children[*affected_index];
                let document_host = i.document_host.clone();

                i.children.insert(*affected_index, NodeItem::new(NodeInfo {
                    path: vec![], /* will be fixed up later */
                    node: childhood.node.clone(),
                    props: childhood.node.props.clone(),
                    offset: childhood.offset,
                    address: addr + childhood.offset.to_size(),
                    document: new_doc.clone(),
                    document_host,
                }));

                Some((*affected_index as u32, 0, 1))
            },

            change::ChangeType::Nest(parent, first_child, last_child, _props) if parent[..] == i.path[..] => {
                let childhood = &new_node.children[*first_child];
                let document_host = i.document_host.clone();
                
                let count_removed = i.children.splice(first_child..=last_child, [NodeItem::new(NodeInfo {
                    path: vec![], /* will be fixed up later */
                    node: childhood.node.clone(),
                    props: childhood.node.props.clone(),
                    offset: childhood.offset,
                    address: addr + childhood.offset.to_size(),
                    document: new_doc.clone(),
                    document_host,
                })]).count();

                Some((*first_child as u32, count_removed as u32, 1))
            },
            change::ChangeType::Nest(_, _, _, _) => None,
            
            change::ChangeType::InsertNode(_, _, _, _) => None,
        };

        /* Fixup children's paths */
        {
            let i = &mut *i;
            for (index, child) in i.children.iter_mut().enumerate() {
                let mut child_info = child.imp().info.get().unwrap().borrow_mut();
                child_info.path.splice(.., i.path.iter().cloned());
                child_info.path.push(index);
            }
        }

        std::mem::drop(i);

        if let Some((index, added, removed)) = items_changed {
            self.items_changed(index, added, removed);
        }
    }
    
    fn update(&self, new_doc: &sync::Arc<document::Document>) {
        if let Some(i) = self.imp().interior.get().map(|i| i.borrow()) {
            let old_doc = i.document.clone();
            std::mem::drop(i);
            
            self.port_doc(&old_doc, new_doc);

            /* We need to avoid notifying gtk of its own property updates. Our system to do this works by filtering out
             * updates that match what GTK thinks the properties already are. Unfortunately, there are some cases where
             * GTK updates a property twice in quick succession, which breaks this filtering, so we need to coalesce
             * updates before deciding whether to filter them out or not. */
            
            for item in &self.imp().interior.get().unwrap().borrow().children {
                item.update();
            }
        }
    }
}

impl NodeItem {
    fn new(info: NodeInfo) -> Self {
        let item: Self = glib::Object::builder().build();
        item.imp().info.set(cell::RefCell::new(info)).unwrap();
        item
    }

    fn stage(&self, info: NodeInfo) {
        *self.imp().staged_info.borrow_mut() = Some(info);
    }

    fn update(&self) {
        if let Some(new_info) = self.imp().staged_info.borrow_mut().take() {
            let mut info = self.imp().info.get().unwrap().borrow_mut();

            println!("updating nodeinfo {} -> {}", info.props.name, new_info.props.name);
        
            let changed_name = info.props.name != new_info.props.name;
            let changed_offset = info.offset != new_info.offset;
            let changed_address = info.address != new_info.address;

            *info = new_info;

            /* as soon as we start notifying, callbacks are allowed to try to
             * retrieve the new properties, requiring a reference to our
             * interior. */
            std::mem::drop(info);

            if changed_name { self.notify("name"); }
            if changed_offset { self.notify("offset"); }
            if changed_address { self.notify("address"); }
        }
    }
}

impl StructureSelectionModel {
    pub fn new(document_host: sync::Arc<document::DocumentHost>) -> StructureSelectionModel {
        let document = document_host.get();
        
        let model: StructureSelectionModel = glib::Object::builder().build();

        let tree_model = create_tree_list_model(document_host.clone(), document.clone(), true);
        tree_model.connect_items_changed(clone!(@weak model => move |_, pos, removed, added| model.items_changed(pos, removed, added)));

        let subscriber = helpers::subscribe_to_document_updates(model.downgrade(), document_host.clone(), document.clone(), move |model, new_document| {
            model.imp().update(new_document);
        });
        
        *model.imp().interior.borrow_mut() = Some(imp::StructureSelectionModelInterior {
            tree_model,
            mode: SelectionMode::Empty,
            document,
            document_host,
            subscriber,
        });

        model
    }
    
    pub fn selection_mode(&self) -> (SelectionMode, sync::Arc<document::Document>) {
        let i = self.imp().borrow_interior().unwrap();
        (i.mode.clone(), i.document.clone())
    }
}

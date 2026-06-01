use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter, Write};
use std::rc::Rc;

#[derive(Clone, Hash, PartialEq, Eq)]
struct PathNode {
    parent: Path,
    index: String,
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct Path(Option<Rc<PathNode>>);

impl Display for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_root() {
            f.write_str("")
        } else {
            if !self.get_parent().unwrap().is_root() {
                Display::fmt(&self.get_parent().unwrap(), f)?;
                f.write_str("::")?;
            }
            f.write_str(&self.get_suffix().unwrap())
        }
    }
}

impl Debug for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Path::parse(\"")?;
        f.write_str(&self.to_string().escape_debug().to_string())?;
        f.write_str("\")")
    }
}

impl Path {
    pub const ROOT: Path = Path(None);

    pub fn is_root(&self) -> bool {
        self.0.is_none()
    }

    pub fn get_parent(&self) -> Option<Path> {
        if let Some(node) = &self.0 {
            Some(node.parent.clone())
        } else {
            None
        }
    }

    pub fn subpath(&self, index: String) -> Path {
        if index.len() == 0 {
            panic!("empty subpath")
        }

        if index.contains("::") {
            panic!("illegal :: in subpath")
        }

        Path(Some(Rc::new(PathNode {
            parent: self.clone(),
            index,
        })))
    }

    pub fn is_subnode_of(&self, other: &Path) -> bool {
        let mut node = self.clone();
        while !node.is_root() {
            if node == *other {
                return true;
            }

            node = node.get_parent().unwrap();
        }
        true
    }

    pub fn get_suffix(&self) -> Option<String> {
        if let Some(s) = &self.0 {
            Some(s.index.clone())
        } else {
            None
        }
    }

    pub fn parse(s: String) -> Self {
        if s.len() == 0 {
            return Self::ROOT;
        }

        let mut path = Self::ROOT;
        for seg in s.split("::") {
            path = path.subpath(seg.to_string());
        }
        path
    }
}

#[derive(Debug)]
pub struct NamespaceNode<T> {
    definition: Option<T>,
    sub_nodes: HashMap<String, Box<NamespaceNode<T>>>,
}

#[derive(Debug)]
pub struct Namespace<T> {
    root_node: NamespaceNode<T>
}

impl<T> Namespace<T> {
    pub fn new() -> Self {
        Self {
            root_node: NamespaceNode {
                definition: None,
                sub_nodes: HashMap::new()
            }
        }
    }

    fn get_or_make_node(&mut self, path: &Path) -> &mut NamespaceNode<T> {
        if path.is_root() {
            return &mut self.root_node;
        }

        let parent = path.get_parent().unwrap();
        let suffix = path.get_suffix().unwrap();

        let parent_node = self.get_or_make_node(&parent);

        if parent_node.sub_nodes.contains_key(&suffix) {
            parent_node.sub_nodes.get_mut(&suffix).unwrap()
        } else {
            parent_node.sub_nodes.insert(suffix.clone(), Box::new(NamespaceNode {
                definition: None,
                sub_nodes: HashMap::new(),
            }));
            parent_node.sub_nodes.get_mut(&suffix).unwrap()
        }
    }

    fn get_node(&self, path: &Path) -> Option<&NamespaceNode<T>> {
        if path.is_root() {
            return Some(&self.root_node);
        }

        let parent = path.get_parent().unwrap();
        let suffix = path.get_suffix().unwrap();

        let parent_node = self.get_node(&parent)?;

        parent_node.sub_nodes.get(&suffix).map(|n| &**n)
    }

    fn get_node_mut(&mut self, path: &Path) -> Option<&mut NamespaceNode<T>> {
        if path.is_root() {
            return Some(&mut self.root_node);
        }

        let parent = path.get_parent().unwrap();
        let suffix = path.get_suffix().unwrap();

        let parent_node = self.get_node_mut(&parent)?;

        parent_node.sub_nodes.get_mut(&suffix).map(|n| &mut **n)
    }

    pub fn iter_children(&self, parent: &Path) -> Option<impl Iterator<Item = (Path, &T)>> {
        let Some(node) = self.get_node(parent) else {
            return None;
        };

        Some(node.sub_nodes.iter().filter_map(move |(k, v)| {
            Some((parent.subpath(k.to_string()), v.definition.as_ref()?))
        }).into_iter())
    }

    pub fn iter_children_mut(&mut self, parent: &Path) -> Option<impl Iterator<Item = (Path, &mut T)>> {
        let Some(node) = self.get_node_mut(parent) else {
            return None;
        };

        Some(node.sub_nodes.iter_mut().filter_map(move |(k, v)| {
            Some((parent.subpath(k.to_string()), v.definition.as_mut()?))
        }).into_iter())
    }

    pub fn set(&mut self, path: Path, value: T) {
        let node = self.get_or_make_node(&path);

        node.definition = Some(value);
    }

    pub fn get(&self, path: &Path) -> Option<&T> {
        let node = self.get_node(path)?;

        node.definition.as_ref()
    }

    pub fn get_mut(&mut self, path: &Path) -> Option<&mut T> {
        let node = self.get_node_mut(path)?;

        node.definition.as_mut()
    }
}
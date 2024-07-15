use std::{collections::VecDeque, ops::SubAssign, ptr::NonNull};

/// A min-pairing heap data structure.
#[derive(Debug)]
pub struct PairingHeap<K, P> {
    root: Option<NonNull<Inner<K, P>>>,
    len: usize,
}

// implement Clone
impl<K: Clone, P: Clone + PartialOrd> Clone for PairingHeap<K, P> {
    fn clone(&self) -> Self {
        let mut new_heap = Self::default();
        let mut queue = VecDeque::new();
        if let Some(root) = self.root {
            queue.push_back(root);
        }

        while let Some(node) = queue.pop_front() {
            unsafe {
                let node = node.as_ref();
                let _new_node = new_heap.insert2(node.key.clone(), node.prio.clone());
                if let Some(left) = node.left {
                    queue.push_back(left);
                }
                if let Some(right) = node.right {
                    queue.push_back(right);
                }
            }
        }

        new_heap
    }
}

impl<K, P> PairingHeap<K, P> {
    /// Creates an empty pairing heap.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of elements stored in the heap.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Checks whether the heap is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the minimum element, which is the root element, and its priority in a tuple of the heap.
    #[inline]
    pub fn find_min(&self) -> Option<(&K, &P)> {
        match self.root {
            Some(node) => unsafe {
                let r = node.as_ref();
                Some((&r.key, &r.prio))
            },
            None => None,
        }
    }

    /// Merges two heaps together and forms a new heap.
    ///
    /// If one heap is empty, the other heap will be returned and vice versa. Otherwise, a new heap
    /// will be created, whose root is the root that has a smaller value. The other root will be
    /// inserted in the new heap.
    #[inline]
    pub fn merge(mut self, mut other: Self) -> Self
    where
        P: PartialOrd,
    {
        let len = self.len() + other.len();
        let root = Self::merge_nodes(self.root, other.root);

        self.root = None;
        other.root = None;

        Self { root, len }
    }

    #[inline]
    fn merge_nodes(node1: Option<NonNull<Inner<K, P>>>, node2: Option<NonNull<Inner<K, P>>>) -> Option<NonNull<Inner<K, P>>>
    where
        P: PartialOrd,
    {
        match (node1, node2) {
            (Some(root1), Some(root2)) => unsafe {
                let root = if root1.as_ref().prio < root2.as_ref().prio {
                    Self::meld(root1, root2)
                } else {
                    Self::meld(root2, root1)
                };
                Some(root)
            },
            (Some(_), None) => node1,
            (None, Some(_)) => node2,
            _ => node1,
        }
    }

    #[inline(always)]
    unsafe fn meld(node1: NonNull<Inner<K, P>>, node2: NonNull<Inner<K, P>>) -> NonNull<Inner<K, P>> {
        (*node2.as_ptr()).parent = Some(node1);
        (*node2.as_ptr()).right = node1.as_ref().left;
        (*node1.as_ptr()).left = Some(node2);
        node1
    }

    /// Inserts a new element to the heap.
    #[inline]
    pub fn insert(&mut self, key: K, prio: P)
    where
        P: PartialOrd,
    {
        self.insert2(key, prio);
    }

    // Expose HeapElmt to pub, no?
    #[inline]
    pub(crate) fn insert2(&mut self, key: K, prio: P) -> HeapElmt<K, P>
    where
        P: PartialOrd,
    {
        let node = NonNull::new(Box::leak(Box::new(Inner::new(key, prio))));

        self.root = Self::merge_nodes(self.root, node);
        self.len += 1;

        HeapElmt { inner: node }
    }

    /// Decreases the priority of a key by the amount given in ```delta```.
    pub fn decrease_prio(&mut self, key: &K, delta: P)
    where
        K: PartialEq,
        P: PartialOrd + SubAssign,
    {
        if let Some(root) = self.root {
            unsafe {
                if &root.as_ref().key == key {
                    (*root.as_ptr()).prio -= delta;
                    return;
                }

                let mut targ = None;
                let mut prev = None;
                let mut tmp_nodes = VecDeque::with_capacity(self.len << 2);
                let mut traverse = root.as_ref().left;

                while let Some(node) = traverse {
                    if &node.as_ref().key == key {
                        targ = traverse;
                        break;
                    }

                    prev = traverse;
                    tmp_nodes.push_back(traverse);

                    if node.as_ref().right.is_some() {
                        traverse = node.as_ref().right;
                    } else {
                        while let Some(front) = tmp_nodes.pop_front() {
                            traverse = front.unwrap().as_ref().left;
                            if traverse.is_some() {
                                break;
                            }
                        }
                    }
                }

                if let Some(node) = targ {
                    // Every node must have a parent. So unwrap() here shouldn't panic.
                    let parent = node.as_ref().parent.unwrap();
                    (*node.as_ptr()).prio -= delta;

                    if parent.as_ref().prio < node.as_ref().prio {
                        return;
                    }

                    if parent.as_ref().left == targ {
                        (*parent.as_ptr()).left = node.as_ref().right;
                    }

                    if let Some(prev_node) = prev {
                        if prev_node.as_ref().right == targ {
                            (*prev_node.as_ptr()).right = node.as_ref().right;
                        }
                    }

                    (*node.as_ptr()).parent = None;
                    (*node.as_ptr()).right = None;

                    self.root = Self::merge_nodes(self.root, targ);
                }
            }
        }
    }

    // TODO: currently only works when new_prio < prio.
    pub(crate) fn update_prio(&mut self, node: &HeapElmt<K, P>, new_prio: P)
    where
        P: PartialOrd,
    {
        unsafe {
            self.update(node.inner, new_prio);
        }
    }

    unsafe fn update(&mut self, targ: Option<NonNull<Inner<K, P>>>, new_prio: P)
    where
        P: PartialOrd,
    {
        if let Some(node) = targ {
            match node.as_ref().parent {
                Some(parent) => {
                    let mut prev = parent.as_ref().left;

                    while let Some(prev_node) = prev {
                        if prev_node.as_ref().right == targ {
                            break;
                        } else {
                            prev = prev_node.as_ref().right;
                        }
                    }

                    (*node.as_ptr()).prio = new_prio;

                    if parent.as_ref().prio < node.as_ref().prio {
                        return;
                    }

                    if parent.as_ref().left == targ {
                        (*parent.as_ptr()).left = node.as_ref().right;
                    }

                    if let Some(prev_node) = prev {
                        if prev_node.as_ref().right == targ {
                            (*prev_node.as_ptr()).right = node.as_ref().right;
                        }
                    }

                    (*node.as_ptr()).parent = None;
                    (*node.as_ptr()).right = None;

                    self.root = Self::merge_nodes(self.root, targ);
                }
                None => {
                    (*node.as_ptr()).prio = new_prio;
                }
            };
        }
    }

    /// Deletes the minimum element, which is the root, of the heap, and then returns the root's key value and priority.
    pub fn delete_min(&mut self) -> Option<(K, P)>
    where
        P: PartialOrd,
    {
        self.root.map(|root| unsafe {
            self.len -= 1;
            let mut targ = (*root.as_ptr()).left.take();
            if targ.is_none() {
                self.root = None;
            } else {
                // TODO: optimise so that capacity is known here.
                let mut tmp_nodes = VecDeque::new();

                // First pass: left to right
                while let Some(node) = targ {
                    (*node.as_ptr()).parent = None;
                    let right = (*node.as_ptr()).right.take();

                    let node_next = match right {
                        Some(node_right) => {
                            let next = (*node_right.as_ptr()).right.take();
                            (*node_right.as_ptr()).parent = None;
                            next
                        }
                        None => None,
                    };

                    tmp_nodes.push_back(Self::merge_nodes(Some(node), right));

                    targ = node_next;
                }

                // Second pass: right to left
                // If left is not None, there must be at least one element in VecDeque.
                // So unwrap() is safe here.
                let mut node = tmp_nodes.pop_back().unwrap();

                while let Some(node_prev) = tmp_nodes.pop_back() {
                    node = Self::merge_nodes(node, node_prev);
                }

                self.root = node;
            }
            let node = Box::from_raw(root.as_ptr());
            node.into_value()
        })
    }
}

impl<K, P> Default for PairingHeap<K, P> {
    fn default() -> Self {
        Self { root: None, len: 0 }
    }
}

impl<K, P> Drop for PairingHeap<K, P> {
    fn drop(&mut self) {
        // Remove all children of a node, then the node itself.
        // Returns the next sibling in the end.

        unsafe fn remove<K, P>(targ: Option<NonNull<Inner<K, P>>>) -> Option<NonNull<Inner<K, P>>> {
            if let Some(node) = targ {
                while let Some(left) = node.as_ref().left {
                    (*node.as_ptr()).left = remove(Some(left));
                }

                let sibling = (*node.as_ptr()).right.take();
                (*node.as_ptr()).parent = None;
                let _ = Box::from_raw(node.as_ptr());

                sibling
            } else {
                None
            }
        }

        unsafe {
            remove(self.root);
        }

        self.root = None;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HeapElmt<K, P> {
    inner: Option<NonNull<Inner<K, P>>>,
}

impl<K, P> HeapElmt<K, P> {
    pub(crate) fn is_none(&self) -> bool {
        self.inner.is_none()
    }

    pub(crate) fn none(&mut self) {
        self.inner = None;
    }
}

impl<K, P> Default for HeapElmt<K, P> {
    fn default() -> Self {
        Self { inner: None }
    }
}

#[derive(Debug)]
struct Inner<K, P> {
    /// Pointer to a node's parent.
    parent: Option<NonNull<Inner<K, P>>>,
    /// Pointer to a node's first (or left-most) child.
    left: Option<NonNull<Inner<K, P>>>,
    /// Pointer to a node's next older sibling.
    right: Option<NonNull<Inner<K, P>>>,
    key: K,
    prio: P,
}

impl<K, P> Inner<K, P> {
    fn new(key: K, prio: P) -> Self {
        Self {
            key,
            prio,
            parent: None,
            left: None,
            right: None,
        }
    }

    fn into_value(self) -> (K, P) {
        (self.key, self.prio)
    }
}

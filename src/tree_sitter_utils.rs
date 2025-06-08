use std::{collections::VecDeque, ops::ControlFlow};

use tree_sitter::Node;

pub fn walk_children<'a, F, B>(node: Node<'a>, mut on_node: F) -> Option<B>
where
    F: FnMut(Node<'a>) -> ControlFlow<B>,
{
    let mut stack = VecDeque::new();
    let mut children = Vec::new();
    stack.push_back(node);

    while let Some(current_node) = stack.pop_back() {
        if let ControlFlow::Break(value) = on_node(current_node) {
            return Some(value);
        }

        let mut cursor = current_node.walk();
        if cursor.goto_first_child() {
            children.clear();
            children.push(cursor.node());
            while cursor.goto_next_sibling() {
                children.push(cursor.node());
            }

            for child in children.iter().rev() {
                stack.push_back(*child);
            }
        }
    }

    None
}

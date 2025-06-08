use std::{ops::ControlFlow, path::Path};

use color_eyre::eyre::{Context as _, ContextCompat, Result, bail};
use tree_sitter::{Node, Parser};

use crate::{Language, TestCommand, tree_sitter_utils::walk_children};

#[derive(Debug, Clone, Copy)]
pub struct GoImpl;

impl Language for GoImpl {
    fn test_command(&self, file: &Path, line: usize) -> Result<TestCommand> {
        let mut parser = Parser::new();

        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .context("error loading Go grammar")?;

        let source = std::fs::read_to_string(file)?;
        let tree = parser.parse(&source, None).unwrap();
        let root = tree.root_node();

        let find_node_at_line = |line: usize| -> Result<Node<'_>> {
            walk_children(root, |node| {
                if node.start_position().row + 1 == line {
                    return ControlFlow::Break(node);
                }
                ControlFlow::Continue(())
            })
            .with_context(|| format!("no syntax node found at line {line}"))
        };

        let node_at_line = match find_node_at_line(line) {
            Ok(node) => node,
            Err(err) => 'block: {
                if let Ok(node) = find_node_at_line(line + 1) {
                    break 'block node;
                }
                if let Ok(node) = find_node_at_line(line - 1) {
                    break 'block node;
                }
                return Err(err);
            }
        };

        let mut args = Vec::from(["test".to_owned(), "-count=1".to_owned()]);

        let mut function_name = None;
        if let Some(parent_function) = self.parent_test_function(node_at_line, &source)? {
            if parent_function.kind() == "method_declaration" {
                let identifier = parent_function.child(2).unwrap();
                function_name = Some(identifier.utf8_text(source.as_bytes())?);
                args.extend(["-run".to_owned(), format!("/{}$", function_name.unwrap())]);
            } else if parent_function.kind() == "function_declaration" {
                let identifier = parent_function.child(1).unwrap();
                function_name = Some(identifier.utf8_text(source.as_bytes())?);
                args.extend(["-run".to_owned(), format!("{}$", function_name.unwrap())]);
            } else {
                bail!("failed to parse function node: {parent_function:?}");
            }
        }

        args.push(file.parent().unwrap().to_str().unwrap().to_owned());

        Ok(TestCommand {
            command: "go".to_owned(),
            args,
            statusline: if let Some(function_name) = function_name {
                function_name.to_owned()
            } else {
                file.file_name().unwrap().to_str().unwrap().to_owned()
            },
        })
    }
}

impl GoImpl {
    fn parent_test_function<'a>(self, node: Node<'a>, source: &str) -> Result<Option<Node<'a>>> {
        let mut parent = Some(node);
        while let Some(node) = parent {
            if node.kind() == "method_declaration"
                && let Some(ident) = node.child(2)
                && ident.utf8_text(source.as_bytes())?.starts_with("Test")
            {
                return Ok(Some(node));
            }
            if node.kind() == "function_declaration"
                && let Some(ident) = node.child(1)
                && ident.utf8_text(source.as_bytes())?.starts_with("Test")
            {
                return Ok(Some(node));
            }
            parent = node.parent();
        }
        Ok(None)
    }
}

use std::{ops::ControlFlow, path::Path};

use color_eyre::eyre::{Context as _, ContextCompat, Result, bail};
use tree_sitter::{Node, Parser};

use crate::{Language, TestCommand, TestCommands, tree_sitter_utils::walk_children};

#[derive(Debug, Clone, Copy)]
pub struct GoImpl;

impl Language for GoImpl {
    fn test_commands(&self, file: &Path, line: usize) -> Result<TestCommands> {
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

        let function_name =
            if let Some(parent_function) = self.parent_test_function(node_at_line, &source)? {
                if parent_function.kind() == "method_declaration" {
                    let identifier = parent_function.child(2).unwrap();
                    Some(FuncName::Method(identifier.utf8_text(source.as_bytes())?))
                } else if parent_function.kind() == "function_declaration" {
                    let identifier = parent_function.child(1).unwrap();
                    Some(FuncName::Func(identifier.utf8_text(source.as_bytes())?))
                } else {
                    bail!("failed to parse function node: {parent_function:?}");
                }
            } else {
                None
            };

        let file_command = TestCommand {
            command: "go".to_owned(),
            args: ["test".to_owned(), "-count=1".to_owned()]
                .into_iter()
                .chain([file.parent().unwrap().to_str().unwrap().to_owned()])
                .collect::<Vec<_>>(),
        };

        let file_and_line_command = TestCommand {
            command: "go".to_owned(),
            args: ["test".to_owned(), "-count=1".to_owned()]
                .into_iter()
                .chain(
                    function_name
                        .map(|function_name| match function_name {
                            FuncName::Func(name) => ["-run".to_owned(), format!("{name}$")],
                            FuncName::Method(name) => ["-run".to_owned(), format!("/{name}$")],
                        })
                        .into_iter()
                        .flatten(),
                )
                .chain([file.parent().unwrap().to_str().unwrap().to_owned()])
                .collect::<Vec<_>>(),
        };

        let file_debugger_command = TestCommand {
            command: "dlv".to_owned(),
            args: Vec::from([
                "test".to_owned(),
                file.parent().unwrap().to_str().unwrap().to_owned(),
                "--headless".to_owned(),
                "-l".to_owned(),
                "127.0.0.1:38697".to_owned(),
            ]),
        };

        let file_and_line_debugger_command = TestCommand {
            command: "dlv".to_owned(),
            args: Vec::from([
                "test".to_owned(),
                file.parent().unwrap().to_str().unwrap().to_owned(),
                "--headless".to_owned(),
                "-l".to_owned(),
                "127.0.0.1:38697".to_owned(),
                "--".to_owned(),
                "-test.run".to_owned(),
            ])
            .into_iter()
            .chain(function_name.map(|function_name| match function_name {
                FuncName::Func(name) => {
                    format!("{name}$")
                }
                FuncName::Method(name) => {
                    format!("/{name}$")
                }
            }))
            .collect(),
        };

        Ok(TestCommands {
            file: file_command,
            file_and_line: file_and_line_command,
            file_debugger: Some(file_debugger_command),
            file_and_line_debugger: Some(file_and_line_debugger_command),
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

#[derive(Debug, Copy, Clone)]
enum FuncName<'a> {
    Func(&'a str),
    Method(&'a str),
}

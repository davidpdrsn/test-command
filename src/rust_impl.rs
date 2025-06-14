use std::{
    ops::ControlFlow,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{Context as _, ContextCompat, Result, bail};
use serde::Deserialize;
use tree_sitter::{Node, Parser};

use crate::{Language, TestCommand, tree_sitter_utils::walk_children};

#[derive(Debug, Clone, Copy)]
pub struct RustImpl;

impl Language for RustImpl {
    fn test_command(&self, file: &Path, line: usize) -> Result<TestCommand> {
        let cargo_toml = self.parent_cargo_toml()?;
        let cargo_toml = std::fs::read_to_string(cargo_toml)?;
        let cargo_toml = toml::from_str::<CargoToml>(&cargo_toml)?;

        let mut parser = Parser::new();

        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .context("error loading Rust grammar")?;

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

        let mut path = Vec::new();
        path.extend(self.parent_file_mods(file)?);
        path.extend(self.parent_source_mods(node_at_line, &source)?);

        if let Some(parent_function) = self.parent_test_function(node_at_line, &source)? {
            let identifier = walk_children(parent_function, |node| {
                if node.kind() == "identifier" {
                    return ControlFlow::Break(node);
                }
                ControlFlow::Continue(())
            })
            .context("failed to find function identifier")?;

            let function_name = identifier.utf8_text(source.as_bytes())?;
            path.push(function_name.to_owned());
        }

        Ok(TestCommand {
            command: "cargo".to_owned(),
            args: Vec::from([
                "test".to_owned(),
                "--all-features".to_owned(),
                "-p".to_owned(),
                cargo_toml.package.name,
                path.join("::"),
            ]),
            statusline: path.join("::"),
        })
    }
}

impl RustImpl {
    fn parent_test_function<'a>(self, node: Node<'a>, source: &str) -> Result<Option<Node<'a>>> {
        let mut parent = Some(node);
        while let Some(node) = parent {
            let has_test_attr = {
                let mut node = node;
                std::iter::from_fn(move || {
                    node = node.prev_sibling()?;
                    Some(node)
                })
                .take_while(|node| node.kind() != "function_item")
                .filter(|node| node.kind() == "attribute_item")
                .any(|node| {
                    node.utf8_text(source.as_bytes())
                        .is_ok_and(|text| text == "#[test]" || text.contains("::test"))
                })
            };

            if node.kind() == "function_item" && has_test_attr {
                return Ok(Some(node));
            }
            parent = node.parent();
        }
        Ok(None)
    }

    fn parent_source_mods(self, node: Node<'_>, source: &str) -> Result<Vec<String>> {
        let mut mods = Vec::new();
        let mut parent = node.parent();
        while let Some(node) = parent {
            if node.kind() == "mod_item"
                && let Some(child) = node.child(1)
            {
                mods.push(child.utf8_text(source.as_bytes())?.to_owned());
            }
            parent = node.parent();
        }
        mods.reverse();
        Ok(mods)
    }

    fn parent_cargo_toml(self) -> Result<PathBuf> {
        #[derive(Deserialize)]
        struct CargoMetadata {
            workspace_root: String,
        }

        let output = std::process::Command::new("cargo")
            .args(["metadata"])
            .output()
            .context("`cargo metadata` failed")?;
        let output = String::from_utf8(output.stdout)?;
        let output = serde_json::from_str::<CargoMetadata>(&output)?;
        Ok(Path::new(&output.workspace_root).join("Cargo.toml"))
    }

    fn parent_file_mods(self, path: &Path) -> Result<Vec<String>> {
        let mut mods = Vec::new();
        let mut seen_src = false;
        for comp in path.components() {
            match comp {
                std::path::Component::Normal(os_str) => {
                    if os_str == "src" {
                        if seen_src {
                            bail!("unexpected `src` dir");
                        } else {
                            seen_src = true;
                        }
                    } else if seen_src
                        && let Some(file_stem) = Path::new(os_str).file_stem()
                        && file_stem != "mod"
                        && file_stem != "main"
                        && file_stem != "lib"
                    {
                        mods.push(
                            file_stem
                                .to_str()
                                .context("invalid utf-8 in path")?
                                .to_owned(),
                        );
                    }
                }

                std::path::Component::Prefix(_)
                | std::path::Component::CurDir
                | std::path::Component::ParentDir => {
                    bail!("unexpected path component: {comp:?}");
                }
                std::path::Component::RootDir => {}
            }
        }
        Ok(mods)
    }
}

#[derive(Deserialize, Debug)]
struct CargoToml {
    package: CargoTomlPackage,
}

#[derive(Deserialize, Debug)]
struct CargoTomlPackage {
    name: String,
}

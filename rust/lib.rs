// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2023, Douglas Creager.
// Licensed under the MIT license.
// Please see the LICENSE file in this distribution for license details.
// ------------------------------------------------------------------------------------------------

use std::ffi::c_char;
use std::ffi::c_int;
use std::ffi::c_void;

use lua;
use tree_sitter::Tree;

/// An extension trait that lets you load the `ltreesitter` module into a Lua environment.
pub trait LTreeSitter {
    /// Loads the `ltreesitter` module into a Lua environment.  If `global` is true, sets the
    /// global `ltreesitter` variable to the loaded module.
    fn open_ltreesitter(&mut self, global: bool);
}

impl LTreeSitter for lua::State {
    fn open_ltreesitter(&mut self, global: bool) {
        extern "C" {
            fn luaopen_ltreesitter(l: *mut lua::ffi::lua_State) -> c_int;
        }
        self.requiref("ltreesitter", Some(luaopen_ltreesitter), global);
        self.pop(1);
    }
}

// Replace this with a call to Tree::into_raw once a >0.28.8 release is cut.
fn tree_into_raw(tree: Tree) -> *mut c_void {
    // The Lua wrapper will take ownership of the tree.
    let tree = std::mem::ManuallyDrop::new(tree);
    // Pull some shenanigans to access the tree's TSTree pointer.
    type RawTree = std::ptr::NonNull<c_void>;
    let raw_tree: RawTree = unsafe { std::mem::transmute(tree) };
    raw_tree.as_ptr()
}

/// An extension trait that lets you combine a [`tree_sitter::Tree`] with the source code that it
/// was parsed from.
pub trait WithSource {
    /// Combines a [`tree_sitter::Tree`] with the source code that it was parsed from.
    fn with_source<'a>(self, src: &'a [u8]) -> TreeWithSource<'a>;
}

/// The combination of a [`tree_sitter::Tree`] with the source code that it was parsed from.  This
/// type implements the [`lua::ToLua`] trait, so you can push it onto a Lua stack.
pub struct TreeWithSource<'a> {
    pub tree: tree_sitter::Tree,
    pub src: &'a [u8],
}

impl WithSource for tree_sitter::Tree {
    fn with_source<'a>(self, src: &'a [u8]) -> TreeWithSource<'a> {
        TreeWithSource {
            tree: self,
            src: src.as_ref(),
        }
    }
}

impl lua::ToLua for TreeWithSource<'_> {
    fn to_lua(&self, l: &mut lua::State) {
        extern "C" {
            fn ltreesitter_push_tree(
                l: *mut lua::ffi::lua_State,
                t: *mut c_void,
                src_len: usize,
                src: *const c_char,
            );
        }
        let tree = tree_into_raw(self.tree.clone());
        // ltreesitter makes a copy of src, so src does _not_ need to outlive the Lua state.
        let src_len = self.src.len();
        let src = self.src.as_ptr() as *const c_char;
        unsafe { ltreesitter_push_tree(l.as_ptr(), tree, src_len, src) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait CheckLua {
        /// Executes a chunk of Lua code.  If it returns a string, interprets that string as an
        /// error message, and translates that into an `anyhow` error.
        fn check(&mut self, chunk: &str) -> Result<(), anyhow::Error>;
    }

    impl CheckLua for lua::State {
        fn check(&mut self, chunk: &str) -> Result<(), anyhow::Error> {
            self.do_string(chunk).with_error(self)?;
            Ok(())
        }
    }

    #[test]
    fn can_consume_parse_tree_from_lua() -> Result<(), anyhow::Error> {
        let code = br#"
          def double(x):
              return x * 2
        "#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(tree_sitter_python::language())?;
        let parsed = parser.parse(code, None).expect("Cannot parse Python code");
        let mut l = lua::State::new();
        l.open_base();
        l.open_ltreesitter(false);
        l.push(parsed.with_source(code));
        l.set_global("parsed");
        l.check(
            r#"
              local root = parsed:root()
              assert(root:type() == "module", "expected module as root of tree")
            "#,
        )?;
        Ok(())
    }
}

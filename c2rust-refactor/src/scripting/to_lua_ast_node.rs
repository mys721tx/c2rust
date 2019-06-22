use std::cell::{Ref, RefCell};
use std::ops::DerefMut;
use std::sync::Arc;

use rustc::hir::def::Def;
use syntax::ast::*;
use syntax::ptr::P;
use syntax::mut_visit::*;
use syntax_pos::Span;

use rlua::{Context, Error, Function, Result, Scope, ToLua, UserData, UserDataMethods, Value};

use crate::ast_manip::{util, visit_nodes, AstName, AstNode, WalkAst};
use super::DisplayLuaError;

pub(crate) trait ToLuaExt {
    fn to_lua<'lua>(self, lua: Context<'lua>) -> Result<Value<'lua>>;
}

pub(crate) trait ToLuaScoped {
    fn to_lua_scoped<'lua, 'scope>(self, lua: Context<'lua>, scope: &Scope<'lua, 'scope>) -> Result<Value<'lua>>;
}

impl<T> ToLuaExt for T
    where T: Sized,
          LuaAstNode<T>: 'static + UserData + Send,
{
    fn to_lua<'lua>(self, lua: Context<'lua>) -> Result<Value<'lua>> {
        lua.create_userdata(LuaAstNode::new(self))?.to_lua(lua)
    }
}
    

impl<T> ToLuaScoped for T
    where T: 'static + Sized,
          LuaAstNode<T>: UserData,
{
    fn to_lua_scoped<'lua, 'scope>(self, lua: Context<'lua>, scope: &Scope<'lua, 'scope>) -> Result<Value<'lua>> {
        scope.create_static_userdata(LuaAstNode::new(self)).and_then(|v| v.to_lua(lua))
    }
}


// impl<'lua, 'scope, T> ToLuaScoped<'lua, 'scope> for T
//     where T: 'static,
//           LuaAstNode<T>: UserData,
// {
//     fn to_lua_scoped(self, lua: Context<'lua>, scope: &Scope<'lua, 'scope>) -> Result<Value<'lua>> {
//         scope.create_static_userdata(LuaAstNode::new(self)).and_then(|v| v.to_lua(lua))
//     }
// }

/// Holds a rustc AST node that can be passed back and forth to Lua as a scoped,
/// static userdata. Implement UserData for LuaAstNode<T> to support an AST node
/// T.
#[derive(Clone)]
pub(crate) struct LuaAstNode<T> (Arc<RefCell<T>>);

impl<T> LuaAstNode<T> {
    pub fn new(item: T) -> Self {
        Self(Arc::new(RefCell::new(item)))
    }

    pub fn into_inner(self) -> T {
        Arc::try_unwrap(self.0)
            .unwrap_or_else(|_| panic!("LuaAstNode is duplicated"))
            .into_inner()
    }

    pub fn borrow(&self) -> Ref<T> {
        self.0.borrow()
    }

    pub fn map<F>(&self, f: F)
        where F: Fn(&mut T)
    {
        f(self.0.borrow_mut().deref_mut());
    }
}

impl<T> LuaAstNode<T>
    where T: WalkAst
{
    pub fn walk<V: MutVisitor>(&self, visitor: &mut V) {
        self.0.borrow_mut().walk(visitor);
    }
}

unsafe impl Send for LuaAstNode<P<Item>> {}
#[allow(unused_doc_comments)]
impl UserData for LuaAstNode<P<Item>> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get_kind", |_lua_ctx, this, ()| {
            Ok(this.0.borrow().node.ast_name())
        });

        methods.add_method("get_id", |lua_ctx, this, ()| {
            Ok(this.0.borrow().id.to_lua(lua_ctx))
        });

        methods.add_method("get_ident", |_lua_ctx, this, ()| {
            Ok(this.0.borrow().ident.to_string())
        });

        methods.add_method("set_ident", |_lua_ctx, this, ident: String| {
            this.0.borrow_mut().ident = Ident::from_str(&ident);
            Ok(())
        });

        /// Visit statements
        // @function visit_stmts
        // @tparam function(LuaAstNode) callback Function to call when visiting each statement
        methods.add_method("visit_stmts", |lua_ctx, this, callback: Function| {
            visit_nodes(&**this.borrow(), |node: &Stmt| {
                callback.call::<_, ()>(node.clone().to_lua(lua_ctx))
                .unwrap_or_else(|e| panic!("Lua callback failed in visit_stmts: {}", DisplayLuaError(e)));
            });
            Ok(())
        });

        methods.add_method("visit_items", |lua_ctx, this, callback: Function| {
            visit_nodes(&**this.borrow(), |node: &Item| {
                callback.call::<_, ()>(P(node.clone()).to_lua(lua_ctx))
                .unwrap_or_else(|e| panic!("Lua callback failed in visit_items: {}", DisplayLuaError(e)));
            });
            Ok(())
        });

        methods.add_method("visit_foreign_items", |lua_ctx, this, callback: Function| {
            visit_nodes(&**this.borrow(), |node: &ForeignItem| {
                callback.call::<_, ()>(P(node.clone()).to_lua(lua_ctx))
                .unwrap_or_else(|e| panic!("Lua callback failed in visit_foreign_items: {}", DisplayLuaError(e)));
            });
            Ok(())
        });

        methods.add_method("get_node", |lua_ctx, this, ()| {
            match this.0.borrow().node.clone() {
                ItemKind::Use(e) => Ok(e.to_lua(lua_ctx)),
                node => Err(Error::external(format!("Item node {:?} not implemented yet", node))),
            }
        });
    }
}

unsafe impl Send for LuaAstNode<P<ForeignItem>> {}
impl UserData for LuaAstNode<P<ForeignItem>> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get_kind", |_lua_ctx, this, ()| {
            Ok(this.0.borrow().node.ast_name())
        });

        methods.add_method("get_id", |lua_ctx, this, ()| {
            Ok(this.0.borrow().id.to_lua(lua_ctx))
        });

        methods.add_method("get_ident", |_lua_ctx, this, ()| {
            Ok(this.0.borrow().ident.to_string())
        });

        methods.add_method("set_ident", |_lua_ctx, this, ident: String| {
            this.0.borrow_mut().ident = Ident::from_str(&ident);
            Ok(())
        });
    }
}

impl UserData for LuaAstNode<QSelf> {}


unsafe impl Send for LuaAstNode<Path> {}
impl UserData for LuaAstNode<Path> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get_span", |lua_ctx, this, ()| {
            this.0.borrow().span.to_lua(lua_ctx)
        });
        methods.add_method("has_generic_args", |_lua_ctx, this, ()| {
            Ok(this.0.borrow().segments.iter().any(|s| s.args.is_some()))
        });
        methods.add_method("get_segments", |_lua_ctx, this, ()| {
            Ok(this.0.borrow().segments.iter().map(|s| s.ident.to_string()).collect::<Vec<_>>())
        });
        methods.add_method("set_segments", |_lua_ctx, this, new_segments: Vec<String>| {
            let has_generic_args = this.0.borrow().segments.iter().any(|s| s.args.is_some());
            if has_generic_args {
                Err(Error::external("One or more path segments have generic args, cannot set segments as strings"))
            } else {
                this.0.borrow_mut().segments = new_segments.into_iter().map(|new_seg| {
                    PathSegment::from_ident(Ident::from_str(&new_seg))
                }).collect();
                Ok(())
            }
        });
        methods.add_method("map_segments", |lua_ctx, this, callback: Function| {
            let new_segments = lua_ctx.scope(|scope| {
                let segments = this.0.borrow().segments.iter().map(|s| scope.create_static_userdata(LuaAstNode::new(s.clone())).unwrap()).collect::<Vec<_>>().to_lua(lua_ctx);
                callback.call::<_, Vec<LuaAstNode<PathSegment>>>(segments)
            }).unwrap();
            this.0.borrow_mut().segments = new_segments.into_iter().map(|s| s.into_inner()).collect();
            Ok(())
        });
    }
}

impl UserData for LuaAstNode<PathSegment> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get_ident", |_lua_ctx, this, ()| {
            Ok(this.0.borrow().ident.to_string())
        });
    }
}

unsafe impl Send for LuaAstNode<Def> {}
impl UserData for LuaAstNode<Def> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get_namespace", |_lua_ctx, this, ()| {
            Ok(util::namespace(&*this.0.borrow()).map(|namespace| namespace.descr()))
        });
    }
}


impl ToLuaExt for NodeId {
    fn to_lua<'lua>(self, lua: Context<'lua>) -> Result<Value<'lua>> {
        self.as_u32().to_lua(lua)
    }
}

struct SpanData(syntax_pos::SpanData);

impl UserData for SpanData {}

impl ToLuaExt for Span {
    fn to_lua<'lua>(self, lua: Context<'lua>) -> Result<Value<'lua>> {
        lua.create_userdata(SpanData(self.data())).unwrap().to_lua(lua)
    }
}

unsafe impl Send for LuaAstNode<P<Expr>> {}
impl UserData for LuaAstNode<P<Expr>> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get_kind", |_lua_ctx, this, ()| {
            Ok(this.0.borrow().node.ast_name())
        });

        methods.add_method("get_node", |lua_ctx, this, ()| {
            match this.0.borrow().node.clone() {
                ExprKind::Lit(x) => Ok(x.to_lua(lua_ctx)),
                node => Err(Error::external(format!("Expr node {:?} not implemented yet", node))),
            }
        })
    }
}

unsafe impl Send for LuaAstNode<P<Ty>> {}
impl UserData for LuaAstNode<P<Ty>> {}

unsafe impl Send for LuaAstNode<Vec<Stmt>> {}
impl UserData for LuaAstNode<Vec<Stmt>> {}

unsafe impl Send for LuaAstNode<Stmt> {}
impl UserData for LuaAstNode<Stmt> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get_kind", |_lua_ctx, this, ()| {
            Ok(this.0.borrow().node.ast_name())
        });
        methods.add_method("get_node", |lua_ctx, this, ()| {
            match this.0.borrow().node.clone() {
                StmtKind::Expr(e) | StmtKind::Semi(e) => Ok(e.to_lua(lua_ctx)),
                StmtKind::Local(l) => Ok(l.to_lua(lua_ctx)),
                StmtKind::Item(i) => Ok(i.to_lua(lua_ctx)),
                StmtKind::Mac(_) => Err(Error::external(format!("Mac stmts aren't implemented yet"))),
            }
        });
    }
}

unsafe impl Send for LuaAstNode<P<Pat>> {}
impl UserData for LuaAstNode<P<Pat>> {}

unsafe impl Send for LuaAstNode<Crate> {}
impl UserData for LuaAstNode<Crate> {}

unsafe impl Send for LuaAstNode<P<Local>> {}
impl UserData for LuaAstNode<P<Local>> {}

unsafe impl Send for LuaAstNode<Lit> {}
impl UserData for LuaAstNode<Lit> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get_value", |lua_ctx, this, ()| {
            match this.0.borrow().node {
                LitKind::Str(s, _) => {
                    Ok(s.to_string().to_lua(lua_ctx))
                }
                LitKind::Int(i, _suffix) => Ok(i.to_lua(lua_ctx)),
                LitKind::Bool(b) => Ok(b.to_lua(lua_ctx)),
                ref node => {
                    return Err(Error::external(format!(
                        "{:?} is not yet implemented",
                        node
                    )));
                }
            }
        });
    }
}

unsafe impl Send for LuaAstNode<Mod> {}
impl UserData for LuaAstNode<Mod> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("insert_item", |_lua_ctx, this, (index, item): (usize, LuaAstNode<P<Item>>)| {
            this.0.borrow_mut().items.insert(index, item.borrow().clone());
            Ok(())
        });
    }
}

unsafe impl Send for LuaAstNode<P<UseTree>> {}
impl UserData for LuaAstNode<P<UseTree>> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get_kind", |_lua_ctx, this, ()| {
            Ok(this.0.borrow().kind.ast_name())
        });

        methods.add_method("get_prefix", |lua_ctx, this, ()| {
            this.0.borrow().prefix.clone().to_lua(lua_ctx)
        });

        methods.add_method("get_rename", |_lua_ctx, this, ()| {
            match this.0.borrow().kind {
                UseTreeKind::Simple(Some(rename), _, _) => Ok(Some(rename.to_string())),
                _ => Ok(None),
            }
        });

        methods.add_method("get_nested", |lua_ctx, this, ()| {
            match &this.0.borrow().kind {
                UseTreeKind::Nested(trees) => Ok(Some(
                    trees.clone()
                        .into_iter()
                        .map(|(tree, id)| Ok(vec![P(tree).to_lua(lua_ctx)?, id.to_lua(lua_ctx)?]))
                        .collect::<Result<Vec<_>>>()?
                )),
                _ => Ok(None),
            }
        });
    }
}

impl ToLuaExt for AstNode {
    fn to_lua<'lua>(self, lua: Context<'lua>) -> Result<Value<'lua>> {
        match self {
            AstNode::Crate(x) => x.to_lua(lua),
            AstNode::Expr(x) => x.to_lua(lua),
            AstNode::Pat(x) => x.to_lua(lua),
            AstNode::Ty(x) => x.to_lua(lua),
            AstNode::Stmts(x) => x.to_lua(lua),
            AstNode::Stmt(x) => x.to_lua(lua),
            AstNode::Item(x) => x.to_lua(lua),
        }
    }
}
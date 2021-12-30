use std::mem;

use indexmap::IndexSet;
use once_cell::sync::Lazy;
use regex::Regex;
use sha1::{Digest, Sha1};
use swc_atoms::JsWord;
use swc_common::{util::take::Take, SourceMap, Spanned, DUMMY_SP};
use swc_ecma_ast::*;
use swc_ecma_utils::{private_ident, quote_ident, ExprFactory};
use swc_ecma_visit::{
    noop_visit_mut_type, noop_visit_type, Visit, VisitMut, VisitMutWith, VisitWith,
};

use crate::RefreshOptions;

use super::util::{is_builtin_hook, make_call_expr, make_call_stmt, CollectIdent};

// function that use hooks
struct HookSig {
    handle: Ident,
    // need to add an extra register, or alreay inlined
    hooks: Vec<Hook>,
}

impl HookSig {
    fn new(hooks: Vec<Hook>) -> Self {
        HookSig {
            handle: private_ident!("_s"),
            hooks,
        }
    }
}

struct Hook {
    callee: HookCall,
    key: String,
}

// we only consider two kinds of callee as hook call
enum HookCall {
    Ident(Ident),
    Member(Expr, Ident), // for obj and prop
}
pub struct HookRegister<'a> {
    pub options: &'a RefreshOptions,
    pub ident: Vec<Ident>,
    pub extra_stmt: Vec<Stmt>,
    pub scope_binding: IndexSet<JsWord>,
    pub cm: &'a SourceMap,
    pub should_reset: bool,
}

impl<'a> HookRegister<'a> {
    pub fn gen_hook_handle(&mut self) -> Stmt {
        Stmt::Decl(Decl::Var(VarDecl {
            span: DUMMY_SP,
            kind: VarDeclKind::Var,
            decls: self
                .ident
                .take()
                .into_iter()
                .map(|id| VarDeclarator {
                    span: DUMMY_SP,
                    name: Pat::Ident(BindingIdent::from(id)),
                    init: Some(Box::new(make_call_expr(quote_ident!(self
                        .options
                        .refresh_sig
                        .clone())))),
                    definite: false,
                })
                .collect(),
            declare: false,
        }))
    }

    // The second call is around the function itself. This is used to associate a
    // type with a signature.
    // Unlike with $RefreshReg$, this needs to work for nested declarations too.
    fn wrap_with_register(&self, handle: Ident, func: Expr, hooks: Vec<Hook>) -> Expr {
        let mut args = vec![func.as_arg()];
        let mut sign = Vec::new();
        let mut custom_hook = Vec::new();

        for hook in hooks {
            let name = match &hook.callee {
                HookCall::Ident(i) => i,
                HookCall::Member(_, i) => i,
            };
            sign.push(format!("{}{{{}}}", name.sym, hook.key));
            match &hook.callee {
                HookCall::Ident(ident) if !is_builtin_hook(ident) => {
                    custom_hook.push(hook.callee);
                }
                HookCall::Member(obj, prop) if !is_builtin_hook(prop) => {
                    if let Expr::Ident(ident) = obj {
                        if ident.sym.as_ref() != "React" {
                            custom_hook.push(hook.callee);
                        }
                    }
                }
                _ => (),
            };
        }

        // this is just for pass test
        let has_escape = sign.len() > 1;
        let sign = sign.join("\n");
        let sign = if self.options.emit_full_signatures {
            sign
        } else {
            let mut hasher = Sha1::new();
            hasher.update(sign);
            base64::encode(hasher.finalize())
        };

        args.push(
            Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: sign.into(),
                has_escape,
                kind: StrKind::Synthesized,
            }))
            .as_arg(),
        );

        let mut should_reset = self.should_reset;

        let mut custom_hook_in_scope = Vec::new();
        for hook in custom_hook {
            let ident = match &hook {
                HookCall::Ident(ident) => Some(ident),
                HookCall::Member(Expr::Ident(ident), _) => Some(ident),
                _ => None,
            };
            if let None = ident.and_then(|id| self.scope_binding.get(&id.sym)) {
                // We don't have anything to put in the array because Hook is out of scope.
                // Since it could potentially have been edited, remount the component.
                should_reset = true;
            } else {
                custom_hook_in_scope.push(hook);
            }
        }

        if should_reset || custom_hook_in_scope.len() > 0 {
            args.push(
                Expr::Lit(Lit::Bool(Bool {
                    span: DUMMY_SP,
                    value: should_reset,
                }))
                .as_arg(),
            );
        }

        if custom_hook_in_scope.len() > 0 {
            let elems = custom_hook_in_scope
                .into_iter()
                .map(|hook| {
                    Some(ExprOrSpread {
                        spread: None,
                        expr: Box::new(match hook {
                            HookCall::Ident(ident) => Expr::Ident(ident),
                            HookCall::Member(obj, prop) => Expr::Member(MemberExpr {
                                span: DUMMY_SP,
                                obj: ExprOrSuper::Expr(Box::new(obj)),
                                prop: Box::new(Expr::Ident(prop)),
                                computed: false,
                            }),
                        }),
                    })
                })
                .collect();
            args.push(
                Expr::Fn(FnExpr {
                    ident: None,
                    function: Function {
                        is_generator: false,
                        is_async: false,
                        params: Vec::new(),
                        decorators: Vec::new(),
                        span: DUMMY_SP,
                        body: Some(BlockStmt {
                            span: DUMMY_SP,
                            stmts: vec![Stmt::Return(ReturnStmt {
                                span: DUMMY_SP,
                                arg: Some(Box::new(Expr::Array(ArrayLit {
                                    span: DUMMY_SP,
                                    elems,
                                }))),
                            })],
                        }),
                        type_params: None,
                        return_type: None,
                    },
                })
                .as_arg(),
            );
        }

        Expr::Call(CallExpr {
            span: DUMMY_SP,
            callee: ExprOrSuper::Expr(Box::new(Expr::Ident(handle))),
            args,
            type_args: None,
        })
    }

    fn gen_hook_register_stmt(&mut self, ident: Ident, sig: HookSig) {
        self.ident.push(sig.handle.clone());
        self.extra_stmt.push(Stmt::Expr(ExprStmt {
            span: DUMMY_SP,
            expr: Box::new(self.wrap_with_register(sig.handle, Expr::Ident(ident), sig.hooks)),
        }))
    }
}

impl<'a> VisitMut for HookRegister<'a> {
    noop_visit_mut_type!();

    fn visit_mut_block_stmt(&mut self, b: &mut BlockStmt) {
        let mut current_scope = IndexSet::new();

        // TODO: merge with collect_decls
        for stmt in &b.stmts {
            stmt.collect_ident(&mut current_scope);
        }
        let orig_binding = self.scope_binding.len();
        self.scope_binding.extend(current_scope);
        let current_binding = self.scope_binding.len();

        let old_ident = self.ident.take();
        let old_stmts = self.extra_stmt.take();

        let stmt_count = b.stmts.len();
        let stmts = mem::replace(&mut b.stmts, Vec::with_capacity(stmt_count));

        for mut stmt in stmts {
            stmt.visit_mut_children_with(self);
            self.scope_binding.truncate(current_binding);

            b.stmts.push(stmt);
            b.stmts.append(&mut self.extra_stmt);
        }

        if self.ident.len() > 0 {
            b.stmts.insert(0, self.gen_hook_handle())
        }

        self.scope_binding.truncate(orig_binding);
        self.ident = old_ident;
        self.extra_stmt = old_stmts;
    }

    fn visit_mut_expr(&mut self, e: &mut Expr) {
        e.visit_mut_children_with(self);

        match e {
            Expr::Fn(FnExpr {
                function: Function {
                    body: Some(body), ..
                },
                ..
            }) => {
                let sig = collect_hooks(&mut body.stmts, self.cm);

                if let Some(HookSig { handle, hooks }) = sig {
                    self.ident.push(handle.clone());
                    *e = self.wrap_with_register(handle, e.take(), hooks);
                }
            }
            Expr::Arrow(ArrowExpr { body, .. }) => {
                let sig = collect_hooks_arrow(body, self.cm);

                if let Some(HookSig { handle, hooks }) = sig {
                    self.ident.push(handle.clone());
                    *e = self.wrap_with_register(handle, e.take(), hooks);
                }
            }
            _ => (),
        }
    }

    fn visit_mut_var_decl(&mut self, n: &mut VarDecl) {
        // we don't want visit_mut_expr to mess up with function name inference
        // so intercept it here

        for decl in n.decls.iter_mut() {
            if let VarDeclarator {
                // it doesn't quite make sense for other Pat to appear here
                name: Pat::Ident(BindingIdent { id, .. }),
                init: Some(init),
                ..
            } = decl
            {
                match init.as_mut() {
                    Expr::Fn(FnExpr {
                        function:
                            Function {
                                body: Some(body), ..
                            },
                        ..
                    }) => {
                        if let Some(sig) = collect_hooks(&mut body.stmts, self.cm) {
                            self.gen_hook_register_stmt(id.clone(), sig);
                        }
                    }
                    Expr::Arrow(ArrowExpr { body, .. }) => {
                        if let Some(sig) = collect_hooks_arrow(body, self.cm) {
                            self.gen_hook_register_stmt(id.clone(), sig);
                        }
                    }
                    _ => self.visit_mut_expr(init),
                }
            } else {
                decl.visit_mut_children_with(self)
            }
        }
    }

    fn visit_mut_default_decl(&mut self, d: &mut DefaultDecl) {
        d.visit_mut_children_with(self);

        // only when expr has ident
        if let DefaultDecl::Fn(FnExpr {
            ident: Some(ident),
            function: Function {
                body: Some(body), ..
            },
        }) = d
        {
            if let Some(sig) = collect_hooks(&mut body.stmts, self.cm) {
                self.gen_hook_register_stmt(ident.clone(), sig);
            }
        }
    }

    fn visit_mut_fn_decl(&mut self, f: &mut FnDecl) {
        f.visit_mut_children_with(self);

        if let Some(body) = &mut f.function.body {
            if let Some(sig) = collect_hooks(&mut body.stmts, self.cm) {
                self.gen_hook_register_stmt(f.ident.clone(), sig);
            }
        }
    }
}

fn collect_hooks(stmts: &mut Vec<Stmt>, cm: &SourceMap) -> Option<HookSig> {
    let mut hook = HookCollector {
        state: Vec::new(),
        cm,
    };

    stmts.visit_with(&mut hook);

    if hook.state.len() > 0 {
        let sig = HookSig::new(hook.state);
        stmts.insert(0, make_call_stmt(sig.handle.clone()));

        Some(sig)
    } else {
        None
    }
}

fn collect_hooks_arrow(body: &mut BlockStmtOrExpr, cm: &SourceMap) -> Option<HookSig> {
    match body {
        BlockStmtOrExpr::BlockStmt(block) => collect_hooks(&mut block.stmts, cm),
        BlockStmtOrExpr::Expr(expr) => {
            let mut hook = HookCollector {
                state: Vec::new(),
                cm,
            };

            expr.visit_with(&mut hook);

            if hook.state.len() > 0 {
                let sig = HookSig::new(hook.state);
                *body = BlockStmtOrExpr::BlockStmt(BlockStmt {
                    span: expr.span(),
                    stmts: vec![
                        make_call_stmt(sig.handle.clone()),
                        Stmt::Return(ReturnStmt {
                            span: expr.span(),
                            arg: Some(Box::new(expr.as_mut().take())),
                        }),
                    ],
                });
                Some(sig)
            } else {
                None
            }
        }
    }
}

struct HookCollector<'a> {
    state: Vec<Hook>,
    cm: &'a SourceMap,
}

static IS_HOOK_LIKE: Lazy<Regex> = Lazy::new(|| Regex::new("^use[A-Z]").unwrap());
impl<'a> HookCollector<'a> {
    fn get_hook_from_call_expr(&self, expr: &CallExpr, lhs: Option<&Pat>) -> Option<Hook> {
        let callee = if let ExprOrSuper::Expr(callee) = &expr.callee {
            Some(callee.as_ref())
        } else {
            None
        }?;
        let mut hook_call = None;
        let ident = match callee {
            Expr::Ident(ident) => {
                hook_call = Some(HookCall::Ident(ident.clone()));
                Some(ident)
            }
            Expr::Member(MemberExpr {
                obj: ExprOrSuper::Expr(obj),
                prop,
                ..
            }) => {
                if let Expr::Ident(ident) = prop.as_ref() {
                    hook_call = Some(HookCall::Member(*obj.clone(), ident.clone()));
                    Some(ident)
                } else {
                    None
                }
            }
            _ => None,
        }?;
        let name = if IS_HOOK_LIKE.is_match(&ident.sym) {
            Some(ident)
        } else {
            None
        }?;
        let mut key = if let Some(name) = lhs {
            self.cm
                .span_to_snippet(name.span())
                .unwrap_or_else(|_| String::new())
        } else {
            String::new()
        };
        // Some built-in Hooks reset on edits to arguments.
        if &name.sym == "useState" && expr.args.len() > 0 {
            // useState first argument is initial state.
            key += &format!(
                "({})",
                self.cm
                    .span_to_snippet(expr.args[0].span())
                    .unwrap_or_else(|_| String::new())
            );
        } else if &name.sym == "useReducer" && expr.args.len() > 1 {
            // useReducer second argument is initial state.
            key += &format!(
                "({})",
                self.cm
                    .span_to_snippet(expr.args[1].span())
                    .unwrap_or("".to_string())
            );
        }

        let callee = hook_call?;
        Some(Hook { callee, key })
    }

    fn get_hook_from_expr(&self, expr: &Expr, lhs: Option<&Pat>) -> Option<Hook> {
        if let Expr::Call(call) = expr {
            self.get_hook_from_call_expr(call, lhs)
        } else {
            None
        }
    }
}

impl<'a> Visit for HookCollector<'a> {
    noop_visit_type!();

    fn visit_block_stmt_or_expr(&mut self, _: &BlockStmtOrExpr) {}

    fn visit_block_stmt(&mut self, _: &BlockStmt) {}

    fn visit_expr(&mut self, expr: &Expr) {
        expr.visit_children_with(self);

        if let Expr::Call(call) = expr {
            if let Some(hook) = self.get_hook_from_call_expr(call, None) {
                self.state.push(hook)
            }
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expr(ExprStmt { expr, .. }) => {
                if let Some(hook) = self.get_hook_from_expr(expr, None) {
                    self.state.push(hook)
                } else {
                    stmt.visit_children_with(self)
                }
            }
            Stmt::Decl(Decl::Var(var_decl)) => {
                for decl in &var_decl.decls {
                    if let Some(init) = &decl.init {
                        if let Some(hook) = self.get_hook_from_expr(init, Some(&decl.name)) {
                            self.state.push(hook)
                        } else {
                            stmt.visit_children_with(self)
                        }
                    } else {
                        stmt.visit_children_with(self)
                    }
                }
            }
            Stmt::Return(ReturnStmt { arg: Some(arg), .. }) => {
                if let Some(hook) = self.get_hook_from_expr(arg.as_ref(), None) {
                    self.state.push(hook)
                } else {
                    stmt.visit_children_with(self)
                }
            }
            _ => stmt.visit_children_with(self),
        }
    }
}
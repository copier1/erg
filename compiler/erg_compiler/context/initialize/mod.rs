//! defines type information for builtin objects (in `Context`)
//!
//! 組み込みオブジェクトの型情報を(Contextに)定義
pub mod const_func;
pub mod py_mods;

use std::path::PathBuf;

use erg_common::config::ErgConfig;
use erg_common::dict;
// use erg_common::error::Location;
use erg_common::vis::Visibility;
use erg_common::Str;
use erg_common::{set, unique_in_place};

use crate::ty::free::fresh_varname;
use crate::ty::typaram::TyParam;
use crate::ty::value::ValueObj;
use crate::ty::Type;
use crate::ty::{constructors::*, BuiltinConstSubr, ConstSubr, Predicate};
use ParamSpec as PS;
use Type::*;

use erg_parser::ast::VarName;

use crate::context::initialize::const_func::*;
use crate::context::instantiate::ConstTemplate;
use crate::context::{
    ClassDefType, Context, ContextKind, DefaultInfo, MethodType, ParamSpec, TraitInstance,
};
use crate::mod_cache::SharedModuleCache;
use crate::varinfo::{Mutability, VarInfo, VarKind};
use DefaultInfo::*;
use Mutability::*;
use VarKind::*;
use Visibility::*;

impl Context {
    fn register_builtin_decl(&mut self, name: &'static str, t: Type, vis: Visibility) {
        let impl_of = if let ContextKind::MethodDefs(Some(tr)) = &self.kind {
            Some(tr.clone())
        } else {
            None
        };
        let name = VarName::from_static(name);
        if self.decls.get(&name).is_some() {
            panic!("already registered: {name}");
        } else {
            self.decls.insert(
                name,
                VarInfo::new(t, Immutable, vis, Builtin, None, impl_of),
            );
        }
    }

    fn register_builtin_impl(
        &mut self,
        name: &'static str,
        t: Type,
        muty: Mutability,
        vis: Visibility,
    ) {
        let impl_of = if let ContextKind::MethodDefs(Some(tr)) = &self.kind {
            Some(tr.clone())
        } else {
            None
        };
        let name = VarName::from_static(name);
        if self.locals.get(&name).is_some() {
            panic!("already registered: {name}");
        } else {
            self.locals
                .insert(name, VarInfo::new(t, muty, vis, Builtin, None, impl_of));
        }
    }

    fn register_builtin_immutable_private_var(&mut self, name: &'static str, t: Type) {
        self.register_builtin_impl(name, t, Immutable, Private)
    }

    fn register_builtin_const(&mut self, name: &str, vis: Visibility, obj: ValueObj) {
        if self.rec_get_const_obj(name).is_some() {
            panic!("already registered: {name}");
        } else {
            let impl_of = if let ContextKind::MethodDefs(Some(tr)) = &self.kind {
                Some(tr.clone())
            } else {
                None
            };
            // TODO: not all value objects are comparable
            let vi = VarInfo::new(
                v_enum(set! {obj.clone()}),
                Const,
                vis,
                Builtin,
                None,
                impl_of,
            );
            self.consts.insert(VarName::from_str(Str::rc(name)), obj);
            self.locals.insert(VarName::from_str(Str::rc(name)), vi);
        }
    }

    fn register_const_param_defaults(&mut self, name: &'static str, params: Vec<ConstTemplate>) {
        if self.const_param_defaults.get(name).is_some() {
            panic!("already registered: {name}");
        } else {
            self.const_param_defaults.insert(Str::ever(name), params);
        }
    }

    /// FIXME: トレイトの汎化型を指定するのにも使っているので、この名前は適当でない
    pub(crate) fn register_superclass(&mut self, sup: Type, sup_ctx: &Context) {
        self.super_classes.push(sup);
        self.super_classes.extend(sup_ctx.super_classes.clone());
        self.super_traits.extend(sup_ctx.super_traits.clone());
        unique_in_place(&mut self.super_classes);
        unique_in_place(&mut self.super_traits);
    }

    pub(crate) fn register_supertrait(&mut self, sup: Type, sup_ctx: &Context) {
        self.super_traits.push(sup);
        self.super_traits.extend(sup_ctx.super_traits.clone());
        unique_in_place(&mut self.super_traits);
    }

    fn register_builtin_type(&mut self, t: Type, ctx: Self, vis: Visibility, muty: Mutability) {
        if t.typarams_len().is_none() {
            self.register_mono_type(t, ctx, vis, muty);
        } else {
            self.register_poly_type(t, ctx, vis, muty);
        }
    }

    fn register_mono_type(&mut self, t: Type, ctx: Self, vis: Visibility, muty: Mutability) {
        if self.rec_get_mono_type(&t.local_name()).is_some() {
            panic!("{} has already been registered", t.local_name());
        } else if self.rec_get_const_obj(&t.local_name()).is_some() {
            panic!("{} has already been registered as const", t.local_name());
        } else {
            let name = VarName::from_str(t.local_name());
            let meta_t = match ctx.kind {
                ContextKind::Class => Type::ClassType,
                ContextKind::Trait => Type::TraitType,
                _ => Type::Type,
            };
            self.locals.insert(
                name.clone(),
                VarInfo::new(meta_t, muty, vis, Builtin, None, None),
            );
            self.consts
                .insert(name.clone(), ValueObj::builtin_t(t.clone()));
            for impl_trait in ctx.super_traits.iter() {
                if let Some(impls) = self.trait_impls.get_mut(&impl_trait.qual_name()) {
                    impls.insert(TraitInstance::new(t.clone(), impl_trait.clone()));
                } else {
                    self.trait_impls.insert(
                        impl_trait.qual_name(),
                        set![TraitInstance::new(t.clone(), impl_trait.clone())],
                    );
                }
            }
            for (trait_method, vi) in ctx.decls.iter() {
                if let Some(types) = self.method_to_traits.get_mut(trait_method.inspect()) {
                    types.push(MethodType::new(t.clone(), vi.t.clone()));
                } else {
                    self.method_to_traits.insert(
                        trait_method.inspect().clone(),
                        vec![MethodType::new(t.clone(), vi.t.clone())],
                    );
                }
            }
            for (class_method, vi) in ctx.locals.iter() {
                if let Some(types) = self.method_to_classes.get_mut(class_method.inspect()) {
                    types.push(MethodType::new(t.clone(), vi.t.clone()));
                } else {
                    self.method_to_classes.insert(
                        class_method.inspect().clone(),
                        vec![MethodType::new(t.clone(), vi.t.clone())],
                    );
                }
            }
            self.mono_types.insert(name, (t, ctx));
        }
    }

    // FIXME: MethodDefsと再代入は違う
    fn register_poly_type(&mut self, t: Type, ctx: Self, vis: Visibility, muty: Mutability) {
        // FIXME: panic
        if let Some((_, root_ctx)) = self.poly_types.get_mut(&t.local_name()) {
            root_ctx.methods_list.push((ClassDefType::Simple(t), ctx));
        } else {
            let name = VarName::from_str(t.local_name());
            let meta_t = match ctx.kind {
                ContextKind::Class => Type::ClassType,
                ContextKind::Trait => Type::TraitType,
                _ => Type::Type,
            };
            self.locals.insert(
                name.clone(),
                VarInfo::new(meta_t, muty, vis, Builtin, None, None),
            );
            self.consts
                .insert(name.clone(), ValueObj::builtin_t(t.clone()));
            for impl_trait in ctx.super_traits.iter() {
                if let Some(impls) = self.trait_impls.get_mut(&impl_trait.qual_name()) {
                    impls.insert(TraitInstance::new(t.clone(), impl_trait.clone()));
                } else {
                    self.trait_impls.insert(
                        impl_trait.qual_name(),
                        set![TraitInstance::new(t.clone(), impl_trait.clone())],
                    );
                }
            }
            for (trait_method, vi) in ctx.decls.iter() {
                if let Some(traits) = self.method_to_traits.get_mut(trait_method.inspect()) {
                    traits.push(MethodType::new(t.clone(), vi.t.clone()));
                } else {
                    self.method_to_traits.insert(
                        trait_method.inspect().clone(),
                        vec![MethodType::new(t.clone(), vi.t.clone())],
                    );
                }
            }
            for (class_method, vi) in ctx.locals.iter() {
                if let Some(types) = self.method_to_classes.get_mut(class_method.inspect()) {
                    types.push(MethodType::new(t.clone(), vi.t.clone()));
                } else {
                    self.method_to_classes.insert(
                        class_method.inspect().clone(),
                        vec![MethodType::new(t.clone(), vi.t.clone())],
                    );
                }
            }
            self.poly_types.insert(name, (t, ctx));
        }
    }

    fn register_builtin_patch(
        &mut self,
        name: &'static str,
        ctx: Self,
        vis: Visibility,
        muty: Mutability,
    ) {
        if self.patches.contains_key(name) {
            panic!("{} has already been registered", name);
        } else {
            let name = VarName::from_static(name);
            self.locals.insert(
                name.clone(),
                VarInfo::new(Patch, muty, vis, Builtin, None, None),
            );
            for method_name in ctx.locals.keys() {
                if let Some(patches) = self.method_impl_patches.get_mut(method_name) {
                    patches.push(name.clone());
                } else {
                    self.method_impl_patches
                        .insert(method_name.clone(), vec![name.clone()]);
                }
            }
            self.patches.insert(name, ctx);
        }
    }

    fn init_builtin_consts(&mut self) {
        // TODO: this is not a const, but a special property
        self.register_builtin_immutable_private_var("__name__", Str);
        self.register_builtin_immutable_private_var("license", mono("_sitebuiltins._Printer"));
        self.register_builtin_immutable_private_var("credits", mono("_sitebuiltins._Printer"));
        self.register_builtin_immutable_private_var("copyright", mono("_sitebuiltins._Printer"));
    }

    /// see std/prelude.er
    /// All type boundaries are defined in each subroutine
    /// `push_subtype_bound`, etc. are used for type boundary determination in user-defined APIs
    // 型境界はすべて各サブルーチンで定義する
    // push_subtype_boundなどはユーザー定義APIの型境界決定のために使用する
    fn init_builtin_traits(&mut self) {
        let unpack = Self::builtin_mono_trait("Unpack", 2);
        let inheritable_type = Self::builtin_mono_trait("InheritableType", 2);
        let named = Self::builtin_mono_trait("Named", 2);
        let mut mutable = Self::builtin_mono_trait("Mutable", 2);
        let immut_t = proj(mono_q("Self"), "ImmutType");
        let f_t = func(vec![kw("old", immut_t.clone())], None, vec![], immut_t);
        let t = pr1_met(ref_mut(mono_q("Self"), None), f_t, NoneType);
        let t = quant(t, set! { subtypeof(mono_q("Self"), mono("Immutizable")) });
        mutable.register_builtin_decl("update!", t, Public);
        // REVIEW: Immutatable?
        let mut immutizable = Self::builtin_mono_trait("Immutizable", 2);
        immutizable.register_superclass(mono("Mutable"), &mutable);
        immutizable.register_builtin_decl("ImmutType", Type, Public);
        // REVIEW: Mutatable?
        let mut mutizable = Self::builtin_mono_trait("Mutizable", 2);
        mutizable.register_builtin_decl("MutType!", Type, Public);
        let pathlike = Self::builtin_mono_trait("PathLike", 2);
        /* Readable */
        let mut readable = Self::builtin_mono_trait("Readable!", 2);
        let t_read = pr_met(
            ref_mut(mono_q("Self"), None),
            vec![],
            None,
            vec![kw("n", Int)],
            Str,
        );
        let t_read = quant(
            t_read,
            set! { subtypeof(mono_q("Self"), mono("Readable!")) },
        );
        readable.register_builtin_decl("read!", t_read, Public);
        /* Writable */
        let mut writable = Self::builtin_mono_trait("Writable!", 2);
        let t_write = pr1_kw_met(ref_mut(mono_q("Self"), None), kw("s", Str), Nat);
        let t_write = quant(
            t_write,
            set! { subtypeof(mono_q("Self"), mono("Writable!")) },
        );
        writable.register_builtin_decl("write!", t_write, Public);
        /* Show */
        let mut show = Self::builtin_mono_trait("Show", 2);
        let t_show = fn0_met(ref_(mono_q("Self")), Str);
        let t_show = quant(t_show, set! { subtypeof(mono_q("Self"), mono("Show")) });
        show.register_builtin_decl("to_str", t_show, Public);
        /* In */
        let mut in_ = Self::builtin_poly_trait("In", vec![PS::t("T", NonDefault)], 2);
        let params = vec![PS::t("T", NonDefault)];
        let input = Self::builtin_poly_trait("Input", params.clone(), 2);
        let output = Self::builtin_poly_trait("Output", params, 2);
        in_.register_superclass(poly("Input", vec![ty_tp(mono_q("T"))]), &input);
        let op_t = fn1_met(mono_q("T"), mono_q("I"), Bool);
        let op_t = quant(
            op_t,
            set! { static_instance("T", Type), subtypeof(mono_q("I"), poly("In", vec![ty_tp(mono_q("T"))])) },
        );
        in_.register_builtin_decl("__in__", op_t, Public);
        /* Eq */
        // Erg does not have a trait equivalent to `PartialEq` in Rust
        // This means, Erg's `Float` cannot be compared with other `Float`
        // use `l - r < EPSILON` to check if two floats are almost equal
        let mut eq = Self::builtin_poly_trait("Eq", vec![PS::t("R", WithDefault)], 2);
        eq.register_superclass(poly("Output", vec![ty_tp(mono_q("R"))]), &output);
        // __eq__: |Self <: Eq()| Self.(Self) -> Bool
        let op_t = fn1_met(mono_q("Self"), mono_q("R"), Bool);
        let op_t = quant(
            op_t,
            set! {
                subtypeof(mono_q("Self"), poly("Eq", vec![ty_tp(mono_q("R"))])),
                static_instance("R", Type)
            },
        );
        eq.register_builtin_decl("__eq__", op_t, Public);
        /* Partial_ord */
        let mut partial_ord =
            Self::builtin_poly_trait("PartialOrd", vec![PS::t("R", WithDefault)], 2);
        partial_ord.register_superclass(poly("Eq", vec![ty_tp(mono_q("R"))]), &eq);
        let op_t = fn1_met(mono_q("Self"), mono_q("R"), or(mono("Ordering"), NoneType));
        let op_t = quant(
            op_t,
            set! {
                subtypeof(mono_q("Self"), poly("PartialOrd", vec![ty_tp(mono_q("R"))])),
                static_instance("R", Type)
            },
        );
        partial_ord.register_builtin_decl("__partial_cmp__", op_t, Public);
        /* Ord */
        let mut ord = Self::builtin_mono_trait("Ord", 2);
        ord.register_superclass(poly("Eq", vec![ty_tp(mono("Self"))]), &eq);
        ord.register_superclass(poly("PartialOrd", vec![ty_tp(mono("Self"))]), &partial_ord);
        // FIXME: poly trait
        /* Num */
        let num = Self::builtin_mono_trait("Num", 2);
        /* vec![
            poly("Add", vec![]),
            poly("Sub", vec![]),
            poly("Mul", vec![]),
        ], */
        /* Seq */
        let mut seq = Self::builtin_poly_trait("Seq", vec![PS::t("T", NonDefault)], 2);
        seq.register_superclass(poly("Output", vec![ty_tp(mono_q("T"))]), &output);
        let self_t = mono_q("Self");
        let t = fn0_met(self_t.clone(), Nat);
        let t = quant(
            t,
            set! {subtypeof(self_t.clone(), poly("Seq", vec![TyParam::erased(Type)]))},
        );
        seq.register_builtin_decl("len", t, Public);
        let t = fn1_met(self_t.clone(), Nat, mono_q("T"));
        let t = quant(
            t,
            set! {subtypeof(self_t, poly("Seq", vec![ty_tp(mono_q("T"))])), static_instance("T", Type)},
        );
        // Seq.get: |Self <: Seq(T)| Self.(Nat) -> T
        seq.register_builtin_decl("get", t, Public);
        /* Iterable */
        let mut iterable = Self::builtin_poly_trait("Iterable", vec![PS::t("T", NonDefault)], 2);
        iterable.register_superclass(poly("Output", vec![ty_tp(mono_q("T"))]), &output);
        let self_t = mono_q("Self");
        let t = fn0_met(self_t.clone(), proj(self_t.clone(), "Iter"));
        let t = quant(
            t,
            set! {subtypeof(self_t, poly("Iterable", vec![ty_tp(mono_q("T"))]))},
        );
        iterable.register_builtin_decl("iter", t, Public);
        iterable.register_builtin_decl("Iter", Type, Public);
        let r = mono_q("R");
        let r_bound = static_instance("R", Type);
        let params = vec![PS::t("R", WithDefault)];
        let ty_params = vec![ty_tp(mono_q("R"))];
        /* Num */
        let mut add = Self::builtin_poly_trait("Add", params.clone(), 2);
        // Rについて共変(__add__の型とは関係ない)
        add.register_superclass(poly("Output", vec![ty_tp(mono_q("R"))]), &output);
        let self_bound = subtypeof(mono_q("Self"), poly("Add", ty_params.clone()));
        let op_t = fn1_met(mono_q("Self"), r.clone(), proj(mono_q("Self"), "Output"));
        let op_t = quant(op_t, set! {r_bound.clone(), self_bound});
        add.register_builtin_decl("__add__", op_t, Public);
        add.register_builtin_decl("Output", Type, Public);
        /* Sub */
        let mut sub = Self::builtin_poly_trait("Sub", params.clone(), 2);
        sub.register_superclass(poly("Output", vec![ty_tp(mono_q("R"))]), &output);
        let op_t = fn1_met(mono_q("Self"), r.clone(), proj(mono_q("Self"), "Output"));
        let self_bound = subtypeof(mono_q("Self"), poly("Sub", ty_params.clone()));
        let op_t = quant(op_t, set! {r_bound.clone(), self_bound});
        sub.register_builtin_decl("__sub__", op_t, Public);
        sub.register_builtin_decl("Output", Type, Public);
        /* Mul */
        let mut mul = Self::builtin_poly_trait("Mul", params.clone(), 2);
        mul.register_superclass(poly("Output", vec![ty_tp(mono_q("R"))]), &output);
        let op_t = fn1_met(mono_q("Self"), r.clone(), proj(mono_q("Self"), "Output"));
        let self_bound = subtypeof(mono_q("Self"), poly("Mul", ty_params.clone()));
        let op_t = quant(op_t, set! {r_bound.clone(), self_bound});
        mul.register_builtin_decl("__mul__", op_t, Public);
        mul.register_builtin_decl("Output", Type, Public);
        /* Div */
        let mut div = Self::builtin_poly_trait("Div", params.clone(), 2);
        div.register_superclass(poly("Output", vec![ty_tp(mono_q("R"))]), &output);
        let op_t = fn1_met(mono_q("Self"), r.clone(), proj(mono_q("Self"), "Output"));
        let self_bound = subtypeof(mono_q("Self"), poly("Div", ty_params.clone()));
        let op_t = quant(op_t, set! {r_bound.clone(), self_bound});
        div.register_builtin_decl("__div__", op_t, Public);
        div.register_builtin_decl("Output", Type, Public);
        /* FloorDiv */
        let mut floor_div = Self::builtin_poly_trait("FloorDiv", params, 2);
        floor_div.register_superclass(poly("Output", vec![ty_tp(mono_q("R"))]), &output);
        let op_t = fn1_met(mono_q("Self"), r, proj(mono_q("Self"), "Output"));
        let self_bound = subtypeof(mono_q("Self"), poly("FloorDiv", ty_params.clone()));
        let op_t = quant(op_t, set! {r_bound, self_bound});
        floor_div.register_builtin_decl("__floordiv__", op_t, Public);
        floor_div.register_builtin_decl("Output", Type, Public);
        self.register_builtin_type(mono("Unpack"), unpack, Private, Const);
        self.register_builtin_type(mono("InheritableType"), inheritable_type, Private, Const);
        self.register_builtin_type(mono("Named"), named, Private, Const);
        self.register_builtin_type(mono("Mutable"), mutable, Private, Const);
        self.register_builtin_type(mono("Immutizable"), immutizable, Private, Const);
        self.register_builtin_type(mono("Mutizable"), mutizable, Private, Const);
        self.register_builtin_type(mono("PathLike"), pathlike, Private, Const);
        self.register_builtin_type(mono("Readable!"), readable, Private, Const);
        self.register_builtin_type(mono("Writable!"), writable, Private, Const);
        self.register_builtin_type(mono("Show"), show, Private, Const);
        self.register_builtin_type(
            poly("Input", vec![ty_tp(mono_q("T"))]),
            input,
            Private,
            Const,
        );
        self.register_builtin_type(
            poly("Output", vec![ty_tp(mono_q("T"))]),
            output,
            Private,
            Const,
        );
        self.register_builtin_type(poly("In", vec![ty_tp(mono_q("T"))]), in_, Private, Const);
        self.register_builtin_type(poly("Eq", vec![ty_tp(mono_q("R"))]), eq, Private, Const);
        self.register_builtin_type(
            poly("PartialOrd", vec![ty_tp(mono_q("R"))]),
            partial_ord,
            Private,
            Const,
        );
        self.register_builtin_type(mono("Ord"), ord, Private, Const);
        self.register_builtin_type(mono("Num"), num, Private, Const);
        self.register_builtin_type(poly("Seq", vec![ty_tp(mono_q("T"))]), seq, Private, Const);
        self.register_builtin_type(
            poly("Iterable", vec![ty_tp(mono_q("T"))]),
            iterable,
            Private,
            Const,
        );
        self.register_builtin_type(poly("Add", ty_params.clone()), add, Private, Const);
        self.register_builtin_type(poly("Sub", ty_params.clone()), sub, Private, Const);
        self.register_builtin_type(poly("Mul", ty_params.clone()), mul, Private, Const);
        self.register_builtin_type(poly("Div", ty_params.clone()), div, Private, Const);
        self.register_builtin_type(poly("FloorDiv", ty_params), floor_div, Private, Const);
        self.register_const_param_defaults(
            "Eq",
            vec![ConstTemplate::Obj(ValueObj::builtin_t(mono_q("Self")))],
        );
        self.register_const_param_defaults(
            "PartialOrd",
            vec![ConstTemplate::app("Self", vec![], vec![])],
        );
        self.register_const_param_defaults(
            "Add",
            vec![ConstTemplate::Obj(ValueObj::builtin_t(mono_q("Self")))],
        );
        self.register_const_param_defaults(
            "Sub",
            vec![ConstTemplate::Obj(ValueObj::builtin_t(mono_q("Self")))],
        );
        self.register_const_param_defaults(
            "Mul",
            vec![ConstTemplate::Obj(ValueObj::builtin_t(mono_q("Self")))],
        );
        self.register_const_param_defaults(
            "Div",
            vec![ConstTemplate::Obj(ValueObj::builtin_t(mono_q("Self")))],
        );
        self.register_const_param_defaults(
            "FloorDiv",
            vec![ConstTemplate::Obj(ValueObj::builtin_t(mono_q("Self")))],
        );
    }

    fn init_builtin_classes(&mut self) {
        /* Obj */
        let mut obj = Self::builtin_mono_class("Obj", 2);
        let t = fn0_met(mono_q("Self"), mono_q("Self"));
        let t = quant(t, set! {subtypeof(mono_q("Self"), Obj)});
        obj.register_builtin_impl("clone", t, Const, Public);
        obj.register_builtin_impl("__module__", Str, Const, Public);
        obj.register_builtin_impl("__sizeof__", fn0_met(Obj, Nat), Const, Public);
        obj.register_builtin_impl("__repr__", fn0_met(Obj, Str), Immutable, Public);
        obj.register_builtin_impl("__str__", fn0_met(Obj, Str), Immutable, Public);
        obj.register_builtin_impl(
            "__dict__",
            fn0_met(Obj, dict! {Str => Obj}.into()),
            Immutable,
            Public,
        );
        obj.register_builtin_impl("__bytes__", fn0_met(Obj, mono("Bytes")), Immutable, Public);
        let mut obj_in = Self::builtin_methods(Some(poly("In", vec![ty_tp(Type)])), 2);
        obj_in.register_builtin_impl("__in__", fn1_met(Obj, Type, Bool), Const, Public);
        obj.register_trait(Obj, obj_in);
        let mut obj_mutizable = Self::builtin_methods(Some(mono("Mutizable")), 1);
        obj_mutizable.register_builtin_const("MutType!", Public, ValueObj::builtin_t(mono("Obj!")));
        obj.register_trait(Obj, obj_mutizable);
        // Obj does not implement Eq

        /* Float */
        let mut float = Self::builtin_mono_class("Float", 2);
        float.register_superclass(Obj, &obj);
        // TODO: support multi platform
        float.register_builtin_const("EPSILON", Public, ValueObj::Float(2.220446049250313e-16));
        float.register_builtin_impl("Real", Float, Const, Public);
        float.register_builtin_impl("Imag", Float, Const, Public);
        float.register_marker_trait(mono("Num"));
        float.register_marker_trait(mono("Ord"));
        let mut float_partial_ord =
            Self::builtin_methods(Some(poly("PartialOrd", vec![ty_tp(Float)])), 2);
        float_partial_ord.register_builtin_impl(
            "__cmp__",
            fn1_met(Float, Float, mono("Ordering")),
            Const,
            Public,
        );
        float.register_trait(Float, float_partial_ord);
        // Float doesn't have an `Eq` implementation
        let op_t = fn1_met(Float, Float, Float);
        let mut float_add = Self::builtin_methods(Some(poly("Add", vec![ty_tp(Float)])), 2);
        float_add.register_builtin_impl("__add__", op_t.clone(), Const, Public);
        float_add.register_builtin_const("Output", Public, ValueObj::builtin_t(Float));
        float.register_trait(Float, float_add);
        let mut float_sub = Self::builtin_methods(Some(poly("Sub", vec![ty_tp(Float)])), 2);
        float_sub.register_builtin_impl("__sub__", op_t.clone(), Const, Public);
        float_sub.register_builtin_const("Output", Public, ValueObj::builtin_t(Float));
        float.register_trait(Float, float_sub);
        let mut float_mul = Self::builtin_methods(Some(poly("Mul", vec![ty_tp(Float)])), 2);
        float_mul.register_builtin_impl("__mul__", op_t.clone(), Const, Public);
        float_mul.register_builtin_const("Output", Public, ValueObj::builtin_t(Float));
        float_mul.register_builtin_const("PowOutput", Public, ValueObj::builtin_t(Float));
        float.register_trait(Float, float_mul);
        let mut float_div = Self::builtin_methods(Some(poly("Div", vec![ty_tp(Float)])), 2);
        float_div.register_builtin_impl("__div__", op_t.clone(), Const, Public);
        float_div.register_builtin_const("Output", Public, ValueObj::builtin_t(Float));
        float_div.register_builtin_const("ModOutput", Public, ValueObj::builtin_t(Float));
        float.register_trait(Float, float_div);
        let mut float_floordiv =
            Self::builtin_methods(Some(poly("FloorDiv", vec![ty_tp(Float)])), 2);
        float_floordiv.register_builtin_impl("__floordiv__", op_t, Const, Public);
        float_floordiv.register_builtin_const("Output", Public, ValueObj::builtin_t(Float));
        float.register_trait(Float, float_floordiv);
        let mut float_mutizable = Self::builtin_methods(Some(mono("Mutizable")), 2);
        float_mutizable.register_builtin_const(
            "MutType!",
            Public,
            ValueObj::builtin_t(mono("Float!")),
        );
        float.register_trait(Float, float_mutizable);
        let mut float_show = Self::builtin_methods(Some(mono("Show")), 1);
        let t = fn0_met(Float, Str);
        float_show.register_builtin_impl("to_str", t, Immutable, Public);
        float.register_trait(Float, float_show);

        /* Ratio */
        // TODO: Int, Nat, Boolの継承元をRatioにする(今はFloat)
        let mut ratio = Self::builtin_mono_class("Ratio", 2);
        ratio.register_superclass(Obj, &obj);
        ratio.register_builtin_impl("Real", Ratio, Const, Public);
        ratio.register_builtin_impl("Imag", Ratio, Const, Public);
        ratio.register_marker_trait(mono("Num"));
        ratio.register_marker_trait(mono("Ord"));
        let mut ratio_partial_ord =
            Self::builtin_methods(Some(poly("PartialOrd", vec![ty_tp(Ratio)])), 2);
        ratio_partial_ord.register_builtin_impl(
            "__cmp__",
            fn1_met(Ratio, Ratio, mono("Ordering")),
            Const,
            Public,
        );
        ratio.register_trait(Ratio, ratio_partial_ord);
        let mut ratio_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(Ratio)])), 2);
        ratio_eq.register_builtin_impl("__eq__", fn1_met(Ratio, Ratio, Bool), Const, Public);
        ratio.register_trait(Ratio, ratio_eq);
        let op_t = fn1_met(Ratio, Ratio, Ratio);
        let mut ratio_add = Self::builtin_methods(Some(poly("Add", vec![ty_tp(Ratio)])), 2);
        ratio_add.register_builtin_impl("__add__", op_t.clone(), Const, Public);
        ratio_add.register_builtin_const("Output", Public, ValueObj::builtin_t(Ratio));
        ratio.register_trait(Ratio, ratio_add);
        let mut ratio_sub = Self::builtin_methods(Some(poly("Sub", vec![ty_tp(Ratio)])), 2);
        ratio_sub.register_builtin_impl("__sub__", op_t.clone(), Const, Public);
        ratio_sub.register_builtin_const("Output", Public, ValueObj::builtin_t(Ratio));
        ratio.register_trait(Ratio, ratio_sub);
        let mut ratio_mul = Self::builtin_methods(Some(poly("Mul", vec![ty_tp(Ratio)])), 2);
        ratio_mul.register_builtin_impl("__mul__", op_t.clone(), Const, Public);
        ratio_mul.register_builtin_const("Output", Public, ValueObj::builtin_t(Ratio));
        ratio_mul.register_builtin_const("PowOutput", Public, ValueObj::builtin_t(Ratio));
        ratio.register_trait(Ratio, ratio_mul);
        let mut ratio_div = Self::builtin_methods(Some(poly("Div", vec![ty_tp(Ratio)])), 2);
        ratio_div.register_builtin_impl("__div__", op_t.clone(), Const, Public);
        ratio_div.register_builtin_const("Output", Public, ValueObj::builtin_t(Ratio));
        ratio_div.register_builtin_const("ModOutput", Public, ValueObj::builtin_t(Ratio));
        ratio.register_trait(Ratio, ratio_div);
        let mut ratio_floordiv =
            Self::builtin_methods(Some(poly("FloorDiv", vec![ty_tp(Ratio)])), 2);
        ratio_floordiv.register_builtin_impl("__floordiv__", op_t, Const, Public);
        ratio_floordiv.register_builtin_const("Output", Public, ValueObj::builtin_t(Ratio));
        ratio.register_trait(Ratio, ratio_floordiv);
        let mut ratio_mutizable = Self::builtin_methods(Some(mono("Mutizable")), 2);
        ratio_mutizable.register_builtin_const(
            "MutType!",
            Public,
            ValueObj::builtin_t(mono("Ratio!")),
        );
        ratio.register_trait(Ratio, ratio_mutizable);
        let mut ratio_show = Self::builtin_methods(Some(mono("Show")), 1);
        let t = fn0_met(Ratio, Str);
        ratio_show.register_builtin_impl("to_str", t, Immutable, Public);
        ratio.register_trait(Ratio, ratio_show);

        /* Int */
        let mut int = Self::builtin_mono_class("Int", 2);
        int.register_superclass(Float, &float); // TODO: Float -> Ratio
        int.register_marker_trait(mono("Num"));
        int.register_marker_trait(mono("Ord"));
        int.register_marker_trait(poly("Eq", vec![ty_tp(Int)]));
        // class("Rational"),
        // class("Integral"),
        int.register_builtin_impl("abs", fn0_met(Int, Nat), Immutable, Public);
        let mut int_partial_ord =
            Self::builtin_methods(Some(poly("PartialOrd", vec![ty_tp(Int)])), 2);
        int_partial_ord.register_builtin_impl(
            "__partial_cmp__",
            fn1_met(Int, Int, or(mono("Ordering"), NoneType)),
            Const,
            Public,
        );
        int.register_trait(Int, int_partial_ord);
        let mut int_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(Int)])), 2);
        int_eq.register_builtin_impl("__eq__", fn1_met(Int, Int, Bool), Const, Public);
        int.register_trait(Int, int_eq);
        // __div__ is not included in Int (cast to Ratio)
        let op_t = fn1_met(Int, Int, Int);
        let mut int_add = Self::builtin_methods(Some(poly("Add", vec![ty_tp(Int)])), 2);
        int_add.register_builtin_impl("__add__", op_t.clone(), Const, Public);
        int_add.register_builtin_const("Output", Public, ValueObj::builtin_t(Int));
        int.register_trait(Int, int_add);
        let mut int_sub = Self::builtin_methods(Some(poly("Sub", vec![ty_tp(Int)])), 2);
        int_sub.register_builtin_impl("__sub__", op_t.clone(), Const, Public);
        int_sub.register_builtin_const("Output", Public, ValueObj::builtin_t(Int));
        int.register_trait(Int, int_sub);
        let mut int_mul = Self::builtin_methods(Some(poly("Mul", vec![ty_tp(Int)])), 2);
        int_mul.register_builtin_impl("__mul__", op_t.clone(), Const, Public);
        int_mul.register_builtin_const("Output", Public, ValueObj::builtin_t(Int));
        int_mul.register_builtin_const("PowOutput", Public, ValueObj::builtin_t(Nat));
        int.register_trait(Int, int_mul);
        let mut int_floordiv = Self::builtin_methods(Some(poly("FloorDiv", vec![ty_tp(Int)])), 2);
        int_floordiv.register_builtin_impl("__floordiv__", op_t, Const, Public);
        int_floordiv.register_builtin_const("Output", Public, ValueObj::builtin_t(Int));
        int.register_trait(Int, int_floordiv);
        let mut int_mutizable = Self::builtin_methods(Some(mono("Mutizable")), 2);
        int_mutizable.register_builtin_const("MutType!", Public, ValueObj::builtin_t(mono("Int!")));
        int.register_trait(Int, int_mutizable);
        let mut int_show = Self::builtin_methods(Some(mono("Show")), 1);
        let t = fn0_met(Int, Str);
        int_show.register_builtin_impl("to_str", t, Immutable, Public);
        int.register_trait(Int, int_show);
        int.register_builtin_impl("Real", Int, Const, Public);
        int.register_builtin_impl("Imag", Int, Const, Public);

        /* Nat */
        let mut nat = Self::builtin_mono_class("Nat", 10);
        nat.register_superclass(Int, &int);
        // class("Rational"),
        // class("Integral"),
        nat.register_builtin_impl(
            "times!",
            pr_met(
                Nat,
                vec![kw("p", nd_proc(vec![], None, NoneType))],
                None,
                vec![],
                NoneType,
            ),
            Immutable,
            Public,
        );
        nat.register_marker_trait(mono("Num"));
        nat.register_marker_trait(mono("Ord"));
        let mut nat_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(Nat)])), 2);
        nat_eq.register_builtin_impl("__eq__", fn1_met(Nat, Nat, Bool), Const, Public);
        nat.register_trait(Nat, nat_eq);
        let mut nat_partial_ord =
            Self::builtin_methods(Some(poly("PartialOrd", vec![ty_tp(Nat)])), 2);
        nat_partial_ord.register_builtin_impl(
            "__cmp__",
            fn1_met(Nat, Nat, mono("Ordering")),
            Const,
            Public,
        );
        nat.register_trait(Nat, nat_partial_ord);
        // __sub__, __div__ is not included in Nat (cast to Int/ Ratio)
        let op_t = fn1_met(Nat, Nat, Nat);
        let mut nat_add = Self::builtin_methods(Some(poly("Add", vec![ty_tp(Nat)])), 2);
        nat_add.register_builtin_impl("__add__", op_t.clone(), Const, Public);
        nat_add.register_builtin_const("Output", Public, ValueObj::builtin_t(Nat));
        nat.register_trait(Nat, nat_add);
        let mut nat_mul = Self::builtin_methods(Some(poly("Mul", vec![ty_tp(Nat)])), 2);
        nat_mul.register_builtin_impl("__mul__", op_t.clone(), Const, Public);
        nat_mul.register_builtin_const("Output", Public, ValueObj::builtin_t(Nat));
        nat.register_trait(Nat, nat_mul);
        let mut nat_floordiv = Self::builtin_methods(Some(poly("FloorDiv", vec![ty_tp(Nat)])), 2);
        nat_floordiv.register_builtin_impl("__floordiv__", op_t, Const, Public);
        nat_floordiv.register_builtin_const("Output", Public, ValueObj::builtin_t(Nat));
        nat.register_trait(Nat, nat_floordiv);
        let mut nat_mutizable = Self::builtin_methods(Some(mono("Mutizable")), 2);
        nat_mutizable.register_builtin_const("MutType!", Public, ValueObj::builtin_t(mono("Nat!")));
        nat.register_trait(Nat, nat_mutizable);
        nat.register_builtin_impl("Real", Nat, Const, Public);
        nat.register_builtin_impl("Imag", Nat, Const, Public);

        /* Bool */
        let mut bool_ = Self::builtin_mono_class("Bool", 10);
        bool_.register_superclass(Nat, &nat);
        // class("Rational"),
        // class("Integral"),
        // TODO: And, Or trait
        bool_.register_builtin_impl("__and__", fn1_met(Bool, Bool, Bool), Const, Public);
        bool_.register_builtin_impl("__or__", fn1_met(Bool, Bool, Bool), Const, Public);
        bool_.register_marker_trait(mono("Num"));
        bool_.register_marker_trait(mono("Ord"));
        let mut bool_partial_ord =
            Self::builtin_methods(Some(poly("PartialOrd", vec![ty_tp(Bool)])), 2);
        bool_partial_ord.register_builtin_impl(
            "__cmp__",
            fn1_met(Bool, Bool, mono("Ordering")),
            Const,
            Public,
        );
        bool_.register_trait(Bool, bool_partial_ord);
        let mut bool_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(Bool)])), 2);
        bool_eq.register_builtin_impl("__eq__", fn1_met(Bool, Bool, Bool), Const, Public);
        bool_.register_trait(Bool, bool_eq);
        let mut bool_mutizable = Self::builtin_methods(Some(mono("Mutizable")), 2);
        bool_mutizable.register_builtin_const(
            "MutType!",
            Public,
            ValueObj::builtin_t(mono("Bool!")),
        );
        bool_.register_trait(Bool, bool_mutizable);
        let mut bool_show = Self::builtin_methods(Some(mono("Show")), 1);
        bool_show.register_builtin_impl("to_str", fn0_met(Bool, Str), Immutable, Public);
        bool_.register_trait(Bool, bool_show);
        /* Str */
        let mut str_ = Self::builtin_mono_class("Str", 10);
        str_.register_superclass(Obj, &obj);
        str_.register_marker_trait(mono("Ord"));
        str_.register_marker_trait(mono("PathLike"));
        str_.register_builtin_impl(
            "replace",
            fn_met(
                Str,
                vec![kw("pat", Str), kw("into", Str)],
                None,
                vec![],
                Str,
            ),
            Immutable,
            Public,
        );
        str_.register_builtin_impl(
            "encode",
            fn_met(
                Str,
                vec![],
                None,
                vec![kw("encoding", Str), kw("errors", Str)],
                mono("Bytes"),
            ),
            Immutable,
            Public,
        );
        let mut str_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(Str)])), 2);
        str_eq.register_builtin_impl("__eq__", fn1_met(Str, Str, Bool), Const, Public);
        str_.register_trait(Str, str_eq);
        let mut str_seq = Self::builtin_methods(Some(poly("Seq", vec![ty_tp(Str)])), 2);
        str_seq.register_builtin_impl("len", fn0_met(Str, Nat), Const, Public);
        str_seq.register_builtin_impl("get", fn1_met(Str, Nat, Str), Const, Public);
        str_.register_trait(Str, str_seq);
        let mut str_add = Self::builtin_methods(Some(poly("Add", vec![ty_tp(Str)])), 2);
        str_add.register_builtin_impl("__add__", fn1_met(Str, Str, Str), Const, Public);
        str_add.register_builtin_const("Output", Public, ValueObj::builtin_t(Str));
        str_.register_trait(Str, str_add);
        let mut str_mul = Self::builtin_methods(Some(poly("Mul", vec![ty_tp(Nat)])), 2);
        str_mul.register_builtin_impl("__mul__", fn1_met(Str, Nat, Str), Const, Public);
        str_mul.register_builtin_const("Output", Public, ValueObj::builtin_t(Str));
        str_.register_trait(Str, str_mul);
        let mut str_mutizable = Self::builtin_methods(Some(mono("Mutizable")), 2);
        str_mutizable.register_builtin_const("MutType!", Public, ValueObj::builtin_t(mono("Str!")));
        str_.register_trait(Str, str_mutizable);
        let mut str_show = Self::builtin_methods(Some(mono("Show")), 1);
        str_show.register_builtin_impl("to_str", fn0_met(Str, Str), Immutable, Public);
        str_.register_trait(Str, str_show);
        let mut str_iterable = Self::builtin_methods(Some(poly("Iterable", vec![ty_tp(Str)])), 2);
        str_iterable.register_builtin_impl(
            "iter",
            fn0_met(Str, mono("StrIterator")),
            Immutable,
            Public,
        );
        str_.register_trait(Str, str_iterable);
        /* NoneType */
        let mut nonetype = Self::builtin_mono_class("NoneType", 10);
        nonetype.register_superclass(Obj, &obj);
        let mut nonetype_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(NoneType)])), 2);
        nonetype_eq.register_builtin_impl(
            "__eq__",
            fn1_met(NoneType, NoneType, Bool),
            Const,
            Public,
        );
        nonetype.register_trait(NoneType, nonetype_eq);
        let mut nonetype_show = Self::builtin_methods(Some(mono("Show")), 1);
        nonetype_show.register_builtin_impl("to_str", fn0_met(NoneType, Str), Immutable, Public);
        nonetype.register_trait(NoneType, nonetype_show);
        /* Type */
        let mut type_ = Self::builtin_mono_class("Type", 2);
        type_.register_superclass(Obj, &obj);
        type_.register_builtin_impl(
            "mro",
            array_t(Type, TyParam::erased(Nat)),
            Immutable,
            Public,
        );
        type_.register_marker_trait(mono("Named"));
        let mut type_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(Type)])), 2);
        type_eq.register_builtin_impl("__eq__", fn1_met(Type, Type, Bool), Const, Public);
        type_.register_trait(Type, type_eq);
        let mut class_type = Self::builtin_mono_class("ClassType", 2);
        class_type.register_superclass(Type, &type_);
        class_type.register_marker_trait(mono("Named"));
        let mut class_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(ClassType)])), 2);
        class_eq.register_builtin_impl(
            "__eq__",
            fn1_met(ClassType, ClassType, Bool),
            Const,
            Public,
        );
        class_type.register_trait(ClassType, class_eq);
        let mut trait_type = Self::builtin_mono_class("TraitType", 2);
        trait_type.register_superclass(Type, &type_);
        trait_type.register_marker_trait(mono("Named"));
        let mut trait_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(TraitType)])), 2);
        trait_eq.register_builtin_impl(
            "__eq__",
            fn1_met(TraitType, TraitType, Bool),
            Const,
            Public,
        );
        trait_type.register_trait(TraitType, trait_eq);
        let g_module_t = mono("GenericModule");
        let mut generic_module = Self::builtin_mono_class("GenericModule", 2);
        generic_module.register_superclass(Obj, &obj);
        generic_module.register_marker_trait(mono("Named"));
        let mut generic_module_eq =
            Self::builtin_methods(Some(poly("Eq", vec![ty_tp(g_module_t.clone())])), 2);
        generic_module_eq.register_builtin_impl(
            "__eq__",
            fn1_met(g_module_t.clone(), g_module_t.clone(), Bool),
            Const,
            Public,
        );
        generic_module.register_trait(g_module_t.clone(), generic_module_eq);
        let module_t = module(mono_q_tp("Path"));
        let mut module = Self::builtin_poly_class("Module", vec![PS::named_nd("Path", Str)], 2);
        module.register_superclass(g_module_t.clone(), &generic_module);
        /* Array */
        let mut array_ =
            Self::builtin_poly_class("Array", vec![PS::t_nd("T"), PS::named_nd("N", Nat)], 10);
        array_.register_superclass(Obj, &obj);
        array_.register_marker_trait(poly("Output", vec![ty_tp(mono_q("T"))]));
        let n = mono_q_tp("N");
        let m = mono_q_tp("M");
        let arr_t = array_t(mono_q("T"), n.clone());
        let t = fn_met(
            arr_t.clone(),
            vec![kw("rhs", array_t(mono_q("T"), m.clone()))],
            None,
            vec![],
            array_t(mono_q("T"), n + m),
        );
        let t = quant(
            t,
            set! {static_instance("T", Type), static_instance("N", Nat), static_instance("M", Nat)},
        );
        array_.register_builtin_impl("concat", t, Immutable, Public);
        // Array(T, N)|<: Add(Array(T, M))|.
        //     Output = Array(T, N + M)
        //     __add__: (self: Array(T, N), other: Array(T, M)) -> Array(T, N + M) = Array.concat
        /*
        let mut array_add = Self::builtin_methods("Add", 2);
        array_add.register_builtin_impl("__add__", t, Immutable, Public);
        let out_t = array_t(mono_q("T"), n + m.clone());
        array_add.register_builtin_const("Output", Public, ValueObj::builtin_t(out_t));
        array_.register_trait(arr_t.clone(), poly("Add", vec![ty_tp(array_t(mono_q("T"), m))]), array_add);
        */
        let mut_type = ValueObj::builtin_t(poly(
            "Array!",
            vec![TyParam::t(mono_q("T")), TyParam::mono_q("N").mutate()],
        ));
        // [T; N].MutType! = [T; !N] (neither [T!; N] nor [T; N]!)
        array_.register_builtin_const("MutType!", Public, mut_type);
        let var = Str::from(fresh_varname());
        let input = refinement(
            var.clone(),
            Nat,
            set! { Predicate::le(var, mono_q_tp("N") - value(1usize)) },
        );
        // __getitem__: |T, N|(self: [T; N], _: {I: Nat | I <= N}) -> T
        let array_getitem_t = fn1_kw_met(
            array_t(mono_q("T"), mono_q_tp("N")),
            anon(input),
            mono_q("T"),
        );
        let array_getitem_t = quant(
            array_getitem_t,
            set! { static_instance("T", Type), static_instance("N", Nat) },
        );
        let get_item = ValueObj::Subr(ConstSubr::Builtin(BuiltinConstSubr::new(
            "__getitem__",
            __array_getitem__,
            array_getitem_t,
            None,
        )));
        array_.register_builtin_const("__getitem__", Public, get_item);
        let mut array_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(arr_t.clone())])), 2);
        array_eq.register_builtin_impl(
            "__eq__",
            fn1_met(arr_t.clone(), arr_t.clone(), Bool),
            Const,
            Public,
        );
        array_.register_trait(arr_t.clone(), array_eq);
        array_.register_marker_trait(mono("Mutizable"));
        array_.register_marker_trait(poly("Seq", vec![ty_tp(mono_q("T"))]));
        let mut array_show = Self::builtin_methods(Some(mono("Show")), 1);
        array_show.register_builtin_impl("to_str", fn0_met(arr_t.clone(), Str), Immutable, Public);
        array_.register_trait(arr_t.clone(), array_show);
        let mut array_iterable =
            Self::builtin_methods(Some(poly("Iterable", vec![ty_tp(mono_q("T"))])), 2);
        array_iterable.register_builtin_impl(
            "iter",
            fn0_met(Str, mono("ArrayIterator")),
            Immutable,
            Public,
        );
        array_.register_trait(arr_t.clone(), array_iterable);
        /* Set */
        let mut set_ =
            Self::builtin_poly_class("Set", vec![PS::t_nd("T"), PS::named_nd("N", Nat)], 10);
        let n = mono_q_tp("N");
        let m = mono_q_tp("M");
        let set_t = set_t(mono_q("T"), n.clone());
        set_.register_superclass(Obj, &obj);
        set_.register_marker_trait(poly("Output", vec![ty_tp(mono_q("T"))]));
        let t = fn_met(
            set_t.clone(),
            vec![kw("rhs", array_t(mono_q("T"), m.clone()))],
            None,
            vec![],
            array_t(mono_q("T"), n + m),
        );
        let t = quant(
            t,
            set! {static_instance("N", Nat), static_instance("M", Nat)},
        );
        set_.register_builtin_impl("concat", t, Immutable, Public);
        let mut_type = ValueObj::builtin_t(poly(
            "Set!",
            vec![TyParam::t(mono_q("T")), TyParam::mono_q("N").mutate()],
        ));
        set_.register_builtin_const("MutType!", Public, mut_type);
        let mut set_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(set_t.clone())])), 2);
        set_eq.register_builtin_impl(
            "__eq__",
            fn1_met(set_t.clone(), set_t.clone(), Bool),
            Const,
            Public,
        );
        set_.register_trait(set_t.clone(), set_eq);
        set_.register_marker_trait(mono("Mutizable"));
        set_.register_marker_trait(poly("Seq", vec![ty_tp(mono_q("T"))]));
        let mut set_show = Self::builtin_methods(Some(mono("Show")), 1);
        set_show.register_builtin_impl("to_str", fn0_met(set_t.clone(), Str), Immutable, Public);
        set_.register_trait(set_t.clone(), set_show);
        let g_dict_t = mono("GenericDict");
        let mut generic_dict = Self::builtin_mono_class("GenericDict", 2);
        generic_dict.register_superclass(Obj, &obj);
        let mut generic_dict_eq =
            Self::builtin_methods(Some(poly("Eq", vec![ty_tp(g_dict_t.clone())])), 2);
        generic_dict_eq.register_builtin_impl(
            "__eq__",
            fn1_met(g_dict_t.clone(), g_dict_t.clone(), Bool),
            Const,
            Public,
        );
        generic_dict.register_trait(g_dict_t.clone(), generic_dict_eq);
        // .get: _: T -> T or None
        let dict_get_t = fn1_met(g_dict_t.clone(), mono_q("T"), or(mono_q("T"), NoneType));
        let dict_get_t = quant(dict_get_t, set! {static_instance("T", Type)});
        generic_dict.register_builtin_impl("get", dict_get_t, Immutable, Public);
        let dict_t = poly("Dict", vec![mono_q_tp("D")]);
        let mut dict_ =
            // TODO: D <: GenericDict
            Self::builtin_poly_class("Dict", vec![PS::named_nd("D", mono("GenericDict"))], 10);
        dict_.register_superclass(g_dict_t.clone(), &generic_dict);
        dict_.register_marker_trait(poly("Output", vec![ty_tp(mono_q("D"))]));
        // __getitem__: _: T -> D[T]
        let dict_getitem_t = fn1_met(
            dict_t.clone(),
            mono_q("T"),
            proj_call(mono_q_tp("D"), "__getitem__", vec![ty_tp(mono_q("T"))]),
        );
        let dict_getitem_t = quant(
            dict_getitem_t,
            set! {static_instance("D", mono("GenericDict")), static_instance("T", Type)},
        );
        let get_item = ValueObj::Subr(ConstSubr::Builtin(BuiltinConstSubr::new(
            "__getitem__",
            __dict_getitem__,
            dict_getitem_t,
            None,
        )));
        dict_.register_builtin_const("__getitem__", Public, get_item);
        /* Bytes */
        let mut bytes = Self::builtin_mono_class("Bytes", 2);
        bytes.register_superclass(Obj, &obj);
        let mut generic_tuple = Self::builtin_mono_class("GenericTuple", 1);
        generic_tuple.register_superclass(Obj, &obj);
        let mut tuple_eq =
            Self::builtin_methods(Some(poly("Eq", vec![ty_tp(mono("GenericTuple"))])), 2);
        tuple_eq.register_builtin_impl(
            "__eq__",
            fn1_met(mono("GenericTuple"), mono("GenericTuple"), Bool),
            Const,
            Public,
        );
        generic_tuple.register_trait(mono("GenericTuple"), tuple_eq);
        // Ts <: GenericArray
        let tuple_t = poly("Tuple", vec![mono_q_tp("Ts")]);
        let mut tuple_ =
            Self::builtin_poly_class("Tuple", vec![PS::named_nd("Ts", mono_q("Ts"))], 2);
        tuple_.register_superclass(mono("GenericTuple"), &generic_tuple);
        tuple_.register_marker_trait(poly("Output", vec![ty_tp(mono_q("Ts"))]));
        // __Tuple_getitem__: (self: Tuple(Ts), _: {N}) -> Ts[N]
        let return_t = proj_call(mono_q_tp("Ts"), "__getitem__", vec![mono_q_tp("N")]);
        let tuple_getitem_t = fn1_met(
            tuple_t.clone(),
            tp_enum(Nat, set! {mono_q_tp("N")}),
            return_t,
        );
        let tuple_getitem_t = quant(
            tuple_getitem_t,
            set! {static_instance("Ts", array_t(Type, mono_q_tp("N"))), static_instance("N", Nat)},
        );
        tuple_.register_builtin_impl("__Tuple_getitem__", tuple_getitem_t, Const, Public);
        /* record */
        let mut record = Self::builtin_mono_class("Record", 2);
        record.register_superclass(Obj, &obj);
        /* Or (true or type) */
        let or_t = poly("Or", vec![ty_tp(mono_q("L")), ty_tp(mono_q("R"))]);
        let mut or = Self::builtin_poly_class("Or", vec![PS::t_nd("L"), PS::t_nd("R")], 2);
        or.register_superclass(Obj, &obj);
        /* Iterators */
        let mut str_iterator = Self::builtin_mono_class("StrIterator", 1);
        str_iterator.register_superclass(Obj, &obj);
        let mut array_iterator = Self::builtin_poly_class("ArrayIterator", vec![PS::t_nd("T")], 1);
        array_iterator.register_superclass(Obj, &obj);
        /* Float_mut */
        let mut float_mut = Self::builtin_mono_class("Float!", 2);
        float_mut.register_superclass(Float, &float);
        let mut float_mut_mutable = Self::builtin_methods(Some(mono("Mutable")), 2);
        float_mut_mutable.register_builtin_const("ImmutType", Public, ValueObj::builtin_t(Float));
        let f_t = kw("f", func(vec![kw("old", Float)], None, vec![], Float));
        let t = pr_met(
            ref_mut(mono("Float!"), None),
            vec![f_t],
            None,
            vec![],
            NoneType,
        );
        float_mut_mutable.register_builtin_impl("update!", t, Immutable, Public);
        float_mut.register_trait(mono("Float!"), float_mut_mutable);
        /* Ratio_mut */
        let mut ratio_mut = Self::builtin_mono_class("Ratio!", 2);
        ratio_mut.register_superclass(Ratio, &ratio);
        let mut ratio_mut_mutable = Self::builtin_methods(Some(mono("Mutable")), 2);
        ratio_mut_mutable.register_builtin_const("ImmutType", Public, ValueObj::builtin_t(Ratio));
        let f_t = kw("f", func(vec![kw("old", Ratio)], None, vec![], Ratio));
        let t = pr_met(
            ref_mut(mono("Ratio!"), None),
            vec![f_t],
            None,
            vec![],
            NoneType,
        );
        ratio_mut_mutable.register_builtin_impl("update!", t, Immutable, Public);
        ratio_mut.register_trait(mono("Ratio!"), ratio_mut_mutable);
        /* Int_mut */
        let mut int_mut = Self::builtin_mono_class("Int!", 2);
        int_mut.register_superclass(Int, &int);
        int_mut.register_superclass(mono("Float!"), &float_mut);
        let mut int_mut_mutable = Self::builtin_methods(Some(mono("Mutable")), 2);
        int_mut_mutable.register_builtin_const("ImmutType", Public, ValueObj::builtin_t(Int));
        let f_t = kw("f", func(vec![kw("old", Int)], None, vec![], Int));
        let t = pr_met(
            ref_mut(mono("Int!"), None),
            vec![f_t],
            None,
            vec![],
            NoneType,
        );
        int_mut_mutable.register_builtin_impl("update!", t, Immutable, Public);
        int_mut.register_trait(mono("Int!"), int_mut_mutable);
        let mut nat_mut = Self::builtin_mono_class("Nat!", 2);
        nat_mut.register_superclass(Nat, &nat);
        nat_mut.register_superclass(mono("Int!"), &int_mut);
        /* Nat_mut */
        let mut nat_mut_mutable = Self::builtin_methods(Some(mono("Mutable")), 2);
        nat_mut_mutable.register_builtin_const("ImmutType", Public, ValueObj::builtin_t(Nat));
        let f_t = kw("f", func(vec![kw("old", Nat)], None, vec![], Nat));
        let t = pr_met(
            ref_mut(mono("Nat!"), None),
            vec![f_t],
            None,
            vec![],
            NoneType,
        );
        nat_mut_mutable.register_builtin_impl("update!", t, Immutable, Public);
        nat_mut.register_trait(mono("Nat!"), nat_mut_mutable);
        /* Bool_mut */
        let mut bool_mut = Self::builtin_mono_class("Bool!", 2);
        bool_mut.register_superclass(Bool, &bool_);
        bool_mut.register_superclass(mono("Nat!"), &nat_mut);
        let mut bool_mut_mutable = Self::builtin_methods(Some(mono("Mutable")), 2);
        bool_mut_mutable.register_builtin_const("ImmutType", Public, ValueObj::builtin_t(Bool));
        let f_t = kw("f", func(vec![kw("old", Bool)], None, vec![], Bool));
        let t = pr_met(
            ref_mut(mono("Bool!"), None),
            vec![f_t],
            None,
            vec![],
            NoneType,
        );
        bool_mut_mutable.register_builtin_impl("update!", t, Immutable, Public);
        bool_mut.register_trait(mono("Bool!"), bool_mut_mutable);
        /* Str_mut */
        let mut str_mut = Self::builtin_mono_class("Str!", 2);
        str_mut.register_superclass(Str, &nonetype);
        let mut str_mut_mutable = Self::builtin_methods(Some(mono("Mutable")), 2);
        str_mut_mutable.register_builtin_const("ImmutType", Public, ValueObj::builtin_t(Str));
        let f_t = kw("f", func(vec![kw("old", Str)], None, vec![], Str));
        let t = pr_met(
            ref_mut(mono("Str!"), None),
            vec![f_t],
            None,
            vec![],
            NoneType,
        );
        str_mut_mutable.register_builtin_impl("update!", t, Immutable, Public);
        str_mut.register_trait(mono("Str!"), str_mut_mutable);
        /* File_mut */
        let mut file_mut = Self::builtin_mono_class("File!", 2);
        let mut file_mut_readable = Self::builtin_methods(Some(mono("Readable!")), 1);
        file_mut_readable.register_builtin_impl(
            "read!",
            pr_met(
                ref_mut(mono("File!"), None),
                vec![],
                None,
                vec![kw("n", Int)],
                Str,
            ),
            Immutable,
            Public,
        );
        file_mut.register_trait(mono("File!"), file_mut_readable);
        let mut file_mut_writable = Self::builtin_methods(Some(mono("Writable!")), 1);
        file_mut_writable.register_builtin_impl(
            "write!",
            pr1_kw_met(ref_mut(mono("File!"), None), kw("s", Str), Nat),
            Immutable,
            Public,
        );
        file_mut.register_trait(mono("File!"), file_mut_writable);
        /* Array_mut */
        let array_mut_t = poly("Array!", vec![ty_tp(mono_q("T")), mono_q_tp("N")]);
        let mut array_mut_ = Self::builtin_poly_class(
            "Array!",
            vec![PS::t_nd("T"), PS::named_nd("N", mono("Nat!"))],
            2,
        );
        array_mut_.register_superclass(arr_t.clone(), &array_);
        let t = pr_met(
            ref_mut(
                array_mut_t.clone(),
                Some(poly(
                    "Array!",
                    vec![ty_tp(mono_q("T")), mono_q_tp("N") + value(1usize)],
                )),
            ),
            vec![kw("elem", mono_q("T"))],
            None,
            vec![],
            NoneType,
        );
        let t = quant(
            t,
            set! {static_instance("T", Type), static_instance("N", mono("Nat!"))},
        );
        array_mut_.register_builtin_impl("push!", t, Immutable, Public);
        let t = pr_met(
            array_mut_t.clone(),
            vec![kw("f", nd_func(vec![anon(mono_q("T"))], None, mono_q("T")))],
            None,
            vec![],
            NoneType,
        );
        let t = quant(
            t,
            set! {static_instance("T", Type), static_instance("N", mono("Nat!"))},
        );
        array_mut_.register_builtin_impl("strict_map!", t, Immutable, Public);
        let f_t = kw(
            "f",
            func(vec![kw("old", arr_t.clone())], None, vec![], arr_t.clone()),
        );
        let t = pr_met(
            ref_mut(array_mut_t.clone(), None),
            vec![f_t],
            None,
            vec![],
            NoneType,
        );
        let mut array_mut_mutable = Self::builtin_methods(Some(mono("Mutable")), 2);
        array_mut_mutable.register_builtin_impl("update!", t, Immutable, Public);
        array_mut_.register_trait(array_mut_t.clone(), array_mut_mutable);
        /* Set_mut */
        let set_mut_t = poly("Set!", vec![ty_tp(mono_q("T")), mono_q_tp("N")]);
        let mut set_mut_ = Self::builtin_poly_class(
            "Set!",
            vec![PS::t_nd("T"), PS::named_nd("N", mono("Nat!"))],
            2,
        );
        set_mut_.register_superclass(set_t.clone(), &set_);
        // `add!` will erase N
        let t = pr_met(
            ref_mut(
                set_mut_t.clone(),
                Some(poly(
                    "Set!",
                    vec![ty_tp(mono_q("T")), TyParam::erased(mono("Nat!"))],
                )),
            ),
            vec![kw("elem", mono_q("T"))],
            None,
            vec![],
            NoneType,
        );
        let t = quant(
            t,
            set! {static_instance("T", Type), static_instance("N", mono("Nat!"))},
        );
        set_mut_.register_builtin_impl("add!", t, Immutable, Public);
        let t = pr_met(
            set_mut_t.clone(),
            vec![kw("f", nd_func(vec![anon(mono_q("T"))], None, mono_q("T")))],
            None,
            vec![],
            NoneType,
        );
        let t = quant(
            t,
            set! {static_instance("T", Type), static_instance("N", mono("Nat!"))},
        );
        set_mut_.register_builtin_impl("strict_map!", t, Immutable, Public);
        let f_t = kw(
            "f",
            func(vec![kw("old", set_t.clone())], None, vec![], set_t.clone()),
        );
        let t = pr_met(
            ref_mut(set_mut_t.clone(), None),
            vec![f_t],
            None,
            vec![],
            NoneType,
        );
        let mut set_mut_mutable = Self::builtin_methods(Some(mono("Mutable")), 2);
        set_mut_mutable.register_builtin_impl("update!", t, Immutable, Public);
        set_mut_.register_trait(set_mut_t.clone(), set_mut_mutable);
        /* Range */
        let range_t = poly("Range", vec![TyParam::t(mono_q("T"))]);
        let mut range = Self::builtin_poly_class("Range", vec![PS::t_nd("T")], 2);
        // range.register_superclass(Obj, &obj);
        range.register_superclass(Type, &type_);
        range.register_marker_trait(poly("Output", vec![ty_tp(mono_q("T"))]));
        let mut range_eq = Self::builtin_methods(Some(poly("Eq", vec![ty_tp(range_t.clone())])), 2);
        range_eq.register_builtin_impl(
            "__eq__",
            fn1_met(range_t.clone(), range_t.clone(), Bool),
            Const,
            Public,
        );
        range.register_trait(range_t.clone(), range_eq);
        /* Proc */
        let mut proc = Self::builtin_mono_class("Proc", 2);
        proc.register_superclass(Obj, &obj);
        let mut named_proc = Self::builtin_mono_class("NamedProc", 2);
        named_proc.register_superclass(Obj, &obj);
        named_proc.register_marker_trait(mono("Named"));
        /* Func */
        let mut func = Self::builtin_mono_class("Func", 2);
        func.register_superclass(mono("Proc"), &proc);
        let mut named_func = Self::builtin_mono_class("NamedFunc", 2);
        named_func.register_superclass(mono("Func"), &func);
        named_func.register_marker_trait(mono("Named"));
        let mut qfunc = Self::builtin_mono_class("QuantifiedFunc", 2);
        qfunc.register_superclass(mono("Func"), &func);
        self.register_builtin_type(Obj, obj, Private, Const);
        // self.register_type(mono("Record"), vec![], record, Private, Const);
        self.register_builtin_type(Int, int, Private, Const);
        self.register_builtin_type(Nat, nat, Private, Const);
        self.register_builtin_type(Float, float, Private, Const);
        self.register_builtin_type(Ratio, ratio, Private, Const);
        self.register_builtin_type(Bool, bool_, Private, Const);
        self.register_builtin_type(Str, str_, Private, Const);
        self.register_builtin_type(NoneType, nonetype, Private, Const);
        self.register_builtin_type(Type, type_, Private, Const);
        self.register_builtin_type(ClassType, class_type, Private, Const);
        self.register_builtin_type(TraitType, trait_type, Private, Const);
        self.register_builtin_type(g_module_t, generic_module, Private, Const);
        self.register_builtin_type(module_t, module, Private, Const);
        self.register_builtin_type(arr_t, array_, Private, Const);
        self.register_builtin_type(set_t, set_, Private, Const);
        self.register_builtin_type(g_dict_t, generic_dict, Private, Const);
        self.register_builtin_type(dict_t, dict_, Private, Const);
        self.register_builtin_type(mono("Bytes"), bytes, Private, Const);
        self.register_builtin_type(mono("GenericTuple"), generic_tuple, Private, Const);
        self.register_builtin_type(tuple_t, tuple_, Private, Const);
        self.register_builtin_type(mono("Record"), record, Private, Const);
        self.register_builtin_type(or_t, or, Private, Const);
        self.register_builtin_type(mono("StrIterator"), str_iterator, Private, Const);
        self.register_builtin_type(
            poly("ArrayIterator", vec![ty_tp(mono_q("T"))]),
            array_iterator,
            Private,
            Const,
        );
        self.register_builtin_type(mono("Int!"), int_mut, Private, Const);
        self.register_builtin_type(mono("Nat!"), nat_mut, Private, Const);
        self.register_builtin_type(mono("Float!"), float_mut, Private, Const);
        self.register_builtin_type(mono("Ratio!"), ratio_mut, Private, Const);
        self.register_builtin_type(mono("Bool!"), bool_mut, Private, Const);
        self.register_builtin_type(mono("Str!"), str_mut, Private, Const);
        self.register_builtin_type(mono("File!"), file_mut, Private, Const);
        self.register_builtin_type(array_mut_t, array_mut_, Private, Const);
        self.register_builtin_type(set_mut_t, set_mut_, Private, Const);
        self.register_builtin_type(range_t, range, Private, Const);
        self.register_builtin_type(mono("Proc"), proc, Private, Const);
        self.register_builtin_type(mono("NamedProc"), named_proc, Private, Const);
        self.register_builtin_type(mono("Func"), func, Private, Const);
        self.register_builtin_type(mono("NamedFunc"), named_func, Private, Const);
        self.register_builtin_type(mono("QuantifiedFunc"), qfunc, Private, Const);
    }

    fn init_builtin_funcs(&mut self) {
        let t_abs = nd_func(vec![kw("n", mono("Num"))], None, Nat);
        let t_ascii = nd_func(vec![kw("object", Obj)], None, Str);
        let t_assert = func(
            vec![kw("condition", Bool)],
            None,
            vec![kw("err_message", Str)],
            NoneType,
        );
        let t_bin = nd_func(vec![kw("n", Int)], None, Str);
        let t_chr = nd_func(
            vec![kw("i", Type::from(value(0usize)..=value(1_114_111usize)))],
            None,
            Str,
        );
        let t_classof = nd_func(vec![kw("old", Obj)], None, ClassType);
        let t_compile = nd_func(vec![kw("src", Str)], None, Code);
        let t_cond = nd_func(
            vec![
                kw("condition", Bool),
                kw("then", mono_q("T")),
                kw("else", mono_q("T")),
            ],
            None,
            mono_q("T"),
        );
        let t_cond = quant(t_cond, set! {static_instance("T", Type)});
        let t_discard = nd_func(vec![kw("obj", Obj)], None, NoneType);
        let t_if = func(
            vec![
                kw("cond", Bool),
                kw("then", nd_func(vec![], None, mono_q("T"))),
            ],
            None,
            vec![kw_default(
                "else",
                nd_func(vec![], None, mono_q("U")),
                nd_func(vec![], None, NoneType),
            )],
            or(mono_q("T"), mono_q("U")),
        );
        let t_if = quant(
            t_if,
            set! {static_instance("T", Type), static_instance("U", Type)},
        );
        let t_import = nd_func(
            vec![anon(tp_enum(Str, set! {mono_q_tp("Path")}))],
            None,
            module(mono_q_tp("Path")),
        );
        let t_import = quant(t_import, set! {static_instance("Path", Str)});
        let t_isinstance = nd_func(
            vec![
                kw("object", Obj),
                kw("classinfo", ClassType), // TODO: => ClassInfo
            ],
            None,
            Bool,
        );
        let t_issubclass = nd_func(
            vec![
                kw("subclass", ClassType),
                kw("classinfo", ClassType), // TODO: => ClassInfo
            ],
            None,
            Bool,
        );
        let t_len = nd_func(
            vec![kw("s", poly("Seq", vec![TyParam::erased(Type)]))],
            None,
            Nat,
        );
        let t_log = func(
            vec![],
            Some(kw("objects", ref_(Obj))),
            vec![
                kw("sep", Str),
                kw("end", Str),
                kw("file", mono("Write")),
                kw("flush", Bool),
            ],
            NoneType,
        );
        let t_oct = nd_func(vec![kw("x", Int)], None, Str);
        let t_ord = nd_func(vec![kw("c", Str)], None, Nat);
        let t_panic = nd_func(vec![kw("err_message", Str)], None, Never);
        let m = mono_q("M");
        // TODO: mod
        let t_pow = nd_func(
            vec![kw("base", m.clone()), kw("exp", m.clone())],
            None,
            m.clone(),
        );
        let t_pow = quant(
            t_pow,
            set! {static_instance("M", poly("Mul", vec![ty_tp(m)]))},
        );
        let t_pyimport = nd_func(
            vec![anon(tp_enum(Str, set! {mono_q_tp("Path")}))],
            None,
            module(mono_q_tp("Path")),
        );
        let t_pyimport = quant(t_pyimport, set! {static_instance("Path", Str)});
        let t_quit = func(vec![], None, vec![kw("code", Int)], NoneType);
        let t_exit = t_quit.clone();
        let t_repr = nd_func(vec![kw("object", Obj)], None, Str);
        let t_round = nd_func(vec![kw("number", Float)], None, Int);
        self.register_builtin_impl("abs", t_abs, Immutable, Private);
        self.register_builtin_impl("ascii", t_ascii, Immutable, Private);
        self.register_builtin_impl("assert", t_assert, Const, Private); // assert casting に悪影響が出る可能性があるため、Constとしておく
        self.register_builtin_impl("bin", t_bin, Immutable, Private);
        self.register_builtin_impl("chr", t_chr, Immutable, Private);
        self.register_builtin_impl("classof", t_classof, Immutable, Private);
        self.register_builtin_impl("compile", t_compile, Immutable, Private);
        self.register_builtin_impl("cond", t_cond, Immutable, Private);
        self.register_builtin_impl("discard", t_discard, Immutable, Private);
        self.register_builtin_impl("exit", t_exit, Immutable, Private);
        self.register_builtin_impl("if", t_if, Immutable, Private);
        self.register_builtin_impl("import", t_import, Immutable, Private);
        self.register_builtin_impl("isinstance", t_isinstance, Immutable, Private);
        self.register_builtin_impl("issubclass", t_issubclass, Immutable, Private);
        self.register_builtin_impl("len", t_len, Immutable, Private);
        self.register_builtin_impl("log", t_log, Immutable, Private);
        self.register_builtin_impl("oct", t_oct, Immutable, Private);
        self.register_builtin_impl("ord", t_ord, Immutable, Private);
        self.register_builtin_impl("panic", t_panic, Immutable, Private);
        self.register_builtin_impl("pow", t_pow, Immutable, Private);
        if cfg!(feature = "debug") {
            self.register_builtin_impl("py", t_pyimport.clone(), Immutable, Private);
        }
        self.register_builtin_impl("pyimport", t_pyimport, Immutable, Private);
        self.register_builtin_impl("quit", t_quit, Immutable, Private);
        self.register_builtin_impl("repr", t_repr, Immutable, Private);
        self.register_builtin_impl("round", t_round, Immutable, Private);
    }

    fn init_builtin_const_funcs(&mut self) {
        let class_t = func(
            vec![kw("Requirement", Type)],
            None,
            vec![kw("Impl", Type)],
            ClassType,
        );
        let class = ConstSubr::Builtin(BuiltinConstSubr::new("Class", class_func, class_t, None));
        self.register_builtin_const("Class", Private, ValueObj::Subr(class));
        let inherit_t = func(
            vec![kw("Super", ClassType)],
            None,
            vec![kw("Impl", Type), kw("Additional", Type)],
            ClassType,
        );
        let inherit = ConstSubr::Builtin(BuiltinConstSubr::new(
            "Inherit",
            inherit_func,
            inherit_t,
            None,
        ));
        self.register_builtin_const("Inherit", Private, ValueObj::Subr(inherit));
        let trait_t = func(
            vec![kw("Requirement", Type)],
            None,
            vec![kw("Impl", Type)],
            TraitType,
        );
        let trait_ = ConstSubr::Builtin(BuiltinConstSubr::new("Trait", trait_func, trait_t, None));
        self.register_builtin_const("Trait", Private, ValueObj::Subr(trait_));
        let subsume_t = func(
            vec![kw("Super", TraitType)],
            None,
            vec![kw("Impl", Type), kw("Additional", Type)],
            TraitType,
        );
        let subsume = ConstSubr::Builtin(BuiltinConstSubr::new(
            "Subsume",
            subsume_func,
            subsume_t,
            None,
        ));
        self.register_builtin_const("Subsume", Private, ValueObj::Subr(subsume));
        // decorators
        let inheritable_t = func1(ClassType, ClassType);
        let inheritable = ConstSubr::Builtin(BuiltinConstSubr::new(
            "Inheritable",
            inheritable_func,
            inheritable_t,
            None,
        ));
        self.register_builtin_const("Inheritable", Private, ValueObj::Subr(inheritable));
        // TODO: register Del function object
        let t_del = nd_func(vec![kw("obj", Obj)], None, NoneType);
        self.register_builtin_impl("Del", t_del, Immutable, Private);
    }

    fn init_builtin_procs(&mut self) {
        let t_dir = proc(
            vec![kw("obj", ref_(Obj))],
            None,
            vec![],
            array_t(Str, TyParam::erased(Nat)),
        );
        let t_print = proc(
            vec![],
            Some(kw("objects", ref_(Obj))),
            vec![
                kw("sep", Str),
                kw("end", Str),
                kw("file", mono("Write")),
                kw("flush", Bool),
            ],
            NoneType,
        );
        let t_id = nd_func(vec![kw("old", Obj)], None, Nat);
        let t_input = proc(vec![], None, vec![kw("msg", Str)], Str);
        let t_if = proc(
            vec![
                kw("cond", Bool),
                kw("then", nd_proc(vec![], None, mono_q("T"))),
            ],
            None,
            vec![kw("else", nd_proc(vec![], None, mono_q("T")))],
            or(mono_q("T"), NoneType),
        );
        let t_if = quant(t_if, set! {static_instance("T", Type)});
        let t_for = nd_proc(
            vec![
                kw("iterable", poly("Iterable", vec![ty_tp(mono_q("T"))])),
                kw("p", nd_proc(vec![anon(mono_q("T"))], None, NoneType)),
            ],
            None,
            NoneType,
        );
        let t_for = quant(t_for, set! {static_instance("T", Type)});
        let t_globals = proc(vec![], None, vec![], dict! { Str => Obj }.into());
        let t_locals = proc(vec![], None, vec![], dict! { Str => Obj }.into());
        let t_while = nd_proc(
            vec![
                kw("cond", mono("Bool!")),
                kw("p", nd_proc(vec![], None, NoneType)),
            ],
            None,
            NoneType,
        );
        let t_open = proc(
            vec![kw("file", mono_q("P"))],
            None,
            vec![
                kw("mode", Str),
                kw("buffering", Int),
                kw("encoding", or(Str, NoneType)),
                kw("errors", or(Str, NoneType)),
                kw("newline", or(Str, NoneType)),
                kw("closefd", Bool),
                // param_t("opener", option),
            ],
            mono("File!"),
        );
        let t_open = quant(t_open, set! {subtypeof(mono_q("P"), mono("PathLike"))});
        // TODO: T <: With
        let t_with = nd_proc(
            vec![
                kw("obj", mono_q("T")),
                kw("p!", nd_proc(vec![anon(mono_q("T"))], None, mono_q("U"))),
            ],
            None,
            mono_q("U"),
        );
        let t_with = quant(
            t_with,
            set! {static_instance("T", Type), static_instance("U", Type)},
        );
        self.register_builtin_impl("dir!", t_dir, Immutable, Private);
        self.register_builtin_impl("print!", t_print, Immutable, Private);
        self.register_builtin_impl("id!", t_id, Immutable, Private);
        self.register_builtin_impl("input!", t_input, Immutable, Private);
        self.register_builtin_impl("if!", t_if, Immutable, Private);
        self.register_builtin_impl("for!", t_for, Immutable, Private);
        self.register_builtin_impl("globals!", t_globals, Immutable, Private);
        self.register_builtin_impl("locals!", t_locals, Immutable, Private);
        self.register_builtin_impl("while!", t_while, Immutable, Private);
        self.register_builtin_impl("open!", t_open, Immutable, Private);
        self.register_builtin_impl("with!", t_with, Immutable, Private);
    }

    fn init_builtin_operators(&mut self) {
        /* binary */
        let l = mono_q("L");
        let r = mono_q("R");
        let params = vec![ty_tp(mono_q("R"))];
        let op_t = nd_func(
            vec![kw("lhs", l.clone()), kw("rhs", r.clone())],
            None,
            proj(mono_q("L"), "Output"),
        );
        let op_t = quant(
            op_t,
            set! {
                static_instance("R", Type),
                subtypeof(l.clone(), poly("Add", params.clone()))
            },
        );
        self.register_builtin_impl("__add__", op_t, Const, Private);
        let op_t = bin_op(l.clone(), r.clone(), proj(mono_q("L"), "Output"));
        let op_t = quant(
            op_t,
            set! {
                static_instance("R", Type),
                subtypeof(l.clone(), poly("Sub", params.clone()))
            },
        );
        self.register_builtin_impl("__sub__", op_t, Const, Private);
        let op_t = bin_op(l.clone(), r.clone(), proj(mono_q("L"), "Output"));
        let op_t = quant(
            op_t,
            set! {
                static_instance("R", Type),
                subtypeof(l.clone(), poly("Mul", params.clone()))
            },
        );
        self.register_builtin_impl("__mul__", op_t, Const, Private);
        let op_t = bin_op(l.clone(), r.clone(), proj(mono_q("L"), "Output"));
        let op_t = quant(
            op_t,
            set! {
                static_instance("R", Type),
                subtypeof(l.clone(), poly("Div", params.clone()))
            },
        );
        self.register_builtin_impl("__div__", op_t, Const, Private);
        let op_t = bin_op(l.clone(), r.clone(), proj(mono_q("L"), "Output"));
        let op_t = quant(
            op_t,
            set! {
                static_instance("R", Type),
                subtypeof(l.clone(), poly("FloorDiv", params.clone()))
            },
        );
        self.register_builtin_impl("__floordiv__", op_t, Const, Private);
        let m = mono_q("M");
        let op_t = bin_op(m.clone(), m.clone(), proj(m.clone(), "PowOutput"));
        let op_t = quant(op_t, set! {subtypeof(m, poly("Mul", vec![]))});
        // TODO: add bound: M == M.Output
        self.register_builtin_impl("__pow__", op_t, Const, Private);
        let d = mono_q("D");
        let op_t = bin_op(d.clone(), d.clone(), proj(d.clone(), "ModOutput"));
        let op_t = quant(op_t, set! {subtypeof(d, poly("Div", vec![]))});
        self.register_builtin_impl("__mod__", op_t, Const, Private);
        let e = mono_q("E");
        let op_t = bin_op(e.clone(), e.clone(), Bool);
        let op_t = quant(op_t, set! {subtypeof(e, poly("Eq", vec![]))});
        self.register_builtin_impl("__eq__", op_t.clone(), Const, Private);
        self.register_builtin_impl("__ne__", op_t, Const, Private);
        let op_t = bin_op(l.clone(), r, Bool);
        let op_t = quant(
            op_t,
            set! {
                static_instance("R", Type),
                subtypeof(l, poly("PartialOrd", params))
            },
        );
        self.register_builtin_impl("__lt__", op_t.clone(), Const, Private);
        self.register_builtin_impl("__le__", op_t.clone(), Const, Private);
        self.register_builtin_impl("__gt__", op_t.clone(), Const, Private);
        self.register_builtin_impl("__ge__", op_t, Const, Private);
        self.register_builtin_impl("__and__", bin_op(Bool, Bool, Bool), Const, Private);
        self.register_builtin_impl("__or__", bin_op(Bool, Bool, Bool), Const, Private);
        let t = mono_q("T");
        let op_t = bin_op(t.clone(), t.clone(), range(t.clone()));
        let op_t = quant(op_t, set! {subtypeof(t, mono("Ord"))});
        self.register_builtin_decl("__rng__", op_t.clone(), Private);
        self.register_builtin_decl("__lorng__", op_t.clone(), Private);
        self.register_builtin_decl("__rorng__", op_t.clone(), Private);
        self.register_builtin_decl("__orng__", op_t, Private);
        // TODO: use existential type: |T: Type| (T, In(T)) -> Bool
        let op_t = bin_op(mono_q("I"), mono_q("T"), Bool);
        let op_t = quant(
            op_t,
            set! { static_instance("T", Type), subtypeof(mono_q("I"), poly("In", vec![ty_tp(mono_q("T"))])) },
        );
        self.register_builtin_impl("__in__", op_t, Const, Private);
        /* unary */
        // TODO: Boolの+/-は警告を出したい
        let op_t = func1(mono_q("T"), proj(mono_q("T"), "MutType!"));
        let op_t = quant(op_t, set! {subtypeof(mono_q("T"), mono("Mutizable"))});
        self.register_builtin_impl("__mutate__", op_t, Const, Private);
        let n = mono_q("N");
        let op_t = func1(n.clone(), n.clone());
        let op_t = quant(op_t, set! {subtypeof(n, mono("Num"))});
        self.register_builtin_decl("__pos__", op_t.clone(), Private);
        self.register_builtin_decl("__neg__", op_t, Private);
    }

    fn init_builtin_patches(&mut self) {
        let m = mono_q_tp("M");
        let n = mono_q_tp("N");
        let o = mono_q_tp("O");
        let p = mono_q_tp("P");
        let params = vec![
            PS::named_nd("M", Int),
            PS::named_nd("N", Int),
            PS::named_nd("O", Int),
            PS::named_nd("P", Int),
        ];
        let class = Type::from(&m..=&n);
        // Interval is a bounding patch connecting M..N and (Add(O..P, M+O..N..P), Sub(O..P, M-P..N-O))
        let mut interval = Self::builtin_poly_patch("Interval", class.clone(), params, 2);
        let op_t = fn1_met(
            class.clone(),
            Type::from(&o..=&p),
            Type::from(m.clone() + o.clone()..=n.clone() + p.clone()),
        );
        let mut interval_add =
            Self::builtin_methods(Some(poly("Add", vec![TyParam::from(&o..=&p)])), 2);
        interval_add.register_builtin_impl("__add__", op_t, Const, Public);
        interval_add.register_builtin_const(
            "Output",
            Public,
            ValueObj::builtin_t(Type::from(m.clone() + o.clone()..=n.clone() + p.clone())),
        );
        interval.register_trait(class.clone(), interval_add);
        let mut interval_sub =
            Self::builtin_methods(Some(poly("Sub", vec![TyParam::from(&o..=&p)])), 2);
        let op_t = fn1_met(
            class.clone(),
            Type::from(&o..=&p),
            Type::from(m.clone() - p.clone()..=n.clone() - o.clone()),
        );
        interval_sub.register_builtin_impl("__sub__", op_t, Const, Public);
        interval_sub.register_builtin_const(
            "Output",
            Public,
            ValueObj::builtin_t(Type::from(m - p..=n - o)),
        );
        interval.register_trait(class, interval_sub);
        self.register_builtin_patch("Interval", interval, Private, Const);
        // eq.register_impl("__ne__", op_t,         Const, Public);
        // ord.register_impl("__le__", op_t.clone(), Const, Public);
        // ord.register_impl("__gt__", op_t.clone(), Const, Public);
        // ord.register_impl("__ge__", op_t,         Const, Public);
    }

    pub(crate) fn init_builtins(mod_cache: &SharedModuleCache) {
        // TODO: capacityを正確に把握する
        let mut ctx = Context::builtin_module("<builtins>", 40);
        ctx.init_builtin_consts();
        ctx.init_builtin_funcs();
        ctx.init_builtin_const_funcs();
        ctx.init_builtin_procs();
        ctx.init_builtin_operators();
        ctx.init_builtin_traits();
        ctx.init_builtin_classes();
        ctx.init_builtin_patches();
        mod_cache.register(PathBuf::from("<builtins>"), None, ctx);
    }

    pub fn new_module<S: Into<Str>>(
        name: S,
        cfg: ErgConfig,
        mod_cache: SharedModuleCache,
        py_mod_cache: SharedModuleCache,
    ) -> Self {
        Context::new(
            name.into(),
            cfg,
            ContextKind::Module,
            vec![],
            None,
            Some(mod_cache),
            Some(py_mod_cache),
            Context::TOP_LEVEL,
        )
    }
}

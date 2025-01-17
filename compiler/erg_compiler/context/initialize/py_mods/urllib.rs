use std::path::PathBuf;

use erg_common::vis::Visibility;

use crate::ty::constructors::{kw, module_from_path, mono, or, proc};
use crate::ty::Type;
use Type::*;

use crate::context::Context;
use crate::mod_cache::SharedModuleCache;
use crate::varinfo::Mutability;
use Mutability::*;
use Visibility::*;

impl Context {
    pub(crate) fn init_py_urllib_mod() -> Self {
        let mut urllib = Context::builtin_module("urllib", 4);
        urllib.py_mod_cache = Some(SharedModuleCache::new());
        let mut request_class = Context::builtin_mono_class("Request", 5);
        request_class.register_builtin_impl("data", mono("Bytes"), Immutable, Public);
        urllib.register_builtin_type(mono("urllib.request.Request"), request_class, Public, Const);
        urllib.register_builtin_impl("request", module_from_path("request"), Immutable, Public);
        let mut request = Context::builtin_module("urllib.request", 15);
        let t = proc(
            vec![kw("url", or(Str, mono("urllib.request.Request")))],
            None,
            vec![
                kw("data", or(mono("Bytes"), NoneType)),
                kw("timeout", or(Nat, NoneType)),
            ],
            mono("http.client.HTTPResponse"),
        );
        request.register_builtin_impl("urlopen", t, Immutable, Public);
        urllib.register_builtin_impl("parse", module_from_path("parse"), Immutable, Public);
        let parse = Context::builtin_module("urllib.parse", 15);
        urllib
            .py_mod_cache
            .as_ref()
            .unwrap()
            .register(PathBuf::from("request"), None, request);
        urllib
            .py_mod_cache
            .as_ref()
            .unwrap()
            .register(PathBuf::from("parse"), None, parse);
        urllib
    }
}

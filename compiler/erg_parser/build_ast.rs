use erg_common::config::ErgConfig;
use erg_common::traits::Runnable;
use erg_common::Str;

use crate::ast::AST;
use crate::desugar::Desugarer;
use crate::error::ParserRunnerErrors;
use crate::parse::ParserRunner;

/// Summarize parsing and desugaring
pub struct ASTBuilder {
    runner: ParserRunner,
}

impl ASTBuilder {
    pub fn new(cfg: ErgConfig) -> Self {
        Self {
            runner: ParserRunner::new(cfg),
        }
    }

    pub fn build(&mut self, src: String) -> Result<AST, ParserRunnerErrors> {
        let module = self.runner.parse(src)?;
        let mut desugarer = Desugarer::new();
        let module = desugarer.desugar(module);
        let ast = AST::new(Str::rc(self.runner.cfg().input.filename()), module);
        Ok(ast)
    }
}

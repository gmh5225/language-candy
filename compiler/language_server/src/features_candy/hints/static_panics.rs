use crate::{database::Database, utils::LspPositionConversion};
use candy_frontend::{
    ast_to_hir::AstToHir,
    mir::{Body, Expression, Mir, VisibleExpressions},
    module::Module,
};
use candy_vm::fiber::Panic;
use extension_trait::extension_trait;
use lsp_types::{Diagnostic, DiagnosticSeverity};
use std::mem;

#[extension_trait]
pub impl StaticPanicsOfMir for Mir {
    fn static_panics(&mut self) -> Vec<Panic> {
        let mut errors = vec![];
        self.body
            .collect_static_panics(&mut VisibleExpressions::none_visible(), &mut errors);
        errors
    }
}

#[extension_trait]
impl StaticPanicsOfBody for Body {
    fn collect_static_panics(&mut self, visible: &mut VisibleExpressions, panics: &mut Vec<Panic>) {
        for (id, expression) in &mut self.expressions {
            let mut expression = mem::replace(expression, Expression::Parameter);
            expression.collect_static_panics(visible, panics);
            visible.insert(*id, expression);
        }

        for (id, expression) in &mut self.expressions {
            *expression = visible.remove(*id);
        }
    }
}

#[extension_trait]
impl StaticPanicsOfExpression for Expression {
    fn collect_static_panics(&mut self, visible: &mut VisibleExpressions, panics: &mut Vec<Panic>) {
        match self {
            Expression::Function {
                parameters,
                responsible_parameter,
                body,
                ..
            } => {
                for parameter in parameters.iter() {
                    visible.insert(*parameter, Expression::Parameter);
                }
                visible.insert(*responsible_parameter, Expression::Parameter);

                body.collect_static_panics(visible, panics);

                for parameter in parameters {
                    visible.remove(*parameter);
                }
                visible.remove(*responsible_parameter);
            }
            Expression::Panic {
                reason,
                responsible,
            } => {
                let reason = visible.get(*reason);
                let responsible = visible.get(*responsible);

                let Expression::Text(reason) = reason else { return; };
                let Expression::HirId(responsible) = responsible else { return; };

                panics.push(Panic {
                    reason: reason.to_string(),
                    responsible: responsible.clone(),
                    panicked_child: None,
                });
            }
            _ => {}
        }
    }
}

#[extension_trait]
pub impl StaticPanicToDiagnostic for Panic {
    fn to_diagnostic(&self, db: &Database, module: &Module) -> Diagnostic {
        let call_span = db.hir_id_to_display_span(self.responsible.clone()).unwrap();
        let call_span = db.range_to_lsp_range(module.clone(), call_span);

        Diagnostic {
            range: call_span,
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: None,
            message: self.reason.to_string(),
            related_information: None,
            tags: None,
            data: None,
        }
    }
}

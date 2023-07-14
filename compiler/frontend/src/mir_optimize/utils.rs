use super::pure::PurenessInsights;
use crate::mir::{Body, Expression, Id, VisibleExpressions};
use rustc_hash::{FxHashMap, FxHashSet};
use std::mem;

impl Expression {
    /// All IDs defined inside this expression. For all expressions except
    /// functions, this returns an empty vector. The IDs are returned in the
    /// order that they are defined in.
    pub fn defined_ids(&self) -> Vec<Id> {
        let mut defined = vec![];
        self.collect_defined_ids(&mut defined);
        defined
    }
    fn collect_defined_ids(&self, defined: &mut Vec<Id>) {
        if let Expression::Function {
            parameters,
            responsible_parameter,
            body,
            ..
        } = self
        {
            defined.extend(parameters);
            defined.push(*responsible_parameter);
            body.collect_defined_ids(defined);
        }
    }
}
impl Body {
    pub fn defined_ids(&self) -> Vec<Id> {
        let mut defined = vec![];
        self.collect_defined_ids(&mut defined);
        defined
    }
    fn collect_defined_ids(&self, defined: &mut Vec<Id>) {
        for (id, expression) in self.iter() {
            defined.push(id);
            expression.collect_defined_ids(defined);
        }
    }
}

pub trait ReferenceCounts {
    /// All IDs referenced inside this expression. If this is a function, this
    /// also includes references to locally defined IDs.
    fn reference_counts(&self) -> FxHashMap<Id, usize> {
        let mut reference_counts = FxHashMap::default();
        self.collect_reference_counts(&mut reference_counts);
        reference_counts
    }
    fn collect_reference_counts(&self, reference_counts: &mut FxHashMap<Id, usize>);
}

impl ReferenceCounts for Expression {
    fn collect_reference_counts(&self, reference_counts: &mut FxHashMap<Id, usize>) {
        fn add(reference_counts: &mut FxHashMap<Id, usize>, id: Id) {
            *reference_counts.entry(id).or_default() += 1;
        }
        fn add_all(reference_counts: &mut FxHashMap<Id, usize>, ids: impl IntoIterator<Item = Id>) {
            for id in ids {
                add(reference_counts, id);
            }
        }

        match self {
            Expression::Int(_)
            | Expression::Text(_)
            | Expression::Builtin(_)
            | Expression::HirId(_) => {}
            Expression::Tag { value, .. } => {
                if let Some(value) = value {
                    add(reference_counts, *value);
                }
            }
            Expression::List(items) => {
                add_all(reference_counts, items.iter().copied());
            }
            Expression::Struct(fields) => {
                for (key, value) in fields {
                    add(reference_counts, *key);
                    add(reference_counts, *value);
                }
            }
            Expression::Reference(reference) => {
                add(reference_counts, *reference);
            }
            Expression::Function { body, .. } => body.collect_reference_counts(reference_counts),
            Expression::Parameter => {}
            Expression::Call {
                function,
                arguments,
                responsible,
            } => {
                add(reference_counts, *function);
                add_all(reference_counts, arguments.iter().copied());
                add(reference_counts, *responsible);
            }
            Expression::UseModule {
                current_module: _,
                relative_path,
                responsible,
            } => {
                add(reference_counts, *relative_path);
                add(reference_counts, *responsible);
            }
            Expression::Panic {
                reason,
                responsible,
            } => {
                add(reference_counts, *reason);
                add(reference_counts, *responsible);
            }
            Expression::TraceCallStarts {
                hir_call,
                function,
                arguments,
                responsible,
            } => {
                add(reference_counts, *hir_call);
                add(reference_counts, *function);
                add_all(reference_counts, arguments.iter().copied());
                add(reference_counts, *responsible);
            }
            Expression::TraceCallEnds { return_value } => {
                add(reference_counts, *return_value);
            }
            Expression::TraceExpressionEvaluated {
                hir_expression,
                value,
            } => {
                add(reference_counts, *hir_expression);
                add(reference_counts, *value);
            }
            Expression::TraceFoundFuzzableFunction {
                hir_definition,
                function,
            } => {
                add(reference_counts, *hir_definition);
                add(reference_counts, *function);
            }
        }
    }
}
impl ReferenceCounts for Body {
    fn collect_reference_counts(&self, reference_counts: &mut FxHashMap<Id, usize>) {
        for (_, expression) in self.iter() {
            expression.collect_reference_counts(reference_counts);
        }
    }
}

impl Expression {
    pub fn captured_ids(&self) -> FxHashSet<Id> {
        let mut ids: FxHashSet<_> = self.reference_counts().into_keys().collect();
        for id in self.defined_ids() {
            ids.remove(&id);
        }
        ids
    }
}

impl Body {
    pub fn all_ids(&self) -> FxHashSet<Id> {
        self.reference_counts().into_keys().collect()
    }
}

impl Id {
    pub fn semantically_equals(
        self,
        other: Id,
        visible: &VisibleExpressions,
        pureness: &PurenessInsights,
    ) -> Option<bool> {
        if self == other {
            return Some(true);
        }

        let self_expr = visible.get(self);
        let other_expr = visible.get(other);

        if let Expression::Reference(reference) = self_expr {
            return reference.semantically_equals(other, visible, pureness);
        }
        if let Expression::Reference(reference) = other_expr {
            return self.semantically_equals(*reference, visible, pureness);
        }

        if self_expr.is_parameter() || other_expr.is_parameter() {
            return None;
        }

        if self_expr == other_expr {
            return Some(true);
        }

        if !pureness.is_definition_const(self_expr) || !pureness.is_definition_const(other_expr) {
            return None;
        }

        Some(false)
    }
}

impl Expression {
    /// Replaces all referenced IDs. Does *not* replace IDs that are defined in
    /// this expression.
    pub fn replace_id_references(&mut self, replacer: &mut impl FnMut(&mut Id)) {
        match self {
            Expression::Int(_)
            | Expression::Text(_)
            | Expression::Builtin(_)
            | Expression::HirId(_) => {}
            Expression::Tag { value, .. } => {
                if let Some(value) = value {
                    replacer(value);
                }
            }
            Expression::List(items) => {
                for item in items {
                    replacer(item);
                }
            }
            Expression::Struct(fields) => {
                for (key, value) in fields {
                    replacer(key);
                    replacer(value);
                }
            }
            Expression::Reference(reference) => replacer(reference),
            Expression::Function {
                original_hirs: _,
                parameters,
                responsible_parameter,
                body,
            } => {
                for parameter in parameters {
                    replacer(parameter);
                }
                replacer(responsible_parameter);
                body.replace_id_references(replacer);
            }
            Expression::Parameter => {}
            Expression::Call {
                function,
                arguments,
                responsible,
            } => {
                replacer(function);
                for argument in arguments {
                    replacer(argument);
                }
                replacer(responsible);
            }
            Expression::UseModule {
                current_module: _,
                relative_path,
                responsible,
            } => {
                replacer(relative_path);
                replacer(responsible);
            }
            Expression::Panic {
                reason,
                responsible,
            } => {
                replacer(reason);
                replacer(responsible);
            }
            Expression::TraceCallStarts {
                hir_call,
                function,
                arguments,
                responsible,
            } => {
                replacer(hir_call);
                replacer(function);
                for argument in arguments {
                    replacer(argument);
                }
                replacer(responsible);
            }
            Expression::TraceCallEnds { return_value } => {
                replacer(return_value);
            }
            Expression::TraceExpressionEvaluated {
                hir_expression,
                value,
            } => {
                replacer(hir_expression);
                replacer(value);
            }
            Expression::TraceFoundFuzzableFunction {
                hir_definition,
                function,
            } => {
                replacer(hir_definition);
                replacer(function);
            }
        }
    }
}
impl Body {
    pub fn replace_id_references(&mut self, replacer: &mut impl FnMut(&mut Id)) {
        for (_, expression) in self.iter_mut() {
            expression.replace_id_references(replacer);
        }
    }
}

impl Expression {
    /// Replaces all IDs in this expression using the replacer, including
    /// definitions.
    pub fn replace_ids(&mut self, replacer: &mut impl FnMut(&mut Id)) {
        match self {
            Expression::Function {
                original_hirs: _,
                parameters,
                responsible_parameter,
                body,
            } => {
                for parameter in parameters {
                    replacer(parameter);
                }
                replacer(responsible_parameter);
                body.replace_ids(replacer);
            }
            // All other expressions don't define IDs and instead only contain
            // references. Thus, the function above does the job.
            _ => self.replace_id_references(replacer),
        }
    }
}
impl Body {
    pub fn replace_ids(&mut self, replacer: &mut impl FnMut(&mut Id)) {
        let body = mem::take(self);
        for (mut id, mut expression) in body.into_iter() {
            replacer(&mut id);
            expression.replace_ids(replacer);
            self.push(id, expression);
        }
    }
}

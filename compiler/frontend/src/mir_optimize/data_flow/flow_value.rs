use crate::{
    builtin_functions::BuiltinFunction,
    mir::Id,
    rich_ir::{ReferenceKey, RichIrBuilder, ToRichIr, TokenType},
};
use derive_more::From;
use enumset::EnumSet;
use itertools::Itertools;
use num_bigint::BigInt;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, From, Hash, PartialEq)]
pub enum FlowValue {
    Any,
    Not(BTreeSet<FlowValue>),
    #[from]
    Builtin(BuiltinFunction),
    AnyInt,
    #[from]
    Int(BigInt),
    AnyFunction,
    Function {
        return_value: Box<FlowValue>, // TODO
    },
    AnyList,
    #[from]
    List(Vec<FlowValue>),
    #[from]
    Reference(Id),
    AnyStruct,
    #[from]
    Struct(Vec<(FlowValue, FlowValue)>),
    AnyTag,
    Tag {
        symbol: String,
        value: Option<Box<FlowValue>>,
    },
    AnyText,
    #[from]
    Text(String),
}

impl ToRichIr for FlowValue {
    fn build_rich_ir(&self, builder: &mut RichIrBuilder) {
        match self {
            FlowValue::Any => {
                builder.push("<Any>", TokenType::Type, EnumSet::empty());
            }
            FlowValue::Not(values) => {
                FlowValue::Any.build_rich_ir(builder);
                for value in values {
                    builder.push(" - ", None, EnumSet::empty());
                    value.build_rich_ir(builder);
                }
            }
            FlowValue::Builtin(builtin) => {
                builtin.build_rich_ir(builder);
            }
            FlowValue::AnyInt => {
                builder.push("<Int>", TokenType::Type, EnumSet::empty());
            }
            FlowValue::Int(int) => {
                let range = builder.push(int.to_string(), TokenType::Int, EnumSet::empty());
                builder.push_reference(int.to_owned(), range);
            }
            FlowValue::AnyFunction => {
                builder.push("<Function>", TokenType::Type, EnumSet::empty());
            }
            FlowValue::Function { return_value } => {
                builder.push("{ ", None, EnumSet::empty());
                return_value.build_rich_ir(builder);
                builder.push(" }", None, EnumSet::empty());
            }
            FlowValue::AnyList => {
                builder.push("<List>", TokenType::Type, EnumSet::empty());
            }
            FlowValue::List(items) => {
                builder.push("(", None, EnumSet::empty());
                builder.push_children(items, ", ");
                builder.push(")", None, EnumSet::empty());
            }
            FlowValue::Reference(id) => id.build_rich_ir(builder),
            FlowValue::AnyStruct => {
                builder.push("<Struct>", TokenType::Type, EnumSet::empty());
            }
            FlowValue::Struct(fields) => {
                builder.push("[", None, EnumSet::empty());
                builder.push_children_custom(
                    fields.iter().collect_vec(),
                    |builder, (key, value)| {
                        key.build_rich_ir(builder);
                        builder.push(": ", None, EnumSet::empty());
                        value.build_rich_ir(builder);
                    },
                    ", ",
                );
                builder.push("]", None, EnumSet::empty());
            }
            FlowValue::AnyTag => {
                builder.push("<Tag>", TokenType::Type, EnumSet::empty());
            }
            FlowValue::Tag { symbol, value } => {
                let range = builder.push(symbol, TokenType::Symbol, EnumSet::empty());
                builder.push_reference(ReferenceKey::Symbol(symbol.to_owned()), range);
                if let Some(value) = value {
                    builder.push(" ", None, EnumSet::empty());
                    value.build_rich_ir(builder);
                }
            }
            FlowValue::AnyText => {
                builder.push("<Text>", TokenType::Type, EnumSet::empty());
            }
            FlowValue::Text(text) => {
                let range =
                    builder.push(format!(r#""{}""#, text), TokenType::Text, EnumSet::empty());
                builder.push_reference(text.to_owned(), range);
            }
            FlowValue::Text(text) => {
                let range =
                    builder.push(format!(r#""{}""#, text), TokenType::Text, EnumSet::empty());
                builder.push_reference(text.to_owned(), range);
            }
        }
    }
}

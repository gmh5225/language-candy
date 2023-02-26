use super::{pointer::Pointer, Heap};
use crate::{
    channel::ChannelId,
    lir::{Instruction, Lir},
    mir_to_lir::MirToLir,
};
use candy_frontend::{builtin_functions::BuiltinFunction, hir::Id, module::Module, TracingConfig};
use derive_more::Deref;
use itertools::Itertools;
use num_bigint::BigInt;
use rustc_hash::{FxHashMap, FxHasher};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    iter,
};

#[derive(Clone, Deref)]
pub struct Object {
    pub reference_count: usize,

    #[deref]
    pub data: Data,
}
#[derive(Clone)]
pub enum Data {
    Int(Int),
    Text(Text),
    Symbol(Symbol),
    List(List),
    Struct(Struct),
    HirId(Id),
    Closure(Closure),
    Builtin(Builtin),
    SendPort(SendPort),
    ReceivePort(ReceivePort),
}

#[derive(Clone)]
pub struct Int {
    pub value: BigInt,
}

#[derive(Clone)]
pub struct Text {
    pub value: String,
}

#[derive(Clone)]
pub struct Symbol {
    // TODO: Choose a more efficient representation.
    pub value: String,
}

#[derive(Default, Clone)]
pub struct List {
    pub items: Vec<Pointer>,
}

#[derive(Default, Clone)]
pub struct Struct {
    pub fields: Vec<(u64, Pointer, Pointer)>, // list of hash, key, and value
}

#[derive(Clone)]
pub struct Closure {
    pub captured: Vec<Pointer>,
    pub num_args: usize,
    pub body: Vec<Instruction>,
}

#[derive(Clone)]
pub struct Builtin {
    pub function: BuiltinFunction,
}

impl List {
    fn equals(&self, heap: &Heap, other: &List) -> bool {
        if self.items.len() != other.items.len() {
            return false;
        }

        self.items
            .iter()
            .zip_eq(other.items.iter())
            .all(|(a, &b)| a.equals(heap, b))
    }
}
impl Struct {
    pub fn from_fields(heap: &Heap, fields: FxHashMap<Pointer, Pointer>) -> Self {
        let mut s = Self::default();
        for (key, value) in fields {
            s.insert(heap, key, value);
        }
        s
    }
    /// If the struct contains the key, returns the index of its field.
    /// Otherwise, returns the index of where the key would be inserted.
    fn index_of_key(&self, heap: &Heap, key: Pointer, key_hash: u64) -> Result<usize, usize> {
        let index_of_first_hash_occurrence = self
            .fields
            .partition_point(|(existing_hash, _, _)| *existing_hash < key_hash);
        let fields_with_same_hash = self.fields[index_of_first_hash_occurrence..]
            .iter()
            .enumerate()
            .take_while(|(_, (existing_hash, _, _))| *existing_hash == key_hash)
            .map(|(i, (_, key, _))| (index_of_first_hash_occurrence + i, key));

        for (index, existing_key) in fields_with_same_hash {
            if existing_key.equals(heap, key) {
                return Ok(index);
            }
        }
        Err(index_of_first_hash_occurrence)
    }
    pub fn insert(&mut self, heap: &Heap, key: Pointer, value: Pointer) {
        let hash = key.hash(heap);
        let field = (hash, key, value);
        match self.index_of_key(heap, key, hash) {
            Ok(index) => self.fields[index] = field,
            Err(index) => self.fields.insert(index, field),
        }
    }
    pub fn get(&self, heap: &Heap, key: Pointer) -> Option<Pointer> {
        let index = self.index_of_key(heap, key, key.hash(heap)).ok()?;
        Some(self.fields[index].2)
    }
    fn len(&self) -> usize {
        self.fields.len()
    }
    pub fn iter(&self) -> impl Iterator<Item = (Pointer, Pointer)> {
        self.fields
            .clone()
            .into_iter()
            .map(|(_, key, value)| (key, value))
    }
    fn equals(&self, heap: &Heap, other: &Struct) -> bool {
        if self.len() != other.len() {
            return false;
        }

        self.iter()
            .zip_eq(other.iter())
            .all(|((key_a, value_a), (key_b, value_b))| {
                key_a.equals(heap, key_b) && value_a.equals(heap, value_b)
            })
    }
}

impl Closure {
    pub fn of_module_lir(lir: Lir) -> Self {
        Closure {
            captured: vec![],
            num_args: 0,
            body: lir.instructions,
        }
    }
    pub fn of_module<DB: MirToLir>(
        db: &DB,
        module: Module,
        tracing: TracingConfig,
    ) -> Option<Self> {
        let lir = db.lir(module, tracing)?;
        Some(Self::of_module_lir((*lir).clone()))
    }
}

#[derive(Clone)]
pub struct SendPort {
    pub channel: ChannelId,
}
#[derive(Clone)]
pub struct ReceivePort {
    pub channel: ChannelId,
}

impl SendPort {
    pub fn new(channel: ChannelId) -> Self {
        Self { channel }
    }
}
impl ReceivePort {
    pub fn new(channel: ChannelId) -> Self {
        Self { channel }
    }
}

impl Data {
    pub fn hash(&self, heap: &Heap) -> u64 {
        let mut cache = FxHashMap::default();
        self.hash_with_cache(heap, &mut cache)
    }
    pub fn hash_with_cache(&self, heap: &Heap, cache: &mut FxHashMap<Pointer, u64>) -> u64 {
        let mut state = DefaultHasher::default();
        match self {
            Data::Int(int) => int.value.hash(&mut state),
            Data::Text(text) => text.value.hash(&mut state),
            Data::Symbol(symbol) => symbol.value.hash(&mut state),
            Data::List(List { items }) => {
                for item in items {
                    item.hash_with_cache(heap, cache).hash(&mut state);
                }
            }
            Data::Struct(struct_) => {
                let mut s = 0;
                for (key, value) in struct_.iter() {
                    s ^= key.hash(heap);
                    s ^= value.hash(heap);
                }
                s.hash(&mut state);
            }
            Data::HirId(id) => {
                id.hash(&mut state);
            }
            Data::Closure(closure) => {
                for captured in &closure.captured {
                    captured.hash_with_cache(heap, cache).hash(&mut state);
                }
                closure.num_args.hash(&mut state);
                closure.body.hash(&mut state);
            }
            Data::Builtin(builtin) => builtin.function.hash(&mut state),
            Data::SendPort(port) => port.channel.hash(&mut state),
            Data::ReceivePort(port) => port.channel.hash(&mut state),
        }
        state.finish()
    }

    pub fn equals(&self, heap: &Heap, other: &Self) -> bool {
        match (self, other) {
            (Data::Int(a), Data::Int(b)) => a.value == b.value,
            (Data::Text(a), Data::Text(b)) => a.value == b.value,
            (Data::Symbol(a), Data::Symbol(b)) => a.value == b.value,
            (Data::List(a), Data::List(b)) => a.equals(heap, b),
            (Data::Struct(a), Data::Struct(b)) => a.equals(heap, b),
            (Data::HirId(a), Data::HirId(b)) => a == b,
            (Data::Closure(_), Data::Closure(_)) => false,
            (Data::Builtin(a), Data::Builtin(b)) => a.function == b.function,
            (Data::SendPort(a), Data::SendPort(b)) => a.channel == b.channel,
            (Data::ReceivePort(a), Data::ReceivePort(b)) => a.channel == b.channel,
            _ => false,
        }
    }

    pub fn children(&self) -> Box<dyn Iterator<Item = Pointer> + '_> {
        match self {
            Data::Int(_)
            | Data::Text(_)
            | Data::Symbol(_)
            | Data::Builtin(_)
            | Data::HirId(_)
            | Data::SendPort(_)
            | Data::ReceivePort(_) => Box::new(iter::empty()),
            Data::List(List { items }) => Box::new(items.iter().copied()),
            Data::Struct(struct_) => Box::new(struct_.iter().flat_map(|(a, b)| vec![a, b])),
            Data::Closure(closure) => Box::new(closure.captured.iter().copied()),
        }
    }

    pub fn change_pointers(&mut self, pointer_map: &FxHashMap<Pointer, Pointer>) {
        match self {
            Data::Int(_)
            | Data::Text(_)
            | Data::Symbol(_)
            | Data::HirId(_)
            | Data::Builtin(_)
            | Data::SendPort(_)
            | Data::ReceivePort(_) => {}
            Data::List(List { items }) => {
                for item in items {
                    *item = pointer_map.get(item).copied().unwrap_or(*item);
                }
            }
            Data::Struct(Struct { fields }) => {
                for (_, key, value) in fields {
                    *key = pointer_map.get(key).copied().unwrap_or(*key);
                    *value = pointer_map.get(value).copied().unwrap_or(*value);
                }
            }
            Data::Closure(Closure { captured, .. }) => {
                for captured in captured {
                    *captured = pointer_map.get(captured).copied().unwrap_or(*captured);
                }
            }
        }
    }

    pub fn closure(&self) -> Option<&Closure> {
        if let Data::Closure(closure) = self {
            Some(closure)
        } else {
            None
        }
    }
    pub fn channel(&self) -> Option<ChannelId> {
        if let Data::SendPort(SendPort { channel }) | Data::ReceivePort(ReceivePort { channel }) =
            self
        {
            Some(*channel)
        } else {
            None
        }
    }
}

impl Pointer {
    pub fn hash(&self, heap: &Heap) -> u64 {
        heap.get(*self).hash(heap)
    }
    pub fn hash_with_cache(&self, heap: &Heap, cache: &mut FxHashMap<Pointer, u64>) -> u64 {
        if let Some(hash) = cache.get(self) {
            return *hash;
        }
        let hash = heap.get(*self).hash_with_cache(heap, cache);
        cache.insert(*self, hash);
        hash
    }

    pub fn equals(&self, heap: &Heap, other: Self) -> bool {
        if *self == other {
            return true;
        }
        heap.get(*self).equals(heap, heap.get(other))
    }

    pub fn format(&self, heap: &Heap) -> String {
        self.format_helper(heap, false)
    }
    pub fn format_debug(&self, heap: &Heap) -> String {
        self.format_helper(heap, true)
    }
    fn format_helper(&self, heap: &Heap, is_debug: bool) -> String {
        match &heap.get(*self).data {
            Data::Int(int) => format!("{}", int.value),
            Data::Text(text) => format!("\"{}\"", text.value),
            Data::Symbol(symbol) => symbol.value.to_string(),
            Data::List(List { items }) => format!(
                "({})",
                if items.is_empty() {
                    ",".to_owned()
                } else {
                    items
                        .iter()
                        .map(|item| item.format_helper(heap, is_debug))
                        .join(", ")
                },
            ),
            Data::Struct(struct_) => format!(
                "[{}]",
                struct_
                    .iter()
                    .map(|(key, value)| (
                        key.format_helper(heap, is_debug),
                        value.format_helper(heap, is_debug)
                    ))
                    .sorted_by(|(key_a, _), (key_b, _)| key_a.cmp(key_b))
                    .map(|(key, value)| format!("{}: {}", key, value))
                    .join(", ")
            ),
            Data::HirId(id) => format!("{id:?}"),
            Data::Closure(_) => {
                if is_debug {
                    format!("{{{self}}}")
                } else {
                    "{…}".to_string()
                }
            }
            Data::Builtin(builtin) => format!("builtin{:?}", builtin.function),
            Data::SendPort(port) => format!("sendPort {:?}", port.channel),
            Data::ReceivePort(port) => format!("receivePort {:?}", port.channel),
        }
    }
}

macro_rules! impl_data_try_into_type {
    ($type:ty, $variant:tt, $error_message:expr$(,)?) => {
        impl TryInto<$type> for Data {
            type Error = String;

            fn try_into(self) -> Result<$type, Self::Error> {
                match self {
                    Data::$variant(it) => Ok(it),
                    _ => Err($error_message.to_string()),
                }
            }
        }
        impl<'a> TryInto<&'a $type> for &'a Data {
            type Error = String;

            fn try_into(self) -> Result<&'a $type, Self::Error> {
                match &self {
                    Data::$variant(it) => Ok(it),
                    _ => Err($error_message.to_string()),
                }
            }
        }
    };
}
impl_data_try_into_type!(Int, Int, "Expected an int.");
impl_data_try_into_type!(Text, Text, "Expected a text.");
impl_data_try_into_type!(Symbol, Symbol, "Expected a symbol.");
impl_data_try_into_type!(List, List, "Expected a list.");
impl_data_try_into_type!(Struct, Struct, "Expected a struct.");
impl_data_try_into_type!(Id, HirId, "Expected a HIR ID.");
impl_data_try_into_type!(Closure, Closure, "Expected a closure.");
impl_data_try_into_type!(SendPort, SendPort, "Expected a send port.");
impl_data_try_into_type!(ReceivePort, ReceivePort, "Expected a receive port.");

impl TryInto<bool> for &Data {
    type Error = String;

    fn try_into(self) -> Result<bool, Self::Error> {
        let symbol: &Symbol = self.try_into()?;
        match symbol.value.as_str() {
            "True" => Ok(true),
            "False" => Ok(false),
            _ => Err("Expected `True` or `False`.".to_string()),
        }
    }
}
